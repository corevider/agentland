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
fn updater_status() -> String {
    let endpoints = updater_endpoints();
    if endpoints.is_empty() {
        "Updates are off: no endpoint configured.".to_owned()
    } else {
        format!("Updates come from {} signed endpoint(s).", endpoints.len())
    }
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
        token: generate_token(),
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
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(endpoint)
        .invoke_handler(tauri::generate_handler![core_endpoint, updater_status])
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
