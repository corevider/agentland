//! What Claude Code hands its own status line, kept where the core can read it.
//!
//! Claude Code gives every status line command the limits of the login it runs
//! as — `rate_limits.five_hour` and `rate_limits.seven_day`, each a percentage
//! and the time it comes round — in the JSON on its standard input. That is the
//! engine's own word on the numbers, and getting it touches nobody's
//! credentials; reading them back off a pane's screen only works while some
//! status line happens to print them, and only after it has.
//!
//! So the panes Agentland starts get a status line of its own: the core binary,
//! run as `claude-status-line`. It writes down what it was handed, then passes
//! the same input to the status line the person already had and prints what
//! that prints, so the pane looks the way it always did.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::budget::Usage;

/// The word the core binary answers to when it is run as a status line.
pub const SUBCOMMAND: &str = "claude-status-line";

/// How recent a reading has to be to speak for a login over what its pane's
/// screen says.
pub const FRESH: u64 = 5 * 60;

/// How long a reading is kept before it is swept away.
const KEPT_FOR: u64 = 7 * 24 * 60 * 60;

const FOLDER: &str = "rate-limits";

/// One of a login's windows, as Claude Code reports it.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Window {
    /// Percent spent, from 0 to 100.
    pub used_percentage: f32,
    /// When it comes round, in seconds since the epoch.
    pub resets_at: Option<f64>,
}

/// One pane's word on its login's limits, as written down.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Reading {
    pub session_id: String,
    /// The login folder the pane was started with. None is this machine's own.
    pub config_dir: Option<String>,
    pub five_hour: Option<Window>,
    pub seven_day: Option<Window>,
    pub read_at: u64,
}

impl Reading {
    /// The reading as the rest of the core counts a login: its five hours and
    /// its week. Claude Code drops a window once it has come round, so a week
    /// with no five hours beside it is a five hours that has started again.
    pub fn usage(&self) -> Option<Usage> {
        let week = self.seven_day?;
        Some(Usage {
            session: self.five_hour.map_or(0.0, |window| window.used_percentage),
            weekly: week.used_percentage,
        })
    }

    /// The same numbers in the words a status line prints, for a pane whose
    /// person had no status line of their own.
    pub fn said(&self) -> Option<String> {
        let usage = self.usage()?;
        Some(format!("Session: {:.1}% | Weekly: {:.1}%", usage.session, usage.weekly))
    }
}

fn window_from(held: Option<&Value>) -> Option<Window> {
    let held = held?;
    Some(Window {
        used_percentage: held.get("used_percentage")?.as_f64()? as f32,
        resets_at: held.get("resets_at").and_then(Value::as_f64),
    })
}

/// What Claude Code handed a status line, as a reading worth keeping. None when
/// it said nothing about limits — an API key, or a session that has not had its
/// first answer yet.
pub fn reading_from(handed: &str, config_dir: Option<String>, now: u64) -> Option<Reading> {
    let handed: Value = serde_json::from_str(handed).ok()?;
    let limits = handed.get("rate_limits")?;
    let five_hour = window_from(limits.get("five_hour"));
    let seven_day = window_from(limits.get("seven_day"));
    if five_hour.is_none() && seven_day.is_none() {
        return None;
    }

    let session_id = handed.get("session_id")?.as_str()?.trim();
    if session_id.is_empty() {
        return None;
    }

    Some(Reading {
        session_id: session_id.to_owned(),
        config_dir: config_dir.filter(|dir| !dir.trim().is_empty()),
        five_hour,
        seven_day,
        read_at: now,
    })
}

/// Which login a reading belongs to: this machine's own when the pane was given
/// no folder, a named one when its folder is one of the logins here, and nobody's
/// when it is a folder Agentland does not hold.
pub fn identity_of(reading: &Reading, data_dir: &Path) -> Option<String> {
    let Some(folder) = reading.config_dir.as_deref().map(Path::new) else {
        return Some("claude".to_owned());
    };

    if crate::exec::home().is_some_and(|home| home.join(".claude") == folder) {
        return Some("claude".to_owned());
    }

    crate::accounts::labels(data_dir, "claude")
        .into_iter()
        .find(|label| crate::accounts::dir(data_dir, "claude", label) == folder)
        .map(|label| crate::budget::identity_of("claude", Some(&label)))
}

fn folder(data_dir: &Path) -> PathBuf {
    data_dir.join(FOLDER)
}

/// A session id as a file name: only what an id is made of, so a strange one
/// cannot write anywhere but here.
fn file_for(data_dir: &Path, session_id: &str) -> Option<PathBuf> {
    let name: String = session_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    (!name.is_empty()).then(|| folder(data_dir).join(format!("{name}.json")))
}

/// Write a reading down, whole or not at all, so the core never reads half.
pub fn write(data_dir: &Path, reading: &Reading) -> std::io::Result<()> {
    let file = file_for(data_dir, &reading.session_id)
        .ok_or_else(|| std::io::Error::other("the session has no id to file it under"))?;
    std::fs::create_dir_all(folder(data_dir))?;

    let unfinished = file.with_extension("json.part");
    std::fs::write(&unfinished, serde_json::to_vec(reading)?)?;
    std::fs::rename(unfinished, file)
}

/// The newest reading for every login any pane has written about, sweeping
/// away the ones old enough to say nothing any more.
pub fn readings(data_dir: &Path, now: u64) -> BTreeMap<String, (Usage, u64)> {
    let mut newest: BTreeMap<String, (Usage, u64)> = BTreeMap::new();

    for entry in std::fs::read_dir(folder(data_dir)).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|kind| kind != "json") {
            continue;
        }

        let Some(reading) = std::fs::read(&path)
            .ok()
            .and_then(|raw| serde_json::from_slice::<Reading>(&raw).ok())
        else {
            continue;
        };

        if now.saturating_sub(reading.read_at) > KEPT_FOR {
            let _ = std::fs::remove_file(&path);
            continue;
        }

        let (Some(identity), Some(usage)) = (identity_of(&reading, data_dir), reading.usage()) else {
            continue;
        };

        if newest.get(&identity).is_none_or(|(_, at)| *at < reading.read_at) {
            newest.insert(identity, (usage, reading.read_at));
        }
    }

    newest
}

/// Settings with a status line put in them: this command, drawn with the
/// padding and at the pace of the one the person already had.
pub fn with_status_line(settings: &str, command: &str, theirs: Option<&Value>) -> String {
    let Ok(Value::Object(mut held)) = serde_json::from_str::<Value>(settings) else {
        return settings.to_owned();
    };

    let mut line = serde_json::Map::new();
    line.insert("type".to_owned(), Value::from("command"));
    line.insert("command".to_owned(), Value::from(command));
    for key in ["padding", "refreshInterval"] {
        if let Some(value) = theirs.and_then(|theirs| theirs.get(key)) {
            line.insert(key.to_owned(), value.clone());
        }
    }

    held.insert("statusLine".to_owned(), Value::Object(line));
    serde_json::to_string_pretty(&Value::Object(held)).unwrap_or_else(|_| settings.to_owned())
}

/// A pane's settings with Agentland's status line in them. Unchanged where the
/// core cannot say where its own binary is.
pub fn added_to(settings: &str, data_dir: &Path) -> String {
    match own_binary() {
        Some(binary) => with_status_line(
            settings,
            &command_line(&binary, data_dir),
            their_status_line(None).as_ref(),
        ),
        None => settings.to_owned(),
    }
}

/// The core binary, where it lives now. A binary replaced while it runs is
/// named `… (deleted)` on Linux, and the new one is at the old name.
fn own_binary() -> Option<String> {
    let path = std::env::current_exe().ok()?;
    let path = path.to_string_lossy();
    Some(path.strip_suffix(" (deleted)").unwrap_or(&path).to_owned())
}

/// The command a pane runs as its status line.
pub fn command_line(binary: &str, data_dir: &Path) -> String {
    format!("\"{binary}\" {SUBCOMMAND} \"{}\"", data_dir.display())
}

/// The status line a login's own settings name, when it is a command.
fn their_status_line(config_dir: Option<&str>) -> Option<Value> {
    let home = config_dir
        .map(PathBuf::from)
        .or_else(|| crate::exec::home().map(|home| home.join(".claude")))?;
    let raw = std::fs::read_to_string(home.join("settings.json")).ok()?;
    let settings: Value = serde_json::from_str(&raw).ok()?;
    let line = settings.get("statusLine")?;

    (line.get("type").and_then(Value::as_str) == Some("command")).then(|| line.clone())
}

/// The command a status line hands on to: the person's own, never this one.
pub fn theirs_to_run(line: Option<&Value>) -> Option<String> {
    let command = line?.get("command")?.as_str()?.trim();
    (!command.is_empty() && !command.contains(SUBCOMMAND)).then(|| command.to_owned())
}

fn drawn_by(command: &str, handed: &str) -> Option<Vec<u8>> {
    let (shell, flag) = if cfg!(windows) { ("cmd", "/C") } else { ("sh", "-c") };
    let mut child = crate::exec::command(shell)
        .args([flag, command])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    if let Some(mut input) = child.stdin.take() {
        let _ = input.write_all(handed.as_bytes());
    }

    let output = child.wait_with_output().ok()?;
    (!output.stdout.is_empty()).then_some(output.stdout)
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0)
}

/// Answer as a pane's status line: write down what Claude Code handed over,
/// then draw the line the person already had, or the numbers alone when they
/// had none.
pub fn run(data_dir: &Path) {
    let mut handed = String::new();
    let _ = std::io::stdin().read_to_string(&mut handed);

    let config_dir = std::env::var("CLAUDE_CONFIG_DIR")
        .ok()
        .filter(|dir| !dir.trim().is_empty());
    let reading = reading_from(&handed, config_dir.clone(), now_secs());
    if let Some(reading) = &reading {
        if let Err(error) = write(data_dir, reading) {
            eprintln!("agentland: cannot keep what the status line was handed: {error}");
        }
    }

    let drawn = theirs_to_run(their_status_line(config_dir.as_deref()).as_ref())
        .and_then(|command| drawn_by(&command, &handed));
    let line = drawn.or_else(|| reading.as_ref().and_then(Reading::said).map(String::into_bytes));
    if let Some(line) = line {
        let _ = std::io::stdout().write_all(&line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HANDED: &str = r#"{
        "session_id": "abc-123",
        "model": { "display_name": "Opus" },
        "rate_limits": {
            "five_hour": { "used_percentage": 23.5, "resets_at": 1738425600 },
            "seven_day": { "used_percentage": 41.2, "resets_at": 1738857600 }
        }
    }"#;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("agentland-status-line-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_limits_claude_code_hands_over_are_read() {
        let reading = reading_from(HANDED, None, 100).unwrap();

        assert_eq!(reading.session_id, "abc-123");
        assert_eq!(reading.usage(), Some(Usage { session: 23.5, weekly: 41.2 }));
        assert_eq!(reading.seven_day.unwrap().resets_at, Some(1738857600.0));
    }

    #[test]
    fn a_session_with_no_limits_says_nothing() {
        assert_eq!(reading_from(r#"{"session_id":"a","model":{}}"#, None, 1), None);
        assert_eq!(reading_from(r#"{"session_id":"a","rate_limits":{}}"#, None, 1), None);
        assert_eq!(reading_from("not json", None, 1), None);
    }

    #[test]
    fn a_five_hours_dropped_after_its_reset_has_started_again() {
        let handed = r#"{"session_id":"a","rate_limits":{"seven_day":{"used_percentage":90}}}"#;

        assert_eq!(
            reading_from(handed, None, 1).unwrap().usage(),
            Some(Usage { session: 0.0, weekly: 90.0 }),
        );
    }

    #[test]
    fn five_hours_without_a_week_is_not_a_reading_of_the_login() {
        let handed = r#"{"session_id":"a","rate_limits":{"five_hour":{"used_percentage":10}}}"#;

        assert_eq!(reading_from(handed, None, 1).unwrap().usage(), None);
    }

    #[test]
    fn the_numbers_alone_read_back_as_a_status_line() {
        let said = reading_from(HANDED, None, 1).unwrap().said().unwrap();

        assert_eq!(crate::budget::read_usage(&said), Some(Usage { session: 23.5, weekly: 41.2 }));
    }

    #[test]
    fn the_status_line_goes_in_beside_the_permissions() {
        let theirs: Value = serde_json::json!({ "type": "command", "command": "x", "padding": 0, "refreshInterval": 10 });
        let settings = with_status_line(r#"{"permissions":{"allow":["Read"]}}"#, "\"/bin/core\" claude-status-line \"/d\"", Some(&theirs));
        let settings: Value = serde_json::from_str(&settings).unwrap();

        assert_eq!(settings["permissions"]["allow"][0], "Read");
        assert_eq!(settings["statusLine"]["type"], "command");
        assert_eq!(settings["statusLine"]["command"], "\"/bin/core\" claude-status-line \"/d\"");
        assert_eq!(settings["statusLine"]["padding"], 0);
        assert_eq!(settings["statusLine"]["refreshInterval"], 10);
    }

    #[test]
    fn settings_that_are_not_an_object_are_left_alone() {
        assert_eq!(with_status_line("[]", "c", None), "[]");
    }

    #[test]
    fn the_persons_own_line_is_handed_on_but_never_this_one() {
        let theirs = serde_json::json!({ "type": "command", "command": " ccstatusline " });
        let ours = serde_json::json!({ "type": "command", "command": command_line("/bin/core", Path::new("/d")) });

        assert_eq!(theirs_to_run(Some(&theirs)), Some("ccstatusline".to_owned()));
        assert_eq!(theirs_to_run(Some(&ours)), None);
        assert_eq!(theirs_to_run(Some(&serde_json::json!({ "command": "  " }))), None);
        assert_eq!(theirs_to_run(None), None);
    }

    #[test]
    fn a_strange_session_id_cannot_leave_the_folder() {
        let data_dir = Path::new("/data");

        assert_eq!(file_for(data_dir, "../../etc/x"), Some(PathBuf::from("/data/rate-limits/etcx.json")));
        assert_eq!(file_for(data_dir, "../"), None);
    }

    #[test]
    fn each_login_gets_its_newest_reading() {
        let data_dir = scratch("logins");
        let work = crate::accounts::dir(&data_dir, "claude", "work");
        std::fs::create_dir_all(&work).unwrap();

        let reading = |id: &str, config_dir: Option<&Path>, weekly: f32, read_at: u64| Reading {
            session_id: id.to_owned(),
            config_dir: config_dir.map(|dir| dir.to_string_lossy().into_owned()),
            five_hour: None,
            seven_day: Some(Window { used_percentage: weekly, resets_at: None }),
            read_at,
        };

        write(&data_dir, &reading("own-old", None, 10.0, 1_000)).unwrap();
        write(&data_dir, &reading("own-new", None, 20.0, 2_000)).unwrap();
        write(&data_dir, &reading("work", Some(&work), 70.0, 1_500)).unwrap();
        write(&data_dir, &reading("stranger", Some(Path::new("/nowhere/here")), 99.0, 2_000)).unwrap();

        let held = readings(&data_dir, 2_100);

        assert_eq!(held.get("claude"), Some(&(Usage { session: 0.0, weekly: 20.0 }, 2_000)));
        assert_eq!(held.get("claude/work"), Some(&(Usage { session: 0.0, weekly: 70.0 }, 1_500)));
        assert_eq!(held.len(), 2, "a folder Agentland does not hold is nobody's login");

        let _ = std::fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn a_reading_old_enough_to_say_nothing_is_swept_away() {
        let data_dir = scratch("swept");
        let old = reading_from(HANDED, None, 0).unwrap();
        write(&data_dir, &old).unwrap();

        assert!(readings(&data_dir, KEPT_FOR + 1).is_empty());
        assert!(!folder(&data_dir).join("abc-123.json").exists());

        let _ = std::fs::remove_dir_all(&data_dir);
    }
}
