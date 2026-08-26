use std::sync::Arc;

use agentland_core::{generate_token, serve, PtyManager, ServerConfig};

fn split_env(name: &str, fallback: Vec<String>) -> Vec<String> {
    match std::env::var(name) {
        Ok(value) if !value.trim().is_empty() => value
            .split(',')
            .map(|entry| entry.trim().to_owned())
            .filter(|entry| !entry.is_empty())
            .collect(),
        _ => fallback,
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_target(false).init();

    let port: u16 = std::env::var("AGENTLAND_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(9470);
    let token = std::env::var("AGENTLAND_TOKEN").unwrap_or_else(|_| generate_token());
    let host = std::env::var("AGENTLAND_HOST").unwrap_or_else(|_| "127.0.0.1".to_owned());

    let allowed_hosts = split_env(
        "AGENTLAND_ALLOWED_HOSTS",
        vec![format!("127.0.0.1:{port}"), format!("localhost:{port}")],
    );

    let allowed_origins = split_env(
        "AGENTLAND_ALLOWED_ORIGINS",
        vec![
            "http://localhost:5273".into(),
            "http://127.0.0.1:5273".into(),
            format!("http://127.0.0.1:{port}"),
            format!("http://localhost:{port}"),
            "tauri://localhost".into(),
            "http://tauri.localhost".into(),
            "https://tauri.localhost".into(),
        ],
    );

    println!("core:    http://127.0.0.1:{port}");
    println!("token:   {token}");
    println!("browser: http://localhost:5273/?port={port}&token={token}");

    let manager = Arc::new(PtyManager::new());
    serve(
        manager,
        ServerConfig {
            host,
            port,
            token,
            allowed_hosts,
            allowed_origins,
            data_dir: ServerConfig::data_dir_from_env(),
        },
    )
    .await
}
