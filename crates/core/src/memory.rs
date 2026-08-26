use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{anyhow, bail, Result};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Workspace,
    Repository,
    Agent,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Memory {
    pub id: String,
    pub text: String,
    pub scope: Scope,
    pub scope_id: String,
    pub proposed_by: String,
    #[serde(default)]
    pub approved: bool,
    #[serde(default)]
    pub masked: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ProposeMemory {
    pub text: String,
    #[serde(default = "workspace_scope")]
    pub scope: Scope,
    #[serde(default)]
    pub scope_id: String,
    #[serde(default = "unknown_author")]
    pub proposed_by: String,
}

fn workspace_scope() -> Scope {
    Scope::Workspace
}

fn unknown_author() -> String {
    "unknown".to_owned()
}

const REDACTED: &str = "[redacted]";

fn is_secretish(token: &str) -> bool {
    let trimmed = token.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_');

    let prefixes = [
        "sk-", "sk_", "pk_", "ghp_", "gho_", "ghu_", "ghs_", "ghr_", "github_pat_", "xoxb-",
        "xoxp-", "AKIA", "ASIA", "AIza", "glpat-", "npm_", "dop_v1_", "hf_",
    ];

    if prefixes.iter().any(|prefix| trimmed.starts_with(prefix)) && trimmed.len() >= 12 {
        return true;
    }

    if trimmed.starts_with("eyJ") && trimmed.len() >= 24 {
        return true;
    }

    let long_and_mixed = trimmed.len() >= 32
        && trimmed.chars().any(|c| c.is_ascii_digit())
        && trimmed.chars().any(|c| c.is_ascii_lowercase())
        && trimmed.chars().any(|c| c.is_ascii_uppercase())
        && trimmed.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');

    long_and_mixed
}

pub fn mask_secrets(text: &str) -> (String, bool) {
    let mut masked = false;

    let cleaned: Vec<String> = text
        .split_whitespace()
        .map(|token| {
            if let Some((key, value)) = token.split_once('=') {
                if is_secretish(value) {
                    masked = true;
                    return format!("{key}={REDACTED}");
                }
            }

            if is_secretish(token) {
                masked = true;
                return REDACTED.to_owned();
            }

            token.to_owned()
        })
        .collect();

    (cleaned.join(" "), masked)
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct State {
    #[serde(default)]
    memories: BTreeMap<String, Memory>,
    #[serde(default)]
    next_number: u32,
}

pub struct MemoryStore {
    state: Mutex<State>,
    data_dir: PathBuf,
}

impl MemoryStore {
    pub fn new(data_dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&data_dir);
        let data_dir = fs::canonicalize(&data_dir).unwrap_or(data_dir);
        let state = fs::read_to_string(data_dir.join("memory.json"))
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();

        Self {
            state: Mutex::new(state),
            data_dir,
        }
    }

    fn persist(&self, state: &State) {
        if let Ok(raw) = serde_json::to_string_pretty(state) {
            let _ = fs::write(self.data_dir.join("memory.json"), raw);
        }
    }

    pub fn list(&self) -> Vec<Memory> {
        self.state.lock().memories.values().cloned().collect()
    }

    pub fn approved_for(&self, scope: Scope, scope_id: &str) -> Vec<Memory> {
        self.state
            .lock()
            .memories
            .values()
            .filter(|memory| memory.approved)
            .filter(|memory| {
                memory.scope == Scope::Workspace
                    || (memory.scope == scope && memory.scope_id == scope_id)
            })
            .cloned()
            .collect()
    }

    pub fn propose(&self, request: ProposeMemory) -> Result<Memory> {
        if request.text.trim().is_empty() {
            bail!("a memory needs text");
        }

        let (text, masked) = mask_secrets(&request.text);

        let mut state = self.state.lock();
        state.next_number += 1;
        let id = format!("m{}", state.next_number);

        let memory = Memory {
            id: id.clone(),
            text,
            scope: request.scope,
            scope_id: request.scope_id,
            proposed_by: request.proposed_by,
            approved: false,
            masked,
        };

        state.memories.insert(id, memory.clone());
        self.persist(&state);
        Ok(memory)
    }

    pub fn approve(&self, id: &str, approved: bool) -> Result<Memory> {
        let mut state = self.state.lock();
        let memory = state
            .memories
            .get_mut(id)
            .ok_or_else(|| anyhow!("unknown memory: {id}"))?;
        memory.approved = approved;
        let updated = memory.clone();
        self.persist(&state);
        Ok(updated)
    }

    pub fn forget(&self, id: &str) -> Result<()> {
        let mut state = self.state.lock();
        state
            .memories
            .remove(id)
            .ok_or_else(|| anyhow!("unknown memory: {id}"))?;
        self.persist(&state);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_the_credential_formats_that_actually_leak() {
        let cases = [
            "sk-ant-EXAMPLE-NOT-REAL",
            "ghp_EXAMPLE_NOT_REAL",
            "github_pat_EXAMPLE_NOT_REAL",
            "AKIA-EXAMPLE-NOT-REAL",
            "xoxb-EXAMPLE-NOT-REAL",
            "glpat-EXAMPLE-NOT-REAL",
            "AIza-EXAMPLE-NOT-REAL",
        ];

        for secret in cases {
            let (masked, changed) = mask_secrets(&format!("the key is {secret} keep it"));
            assert!(changed, "{secret} should be masked");
            assert!(!masked.contains(secret), "{secret} survived: {masked}");
            assert!(masked.contains("[redacted]"));
        }
    }

    #[test]
    fn masks_assignments_but_keeps_the_name() {
        let (masked, changed) =
            mask_secrets("run with ANTHROPIC_API_KEY=sk-ant-EXAMPLE-NOT-REAL");
        assert!(changed);
        assert!(masked.contains("ANTHROPIC_API_KEY=[redacted]"), "{masked}");
    }

    #[test]
    fn leaves_ordinary_prose_alone() {
        let text = "The auth service reads its config from config/auth.toml and retries twice.";
        let (masked, changed) = mask_secrets(text);
        assert!(!changed);
        assert_eq!(masked, text);
    }

    #[test]
    fn a_proposed_memory_is_not_usable_until_it_is_approved() {
        let dir = std::env::temp_dir().join("agentland-memory-test");
        let _ = fs::remove_dir_all(&dir);
        let store = MemoryStore::new(dir);

        let memory = store
            .propose(ProposeMemory {
                text: "The database migrations live in db/migrations.".to_owned(),
                scope: Scope::Repository,
                scope_id: "demo".to_owned(),
                proposed_by: "ada".to_owned(),
            })
            .expect("propose");

        assert!(!memory.approved);
        assert!(store.approved_for(Scope::Repository, "demo").is_empty());

        store.approve(&memory.id, true).expect("approve");
        assert_eq!(store.approved_for(Scope::Repository, "demo").len(), 1);
        assert!(store.approved_for(Scope::Repository, "other").is_empty());
    }
}
