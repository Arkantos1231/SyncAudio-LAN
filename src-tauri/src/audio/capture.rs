use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::HeapProducer;
use super::{CHANNELS, SAMPLE_RATE};

/// Starts WASAPI loopback capture on the default output device.
/// Captured f32 samples are pushed into the ring buffer producer.
/// The returned Stream must be kept alive for as long as capture is needed.
pub fn start_capture(mut prod: HeapProducer<f32>) -> anyhow::Result<cpal::Stream> {
    let host = cpal::default_host();

    // Use the default OUTPUT device — WASAPI will capture its loopback signal
    let device = host
        .default_output_device()
        .ok_or_else(|| anyhow::anyhow!("No se encontró dispositivo de salida de audio"))?;

    log::info!("Capturando desde: {}", device.name().unwrap_or_default());

    let stream_config = cpal::StreamConfig {
        channels: CHANNELS,
        sample_rate: cpal::SampleRate(SAMPLE_RATE),
        buffer_size: cpal::BufferSize::Default,
    };

    let stream = device.build_input_stream(
        &stream_config,
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            let written = prod.push_slice(data);
            if written < data.len() {
                log::warn!("Ring buffer lleno — descartando {} muestras", data.len() - written);
            }
        },
        |err| log::error!("Error en captura de audio: {err}"),
        None,
    )?;

    stream.play()?;
    Ok(stream)
}
