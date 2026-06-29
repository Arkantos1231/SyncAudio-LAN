use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::HeapConsumer;

/// Starts audio playback on the default output device using the device's native
/// format. Stereo samples from the ring buffer are upmixed to the device's
/// channel count so that subwoofers and surround speakers are driven correctly.
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

    // min_stereo_samples is based on stereo pairs in the ring buffer.
    let min_stereo_samples = (buffer_size_ms as usize)
        * (config.sample_rate.0 as usize / 1000)
        * 2;

    // Build f32 output stream — WASAPI shared mode converts f32 to the
    // device's native bit depth internally, so we don't need to branch here.
    let stream = device.build_output_stream(
        &config,
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            fill_output(data, &mut cons, native_channels, min_stereo_samples);
        },
        |err| log::error!("Error en playback: {err}"),
        None,
    )?;

    stream.play()?;
    Ok(stream)
}

/// Reads stereo f32 pairs from `cons` and distributes them across `out_channels`
/// speakers using a standard upmix matrix.
///
/// Windows surround channel order: FL, FR, FC, LFE, BL, BR, SL, SR
fn fill_output(
    data: &mut [f32],
    cons: &mut HeapConsumer<f32>,
    out_channels: usize,
    min_stereo_samples: usize,
) {
    if cons.len() < min_stereo_samples {
        data.fill(0.0);
        return;
    }

    let mut stereo = [0.0f32; 2];
    let total_frames = data.len() / out_channels;

    for (frame_idx, out_frame) in data.chunks_mut(out_channels).take(total_frames).enumerate() {
        if cons.len() >= 2 {
            cons.pop_slice(&mut stereo);
        } else {
            // Ring buffer dry — silence the rest
            for s in data[frame_idx * out_channels..].iter_mut() {
                *s = 0.0;
            }
            return;
        }

        let (l, r) = (stereo[0], stereo[1]);

        match out_channels {
            1 => {
                out_frame[0] = (l + r) * 0.5;
            }
            2 => {
                out_frame[0] = l;
                out_frame[1] = r;
            }
            4 => {
                // FL, FR, BL, BR
                out_frame[0] = l;
                out_frame[1] = r;
                out_frame[2] = l;
                out_frame[3] = r;
            }
            6 => {
                // 5.1: FL, FR, FC, LFE, BL, BR
                let center = (l + r) * 0.5;
                let lfe    = (l + r) * 0.5;
                out_frame[0] = l;
                out_frame[1] = r;
                out_frame[2] = center;
                out_frame[3] = lfe;
                out_frame[4] = l;
                out_frame[5] = r;
            }
            8 => {
                // 7.1: FL, FR, FC, LFE, BL, BR, SL, SR
                let center = (l + r) * 0.5;
                let lfe    = (l + r) * 0.5;
                out_frame[0] = l;
                out_frame[1] = r;
                out_frame[2] = center;
                out_frame[3] = lfe;
                out_frame[4] = l;
                out_frame[5] = r;
                out_frame[6] = l;
                out_frame[7] = r;
            }
            _ => {
                let mono = (l + r) * 0.5;
                out_frame.fill(mono);
            }
        }
    }
}
