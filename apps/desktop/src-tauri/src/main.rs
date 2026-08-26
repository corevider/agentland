#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;

use agentland_core::{generate_token, serve, PtyManager, ServerConfig};
use serde::Serialize;
use tauri::Manager;

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

fn main() {
    tracing_subscriber::fmt().with_target(false).init();

    let port = std::env::var("AGENTLAND_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_PORT);

    let endpoint = CoreEndpoint {
        host: "127.0.0.1".into(),
        port,
        token: std::env::var("AGENTLAND_TOKEN").unwrap_or_else(|_| generate_token()),
    };

    let config = ServerConfig {
        host: endpoint.host.clone(),
        port,
        token: endpoint.token.clone(),
        allowed_hosts: vec![format!("127.0.0.1:{port}"), format!("localhost:{port}")],
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
        .manage(endpoint)
        .invoke_handler(tauri::generate_handler![core_endpoint, updater_status, save_capture])
        .setup(move |app| {
            let manager = Arc::new(PtyManager::new());
            app.manage(manager.clone());

            tauri::async_runtime::spawn(async move {
                if let Err(error) = serve(manager, config).await {
                    tracing::error!(%error, "core server stopped");
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to start Agentland");
}
