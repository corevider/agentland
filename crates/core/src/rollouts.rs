//! What Codex keeps of its own sessions.
//!
//! One `rollout-<started>-<id>.jsonl` per session under
//! `$CODEX_HOME/sessions/YYYY/MM/DD`, opening with a `session_meta` line that
//! names the folder the session ran in. Every line carries a `timestamp`; what
//! a person said is a `response_item` message from the `user`, and what the
//! account has left is the `rate_limits` on a `token_count` event. Measured on
//! `codex-cli 0.153.4`.

use std::fs;
use std::io::BufRead;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::budget::Usage;
use crate::meter::Spend;
use crate::transcript::{seconds_of, squash};

/// How many of the newest sessions are read when looking for one. A folder
/// whose last session is further back than that is a folder Codex has not
/// been in lately.
const LOOKED_AT: usize = 200;

/// A session starts a moment after the pane that runs it, and the two clocks
/// are read a moment apart.
const SLACK_SECONDS: u64 = 5;

/// How far back from the end of a session its record is read.
const LINES_READ: usize = 2_000;

#[derive(Clone, Debug, PartialEq)]
pub struct Rollout {
    pub id: String,
    pub cwd: PathBuf,
    pub started: u64,
    pub path: PathBuf,
}

impl Rollout {
    fn text(&self) -> String {
        fs::read_to_string(&self.path).unwrap_or_default()
    }

    /// Read lifecycle events from this exact conversation, excluding earlier
    /// launches of a resumed conversation. Limit reads on long transcripts.
    pub fn activity_since(&self, since: u64) -> Option<crate::activity::State> {
        use std::io::{Read, Seek, SeekFrom};
        let mut file = fs::File::open(&self.path).ok()?;
        let size = file.metadata().ok()?.len();
        file.seek(SeekFrom::Start(size.saturating_sub(256 * 1024))).ok()?;
        let mut bytes = Vec::new(); file.take(256 * 1024).read_to_end(&mut bytes).ok()?;
        activity(&String::from_utf8_lossy(&bytes), since)
    }

    /// Whether this session was told something, in a person's own message.
    pub fn mentions(&self, fingerprint: &str) -> bool {
        mentions(&self.text(), fingerprint)
    }

    /// What each turn of this session cost since a moment.
    pub fn spending_since(&self, since: u64) -> Vec<Spend> {
        spending_since(&self.text(), since)
    }
}

pub fn activity(raw: &str, since: u64) -> Option<crate::activity::State> {
    for line in raw.lines().rev() {
        let Ok(row) = serde_json::from_str::<Value>(line) else { continue; };
        if row.get("type").and_then(Value::as_str) != Some("event_msg") { continue; }
        let Some(at) = row.get("timestamp").and_then(Value::as_str).and_then(seconds_of) else { continue; };
        // The launch clock has second precision: reject its entire first
        // second rather than accepting a preceding turn from that second.
        if at <= since { continue; }
        match row["payload"]["type"].as_str() {
            Some("task_started") => return Some(crate::activity::State::Active),
            Some("task_complete") => return Some(crate::activity::State::Idle),
            Some("turn_aborted") => return Some(crate::activity::State::WaitingInput),
            _ => {}
        }
    }
    None
}

/// Where a login keeps its sessions: the folder handed to it as `CODEX_HOME`,
/// or this machine's own.
pub fn home(login: Option<&(String, String)>) -> Option<PathBuf> {
    if let Some((variable, folder)) = login {
        if variable == "CODEX_HOME" {
            return Some(PathBuf::from(folder));
        }
    }

    std::env::var_os("CODEX_HOME")
        .filter(|held| !held.is_empty())
        .map(PathBuf::from)
        .or_else(|| crate::exec::home().map(|home| home.join(".codex")))
}

/// Session files, newest first. The folders and the file names both sort by
/// when the session started, so nothing has to be opened to order them.
fn newest_first(home: &Path) -> Vec<PathBuf> {
    fn sorted(folder: &Path) -> Vec<PathBuf> {
        let mut held: Vec<PathBuf> = fs::read_dir(folder)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .collect();
        held.sort();
        held.reverse();
        held
    }

    let mut found = Vec::new();
    for year in sorted(&home.join("sessions")) {
        for month in sorted(&year) {
            for day in sorted(&month) {
                for file in sorted(&day) {
                    if file.extension().is_some_and(|kind| kind == "jsonl") {
                        found.push(file);
                        if found.len() == LOOKED_AT {
                            return found;
                        }
                    }
                }
            }
        }
    }

    found
}

fn meta_of(path: &Path) -> Option<Rollout> {
    let mut first = String::new();
    std::io::BufReader::new(fs::File::open(path).ok()?)
        .read_line(&mut first)
        .ok()?;

    let row: Value = serde_json::from_str(&first).ok()?;
    if row.get("type")?.as_str()? != "session_meta" {
        return None;
    }

    let payload = row.get("payload")?;
    let stamp = payload
        .get("timestamp")
        .or_else(|| row.get("timestamp"))
        .and_then(Value::as_str)?;

    Some(Rollout {
        id: payload.get("id")?.as_str()?.to_owned(),
        cwd: PathBuf::from(payload.get("cwd")?.as_str()?),
        started: seconds_of(stamp)?,
        path: path.to_owned(),
    })
}

fn sessions_in(home: &Path, folder: &Path) -> impl Iterator<Item = Rollout> {
    let spelled = folder.to_owned();
    let resolved = fs::canonicalize(folder).unwrap_or_else(|_| folder.to_owned());

    newest_first(home)
        .into_iter()
        .filter_map(|path| meta_of(&path))
        .filter(move |held| held.cwd == spelled || held.cwd == resolved)
}

/// The newest session Codex ran in this folder.
pub fn newest_in(home: &Path, folder: &Path) -> Option<Rollout> {
    sessions_in(home, folder).next()
}

/// The session a pane opened in this folder once it started, that no other
/// agent has claimed.
///
/// Codex names its sessions itself and takes no name from anybody, so an
/// agent's own conversation is found after its pane opens rather than chosen
/// before. The first one after the pane started is the one it opened.
pub fn started_in(home: &Path, folder: &Path, since: u64, taken: &[String]) -> Option<Rollout> {
    let floor = since.saturating_sub(SLACK_SECONDS);
    let spelled = folder.to_owned();
    let resolved = fs::canonicalize(folder).unwrap_or_else(|_| folder.to_owned());

    newest_first(home)
        .into_iter()
        .filter_map(|path| meta_of(&path))
        .take_while(|held| held.started >= floor)
        .filter(|held| held.cwd == spelled || held.cwd == resolved)
        .filter(|held| !taken.contains(&held.id))
        .last()
}

/// A session by the id Codex gave it, while it is still there to be opened.
pub fn find(home: &Path, id: &str) -> Option<Rollout> {
    let ending = format!("-{id}.jsonl");

    newest_first(home)
        .into_iter()
        .find(|path| path.to_string_lossy().ends_with(&ending))
        .and_then(|path| meta_of(&path))
}

/// What the account has left, from the newest session that says.
///
/// Every session on one login reports the same allowance, so the newest one
/// anywhere is the freshest word on it.
pub fn usage_in(home: &Path, now: u64) -> Option<Usage> {
    newest_first(home)
        .into_iter()
        .take(5)
        .find_map(|path| usage(&fs::read_to_string(path).ok()?, now))
}

fn said_by_a_person(row: &Value) -> Option<String> {
    if row.get("type")?.as_str()? != "response_item" {
        return None;
    }

    let payload = row.get("payload")?;
    if payload.get("type")?.as_str()? != "message" || payload.get("role")?.as_str()? != "user" {
        return None;
    }

    let parts = payload.get("content")?.as_array()?;
    Some(
        parts
            .iter()
            .filter_map(|part| part.get("text")?.as_str())
            .collect::<Vec<_>>()
            .join(" "),
    )
}

fn last_rows(raw: &str) -> impl Iterator<Item = Value> + '_ {
    raw.lines()
        .rev()
        .take(LINES_READ)
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
}

/// Whether a session's record holds a message from a person containing this.
/// Codex's own words and the instructions it was started with are not delivery.
pub fn mentions(raw: &str, fingerprint: &str) -> bool {
    let needle = squash(fingerprint);
    if needle.is_empty() {
        return false;
    }

    last_rows(raw)
        .filter_map(|row| said_by_a_person(&row))
        .any(|said| squash(&said).contains(&needle))
}

/// What the account has left, from the last word a session wrote about it.
///
/// The five-hour window is the session and the week is the week. A window
/// whose reset has already passed reads as untouched rather than as the number
/// it stood at before it reset.
pub fn usage(raw: &str, now: u64) -> Option<Usage> {
    last_rows(raw).find_map(|row| {
        let limits = row.get("payload")?.get("rate_limits")?;
        let spent = |window: &str| -> Option<f32> {
            let held = limits.get(window)?;
            let used = held.get("used_percent")?.as_f64()? as f32;
            let resets = held.get("resets_at").and_then(Value::as_u64).unwrap_or(u64::MAX);

            Some(if resets <= now { 0.0 } else { used })
        };

        Some(Usage {
            session: spent("primary")?,
            weekly: spent("secondary")?,
        })
    })
}

/// What each turn cost since a moment, from Codex's own count.
///
/// Codex's `input_tokens` includes what the cache served, where Claude Code's
/// does not, so the cached part is taken out of the input rather than counted
/// twice.
pub fn spending_since(raw: &str, since: u64) -> Vec<Spend> {
    raw.lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(|row| {
            let at = seconds_of(row.get("timestamp")?.as_str()?)?;
            if at < since {
                return None;
            }

            let used = row.get("payload")?.get("info")?.get("last_token_usage")?;
            let number = |key: &str| used.get(key).and_then(Value::as_u64).unwrap_or(0);
            let cached = number("cached_input_tokens");

            Some(Spend {
                at,
                input: number("input_tokens").saturating_sub(cached),
                cached,
                output: number("output_tokens"),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("agentland-rollouts-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("a scratch folder");
        dir
    }

    /// A session file laid out the way Codex lays one out, `clock` as
    /// `HH-MM-SS` the way it spells it in the name.
    fn session(home: &Path, day: &str, clock: &str, id: &str, cwd: &str, rest: &[&str]) -> PathBuf {
        let folder = home.join("sessions").join(day.replace('-', "/"));
        fs::create_dir_all(&folder).expect("the day's folder");

        let stamp = format!("{day}T{}Z", clock.replace('-', ":"));
        let mut lines = vec![format!(
            r#"{{"timestamp":"{stamp}","type":"session_meta","payload":{{"id":"{id}","timestamp":"{stamp}","cwd":"{cwd}"}}}}"#
        )];
        lines.extend(rest.iter().map(|line| (*line).to_owned()));

        let file = folder.join(format!("rollout-{day}T{clock}-{id}.jsonl"));
        fs::write(&file, lines.join("\n")).expect("the session is written");
        file
    }

    const TOLD: &str = r#"{"timestamp":"2026-09-14T19:58:18.538Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Take the project on"}]}}"#;
    const INSTRUCTED: &str = r#"{"timestamp":"2026-09-14T19:58:18.505Z","type":"response_item","payload":{"type":"message","role":"developer","content":[{"type":"input_text","text":"You are the primary agent"}]}}"#;
    const REPLIED: &str = r#"{"timestamp":"2026-09-14T19:58:21.540Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"I will read the board"}]}}"#;

    #[test]
    fn the_newest_session_in_a_folder_is_the_one_found() {
        let home = scratch("newest");
        session(&home, "2026-09-13", "10-00-00", "old", "/w/desk", &[]);
        session(&home, "2026-09-14", "09-00-00", "new", "/w/desk", &[]);
        session(&home, "2026-09-14", "11-00-00", "elsewhere", "/w/other", &[]);

        assert_eq!(newest_in(&home, Path::new("/w/desk")).map(|held| held.id).as_deref(), Some("new"));
        assert_eq!(newest_in(&home, Path::new("/w/nowhere")), None);

        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn a_pane_finds_the_session_it_opened_and_not_one_somebody_claimed() {
        let home = scratch("started");
        session(&home, "2026-09-14", "09-00-00", "before", "/w/shared", &[]);
        session(&home, "2026-09-14", "10-00-10", "first", "/w/shared", &[]);
        session(&home, "2026-09-14", "10-00-20", "second", "/w/shared", &[]);
        let pane_started = seconds_of("2026-09-14T10:00:00Z").expect("a stamp");

        let found = |taken: &[String]| started_in(&home, Path::new("/w/shared"), pane_started, taken).map(|held| held.id);

        assert_eq!(found(&[]).as_deref(), Some("first"), "the first one after the pane opened");
        assert_eq!(found(&["first".to_owned()]).as_deref(), Some("second"), "not one another agent holds");
        assert_eq!(
            started_in(&home, Path::new("/w/shared"), pane_started + 3_600, &[]),
            None,
            "nothing opened since then"
        );

        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn a_session_is_found_again_by_the_id_codex_gave_it() {
        let home = scratch("by-id");
        session(&home, "2026-09-14", "10-00-00", "01a0a17f-b111-7031-93ec-c1200cb49bcf", "/w/desk", &[]);

        let found = find(&home, "01a0a17f-b111-7031-93ec-c1200cb49bcf").expect("it is there");
        assert_eq!(found.cwd, Path::new("/w/desk"));
        assert_eq!(find(&home, "01a0a17f-0000-0000-0000-000000000000"), None);

        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn only_what_a_person_said_counts_as_told() {
        let raw = [INSTRUCTED, TOLD, REPLIED].join("\n");

        assert!(mentions(&raw, "take the   project on"));
        assert!(!mentions(&raw, "You are the primary agent"), "the engine's own instructions are not delivery");
        assert!(!mentions(&raw, "I will read the board"), "the engine's reply is not delivery");
        assert!(!mentions(&raw, "   "));
    }

    #[test]
    fn the_account_is_read_off_the_last_word_and_a_reset_window_is_untouched() {
        let older = r#"{"timestamp":"2026-09-14T19:58:22.477Z","type":"event_msg","payload":{"type":"token_count","info":null,"rate_limits":{"primary":{"used_percent":50.0,"window_minutes":300,"resets_at":1789433902},"secondary":{"used_percent":60.0,"window_minutes":10080,"resets_at":1790020702}}}}"#;
        let newer = r#"{"timestamp":"2026-09-14T19:58:28.068Z","type":"event_msg","payload":{"type":"token_count","info":null,"rate_limits":{"primary":{"used_percent":1.0,"window_minutes":300,"resets_at":1789433902},"secondary":{"used_percent":12.5,"window_minutes":10080,"resets_at":1790020702}}}}"#;
        let raw = [older, TOLD, newer].join("\n");

        assert_eq!(usage(&raw, 1_789_415_895), Some(Usage { session: 1.0, weekly: 12.5 }));
        assert_eq!(
            usage(&raw, 1_789_433_902),
            Some(Usage { session: 0.0, weekly: 12.5 }),
            "the five hours are over, so they are spent on nothing"
        );
        assert_eq!(usage(TOLD, 1_789_415_895), None, "a session that never said is not a session at zero");
    }

    #[test]
    fn a_turn_costs_what_codex_counted_with_the_cache_taken_out() {
        let counted = r#"{"timestamp":"2026-09-14T19:58:22.477Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":15316,"cached_input_tokens":12160,"output_tokens":51}},"rate_limits":null}}"#;
        let limits_only = r#"{"timestamp":"2026-09-14T19:58:23.000Z","type":"event_msg","payload":{"type":"token_count","info":null}}"#;
        let raw = [TOLD, counted, limits_only].join("\n");
        let at = seconds_of("2026-09-14T19:58:22Z").expect("a stamp");

        let spent = spending_since(&raw, at);
        assert_eq!(spent.len(), 1, "a count with no usage in it is not a turn");
        assert_eq!((spent[0].at, spent[0].input, spent[0].cached, spent[0].output), (at, 3_156, 12_160, 51));

        assert!(spending_since(&raw, at + 1).is_empty(), "a turn before the moment is not counted");
    }

    #[test]
    fn a_login_handed_its_own_folder_keeps_its_sessions_there() {
        let login = ("CODEX_HOME".to_owned(), "/accounts/codex/work".to_owned());
        assert_eq!(home(Some(&login)), Some(PathBuf::from("/accounts/codex/work")));

        let claude = ("CLAUDE_CONFIG_DIR".to_owned(), "/accounts/claude/work".to_owned());
        assert_ne!(home(Some(&claude)), Some(PathBuf::from("/accounts/claude/work")));
    }
}

#[cfg(test)]
mod activity_tests {
    use super::*;
    #[test]
    fn native_codex_lifecycle_is_scoped_to_the_current_launch() {
        let raw = r#"{"timestamp":"2026-09-17T08:00:00Z","type":"event_msg","payload":{"type":"task_started"}}
{"timestamp":"2026-09-17T08:00:02Z","type":"event_msg","payload":{"type":"task_complete"}}
{"timestamp":"2026-09-17T08:00:03Z","type":"event_msg","payload":{"type":"token_count"}}"#;
        let start = seconds_of("2026-09-17T07:59:59Z").unwrap();
        assert_eq!(activity(raw, start), Some(crate::activity::State::Idle));
        assert_eq!(activity(raw.lines().next().unwrap(), start), Some(crate::activity::State::Active));
        assert_eq!(activity(raw, start + 4), None, "a resumed conversation cannot inherit the prior launch's idle state");
        let interrupted = format!("{raw}\n{{\"timestamp\":\"2026-09-17T08:00:04Z\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"turn_aborted\"}}}}");
        assert_eq!(activity(&interrupted, start), Some(crate::activity::State::WaitingInput));
    }
}
