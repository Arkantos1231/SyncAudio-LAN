use ringbuf::HeapConsumer;
use tokio::net::UdpSocket;
use crate::audio::FRAME_SIZE;
use super::UDP_PORT;

/// Sends raw PCM f32 frames over UDP.
/// Each packet: [seq u32 BE][count u16 BE][f32 samples as little-endian bytes]
pub async fn run_sender(
    target_ip: String,
    mut cons: HeapConsumer<f32>,
    mut cancel_rx: tokio::sync::oneshot::Receiver<()>,
) -> anyhow::Result<()> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    socket.connect(format!("{target_ip}:{UDP_PORT}")).await?;
    log::info!("Sender UDP conectado a {target_ip}:{UDP_PORT}");

    let mut seq: u32 = 0;
    let mut frame = vec![0f32; FRAME_SIZE];

    loop {
        match cancel_rx.try_recv() {
            Ok(_) | Err(tokio::sync::oneshot::error::TryRecvError::Closed) => break,
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {}
        }

        if cons.len() < FRAME_SIZE {
            tokio::time::sleep(tokio::time::Duration::from_millis(4)).await;
            continue;
        }

        cons.pop_slice(&mut frame);

        // Build packet: [seq u32 BE][sample_count u16 BE][raw f32 LE bytes]
        let sample_bytes = bytemuck::cast_slice::<f32, u8>(&frame);
        let mut packet = Vec::with_capacity(6 + sample_bytes.len());
        packet.extend_from_slice(&seq.to_be_bytes());
        packet.extend_from_slice(&(FRAME_SIZE as u16).to_be_bytes());
        packet.extend_from_slice(sample_bytes);

        if let Err(e) = socket.send(&packet).await {
            log::warn!("Error al enviar UDP: {e}");
        }
        seq = seq.wrapping_add(1);
    }

    log::info!("Sender detenido.");
    Ok(())
}
