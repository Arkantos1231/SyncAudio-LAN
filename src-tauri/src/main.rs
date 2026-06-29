// Prevents console window from appearing on Windows in release builds
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod commands;
mod config;
mod network;
mod state;

use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager,
};
use commands::{do_connect, do_disconnect};
use config::load_config;
use state::AppState;

fn main() {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    let cfg = load_config();
    let auto_connect = cfg.auto_connect;
    let has_saved_ip = !cfg.last_server_ip.is_empty();

    let app_state = Arc::new(AppState::new(cfg));

    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![]),
        ))
        .manage(app_state.clone())
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::set_config,
            commands::get_status,
            commands::get_local_ip,
            commands::set_autostart,
            commands::connect,
            commands::disconnect,
        ])
        .setup(move |app| {
            // ── System Tray ──────────────────────────────────────────────────
            let show_item = MenuItem::with_id(app, "show", "Mostrar Interfaz", true, None::<&str>)?;
            let disconnect_item = MenuItem::with_id(app, "disconnect", "Desconectar", true, None::<&str>)?;
            let sep = PredefinedMenuItem::separator(app)?;
            let quit_item = MenuItem::with_id(app, "quit", "Salir", true, None::<&str>)?;

            let tray_menu = Menu::with_items(app, &[&show_item, &disconnect_item, &sep, &quit_item])?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&tray_menu)
                .tooltip("SyncAudio LAN")
                .on_menu_event({
                    let state = app_state.clone();
                    move |app, event| {
                        match event.id.as_ref() {
                            "show" => {
                                if let Some(win) = app.get_webview_window("main") {
                                    win.show().ok();
                                    win.set_focus().ok();
                                }
                            }
                            "disconnect" => {
                                let state = state.clone();
                                let app = app.clone();
                                tauri::async_runtime::spawn(async move {
                                    do_disconnect(state, app).await;
                                });
                            }
                            "quit" => {
                                app.exit(0);
                            }
                            _ => {}
                        }
                    }
                })
                .build(app)?;

            // ── Silent start + auto-connect ───────────────────────────────────
            let window = app.get_webview_window("main").unwrap();
            if auto_connect && has_saved_ip {
                window.hide().ok();
                let state = app_state.clone();
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    auto_connect_loop(state, app_handle).await;
                });
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            // Intercept the window close button — hide instead of destroy
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                window.hide().ok();
            }
        })
        .run(tauri::generate_context!())
        .expect("Error al ejecutar SyncAudio LAN");
}

/// Starts a background reconnect loop on startup when auto_connect is enabled.
/// Waits 3 seconds for network init, then retries every 5 seconds until connected.
async fn auto_connect_loop(state: Arc<AppState>, app: tauri::AppHandle) {
    use std::sync::atomic::Ordering;
    use state::{STATUS_IDLE, STATUS_CONNECTED};

    log::info!("Auto-connect iniciado, esperando 3s para inicialización de red...");
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    loop {
        // Respect manual disconnect
        if state.connection_status.load(Ordering::Relaxed) != STATUS_IDLE {
            break;
        }

        log::info!("Auto-connect: intentando conexión...");
        let _ = app.emit("status-changed", "connecting");

        match do_connect(state.clone(), app.clone()).await {
            Ok(_) => {
                log::info!("Auto-connect: conectado exitosamente.");
                break;
            }
            Err(e) => {
                log::warn!("Auto-connect fallido: {e}. Reintentando en 5s...");
                state.connection_status.store(STATUS_IDLE, Ordering::Relaxed);
                let _ = app.emit("status-changed", "idle");

                // Wait 5s while checking for external cancellation every 100ms
                for _ in 0..50 {
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    let status = state.connection_status.load(Ordering::Relaxed);
                    if status == STATUS_CONNECTED {
                        // User connected manually in the meantime
                        return;
                    }
                }
            }
        }
    }
}
