use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{anyhow, Result};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Full,
    Approve,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ScopedToken {
    pub id: String,
    pub label: String,
    pub scope: Scope,
    token: String,
}

impl ScopedToken {
    pub fn secret(&self) -> &str {
        &self.token
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct TokenSummary {
    pub id: String,
    pub label: String,
    pub scope: Scope,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct State {
    #[serde(default)]
    tokens: BTreeMap<String, ScopedToken>,
    #[serde(default)]
    next_number: u32,
}

pub struct TokenStore {
    primary: String,
    state: Mutex<State>,
    data_dir: PathBuf,
}

pub fn permits(scope: Scope, method: &str, path: &str) -> bool {
    if scope == Scope::Full {
        return true;
    }

    match method {
        "GET" => matches!(
            path,
            "/agents"
                | "/tasks"
                | "/approvals"
                | "/memories"
                | "/dispatch"
                | "/routines"
                | "/skills"
        ),
        "POST" => {
            path.starts_with("/approvals/")
                || (path.starts_with("/memories/") && path.ends_with("/approve"))
        }
        _ => false,
    }
}

impl TokenStore {
    pub fn new(primary: String, data_dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&data_dir);
        let data_dir = fs::canonicalize(&data_dir).unwrap_or(data_dir);
        let state = crate::db::load_state(&data_dir, "tokens");

        Self {
            primary,
            state: Mutex::new(state),
            data_dir,
        }
    }

    fn persist(&self, state: &State) {
        crate::db::save_state(&self.data_dir, "tokens", state);
    }

    pub fn resolve(&self, presented: &str) -> Option<Scope> {
        if presented.is_empty() {
            return None;
        }

        if presented == self.primary {
            return Some(Scope::Full);
        }

        self.state
            .lock()
            .tokens
            .values()
            .find(|entry| entry.token == presented)
            .map(|entry| entry.scope)
    }

    pub fn list(&self) -> Vec<TokenSummary> {
        self.state
            .lock()
            .tokens
            .values()
            .map(|entry| TokenSummary {
                id: entry.id.clone(),
                label: entry.label.clone(),
                scope: entry.scope,
            })
            .collect()
    }

    pub fn issue(&self, label: String, scope: Scope) -> ScopedToken {
        let mut state = self.state.lock();
        state.next_number += 1;

        let token = ScopedToken {
            id: format!("device{}", state.next_number),
            label,
            scope,
            token: crate::generate_token(),
        };

        state.tokens.insert(token.id.clone(), token.clone());
        self.persist(&state);
        token
    }

    pub fn revoke(&self, id: &str) -> Result<()> {
        let mut state = self.state.lock();
        state
            .tokens
            .remove(id)
            .ok_or_else(|| anyhow!("unknown device: {id}"))?;
        self.persist(&state);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_approval_token_cannot_reach_a_shell() {
        for path in ["/sessions", "/repos", "/integrations/call", "/dispatch/tasks/t1"] {
            assert!(
                !permits(Scope::Approve, "POST", path),
                "{path} must be closed to an approval token"
            );
        }

        assert!(!permits(Scope::Approve, "DELETE", "/agents/ada"));
        assert!(!permits(Scope::Approve, "GET", "/sessions"));
    }

    #[test]
    fn an_approval_token_can_read_and_answer() {
        assert!(permits(Scope::Approve, "GET", "/agents"));
        assert!(permits(Scope::Approve, "GET", "/approvals"));
        assert!(permits(Scope::Approve, "POST", "/approvals/a1"));
        assert!(permits(Scope::Approve, "POST", "/memories/m1/approve"));
    }

    #[test]
    fn an_approval_token_can_read_the_skills_library() {
        assert!(permits(Scope::Approve, "GET", "/skills"));
        assert!(!permits(Scope::Approve, "POST", "/skills"));
        assert!(!permits(Scope::Approve, "GET", "/skills/tdd"));
    }

    #[test]
    fn a_full_token_is_unrestricted() {
        assert!(permits(Scope::Full, "POST", "/sessions"));
        assert!(permits(Scope::Full, "DELETE", "/repos/demo/worktrees/work1"));
    }

    #[test]
    fn revoking_a_device_ends_its_access() {
        let dir = std::env::temp_dir().join("agentland-auth-test");
        let _ = fs::remove_dir_all(&dir);
        let store = TokenStore::new("primary".to_owned(), dir);

        let device = store.issue("phone".to_owned(), Scope::Approve);
        assert_eq!(store.resolve(device.secret()), Some(Scope::Approve));
        assert_eq!(store.resolve("primary"), Some(Scope::Full));

        store.revoke(&device.id).expect("revoke");
        assert_eq!(store.resolve(device.secret()), None);
    }
}
