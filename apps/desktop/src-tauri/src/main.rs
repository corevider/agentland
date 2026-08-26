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
    };

    tauri::Builder::default()
        .manage(endpoint)
        .invoke_handler(tauri::generate_handler![core_endpoint])
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
