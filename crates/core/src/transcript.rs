use std::path::{Path, PathBuf};

/// Where an engine keeps what it was actually told.
///
/// The visible buffer is not proof: text can sit in a composer that was never
/// submitted, or a resume picker can swallow it, and the pane still shows it.
/// The transcript is the engine's own record of the messages it received.
pub struct Transcript {
    pub path: PathBuf,
}

pub fn slug_for(worktree: &Path) -> String {
    worktree
        .to_string_lossy()
        .chars()
        .map(|character| match character {
            '/' | '.' | '_' => '-',
            other => other,
        })
        .collect()
}

pub fn project_root() -> Option<PathBuf> {
    let home = crate::exec::home()?;
    let root = home.join(".claude/projects");
    root.is_dir().then_some(root)
}

/// The transcript for a worktree, if the engine keeps one.
pub fn find(worktree: &Path) -> Option<Transcript> {
    let root = project_root()?;
    let wanted = slug_for(worktree);

    let mut folder = root.join(&wanted);
    if !folder.is_dir() {
        // Engines differ on how they fold a path into a folder name; match on the
        // normalised form rather than insisting on one spelling.
        folder = std::fs::read_dir(&root)
            .ok()?
            .flatten()
            .map(|entry| entry.path())
            .find(|path| {
                path.file_name()
                    .map(|name| slug_for(Path::new(name)) == wanted)
                    .unwrap_or(false)
            })?;
    }

    let newest = std::fs::read_dir(&folder)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().map(|kind| kind == "jsonl").unwrap_or(false))
        .max_by_key(|path| {
            std::fs::metadata(path)
                .and_then(|data| data.modified())
                .ok()
        })?;

    Some(Transcript { path: newest })
}

/// What the engine has spent since a moment, turn by turn.
///
/// The transcript is the only place this is written down: the engine records
/// what each turn cost, and nothing else on the machine does. Reading it is how
/// the app can throttle itself against a per-minute ceiling it does not make the
/// requests for.
///
/// Rows without a timestamp or a usage block are skipped rather than guessed at.
pub fn spending_since(worktree: &Path, since: u64) -> Vec<crate::meter::Spend> {
    let Some(transcript) = find(worktree) else {
        return Vec::new();
    };

    let Ok(text) = std::fs::read_to_string(&transcript.path) else {
        return Vec::new();
    };

    let mut spends = Vec::new();
    for line in text.lines() {
        let Ok(row) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };

        let Some(usage) = row.get("message").and_then(|held| held.get("usage")) else {
            continue;
        };

        let Some(at) = row.get("timestamp").and_then(|held| held.as_str()).and_then(seconds_of)
        else {
            continue;
        };

        if at < since {
            continue;
        }

        let number = |key: &str| usage.get(key).and_then(serde_json::Value::as_u64).unwrap_or(0);

        spends.push(crate::meter::Spend {
            at,
            input: number("input_tokens") + number("cache_creation_input_tokens"),
            cached: number("cache_read_input_tokens"),
            output: number("output_tokens"),
        });
    }

    spends
}

/// Seconds since the epoch, from the engine's own `2026-09-01T13:26:11.984Z`.
///
/// Hand-rolled because the whole of a date library would be carried for one
/// format that never varies, and getting this wrong is visible immediately: a
/// window that never fills, or one that never empties.
fn seconds_of(stamp: &str) -> Option<u64> {
    let (date, rest) = stamp.split_once('T')?;
    let time = rest.split(['.', 'Z']).next()?;

    let mut date = date.split('-');
    let year: i64 = date.next()?.parse().ok()?;
    let month: i64 = date.next()?.parse().ok()?;
    let day: i64 = date.next()?.parse().ok()?;

    let mut clock = time.split(':');
    let hour: i64 = clock.next()?.parse().ok()?;
    let minute: i64 = clock.next()?.parse().ok()?;
    let second: i64 = clock.next()?.parse().ok()?;

    // Days from the civil calendar, the standard shift-March algorithm.
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;

    u64::try_from(days * 86_400 + hour * 3_600 + minute * 60 + second).ok()
}

/// Whether the engine has a record of being told this.
///
/// `None` means there is no transcript to consult — not that nothing arrived.
pub fn was_told(worktree: &Path, fingerprint: &str) -> Option<bool> {
    let transcript = find(worktree)?;
    let raw = std::fs::read_to_string(&transcript.path).ok()?;
    Some(mentions(&raw, fingerprint))
}

pub fn mentions(transcript: &str, fingerprint: &str) -> bool {
    let needle = squash(fingerprint);
    if needle.is_empty() {
        return false;
    }

    transcript
        .lines()
        .rev()
        .take(2_000)
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|row| row.get("type").and_then(|kind| kind.as_str()) == Some("user"))
        .any(|row| squash(&text_of(&row)).contains(&needle))
}

fn text_of(row: &serde_json::Value) -> String {
    let Some(message) = row.get("message") else {
        return String::new();
    };

    match message.get("content") {
        Some(serde_json::Value::String(text)) => text.clone(),
        Some(serde_json::Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| part.get("text").and_then(|text| text.as_str()))
            .collect::<Vec<_>>()
            .join(" "),
        _ => String::new(),
    }
}

fn squash(text: &str) -> String {
    text.chars()
        .filter(|character| !character.is_whitespace())
        .flat_map(|character| character.to_lowercase())
        .collect()
}

#[cfg(test)]
mod clock_tests {
    use super::seconds_of;

    #[test]
    fn the_engines_own_stamp_becomes_seconds() {
        // 2026-09-01T13:26:11Z, checked against `date -u -d ... +%s`.
        assert_eq!(seconds_of("2026-09-01T13:26:11.984Z"), Some(1_788_269_171));
        assert_eq!(seconds_of("2026-09-01T13:26:11Z"), Some(1_788_269_171));
    }

    #[test]
    fn the_epoch_and_a_leap_day_land_where_they_should() {
        assert_eq!(seconds_of("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(seconds_of("2024-02-29T00:00:00Z"), Some(1_709_164_800));
        assert_eq!(seconds_of("2000-03-01T00:00:00Z"), Some(951_868_800));
    }

    #[test]
    fn a_stamp_that_is_not_one_is_not_guessed_at() {
        assert_eq!(seconds_of(""), None);
        assert_eq!(seconds_of("yesterday"), None);
        assert_eq!(seconds_of("2026-09-01"), None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_folds_into_the_folder_name_the_engine_uses() {
        assert_eq!(
            slug_for(Path::new("/home/dev/Desktop/agentland")),
            "-home-dev-Desktop-agentland"
        );
        assert_eq!(
            slug_for(Path::new(
                "/home/dev/Desktop/agentland/apps/desktop/src-tauri/data/worktrees/ccdo/ccdo"
            )),
            "-home-dev-Desktop-agentland-apps-desktop-src-tauri-data-worktrees-ccdo-ccdo",
            "a nested path folds the same way"
        );
    }

    #[test]
    fn a_user_message_counts_and_the_engines_own_words_do_not() {
        let transcript = concat!(
            r#"{"type":"mode","mode":"default"}"#, "\n",
            r#"{"type":"user","message":{"role":"user","content":"Serve /health from server.js"}}"#, "\n",
            r#"{"type":"assistant","message":{"role":"assistant","content":"I will add /health now"}}"#, "\n",
        );

        assert!(mentions(transcript, "Serve /health from server.js"));
        assert!(!mentions(transcript, "I will add /health now"), "the engine's reply is not delivery");
        assert!(!mentions(transcript, "something nobody said"));
    }

    #[test]
    fn wrapping_and_spacing_do_not_hide_the_message() {
        let transcript =
            r#"{"type":"user","message":{"role":"user","content":"Prove /health\n  with a node test"}}"#;

        assert!(mentions(transcript, "Prove /health with a node test"));
    }

    #[test]
    fn a_message_split_into_parts_is_read_whole() {
        let transcript = concat!(
            r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"Note /health"},"#,
            r#"{"type":"text","text":"in the README"}]}}"#,
        );

        assert!(mentions(transcript, "Note /health in the README"));
    }

    #[test]
    fn junk_lines_are_skipped_rather_than_failing_the_read() {
        let transcript = concat!(
            "not json at all\n",
            r#"{"type":"user","message":{"role":"user","content":"the real one"}}"#, "\n",
            "{ truncated\n",
        );

        assert!(mentions(transcript, "the real one"));
    }

    #[test]
    fn an_empty_fingerprint_never_counts_as_delivered() {
        let transcript = r#"{"type":"user","message":{"role":"user","content":"anything"}}"#;
        assert!(!mentions(transcript, "   "));
    }

    #[test]
    fn a_worktree_with_no_transcript_says_it_does_not_know() {
        let nowhere = Path::new("/tmp/agentland-a-path-no-engine-has-ever-opened");
        assert_eq!(was_told(nowhere, "anything"), None);
    }
}
