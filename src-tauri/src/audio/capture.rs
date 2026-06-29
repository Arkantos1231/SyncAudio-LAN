use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use ringbuf::HeapProducer;

/// Starts WASAPI loopback capture on the default output device.
/// Always produces interleaved stereo f32 into the ring buffer.
/// When the device has more than 2 channels (e.g. 5.1 surround), the extra
/// channels — including LFE (bass) — are mixed down so no bass is lost.
pub fn start_capture(mut prod: HeapProducer<f32>) -> anyhow::Result<cpal::Stream> {
    let host = cpal::default_host();

    let device = host
        .default_output_device()
        .ok_or_else(|| anyhow::anyhow!("No se encontró dispositivo de salida de audio"))?;

    log::info!("Capturando loopback desde: {}", device.name().unwrap_or_default());

    // WASAPI loopback requires the device's exact mix format
    let supported = device.default_output_config()?;
    log::info!(
        "Formato nativo del sender: sample_rate={} channels={} format={:?}",
        supported.sample_rate().0,
        supported.channels(),
        supported.sample_format()
    );

    let native_channels = supported.channels() as usize;
    let config = supported.config();

    let stream = match supported.sample_format() {
        SampleFormat::F32 => device.build_input_stream(
            &config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                push_as_stereo(&mut prod, data, native_channels);
            },
            |err| log::error!("Error en captura de audio: {err}"),
            None,
        )?,
        SampleFormat::I16 => device.build_input_stream(
            &config,
            move |data: &[i16], _: &cpal::InputCallbackInfo| {
                let f32s: Vec<f32> = data.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
                push_as_stereo(&mut prod, &f32s, native_channels);
            },
            |err| log::error!("Error en captura de audio: {err}"),
            None,
        )?,
        SampleFormat::U16 => device.build_input_stream(
            &config,
            move |data: &[u16], _: &cpal::InputCallbackInfo| {
                let f32s: Vec<f32> =
                    data.iter().map(|&s| s as f32 / u16::MAX as f32 * 2.0 - 1.0).collect();
                push_as_stereo(&mut prod, &f32s, native_channels);
            },
            |err| log::error!("Error en captura de audio: {err}"),
            None,
        )?,
        fmt => anyhow::bail!("Formato de audio no soportado: {fmt:?}"),
    };

    stream.play()?;
    Ok(stream)
}

/// Downmixes interleaved `src_channels`-channel audio to stereo f32 and pushes
/// it into `prod`. Handles:
///   - Mono (1ch) → duplicate to L and R
///   - Stereo (2ch) → pass through
///   - Surround (≥4ch) → full mix including LFE/bass channel
///
/// Windows 5.1 channel order: FL, FR, FC, LFE, BL, BR
fn push_as_stereo(prod: &mut HeapProducer<f32>, data: &[f32], src_channels: usize) {
    let stereo: Vec<f32> = if src_channels == 2 {
        // Fast path: already stereo
        data.to_vec()
    } else {
        data.chunks(src_channels)
            .flat_map(|frame| {
                let get = |i: usize| frame.get(i).copied().unwrap_or(0.0);

                let (l, r) = match src_channels {
                    1 => {
                        let m = get(0);
                        (m, m)
                    }
                    3 => {
                        // FL, FR, FC
                        let (fl, fr, fc) = (get(0), get(1), get(2));
                        (fl + 0.5 * fc, fr + 0.5 * fc)
                    }
                    4 => {
                        // FL, FR, FC, LFE  (quad or 3.1)
                        let (fl, fr, fc, lfe) = (get(0), get(1), get(2), get(3));
                        (fl + 0.5 * fc + 0.5 * lfe, fr + 0.5 * fc + 0.5 * lfe)
                    }
                    6 => {
                        // 5.1: FL, FR, FC, LFE, BL, BR
                        let (fl, fr, fc, lfe, bl, br) =
                            (get(0), get(1), get(2), get(3), get(4), get(5));
                        (
                            fl + 0.5 * fc + 0.5 * lfe + 0.5 * bl,
                            fr + 0.5 * fc + 0.5 * lfe + 0.5 * br,
                        )
                    }
                    8 => {
                        // 7.1: FL, FR, FC, LFE, BL, BR, SL, SR
                        let (fl, fr, fc, lfe, bl, br, sl, sr) = (
                            get(0), get(1), get(2), get(3),
                            get(4), get(5), get(6), get(7),
                        );
                        (
                            fl + 0.5 * fc + 0.5 * lfe + 0.35 * bl + 0.35 * sl,
                            fr + 0.5 * fc + 0.5 * lfe + 0.35 * br + 0.35 * sr,
                        )
                    }
                    _ => (get(0), get(1)),
                };

                // Normalize to prevent clipping from channel summation
                let scale = 0.70;
                [(l * scale).clamp(-1.0, 1.0), (r * scale).clamp(-1.0, 1.0)]
            })
            .collect()
    };

    let written = prod.push_slice(&stereo);
    if written < stereo.len() {
        log::warn!("Ring buffer lleno — descartando {} muestras", stereo.len() - written);
    }
}
