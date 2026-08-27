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
    vectors: BTreeMap<String, Vec<f32>>,
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
        let state = crate::db::load_state(&data_dir, "memory");

        Self {
            state: Mutex::new(state),
            data_dir,
        }
    }

    fn persist(&self, state: &State) {
        crate::db::save_state(&self.data_dir, "memory", state);
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


    pub fn remember_vector(&self, id: &str, vector: Vec<f32>) {
        let mut state = self.state.lock();
        if state.memories.contains_key(id) {
            state.vectors.insert(id.to_owned(), vector);
            self.persist(&state);
        }
    }

    pub fn without_vectors(&self) -> Vec<Memory> {
        let state = self.state.lock();
        state
            .memories
            .values()
            .filter(|memory| memory.approved)
            .filter(|memory| !state.vectors.contains_key(&memory.id))
            .cloned()
            .collect()
    }

    pub fn recall(
        &self,
        scope: Scope,
        scope_id: &str,
        query: &str,
        query_vector: Option<&[f32]>,
        floor: f32,
        limit: usize,
    ) -> Vec<Recalled> {
        let state = self.state.lock();
        let words = tokens(query);

        let mut scored: Vec<Recalled> = state
            .memories
            .values()
            .filter(|memory| memory.approved)
            .filter(|memory| {
                memory.scope == Scope::Workspace
                    || (memory.scope == scope && memory.scope_id == scope_id)
            })
            .map(|memory| {
                let lexical = lexical_score(&words, &memory.text);
                let semantic = match (query_vector, state.vectors.get(&memory.id)) {
                    (Some(left), Some(right)) => crate::embed::cosine(left, right).max(0.0),
                    _ => 0.0,
                };

                Recalled {
                    memory: memory.clone(),
                    lexical,
                    semantic,
                    score: if semantic >= floor {
                        0.65 * lexical + 0.35 * semantic
                    } else {
                        lexical
                    },
                }
            })
            .collect();

        if words.is_empty() && query_vector.is_none() {
            scored.truncate(limit);
            return scored;
        }

        scored.retain(|entry| entry.lexical > 0.0 || entry.semantic >= floor);
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.memory.id.cmp(&b.memory.id))
        });
        scored.truncate(limit);
        scored
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

#[derive(Clone, Debug, Serialize)]
pub struct Recalled {
    pub memory: Memory,
    pub score: f32,
    pub lexical: f32,
    pub semantic: f32,
}

fn tokens(text: &str) -> Vec<String> {
    text.split(|character: char| !character.is_alphanumeric() && character != '_')
        .filter(|word| word.len() > 1)
        .map(|word| word.to_lowercase())
        .collect()
}

fn exact_looking(word: &str) -> bool {
    word.contains('_') || word.chars().any(|character| character.is_ascii_digit())
}

fn lexical_score(words: &[String], text: &str) -> f32 {
    if words.is_empty() {
        return 0.0;
    }

    let held = tokens(text);
    if held.is_empty() {
        return 0.0;
    }

    let mut hits = 0.0;
    let mut weight = 0.0;

    for word in words {
        let value = if exact_looking(word) { 2.0 } else { 1.0 };
        weight += value;
        if held.iter().any(|entry| entry == word) {
            hits += value;
        }
    }

    if weight == 0.0 {
        0.0
    } else {
        hits / weight
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

#[cfg(test)]
mod recall_tests {
    use super::*;

    fn store(name: &str) -> MemoryStore {
        let dir = std::env::temp_dir().join(format!("agentland-recall-{name}"));
        let _ = fs::remove_dir_all(&dir);
        MemoryStore::new(dir)
    }

    fn keep(store: &MemoryStore, text: &str) -> Memory {
        let memory = store
            .propose(ProposeMemory {
                text: text.to_owned(),
                scope: Scope::Workspace,
                scope_id: String::new(),
                proposed_by: "ada".to_owned(),
            })
            .expect("propose");
        store.approve(&memory.id, true).expect("approve")
    }

    #[test]
    fn an_exact_identifier_outranks_a_paraphrase() {
        let store = store("exact");
        let paraphrase = keep(&store, "the development server reads its port from the environment");
        let exact = keep(&store, "svc_demo reads PORT_4103 from the env");

        let found = store.recall(Scope::Workspace, "", "PORT_4103", None, 0.5, 5);
        assert_eq!(found[0].memory.id, exact.id, "{found:?}");
        assert!(found.iter().all(|entry| entry.memory.id != paraphrase.id));
    }

    #[test]
    fn a_memory_that_shares_no_word_is_left_out_rather_than_ranked_last() {
        let store = store("miss");
        keep(&store, "the reviewer prefers small commits");

        assert!(store.recall(Scope::Workspace, "", "port allocation", None, 0.5, 5).is_empty());
    }

    #[test]
    fn the_vector_only_breaks_ties_it_does_not_overturn_the_words() {
        let store = store("hybrid");
        let wordy = keep(&store, "the port probe scans 4100 upwards");
        let vectorish = keep(&store, "ports are chosen at worktree creation");

        store.remember_vector(&vectorish.id, vec![1.0, 0.0]);
        store.remember_vector(&wordy.id, vec![0.0, 1.0]);

        let query = [1.0, 0.0];
        let found = store.recall(Scope::Workspace, "", "port probe", Some(&query), 0.5, 5);

        assert_eq!(found[0].memory.id, wordy.id, "words win: {found:?}");
        assert!(found[0].semantic == 0.0 || found[0].semantic < found[0].lexical);
    }

    #[test]
    fn a_vector_lifts_a_memory_the_words_would_have_dropped() {
        let store = store("semantic");
        let sibling = keep(&store, "worktrees each get their own listener");
        store.remember_vector(&sibling.id, vec![1.0, 0.0]);

        let words_only = store.recall(Scope::Workspace, "", "port", None, 0.5, 5);
        assert!(words_only.is_empty(), "{words_only:?}");

        let query = [1.0, 0.0];
        let with_vector = store.recall(Scope::Workspace, "", "port", Some(&query), 0.5, 5);
        assert_eq!(with_vector.len(), 1);
        assert!(with_vector[0].semantic > 0.9);
    }

    #[test]
    fn recall_never_reaches_outside_its_scope() {
        let store = store("scope");
        let mine = store
            .propose(ProposeMemory {
                text: "the api repo pins node 20".to_owned(),
                scope: Scope::Repository,
                scope_id: "api".to_owned(),
                proposed_by: "ada".to_owned(),
            })
            .expect("propose");
        store.approve(&mine.id, true).expect("approve");

        assert_eq!(store.recall(Scope::Repository, "api", "node", None, 0.5, 5).len(), 1);
        assert!(store.recall(Scope::Repository, "web", "node", None, 0.5, 5).is_empty());
    }

    #[test]
    fn an_unapproved_memory_is_never_recalled() {
        let store = store("gate");
        store
            .propose(ProposeMemory {
                text: "the deploy key lives in the vault".to_owned(),
                scope: Scope::Workspace,
                scope_id: String::new(),
                proposed_by: "ada".to_owned(),
            })
            .expect("propose");

        assert!(store.recall(Scope::Workspace, "", "deploy key", None, 0.5, 5).is_empty());
    }

    #[test]
    fn the_store_can_say_which_memories_still_need_a_vector() {
        let store = store("pending");
        let first = keep(&store, "one");
        let second = keep(&store, "two");
        store.remember_vector(&first.id, vec![0.1, 0.2]);

        let pending: Vec<String> = store.without_vectors().into_iter().map(|m| m.id).collect();
        assert_eq!(pending, vec![second.id]);
    }

    #[test]
    fn an_unapproved_memory_is_never_sent_to_the_embedder() {
        let store = store("gate-embed");
        store
            .propose(ProposeMemory {
                text: "the deploy key lives in the vault".to_owned(),
                scope: Scope::Workspace,
                scope_id: String::new(),
                proposed_by: "ada".to_owned(),
            })
            .expect("propose");

        assert!(
            store.without_vectors().is_empty(),
            "the approval gate holds for the embedder too"
        );
    }
}

#[cfg(test)]
mod floor_tests {
    use super::*;

    fn store(name: &str) -> MemoryStore {
        let dir = std::env::temp_dir().join(format!("agentland-floor-{name}"));
        let _ = fs::remove_dir_all(&dir);
        MemoryStore::new(dir)
    }

    fn keep(store: &MemoryStore, text: &str) -> Memory {
        let memory = store
            .propose(ProposeMemory {
                text: text.to_owned(),
                scope: Scope::Workspace,
                scope_id: String::new(),
                proposed_by: "ada".to_owned(),
            })
            .expect("propose");
        store.approve(&memory.id, true).expect("approve")
    }

    #[test]
    fn a_faint_vector_does_not_earn_a_place_in_the_brief() {
        let store = store("faint");
        let unrelated = keep(&store, "the reviewer prefers small commits");
        store.remember_vector(&unrelated.id, vec![0.2, 0.98]);

        let query = [1.0, 0.0];
        let strict = store.recall(Scope::Workspace, "", "port", Some(&query), 0.5, 5);
        assert!(strict.is_empty(), "a weak neighbour is not a memory: {strict:?}");

        let loose = store.recall(Scope::Workspace, "", "port", Some(&query), 0.1, 5);
        assert_eq!(loose.len(), 1, "lowering the floor lets it back in");
    }

    #[test]
    fn a_word_match_never_needs_to_clear_the_floor() {
        let store = store("words");
        let named = keep(&store, "the port probe scans upwards from 4100");
        store.remember_vector(&named.id, vec![0.0, 1.0]);

        let query = [1.0, 0.0];
        let found = store.recall(Scope::Workspace, "", "port probe", Some(&query), 0.9, 5);
        assert_eq!(found.len(), 1, "the words are enough on their own");
        assert_eq!(found[0].memory.id, named.id);
        assert_eq!(found[0].score, found[0].lexical, "a vector below the floor is ignored");
    }
}
