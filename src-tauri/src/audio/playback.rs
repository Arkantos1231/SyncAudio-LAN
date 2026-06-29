use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::HeapConsumer;

/// Starts audio playback on the default output device.
/// Prefers 48 kHz stereo f32 for compatibility with the sender's captured format.
/// Falls back to the device's native format if 48 kHz stereo is not supported.
pub fn start_playback(
    mut cons: HeapConsumer<f32>,
    buffer_size_ms: u32,
) -> anyhow::Result<cpal::Stream> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| anyhow::anyhow!("No se encontró dispositivo de salida de audio"))?;

    log::info!("Reproduciendo en: {}", device.name().unwrap_or_default());

    // Prefer 48 kHz stereo so it matches what the sender always delivers
    let config = best_output_config(&device);
    log::info!("Config de playback: sample_rate={} channels={}",
        config.sample_rate.0, config.channels);

    let min_samples = (buffer_size_ms as usize)
        * (config.sample_rate.0 as usize / 1000)
        * config.channels as usize;

    let stream = device.build_output_stream(
        &config,
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            let available = cons.len();
            if available >= min_samples {
                let to_read = data.len().min(available);
                cons.pop_slice(&mut data[..to_read]);
                for s in data[to_read..].iter_mut() {
                    *s = 0.0;
                }
            } else {
                for s in data.iter_mut() {
                    *s = 0.0;
                }
            }
        },
        |err| log::error!("Error en playback: {err}"),
        None,
    )?;

    stream.play()?;
    Ok(stream)
}

/// Returns a StreamConfig for the device, preferring 48 kHz stereo.
/// Falls back to the device's native mix format if 48 kHz stereo is not available.
fn best_output_config(device: &cpal::Device) -> cpal::StreamConfig {
    use cpal::traits::DeviceTrait;

    const PREFERRED_RATE: u32 = 48_000;
    const PREFERRED_CHANNELS: u16 = 2;

    let supports_48k = device
        .supported_output_configs()
        .map(|mut configs| {
            configs.any(|c| {
                c.channels() == PREFERRED_CHANNELS
                    && c.min_sample_rate().0 <= PREFERRED_RATE
                    && c.max_sample_rate().0 >= PREFERRED_RATE
            })
        })
        .unwrap_or(false);

    if supports_48k {
        cpal::StreamConfig {
            channels: PREFERRED_CHANNELS,
            sample_rate: cpal::SampleRate(PREFERRED_RATE),
            buffer_size: cpal::BufferSize::Default,
        }
    } else {
        device
            .default_output_config()
            .map(|c| c.config())
            .unwrap_or(cpal::StreamConfig {
                channels: PREFERRED_CHANNELS,
                sample_rate: cpal::SampleRate(PREFERRED_RATE),
                buffer_size: cpal::BufferSize::Default,
            })
    }
}
