use std::io::{Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use agentland_core::cli::{pane_of, read_args, Hired, Wanted, HELP};
use agentland_core::service::{announced, Endpoint};
use futures_util::StreamExt;
use tokio_tungstenite::tungstenite::Message;

/// Letting go of a pane without stopping anything: ctrl-].
const LET_GO: u8 = 0x1d;

fn core() -> Option<Endpoint> {
    let data_dir = agentland_core::ServerConfig::data_dir_from_env();
    announced(&data_dir)
}

async fn crew(client: &reqwest::Client, core: &Endpoint) -> anyhow::Result<Vec<Hired>> {
    Ok(client
        .get(format!("{}/agents", core.url()))
        .header("x-auth-token", &core.token)
        .send()
        .await?
        .json()
        .await?)
}

async fn status(client: &reqwest::Client, core: &Endpoint) -> anyhow::Result<()> {
    println!("core   {} · pid {}", core.url(), core.pid);

    let held = crew(client, core).await?;
    if held.is_empty() {
        println!("crew   nobody hired yet");
        return Ok(());
    }

    for agent in held {
        println!(
            "{:<10} {:<12} {:<9} {}",
            agent.id,
            agent.role.unwrap_or_default(),
            agent.presence.unwrap_or_default(),
            agent.session_id.unwrap_or_else(|| "no pane".to_owned()),
        );
    }

    Ok(())
}

async fn say(client: &reqwest::Client, core: &Endpoint, who: &str, words: &str) -> anyhow::Result<()> {
    let held = crew(client, core).await?;
    let Some(pane) = pane_of(&held, who) else {
        anyhow::bail!("{who} has no pane open");
    };

    // The words and the carriage return go separately: a multi-line message
    // arrives at an engine as a paste, which swallows a return tacked onto it.
    write_input(client, core, &pane, words).await?;
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    write_input(client, core, &pane, "\r").await?;

    println!("told {who}");
    Ok(())
}

async fn write_input(
    client: &reqwest::Client,
    core: &Endpoint,
    pane: &str,
    data: &str,
) -> anyhow::Result<()> {
    client
        .post(format!("{}/sessions/{pane}/input", core.url()))
        .header("x-auth-token", &core.token)
        .json(&serde_json::json!({ "data": data }))
        .send()
        .await?;

    Ok(())
}

/// Put a pane on this terminal: its output here, this keyboard into it.
///
/// The core is serving that pane whether or not anybody is attached, so letting
/// go stops nothing — which is the whole point of the core being a service.
async fn attach(client: &reqwest::Client, core: &Endpoint, who: &str) -> anyhow::Result<()> {
    let held = crew(client, core).await?;
    let Some(pane) = pane_of(&held, who) else {
        anyhow::bail!("{who} has no pane open");
    };

    let (socket, _) = tokio_tungstenite::connect_async(format!(
        "ws://{}:{}/sessions/{pane}/stream?token={}",
        core.host, core.port, core.token
    ))
    .await?;

    if let Ok((cols, rows)) = crossterm::terminal::size() {
        let _ = client
            .post(format!("{}/sessions/{pane}/resize", core.url()))
            .header("x-auth-token", &core.token)
            .json(&serde_json::json!({ "cols": cols, "rows": rows }))
            .send()
            .await;
    }

    crossterm::terminal::enable_raw_mode()?;
    let letting_go = Arc::new(AtomicBool::new(false));

    // The keyboard is read on a thread of its own: stdin has no async form that
    // is worth the trouble, and a blocked read here would stop the output.
    let keys = {
        let letting_go = letting_go.clone();
        let client = client.clone();
        let core = core.clone();
        let pane = pane.clone();

        std::thread::spawn(move || {
            let mut stdin = std::io::stdin();
            let mut byte = [0_u8; 1];
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("a runtime for the keyboard");

            while !letting_go.load(Ordering::Relaxed) {
                match stdin.read(&mut byte) {
                    Ok(1) => {
                        if byte[0] == LET_GO {
                            letting_go.store(true, Ordering::Relaxed);
                            break;
                        }

                        let typed = String::from_utf8_lossy(&byte).to_string();
                        let _ = runtime.block_on(write_input(&client, &core, &pane, &typed));
                    }
                    Ok(_) => break,
                    Err(_) => break,
                }
            }
        })
    };

    let mut output = socket;
    let mut out = std::io::stdout();

    while !letting_go.load(Ordering::Relaxed) {
        let frame = tokio::time::timeout(std::time::Duration::from_millis(200), output.next()).await;

        match frame {
            Ok(Some(Ok(Message::Binary(bytes)))) => {
                out.write_all(&bytes)?;
                out.flush()?;
            }
            Ok(Some(Ok(Message::Text(_)))) => {}
            Ok(Some(Ok(_))) => {}
            Ok(Some(Err(_))) | Ok(None) => break,
            Err(_) => {}
        }
    }

    letting_go.store(true, Ordering::Relaxed);
    crossterm::terminal::disable_raw_mode()?;
    println!("\r\nlet go of {who}. it is still running.");
    let _ = keys.join();

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let wanted = read_args(&args);

    if wanted == Wanted::Help {
        print!("{HELP}");
        return Ok(());
    }

    let Some(core) = core() else {
        eprintln!("no core is running here — open Agentland, or run agentland-core");
        std::process::exit(1);
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    match wanted {
        Wanted::Status => status(&client, &core).await,
        Wanted::Attach(who) => attach(&client, &core, &who).await,
        Wanted::Say { who, words } => say(&client, &core, &who, &words).await,
        Wanted::Help => Ok(()),
    }
}
