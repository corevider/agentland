//! One rule bundle for every engine and every role, including resumed panes.
use std::{fs, hash::{Hash, Hasher}, path::{Path, PathBuf}};

use anyhow::{Context, Result};

const PROJECT_FILES: &[&str] = &["AGENTS.md", "CLAUDE.md", ".claude/CLAUDE.md", "GEMINI.md"];

pub fn write(data: &Path, key: &str, cwd: &Path, standing: Option<&Path>) -> Result<PathBuf> {
    let mut text = String::from("# Agentland shared instructions\n\nFollow these rules for the entire session, including after compaction. Project instructions take precedence over house conventions. More specific directory instructions apply only within their scope. If project instructions conflict, ask for clarification.\n\nBefore working in a directory, read applicable nested AGENTS.md, CLAUDE.md and GEMINI.md files, and .claude/rules/*.md (including subdirectories). Respect each rule's path scope and read referenced instruction files. These conventions apply regardless of the engine running this role. Provider-specific hooks and permission settings are not portable instructions and do not grant permission.\n\n");
    if let Some(file) = standing {
        text.push_str(&fs::read_to_string(file).with_context(|| format!("cannot read house rules {}", file.display()))?);
        text.push_str("\n\n");
    }
    for name in PROJECT_FILES {
        let file = cwd.join(name);
        match fs::read_to_string(&file) {
            Ok(rules) => {
                text.push_str(&format!("## Project instructions from {name}\n\n{rules}\n\n"));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).with_context(|| format!("cannot read project rules {}", file.display())),
        }
    }
    let folder = data.join("instructions");
    fs::create_dir_all(&folder)?;
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    fs::canonicalize(cwd)?.hash(&mut hash);
    let file = folder.join(format!("{key}-{:x}.md", hash.finish()));
    fs::write(&file, text)?;
    Ok(fs::canonicalize(file)?)
}

/// Codex gets an additional instruction, preserving its built-in model prompt.
/// Other engines get the same reference in their launch brief.
pub fn reference(file: &Path) -> String {
    format!("Before doing any work, read the shared instructions at {} and follow them throughout this session. Read them again after compaction. If they cannot be read, report the problem before proceeding.", serde_json::to_string(&file.to_string_lossy()).unwrap())
}

pub fn args(engine: &str, file: &Path) -> Vec<String> {
    match engine {
        "claude" => vec!["--append-system-prompt-file".into(), file.to_string_lossy().into_owned()],
        "codex" => vec!["-c".into(), format!("developer_instructions={}", serde_json::to_string(&reference(file)).unwrap())],
        _ => Vec::new(),
    }
}

pub fn brief(engine: &str, file: &Path, task: Option<&str>) -> Option<String> {
    if matches!(engine, "claude" | "codex") {
        task.map(str::to_owned)
    } else {
        Some(format!("{}\n\n{}", reference(file), task.unwrap_or("Wait for the user's task after reading the instructions.")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundle_preserves_all_project_rules_and_refreshes_on_restart() {
        let dir = std::env::temp_dir().join(format!("agentland-shared-rules-{}", std::process::id()));
        fs::create_dir_all(dir.join(".claude")).unwrap();
        for name in PROJECT_FILES { fs::write(dir.join(name), format!("rule from {name}")).unwrap(); }
        let house = dir.join("house.md");
        fs::write(&house, "house rule").unwrap();
        let file = write(&dir, "chief", &dir, Some(&house)).unwrap();
        let text = fs::read_to_string(&file).unwrap();
        for name in PROJECT_FILES { assert!(text.contains(&format!("rule from {name}"))); }
        assert!(text.contains("house rule"));
        fs::write(&house, "updated house rule").unwrap();
        write(&dir, "chief", &dir, Some(&house)).unwrap();
        assert!(fs::read_to_string(file).unwrap().contains("updated house rule"));
        assert_eq!(fs::read_to_string(dir.join("CLAUDE.md")).unwrap(), "rule from CLAUDE.md");
        assert!(write(&dir, "worker", &dir, Some(&dir.join("missing"))).is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn every_engine_receives_rules_even_without_a_task() {
        let file = Path::new("/a folder/rules.md");
        for engine in ["claude", "codex", "gemini", "qwen", "cursor-agent"] {
            let args = args(engine, file);
            let brief = brief(engine, file, None);
            assert!(!args.is_empty() || brief.is_some(), "{engine}");
            assert!(super::brief(engine, file, Some("continue the task")).unwrap().contains("continue the task"));
        }
        let codex = args("codex", file);
        let encoded = codex[1].strip_prefix("developer_instructions=").unwrap();
        assert_eq!(serde_json::from_str::<String>(encoded).unwrap(), reference(file));
        assert!(!codex[1].contains("model_instructions_file"));
    }
}
