use ringbuf::HeapProducer;
use tokio::net::UdpSocket;
use super::{UDP_PORT, MAX_PACKET_SIZE};

/// Receives raw PCM f32 frames over UDP and pushes samples into the ring buffer.
pub async fn run_receiver(
    mut prod: HeapProducer<f32>,
    mut cancel_rx: tokio::sync::oneshot::Receiver<()>,
) -> anyhow::Result<()> {
    let socket = UdpSocket::bind(format!("0.0.0.0:{UDP_PORT}")).await?;
    log::info!("Receiver escuchando en puerto {UDP_PORT}");

    let mut buf = vec![0u8; MAX_PACKET_SIZE];

    loop {
        tokio::select! {
            _ = &mut cancel_rx => break,

            result = socket.recv(&mut buf) => {
                match result {
                    Ok(n) if n >= 6 => {
                        let _seq = u32::from_be_bytes(buf[0..4].try_into().unwrap());
                        let count = u16::from_be_bytes(buf[4..6].try_into().unwrap()) as usize;
                        let payload = &buf[6..n];

                        // Reinterpret bytes as f32 samples
                        let expected_bytes = count * 4;
                        if payload.len() >= expected_bytes {
                            let samples: &[f32] = bytemuck::cast_slice(&payload[..expected_bytes]);
                            prod.push_slice(samples);
                        } else {
                            log::warn!("Paquete UDP incompleto: esperado {expected_bytes}B, recibido {}B", payload.len());
                        }
                    }
                    Ok(_) => log::warn!("Paquete UDP demasiado pequeño"),
                    Err(e) => log::warn!("Error al recibir UDP: {e}"),
                }
            }
        }
    }

    log::info!("Receiver detenido.");
    Ok(())
}
