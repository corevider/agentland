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

/// Bring the window back from wherever closing it put it.
fn show_the_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// The crew in one line, for the tray: how many, and what they are up to.
fn crew_line(presences: &[String]) -> String {
    if presences.is_empty() {
        return "Nobody in the crew".to_owned();
    }

    let mut said = vec![format!("{} in the crew", presences.len())];

    for (word, read) in [("working", "working"), ("waiting", "waiting"), ("attention", "need you")] {
        let count = presences.iter().filter(|held| held.as_str() == word).count();
        if count > 0 {
            said.push(format!("{count} {read}"));
        }
    }

    said.join(" · ")
}

fn panes_line(open: usize) -> String {
    match open {
        0 => "No panes open".to_owned(),
        1 => "1 pane open".to_owned(),
        many => format!("{many} panes open"),
    }
}

/// One question to the core, answered as JSON or not at all.
fn ask_the_core(client: &reqwest::blocking::Client, endpoint: &CoreEndpoint, path: &str) -> Option<serde_json::Value> {
    client
        .get(format!("http://{}:{}{path}", endpoint.host, endpoint.port))
        .header("x-auth-token", &endpoint.token)
        .send()
        .ok()
        .filter(|answer| answer.status().is_success())
        .and_then(|answer| answer.json().ok())
}

/// Tell the core to stop the crew and go. Whether or not it answers, the
/// window is leaving: a core that is not there is already stopped.
fn stop_the_crew(endpoint: &CoreEndpoint) {
    let Ok(client) = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
    else {
        return;
    };

    let _ = client
        .post(format!("http://{}:{}/stop", endpoint.host, endpoint.port))
        .header("x-auth-token", &endpoint.token)
        .send();
}

/// An icon in the tray, so closing the window puts it away rather than ending it.
///
/// The crew goes on working in the core whether the window is there or not,
/// and what a person wants from the close button is the window out of the way
/// with somewhere to get it back from. The menu says what the crew is doing,
/// and offers two ways out, named for what they leave behind.
fn put_an_icon_in_the_tray(app: &tauri::App, endpoint: CoreEndpoint) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let crew = MenuItem::with_id(app, "crew", "Asking the core…", false, None::<&str>)?;
    let panes = MenuItem::with_id(app, "panes", "", false, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Open Agentland", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit — the crew keeps working", true, None::<&str>)?;
    let stop = MenuItem::with_id(app, "stop", "Stop the crew and quit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &crew,
            &panes,
            &PredefinedMenuItem::separator(app)?,
            &open,
            &PredefinedMenuItem::separator(app)?,
            &quit,
            &stop,
        ],
    )?;

    let for_stopping = endpoint.clone();
    let mut tray = TrayIconBuilder::with_id("agentland")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("Agentland")
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "open" => show_the_window(app),
            "quit" => app.exit(0),
            "stop" => {
                let handle = app.clone();
                let endpoint = for_stopping.clone();
                std::thread::spawn(move || {
                    stop_the_crew(&endpoint);
                    handle.exit(0);
                });
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_the_window(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }

    let tray = tray.build(app)?;

    // What the crew is doing, read off the core every few seconds and written
    // on the menu, so the icon is worth hovering over when the window is away.
    std::thread::spawn(move || {
        let Ok(client) = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_millis(1500))
            .build()
        else {
            return;
        };

        loop {
            let agents = ask_the_core(&client, &endpoint, "/agents");
            let sessions = ask_the_core(&client, &endpoint, "/sessions");

            let (crew_said, panes_said) = match (agents, sessions) {
                (Some(agents), sessions) => {
                    let presences: Vec<String> = agents
                        .as_array()
                        .map(|held| {
                            held.iter()
                                .filter_map(|agent| agent["presence"].as_str().map(str::to_owned))
                                .collect()
                        })
                        .unwrap_or_default();
                    let open = sessions.and_then(|held| held.as_array().map(Vec::len)).unwrap_or(0);
                    (crew_line(&presences), panes_line(open))
                }
                (None, _) => ("The core is not answering".to_owned(), String::new()),
            };

            let _ = crew.set_text(&crew_said);
            let _ = panes.set_text(&panes_said);
            let _ = tray.set_tooltip(Some(format!("Agentland — {crew_said}")));

            std::thread::sleep(std::time::Duration::from_secs(4));
        }
    });

    Ok(())
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

        // Serving beyond this machine means a phone will ask for it by the
        // address it can see, which is never the one it was told to bind.
        hosts.extend(agentland_core::service::on_this_network(port));

        // The phone's door is one port along, and a browser sends the port it
        // asked for in the Host header.
        hosts.extend(agentland_core::service::on_this_network(port + 1));
        hosts.push(format!("{host}:{}", port + 1));
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

/// Whether a core is listening there and will talk to us.
///
/// A file saying where the core is outlives the process that wrote it, so the
/// only honest test is to knock.
fn answers(endpoint: &agentland_core::service::Endpoint) -> bool {
    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_millis(1500))
        .build()
    {
        Ok(client) => client,
        Err(_) => return false,
    };

    client
        .get(format!("{}/repos", endpoint.url()))
        .header("x-auth-token", &endpoint.token)
        .send()
        .map(|answer| answer.status().is_success())
        .unwrap_or(false)
}

/// The core binary that runs on its own, looked for beside this one first.
///
/// Beside it in a bundle, beside it in a dev build, and on PATH for anybody who
/// has installed it — in that order, because the one shipped with this window
/// is the one that matches it.
fn core_binary() -> Option<std::path::PathBuf> {
    let named = if cfg!(windows) { "agentland-core.exe" } else { "agentland-core" };

    if let Ok(here) = std::env::current_exe() {
        if let Some(beside) = here.parent().map(|dir| dir.join(named)) {
            if beside.is_file() {
                return Some(beside);
            }
        }
    }

    which_on_path(named)
}

fn which_on_path(named: &str) -> Option<std::path::PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(named))
            .find(|held| held.is_file())
    })
}

/// Start the core as its own process, outliving this window.
///
/// Put in its own process group on purpose: closing the window, or the window
/// crashing, must not take the agents with it. That is the whole point.
fn start_the_core(
    binary: &std::path::Path,
    host: &str,
    port: u16,
    token: &str,
    data_dir: &std::path::Path,
) -> std::io::Result<u32> {
    let mut command = std::process::Command::new(binary);
    command
        .env("AGENTLAND_HOST", host)
        .env("AGENTLAND_PORT", port.to_string())
        .env("AGENTLAND_TOKEN", token)
        .env("AGENTLAND_DATA_DIR", data_dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    command.spawn().map(|child| child.id())
}

/// Wait for a core that has just been started to answer.
fn waits_for(endpoint: &agentland_core::service::Endpoint, patience: std::time::Duration) -> bool {
    let until = std::time::Instant::now() + patience;

    while std::time::Instant::now() < until {
        if answers(endpoint) {
            return true;
        }

        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    false
}

fn main() {
    tracing_subscriber::fmt().with_target(false).init();

    let port = std::env::var("AGENTLAND_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_PORT);

    let host = std::env::var("AGENTLAND_HOST").unwrap_or_else(|_| "127.0.0.1".to_owned());

    let data_dir = desktop_data_dir();

    // The core is a service, and this window is one of the things that talks to
    // it. Attach to one that is already running; failing that, start one that
    // will outlive this window; failing that — no binary to start — serve it
    // here, as it used to be, so a window without its service still works.
    let running = agentland_core::service::announced(&data_dir).filter(answers);

    let (endpoint, serve_here) = match running {
        Some(held) => {
            tracing::info!(port = held.port, pid = held.pid, "attached to the core already running");
            (
                CoreEndpoint {
                    host: held.host,
                    port: held.port,
                    token: held.token,
                },
                false,
            )
        }
        None => {
            let token = std::env::var("AGENTLAND_TOKEN").unwrap_or_else(|_| generate_token());
            let wanted = agentland_core::service::Endpoint {
                host: host.clone(),
                port,
                token: token.clone(),
                pid: 0,
            };

            let started = core_binary().and_then(|binary| {
                start_the_core(&binary, &host, port, &token, &data_dir)
                    .map_err(|error| tracing::warn!(%error, "cannot start the core on its own"))
                    .ok()
            });

            match started {
                Some(pid) if waits_for(&wanted, std::time::Duration::from_secs(20)) => {
                    tracing::info!(pid, port, "started the core as its own process");
                    (
                        CoreEndpoint {
                            host: host.clone(),
                            port,
                            token,
                        },
                        false,
                    )
                }
                _ => {
                    tracing::warn!("serving the core in this window: the agents stop when it does");
                    (
                        CoreEndpoint {
                            host: host.clone(),
                            port,
                            token,
                        },
                        true,
                    )
                }
            }
        }
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
        data_dir: data_dir.clone(),
    };

    // What the core was told to bind is not what the window dials: a core
    // serving everybody sits on 0.0.0.0, and a fetch to that address fails
    // with nothing more helpful than "Load failed". Whichever way the core
    // came to be, the window reaches it at the loopback address.
    let dialled = CoreEndpoint {
        host: agentland_core::service::connectable(&endpoint.host).to_owned(),
        port: endpoint.port,
        token: endpoint.token.clone(),
    };
    let for_the_tray = dialled.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        // The window opens where it was left, at the size it was left. The
        // panel layout already survived a restart; the window around it did
        // not, so every start began by dragging it back.
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .manage(dialled)
        .invoke_handler(tauri::generate_handler![core_endpoint, updater_status, save_capture, open_pane_window, close_pane_window])
        .setup(move |app| {
            let manager = Arc::new(PtyManager::new());
            app.manage(manager.clone());

            if serve_here {
                tauri::async_runtime::spawn(async move {
                    if let Err(error) = serve(manager, config).await {
                        tracing::error!(%error, "core server stopped");
                    }
                });
            }

            // The plugin writes the window's size and place when the app exits
            // cleanly, which is not how it always ends: a rebuild, a crash or a
            // kill leaves nothing, and the next start is back to the default
            // 1480x920. So a move or a resize marks it, and the mark is written
            // out a few seconds later — a small file, written only after
            // something changed.
            let handle = app.handle().clone();
            let moved_or_resized = Arc::new(AtomicBool::new(false));
            let watching = moved_or_resized.clone();

            put_an_icon_in_the_tray(app, for_the_tray)?;

            if let Some(window) = app.get_webview_window("main") {
                let hidden = window.clone();
                window.on_window_event(move |event| {
                    if matches!(
                        event,
                        tauri::WindowEvent::Resized(_) | tauri::WindowEvent::Moved(_)
                    ) {
                        watching.store(true, Ordering::Relaxed);
                    }

                    // The close button puts the window away; the tray icon
                    // brings it back, and Quit there is how it ends.
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = hidden.hide();
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
        .build(tauri::generate_context!())
        .expect("failed to start Agentland")
        .run(|app, event| {
            // A dock icon clicked with every window hidden, on a Mac.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                show_the_window(app);
            }

            #[cfg(not(target_os = "macos"))]
            let _ = (app, event);
        });
}

#[cfg(test)]
mod tray_tests {
    use super::{crew_line, panes_line};

    fn words(held: &[&str]) -> Vec<String> {
        held.iter().map(|word| (*word).to_owned()).collect()
    }

    #[test]
    fn an_empty_crew_says_so() {
        assert_eq!(crew_line(&[]), "Nobody in the crew");
    }

    #[test]
    fn the_crew_is_counted_and_what_matters_is_named() {
        let line = crew_line(&words(&["working", "working", "attention", "done"]));
        assert_eq!(line, "4 in the crew · 2 working · 1 need you");
    }

    #[test]
    fn a_state_nobody_is_in_is_not_mentioned() {
        assert_eq!(crew_line(&words(&["waiting"])), "1 in the crew · 1 waiting");
    }

    #[test]
    fn panes_are_counted_in_english() {
        assert_eq!(panes_line(0), "No panes open");
        assert_eq!(panes_line(1), "1 pane open");
        assert_eq!(panes_line(3), "3 panes open");
    }
}
