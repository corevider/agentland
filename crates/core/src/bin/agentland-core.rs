use std::sync::Arc;

use agentland_core::{generate_token, serve, PtyManager, ServerConfig};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_target(false).init();

    let port: u16 = std::env::var("AGENTLAND_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(9470);
    let token = std::env::var("AGENTLAND_TOKEN").unwrap_or_else(|_| generate_token());

    println!("core:    http://127.0.0.1:{port}");
    println!("token:   {token}");
    println!("browser: http://localhost:5273/?port={port}&token={token}");

    let manager = Arc::new(PtyManager::new());
    serve(
        manager,
        ServerConfig {
            host: "127.0.0.1".into(),
            port,
            token,
            allowed_hosts: vec![format!("127.0.0.1:{port}"), format!("localhost:{port}")],
            allowed_origins: vec![
                "http://localhost:5273".into(),
                "http://127.0.0.1:5273".into(),
                "tauri://localhost".into(),
                "http://tauri.localhost".into(),
            "https://tauri.localhost".into(),
            ],
        },
    )
    .await
}
