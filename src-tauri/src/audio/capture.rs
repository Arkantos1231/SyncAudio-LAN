use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use ringbuf::HeapProducer;

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

    // Use the device's native format so WASAPI accepts the loopback stream
    let supported = device.default_output_config()?;
    log::info!("Formato nativo del dispositivo: {:?}", supported);
    let config = supported.config();

    let stream = match supported.sample_format() {
        SampleFormat::F32 => device.build_input_stream(
            &config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let written = prod.push_slice(data);
                if written < data.len() {
                    log::warn!("Ring buffer lleno — descartando {} muestras", data.len() - written);
                }
            },
            |err| log::error!("Error en captura de audio: {err}"),
            None,
        )?,
        SampleFormat::I16 => device.build_input_stream(
            &config,
            move |data: &[i16], _: &cpal::InputCallbackInfo| {
                let samples: Vec<f32> = data.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
                let written = prod.push_slice(&samples);
                if written < samples.len() {
                    log::warn!("Ring buffer lleno — descartando {} muestras", samples.len() - written);
                }
            },
            |err| log::error!("Error en captura de audio: {err}"),
            None,
        )?,
        SampleFormat::U16 => device.build_input_stream(
            &config,
            move |data: &[u16], _: &cpal::InputCallbackInfo| {
                let samples: Vec<f32> = data.iter().map(|&s| s as f32 / u16::MAX as f32 * 2.0 - 1.0).collect();
                let written = prod.push_slice(&samples);
                if written < samples.len() {
                    log::warn!("Ring buffer lleno — descartando {} muestras", samples.len() - written);
                }
            },
            |err| log::error!("Error en captura de audio: {err}"),
            None,
        )?,
        fmt => anyhow::bail!("Formato de audio no soportado: {fmt:?}"),
    };

    stream.play()?;
    Ok(stream)
}
