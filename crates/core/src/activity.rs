//! Native lifecycle facts belong to one launch, never to an agent name that
//! can be reused. Missing/unsupported hooks fall back to pane observations.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
};

pub const SUBCOMMAND: &str = "agent-activity";
pub const FILE_ENV: &str = "AGENTLAND_ACTIVITY_FILE";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Active,
    Idle,
    Blocked,
    WaitingInput,
    Exited,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Reading {
    pub state: State,
    pub at: u64,
}

pub fn read(path: &Path) -> Option<Reading> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

pub fn event(name: &str, payload: &Value) -> Option<State> {
    match name {
        "SessionStart" => Some(State::Unknown),
        "UserPromptSubmit" | "BeforeAgent" | "PreToolUse" => Some(State::Active),
        "Stop" | "AfterAgent" => Some(State::Idle),
        "PermissionRequest" => Some(State::Blocked),
        "SessionEnd" => Some(State::Exited),
        "Notification" => match payload.get("notification_type").and_then(Value::as_str) {
            Some("permission_prompt" | "ToolPermission") => Some(State::Blocked),
            Some("idle_prompt") => Some(State::WaitingInput),
            _ => None,
        },
        _ => None,
    }
}

pub fn run(name: &str) {
    let mut raw = String::new();
    let _ = std::io::stdin().take(64 * 1024).read_to_string(&mut raw);
    let payload = serde_json::from_str(&raw).unwrap_or(Value::Null);
    if let (Some(state), Some(path)) = (event(name, &payload), std::env::var_os(FILE_ENV)) {
        let path = PathBuf::from(path);
        let reading = Reading {
            state,
            at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
        };
        let part = path.with_extension(format!("{}.part", std::process::id()));
        if let Ok(raw) = serde_json::to_vec(&reading) {
            if std::fs::write(&part, raw).is_ok() {
                let _ = std::fs::rename(&part, &path);
            }
        }
    }
    // No prompt text, arguments, credentials or permission decisions are kept.
    println!("{{}}");
}

fn quote(binary: &str, powershell: bool) -> String {
    if powershell {
        format!("& '{}'", binary.replace('\'', "''"))
    } else if cfg!(windows) {
        format!("\"{}\"", binary.replace('"', ""))
    } else {
        format!("'{}'", binary.replace('\'', "'\\''"))
    }
}

fn add_hooks(settings: &mut Value, names: &[&str], binary: &str, engine: &str) {
    if settings.get("hooks").and_then(Value::as_object).is_none() {
        settings["hooks"] = json!({});
    }
    for name in names {
        let command = format!(
            "{} {SUBCOMMAND} {name}",
            quote(binary, cfg!(windows) && engine == "codex")
        );
        let group = json!({ "matcher": "", "hooks": [{ "type": "command", "command": command, "timeout": if engine == "gemini" { 5000 } else { 5 } }] });
        let hooks = &mut settings["hooks"][name];
        if !hooks.is_array() {
            *hooks = json!([]);
        }
        hooks.as_array_mut().unwrap().push(group);
    }
}

pub fn may_deliver(state: Option<State>, pane: &str) -> bool {
    if matches!(state, Some(State::Active | State::Blocked | State::Exited)) {
        return false;
    }
    // Hooks do not describe text the human is currently composing. The pane
    // retains a veto even when a native Stop said the model finished.
    crate::supervisor::safe_to_type(pane, pane) && !crate::supervisor::asking_the_human(pane)
}

pub fn install(
    engine: &str,
    data_dir: &Path,
    args: &mut Vec<String>,
    env: &mut BTreeMap<String, String>,
) -> anyhow::Result<()> {
    if !matches!(engine, "claude" | "gemini") {
        return Ok(());
    }
    let binary = std::env::current_exe()?
        .to_string_lossy()
        .trim_end_matches(" (deleted)")
        .to_owned();
    let root = data_dir.join("activity");
    std::fs::create_dir_all(&root)?;
    let launch = crate::generate_token();
    let file = root.join(format!("{launch}.json"));
    env.insert(FILE_ENV.into(), file.to_string_lossy().into_owned());
    let names: &[&str] = match engine {
        "gemini" => &[
            "SessionStart",
            "BeforeAgent",
            "AfterAgent",
            "Notification",
            "SessionEnd",
        ],
        _ => &[
            "SessionStart",
            "UserPromptSubmit",
            "PreToolUse",
            "PermissionRequest",
            "Stop",
            "Notification",
            "SessionEnd",
        ],
    };
    if engine == "claude" {
        if let Some(index) = args.iter().position(|arg| arg == "--settings") {
            let path = PathBuf::from(&args[index + 1]);
            let mut settings: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
            add_hooks(&mut settings, names, &binary, engine);
            // Each launch gets its own settings snapshot, avoiding concurrent
            // agents rewriting a role's shared settings while it is read.
            let path = root.join(format!("{launch}-settings.json"));
            std::fs::write(&path, serde_json::to_vec(&settings)?)?;
            args[index + 1] = path.to_string_lossy().into_owned();
        }
    } else {
        let system = std::env::var_os("GEMINI_CLI_SYSTEM_SETTINGS_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                if cfg!(target_os = "macos") {
                    "/Library/Application Support/GeminiCli/settings.json".into()
                } else if cfg!(windows) {
                    "C:\\ProgramData\\gemini-cli\\settings.json".into()
                } else {
                    "/etc/gemini-cli/settings.json".into()
                }
            });
        let mut settings = if system.exists() {
            serde_json::from_slice(&std::fs::read(&system)?)?
        } else {
            json!({})
        };
        add_hooks(&mut settings, names, &binary, engine);
        let path = root.join(format!("{launch}-settings.json"));
        std::fs::write(&path, serde_json::to_vec(&settings)?)?;
        // Preserve the native defaults location when overriding system settings.
        if std::env::var_os("GEMINI_CLI_SYSTEM_DEFAULTS_PATH").is_none() {
            if let Some(parent) = system.parent() {
                env.insert(
                    "GEMINI_CLI_SYSTEM_DEFAULTS_PATH".into(),
                    parent
                        .join("system-defaults.json")
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
        env.insert(
            "GEMINI_CLI_SYSTEM_SETTINGS_PATH".into(),
            path.to_string_lossy().into_owned(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_native_adapters_report_the_same_states() {
        for event_name in ["UserPromptSubmit", "BeforeAgent"] {
            assert_eq!(event(event_name, &Value::Null), Some(State::Active));
        }
        for event_name in ["Stop", "AfterAgent"] {
            assert_eq!(event(event_name, &Value::Null), Some(State::Idle));
        }
        assert_eq!(
            event("PermissionRequest", &Value::Null),
            Some(State::Blocked)
        );
        assert_eq!(
            event(
                "Notification",
                &json!({"notification_type":"permission_prompt"})
            ),
            Some(State::Blocked)
        );
        assert_eq!(event("Notification", &json!({"message":"harmless"})), None);
    }
    #[test]
    fn adding_hooks_preserves_existing_rules_and_hooks() {
        let mut settings =
            json!({"permissions":{"deny":["Edit"]},"hooks":{"Stop":[{"user":true}]}});
        add_hooks(&mut settings, &["Stop"], "/a path/bin", "claude");
        assert_eq!(settings["permissions"]["deny"][0], "Edit");
        assert_eq!(settings["hooks"]["Stop"].as_array().unwrap().len(), 2);
    }
    #[test]
    fn native_busy_and_permission_states_veto_an_idle_looking_pane() {
        let idle = "╭────╮\n│ >  │\n╰────╯";
        assert!(may_deliver(Some(State::Idle), idle));
        assert!(may_deliver(None, idle));
        for state in [State::Active, State::Blocked, State::Exited] {
            assert!(!may_deliver(Some(state), idle));
        }
        assert!(!may_deliver(
            Some(State::Idle),
            "╭────╮\n│ > unfinished text │\n╰────╯"
        ));
    }
    #[test]
    fn every_launch_gets_a_distinct_activity_file_even_on_resume() {
        let root = std::env::temp_dir().join(format!("activity-{}", crate::generate_token()));
        let mut first = BTreeMap::new();
        let mut second = BTreeMap::new();
        let mut args = Vec::new();
        install("gemini", &root, &mut args, &mut first).unwrap();
        install("gemini", &root, &mut Vec::new(), &mut second).unwrap();
        assert_ne!(first[FILE_ENV], second[FILE_ENV]);

        std::fs::write(
            &first[FILE_ENV],
            serde_json::to_vec(&Reading {
                state: State::Idle,
                at: 1,
            })
            .unwrap(),
        )
        .unwrap();
        assert!(read(Path::new(&second[FILE_ENV])).is_none());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn codex_launch_does_not_change_hook_trust_or_install_untrusted_hooks() {
        let mut args = vec!["--sandbox".into(), "read-only".into()];
        let mut env = BTreeMap::new();
        let root = std::env::temp_dir().join(format!("codex-activity-{}", crate::generate_token()));
        install("codex", &root, &mut args, &mut env).unwrap();
        assert_eq!(args, ["--sandbox", "read-only"]);
        assert!(env.is_empty());
        assert!(!root.exists());
    }
}
