use std::sync::{
    atomic::{AtomicU8, Ordering},
    Arc,
};
use parking_lot::Mutex;
use crate::config::AppConfig;

// Connection status values
pub const STATUS_IDLE: u8 = 0;
pub const STATUS_CONNECTING: u8 = 1;
pub const STATUS_CONNECTED: u8 = 2;
pub const STATUS_ERROR: u8 = 3;

pub struct StreamHandle {
    pub stop_flag: Arc<std::sync::atomic::AtomicBool>,
    pub cancel_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl StreamHandle {
    pub fn stop(mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(tx) = self.cancel_tx.take() {
            let _ = tx.send(());
        }
    }
}

pub struct AppState {
    pub config: Mutex<AppConfig>,
    pub stream: Mutex<Option<StreamHandle>>,
    pub connection_status: AtomicU8,
}

impl AppState {
    pub fn new(config: AppConfig) -> Self {
        Self {
            config: Mutex::new(config),
            stream: Mutex::new(None),
            connection_status: AtomicU8::new(STATUS_IDLE),
        }
    }

    pub fn disconnect(&self) {
        if let Some(handle) = self.stream.lock().take() {
            handle.stop();
        }
        self.connection_status.store(STATUS_IDLE, Ordering::Relaxed);
    }
}
