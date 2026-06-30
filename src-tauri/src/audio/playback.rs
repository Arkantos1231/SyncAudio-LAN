use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::HeapConsumer;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// Starts audio playback on the default output device using the device's native
/// format. Applies a one-time jitter buffer fill before starting, then plays
/// continuously — only stopping if the buffer is completely empty (instead of
/// re-triggering the fill threshold on every callback).
pub fn start_playback(
    mut cons: HeapConsumer<f32>,
    buffer_size_ms: u32,
) -> anyhow::Result<cpal::Stream> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| anyhow::anyhow!("No se encontró dispositivo de salida de audio"))?;

    log::info!("Reproduciendo en: {}", device.name().unwrap_or_default());

    let supported = device.default_output_config()?;
    log::info!(
        "Formato nativo del receiver: sample_rate={} channels={} format={:?}",
        supported.sample_rate().0,
        supported.channels(),
        supported.sample_format()
    );

    let config = supported.config();
    let native_channels = config.channels as usize;

    // One-time jitter buffer fill: wait until this many stereo samples are
    // available before starting playback. After that, play whatever arrives.
    let fill_threshold = (buffer_size_ms as usize)
        .max(20)  // at least 20ms even if config is lower
        * (config.sample_rate.0 as usize / 1000)
        * 2; // ring buffer always holds stereo (2 ch)

    // Shared state between audio callback and logging task
    let started = Arc::new(AtomicBool::new(false));
    let samples_played = Arc::new(AtomicU64::new(0));
    let silence_callbacks = Arc::new(AtomicU64::new(0));

    let started_cb    = started.clone();
    let played_cb     = samples_played.clone();
    let silence_cb    = silence_callbacks.clone();

    let stream = device.build_output_stream(
        &config,
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            if !started_cb.load(Ordering::Relaxed) {
                if cons.len() < fill_threshold {
                    // Jitter buffer still filling — output silence
                    data.fill(0.0);
                    silence_cb.fetch_add(1, Ordering::Relaxed);
                    return;
                }
                started_cb.store(true, Ordering::Relaxed);
            }

            let available = cons.len();
            if available == 0 {
                // Buffer ran dry — reset so we wait for fill_threshold again
                // before playing (avoids playing a tiny burst then stopping).
                started_cb.store(false, Ordering::Relaxed);
                data.fill(0.0);
                silence_cb.fetch_add(1, Ordering::Relaxed);
                return;
            }

            fill_output(data, &mut cons, native_channels);
            played_cb.fetch_add(data.len() as u64, Ordering::Relaxed);
        },
        |err| log::error!("Error en playback: {err}"),
        None,
    )?;

    // Log buffer health every 5 seconds
    let played_log  = samples_played;
    let silence_log = silence_callbacks;
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(5));
            let played  = played_log.swap(0, Ordering::Relaxed);
            let silence = silence_log.swap(0, Ordering::Relaxed);
            if played + silence > 0 {
                log::info!(
                    "Playback — muestras reproducidas: {}  callbacks en silencio: {}",
                    played, silence
                );
            }
        }
    });

    stream.play()?;
    Ok(stream)
}

/// Distributes stereo f32 pairs from `cons` across `out_channels` speakers.
/// For 2-channel output (the common case), samples are written directly.
/// For surround, the stereo is upmixed including LFE so the subwoofer works.
///
/// Windows surround channel order: FL, FR, FC, LFE, BL, BR, SL, SR
fn fill_output(data: &mut [f32], cons: &mut HeapConsumer<f32>, out_channels: usize) {
    if out_channels == 2 {
        // Fast path: read directly into the output buffer
        let available = cons.len();
        let to_read   = data.len().min(available);
        cons.pop_slice(&mut data[..to_read]);
        data[to_read..].fill(0.0);
        return;
    }

    // Surround upmix — read 2 stereo samples per output frame
    let mut stereo = [0.0f32; 2];
    for out_frame in data.chunks_mut(out_channels) {
        if cons.len() >= 2 {
            cons.pop_slice(&mut stereo);
        } else {
            out_frame.fill(0.0);
            continue;
        }

        let (l, r) = (stereo[0], stereo[1]);
        match out_channels {
            1 => { out_frame[0] = (l + r) * 0.5; }
            4 => { out_frame[0] = l; out_frame[1] = r; out_frame[2] = l; out_frame[3] = r; }
            6 => {
                let center = (l + r) * 0.5;
                let lfe    = (l + r) * 0.5;
                out_frame[0] = l; out_frame[1] = r; out_frame[2] = center;
                out_frame[3] = lfe; out_frame[4] = l; out_frame[5] = r;
            }
            8 => {
                let center = (l + r) * 0.5;
                let lfe    = (l + r) * 0.5;
                out_frame[0] = l; out_frame[1] = r; out_frame[2] = center;
                out_frame[3] = lfe; out_frame[4] = l; out_frame[5] = r;
                out_frame[6] = l; out_frame[7] = r;
            }
            _ => { let mono = (l + r) * 0.5; out_frame.fill(mono); }
        }
    }
}
