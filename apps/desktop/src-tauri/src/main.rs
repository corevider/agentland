#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use agentland_core::{generate_token, serve, PtyManager, ServerConfig};
use serde::Serialize;
use tauri::Manager;
use tauri_plugin_window_state::{AppHandleExt, StateFlags};

const DEFAULT_PORT: u16 = 9470;

#[derive(Clone, Serialize)]
struct CoreEndpoint {
    host: String,
    port: u16,
    token: String,
}

#[tauri::command]
fn core_endpoint(state: tauri::State<'_, CoreEndpoint>) -> CoreEndpoint {
    state.inner().clone()
}

fn updater_endpoints() -> Vec<String> {
    std::env::var("AGENTLAND_UPDATER_ENDPOINTS")
        .unwrap_or_default()
        .split(',')
        .map(|entry| entry.trim().to_owned())
        .filter(|entry| !entry.is_empty())
        .collect()
}

#[tauri::command]
fn save_capture(name: String, data: String) -> Result<String, String> {
    use base64::Engine;

    let payload = data
        .split_once(",")
        .map(|(_, tail)| tail)
        .unwrap_or(&data);

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|error| format!("the capture was not valid base64: {error}"))?;

    let safe: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    let file = if safe.is_empty() {
        "capture".to_owned()
    } else {
        safe
    };

    let directory = std::env::var("AGENTLAND_CAPTURE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;

    let path = directory.join(format!("{file}.png"));
    std::fs::write(&path, bytes).map_err(|error| error.to_string())?;

    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
fn updater_status() -> String {
    let endpoints = updater_endpoints();
    if endpoints.is_empty() {
        "Updates are off: no endpoint configured.".to_owned()
    } else {
        format!("Updates come from {} signed endpoint(s).", endpoints.len())
    }
}

fn allowed_hosts(host: &str, port: u16) -> Vec<String> {
    if let Ok(configured) = std::env::var("AGENTLAND_ALLOWED_HOSTS") {
        let listed: Vec<String> = configured
            .split(',')
            .map(|entry| entry.trim().to_owned())
            .filter(|entry| !entry.is_empty())
            .collect();

        if !listed.is_empty() {
            return listed;
        }
    }

    let mut hosts = vec![format!("127.0.0.1:{port}"), format!("localhost:{port}")];
    if host != "127.0.0.1" && host != "localhost" {
        hosts.push(format!("{host}:{port}"));
    }

    hosts
}

fn desktop_data_dir() -> std::path::PathBuf {
    if let Ok(configured) = std::env::var("AGENTLAND_DATA_DIR") {
        return std::path::PathBuf::from(configured);
    }

    if cfg!(debug_assertions) {
        return std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data");
    }

    dirs_next_data_dir()
        .map(|base| base.join("agentland"))
        .unwrap_or_else(|| std::path::PathBuf::from("data"))
}

fn dirs_next_data_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME")
                .map(std::path::PathBuf::from)
                .map(|home| home.join(".local/share"))
        })
}

#[tauri::command]
async fn open_pane_window(
    app: tauri::AppHandle,
    session_id: String,
    title: String,
) -> Result<(), String> {
    let label = format!("pane-{}", session_id.replace(|c: char| !c.is_alphanumeric(), "-"));

    if let Some(existing) = app.get_webview_window(&label) {
        let _ = existing.set_focus();
        return Ok(());
    }

    let url = format!("index.html?pane={}", urlencoding(&session_id));

    tauri::WebviewWindowBuilder::new(&app, &label, tauri::WebviewUrl::App(url.into()))
        .title(title)
        .inner_size(900.0, 620.0)
        .resizable(true)
        .build()
        .map_err(|error| error.to_string())?;

    Ok(())
}

#[tauri::command]
async fn close_pane_window(app: tauri::AppHandle, session_id: String) -> Result<(), String> {
    let label = format!("pane-{}", session_id.replace(|c: char| !c.is_alphanumeric(), "-"));

    if let Some(window) = app.get_webview_window(&label) {
        window.close().map_err(|error| error.to_string())?;
    }

    Ok(())
}

/// Enough encoding for a session id, which is alphanumeric with dashes.
fn urlencoding(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || character == '-' || character == '_' {
                character.to_string()
            } else {
                format!("%{:02X}", character as u32)
            }
        })
        .collect()
}

fn main() {
    tracing_subscriber::fmt().with_target(false).init();

    let port = std::env::var("AGENTLAND_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_PORT);

    let host = std::env::var("AGENTLAND_HOST").unwrap_or_else(|_| "127.0.0.1".to_owned());

    let endpoint = CoreEndpoint {
        host: host.clone(),
        port,
        token: std::env::var("AGENTLAND_TOKEN").unwrap_or_else(|_| generate_token()),
    };

    let config = ServerConfig {
        host: endpoint.host.clone(),
        port,
        token: endpoint.token.clone(),
        allowed_hosts: allowed_hosts(&host, port),
        allowed_origins: vec![
            "http://localhost:5273".into(),
            "http://127.0.0.1:5273".into(),
            "tauri://localhost".into(),
            "http://tauri.localhost".into(),
            "https://tauri.localhost".into(),
        ],
        data_dir: desktop_data_dir(),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        // The window opens where it was left, at the size it was left. The
        // panel layout already survived a restart; the window around it did
        // not, so every start began by dragging it back.
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .manage(endpoint)
        .invoke_handler(tauri::generate_handler![core_endpoint, updater_status, save_capture, open_pane_window, close_pane_window])
        .setup(move |app| {
            let manager = Arc::new(PtyManager::new());
            app.manage(manager.clone());

            tauri::async_runtime::spawn(async move {
                if let Err(error) = serve(manager, config).await {
                    tracing::error!(%error, "core server stopped");
                }
            });

            // The plugin writes the window's size and place when the app exits
            // cleanly, which is not how it always ends: a rebuild, a crash or a
            // kill leaves nothing, and the next start is back to the default
            // 1480x920. So a move or a resize marks it, and the mark is written
            // out a few seconds later — a small file, written only after
            // something changed.
            let handle = app.handle().clone();
            let moved_or_resized = Arc::new(AtomicBool::new(false));
            let watching = moved_or_resized.clone();

            if let Some(window) = app.get_webview_window("main") {
                window.on_window_event(move |event| {
                    if matches!(
                        event,
                        tauri::WindowEvent::Resized(_) | tauri::WindowEvent::Moved(_)
                    ) {
                        watching.store(true, Ordering::Relaxed);
                    }
                });
            }

            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

                    if moved_or_resized.swap(false, Ordering::Relaxed) {
                        if let Err(error) = handle.save_window_state(StateFlags::all()) {
                            tracing::warn!(%error, "cannot remember where the window is");
                        }
                    }
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to start Agentland");
}
