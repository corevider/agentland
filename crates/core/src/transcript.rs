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
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
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
