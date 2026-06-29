use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;
use ringbuf::HeapProducer;

/// Starts WASAPI loopback capture on the default output device.
/// Always outputs interleaved stereo f32 samples into the ring buffer,
/// downmixing from the device's native channel count if necessary.
pub fn start_capture(mut prod: HeapProducer<f32>) -> anyhow::Result<cpal::Stream> {
    let host = cpal::default_host();

    let device = host
        .default_output_device()
        .ok_or_else(|| anyhow::anyhow!("No se encontró dispositivo de salida de audio"))?;

    log::info!("Capturando desde: {}", device.name().unwrap_or_default());

    // WASAPI loopback requires the device's exact mix format
    let supported = device.default_output_config()?;
    log::info!("Formato nativo: sample_rate={} channels={} format={:?}",
        supported.sample_rate().0, supported.channels(), supported.sample_format());

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
                let f32s: Vec<f32> = data.iter().map(|&s| s as f32 / u16::MAX as f32 * 2.0 - 1.0).collect();
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

/// Pushes interleaved frames from `data` (with `src_channels` channels) into
/// `prod` as stereo (2 channels). Downmixes by taking only the first two
/// channels; if the source is mono, duplicates the single channel.
fn push_as_stereo(prod: &mut HeapProducer<f32>, data: &[f32], src_channels: usize) {
    if src_channels == 2 {
        let written = prod.push_slice(data);
        if written < data.len() {
            log::warn!("Ring buffer lleno — descartando {} muestras", data.len() - written);
        }
        return;
    }

    let stereo: Vec<f32> = data
        .chunks(src_channels)
        .flat_map(|frame| {
            let l = frame[0];
            let r = if src_channels > 1 { frame[1] } else { frame[0] };
            [l, r]
        })
        .collect();

    let written = prod.push_slice(&stereo);
    if written < stereo.len() {
        log::warn!("Ring buffer lleno — descartando {} muestras", stereo.len() - written);
    }
}
