use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::HeapConsumer;

/// Starts audio playback on the default output device.
/// Reads f32 samples from the ring buffer consumer.
/// Outputs silence when the jitter buffer hasn't filled to buffer_size_ms yet (underrun guard).
pub fn start_playback(
    mut cons: HeapConsumer<f32>,
    buffer_size_ms: u32,
) -> anyhow::Result<cpal::Stream> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| anyhow::anyhow!("No se encontró dispositivo de salida de audio"))?;

    log::info!("Reproduciendo en: {}", device.name().unwrap_or_default());

    // Use the device's native format so WASAPI accepts the output stream
    let supported = device.default_output_config()?;
    log::info!("Formato nativo del dispositivo de salida: {:?}", supported);
    let config = supported.config();

    // Minimum samples buffered before we start playing to absorb jitter
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
                // Jitter buffer not filled yet — output silence
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
