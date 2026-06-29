use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri::{AppHandle, Emitter, State};
use crate::{
    audio::{capture, playback, RING_CAPACITY},
    config::{save_config, AppConfig, Role},
    network::{receiver, sender},
    state::{AppState, StreamHandle, STATUS_CONNECTED, STATUS_CONNECTING, STATUS_ERROR, STATUS_IDLE},
};

// ─── Config ──────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_config(state: State<'_, Arc<AppState>>) -> Result<AppConfig, String> {
    Ok(state.config.lock().clone())
}

#[tauri::command]
pub async fn set_config(
    state: State<'_, Arc<AppState>>,
    config: AppConfig,
) -> Result<(), String> {
    *state.config.lock() = config.clone();
    save_config(&config).map_err(|e| e.to_string())
}

// ─── Status ───────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_status(state: State<'_, Arc<AppState>>) -> Result<u8, String> {
    Ok(state.connection_status.load(Ordering::Relaxed))
}

// ─── Utilities ────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_local_ip() -> Result<String, String> {
    use std::net::UdpSocket;
    let sock = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
    sock.connect("8.8.8.8:80").map_err(|e| e.to_string())?;
    Ok(sock.local_addr().map_err(|e| e.to_string())?.ip().to_string())
}

#[tauri::command]
pub async fn set_autostart(enabled: bool, app_handle: AppHandle) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    if enabled {
        app_handle.autolaunch().enable().map_err(|e| e.to_string())?;
    } else {
        app_handle.autolaunch().disable().map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ─── Connect / Disconnect ─────────────────────────────────────────────────────

#[tauri::command]
pub async fn connect(
    state: State<'_, Arc<AppState>>,
    app_handle: AppHandle,
) -> Result<(), String> {
    let state = state.inner().clone();
    do_connect(state, app_handle).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn disconnect(
    state: State<'_, Arc<AppState>>,
    app_handle: AppHandle,
) -> Result<(), String> {
    let state = state.inner().clone();
    do_disconnect(state.clone(), app_handle).await;
    Ok(())
}

// ─── Shared connect/disconnect logic ─────────────────────────────────────────

pub async fn do_connect(state: Arc<AppState>, app: AppHandle) -> anyhow::Result<()> {
    // Stop any existing stream first
    do_disconnect(state.clone(), app.clone()).await;

    let config = state.config.lock().clone();
    state.connection_status.store(STATUS_CONNECTING, Ordering::Relaxed);
    let _ = app.emit("status-changed", "connecting");

    let stop_flag = Arc::new(AtomicBool::new(false));
    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();

    let rb = ringbuf::HeapRb::<f32>::new(RING_CAPACITY);
    let (prod, cons) = rb.split();

    // Mute local output on the sender so audio only plays on the receiver
    if config.role == Role::Sender {
        if let Err(e) = crate::audio::volume::set_output_muted(true) {
            log::warn!("No se pudo silenciar la salida de audio: {e}");
        }
    }

    match config.role {
        Role::Sender => {
            // Audio capture runs on a dedicated thread (cpal::Stream is !Send on Windows)
            let stop_for_thread = stop_flag.clone();
            let state_for_thread = state.clone();
            let app_for_thread = app.clone();

            std::thread::spawn(move || {
                match capture::start_capture(prod) {
                    Ok(stream) => {
                        use cpal::traits::StreamTrait;
                        stream.play().ok();
                        while !stop_for_thread.load(Ordering::Relaxed) {
                            std::thread::sleep(std::time::Duration::from_millis(50));
                        }
                        drop(stream);
                    }
                    Err(e) => {
                        log::error!("Captura fallida: {e}");
                        state_for_thread.connection_status.store(STATUS_ERROR, Ordering::Relaxed);
                        let _ = app_for_thread.emit("status-changed", "error");
                        let _ = app_for_thread.emit("error-message", e.to_string());
                    }
                }
            });

            let target_ip = config.last_server_ip.clone();
            let state_for_task = state.clone();
            let app_for_task = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = sender::run_sender(target_ip, cons, cancel_rx).await {
                    log::error!("Sender fallido: {e}");
                    state_for_task.connection_status.store(STATUS_ERROR, Ordering::Relaxed);
                    let _ = app_for_task.emit("status-changed", "error");
                    let _ = app_for_task.emit("error-message", e.to_string());
                }
            });
        }

        Role::Receiver => {
            // Playback also runs on a dedicated thread
            let stop_for_thread = stop_flag.clone();
            let state_for_thread = state.clone();
            let app_for_thread = app.clone();
            let buffer_ms = config.buffer_size_ms;

            std::thread::spawn(move || {
                match playback::start_playback(cons, buffer_ms) {
                    Ok(stream) => {
                        use cpal::traits::StreamTrait;
                        stream.play().ok();
                        while !stop_for_thread.load(Ordering::Relaxed) {
                            std::thread::sleep(std::time::Duration::from_millis(50));
                        }
                        drop(stream);
                    }
                    Err(e) => {
                        log::error!("Playback fallido: {e}");
                        state_for_thread.connection_status.store(STATUS_ERROR, Ordering::Relaxed);
                        let _ = app_for_thread.emit("status-changed", "error");
                        let _ = app_for_thread.emit("error-message", e.to_string());
                    }
                }
            });

            let state_for_task = state.clone();
            let app_for_task = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = receiver::run_receiver(prod, cancel_rx).await {
                    log::error!("Receiver fallido: {e}");
                    state_for_task.connection_status.store(STATUS_ERROR, Ordering::Relaxed);
                    let _ = app_for_task.emit("status-changed", "error");
                    let _ = app_for_task.emit("error-message", e.to_string());
                }
            });
        }
    }

    *state.stream.lock() = Some(StreamHandle {
        stop_flag,
        cancel_tx: Some(cancel_tx),
    });

    state.connection_status.store(STATUS_CONNECTED, Ordering::Relaxed);
    let _ = app.emit("status-changed", "connected");

    // Persist the last successful connection
    save_config(&config).ok();

    Ok(())
}

pub async fn do_disconnect(state: Arc<AppState>, app: AppHandle) {
    let handle = state.stream.lock().take();
    if let Some(h) = handle {
        h.stop();
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    }
    // Restore audio output (in case we were muted as sender)
    if let Err(e) = crate::audio::volume::set_output_muted(false) {
        log::warn!("No se pudo restaurar el audio: {e}");
    }
    state.connection_status.store(STATUS_IDLE, Ordering::Relaxed);
    let _ = app.emit("status-changed", "idle");
}
