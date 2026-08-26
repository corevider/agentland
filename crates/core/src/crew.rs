use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::pty::{PtyManager, PtySpawnSpec};

#[derive(Clone, Debug, Serialize)]
pub struct Engine {
    pub id: &'static str,
    pub name: &'static str,
    pub command: &'static str,
    pub resume_flag: Option<&'static str>,
    pub prompt_style: PromptStyle,
    pub installed: bool,
    pub version: Option<String>,
}

const CATALOG: &[(&str, &str, &str, Option<&str>, PromptStyle)] = &[
    ("claude", "Claude Code", "claude", Some("--continue"), PromptStyle::Positional),
    ("codex", "Codex CLI", "codex", Some("resume"), PromptStyle::Positional),
    ("gemini", "Gemini CLI", "gemini", None, PromptStyle::Flag("-p")),
    ("opencode", "OpenCode", "opencode", None, PromptStyle::Positional),
    ("crush", "Crush", "crush", None, PromptStyle::Positional),
    ("goose", "Goose", "goose", Some("--resume"), PromptStyle::None),
    ("qwen", "Qwen Code", "qwen", None, PromptStyle::Positional),
    ("cursor-agent", "Cursor Agent", "cursor-agent", None, PromptStyle::Positional),
];

#[derive(Clone, Copy, Debug, Serialize)]
pub enum PromptStyle {
    Positional,
    Flag(&'static str),
    None,
}

pub fn engines() -> Vec<Engine> {
    CATALOG
        .iter()
        .map(|(id, name, command, resume_flag, prompt_style)| {
            let version = Command::new(command)
                .arg("--version")
                .output()
                .ok()
                .filter(|output| output.status.success())
                .map(|output| {
                    String::from_utf8_lossy(&output.stdout)
                        .lines()
                        .next()
                        .unwrap_or_default()
                        .trim()
                        .to_owned()
                });

            Engine {
                id,
                name,
                command,
                resume_flag: *resume_flag,
                prompt_style: *prompt_style,
                installed: version.is_some(),
                version,
            }
        })
        .collect()
}

fn engine(id: &str) -> Option<Engine> {
    engines().into_iter().find(|entry| entry.id == id)
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentState {
    Idle,
    Working,
    Offline,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub role: String,
    pub engine_id: String,
    pub repository_id: String,
    pub worktree: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default = "offline")]
    pub state: AgentState,
}

fn offline() -> AgentState {
    AgentState::Offline
}

#[derive(Clone, Debug, Deserialize)]
pub struct HireRequest {
    pub name: String,
    #[serde(default = "default_role")]
    pub role: String,
    pub engine_id: String,
    pub repository_id: String,
    pub worktree: String,
}

fn default_role() -> String {
    "implementer".to_owned()
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct State {
    #[serde(default)]
    agents: BTreeMap<String, Agent>,
}

pub struct Crew {
    manager: Arc<PtyManager>,
    state: Mutex<State>,
    data_dir: PathBuf,
    endpoint: Mutex<Option<(u16, String)>>,
}

fn slugify(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_owned()
}

impl Crew {
    pub fn new(manager: Arc<PtyManager>, data_dir: PathBuf) -> Arc<Self> {
        let _ = fs::create_dir_all(&data_dir);
        let data_dir = fs::canonicalize(&data_dir).unwrap_or(data_dir);
        let state = fs::read_to_string(data_dir.join("crew.json"))
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();

        Arc::new(Self {
            manager,
            state: Mutex::new(state),
            data_dir,
            endpoint: Mutex::new(None),
        })
    }

    pub fn set_endpoint(&self, port: u16, token: String) {
        *self.endpoint.lock() = Some((port, token));
    }

    fn persist(&self, state: &State) {
        if let Ok(raw) = serde_json::to_string_pretty(state) {
            let _ = fs::write(self.data_dir.join("crew.json"), raw);
        }
    }

    pub fn list(&self) -> Vec<Agent> {
        self.state.lock().agents.values().cloned().collect()
    }

    pub fn hire(&self, request: HireRequest) -> Result<Agent> {
        let engine = engine(&request.engine_id)
            .ok_or_else(|| anyhow!("unknown engine: {}", request.engine_id))?;
        if !engine.installed {
            bail!("{} is not on PATH", engine.command);
        }

        let id = slugify(&request.name);
        if id.is_empty() {
            bail!("name must contain letters or digits");
        }

        let mut state = self.state.lock();
        if state.agents.contains_key(&id) {
            bail!("an agent named {id} already exists");
        }

        let agent = Agent {
            id: id.clone(),
            name: request.name,
            role: request.role,
            engine_id: request.engine_id,
            repository_id: request.repository_id,
            worktree: request.worktree,
            session_id: None,
            state: AgentState::Idle,
        };

        state.agents.insert(id, agent.clone());
        self.persist(&state);

        Ok(agent)
    }

    pub fn start(
        &self,
        id: &str,
        worktree_path: &Path,
        resume: bool,
        brief: Option<&str>,
    ) -> Result<Agent> {
        let agent = self
            .state
            .lock()
            .agents
            .get(id)
            .cloned()
            .ok_or_else(|| anyhow!("unknown agent: {id}"))?;

        if let Some(session_id) = &agent.session_id {
            if self.manager.get(session_id).is_some() {
                bail!("{id} is already running");
            }
        }

        let engine = engine(&agent.engine_id)
            .ok_or_else(|| anyhow!("unknown engine: {}", agent.engine_id))?;
        if !engine.installed {
            bail!("{} is not on PATH", engine.command);
        }

        let mut args = Vec::new();
        if resume {
            if let Some(flag) = engine.resume_flag {
                args.push(flag.to_owned());
            }
        }

        if let Some(text) = brief.filter(|value| !value.trim().is_empty()) {
            match engine.prompt_style {
                PromptStyle::Positional => args.push(text.to_owned()),
                PromptStyle::Flag(flag) => {
                    args.push(flag.to_owned());
                    args.push(text.to_owned());
                }
                PromptStyle::None => {}
            }
        }

        let mut env = BTreeMap::new();
        env.insert("AGENTLAND_AGENT".to_owned(), agent.id.clone());
        env.insert("AGENTLAND_ROLE".to_owned(), agent.role.clone());

        if let Some((port, token)) = self.endpoint.lock().clone() {
            env.insert("AGENTLAND_PORT".to_owned(), port.to_string());
            env.insert("AGENTLAND_TOKEN".to_owned(), token);
        }

        let session = self.manager.spawn(PtySpawnSpec {
            command: engine.command.to_owned(),
            args,
            cwd: Some(worktree_path.to_string_lossy().to_string()),
            env,
            cols: 120,
            rows: 32,
        })?;

        let mut state = self.state.lock();
        let stored = state
            .agents
            .get_mut(id)
            .ok_or_else(|| anyhow!("unknown agent: {id}"))?;
        stored.session_id = Some(session.id.clone());
        stored.state = AgentState::Working;
        let updated = stored.clone();
        self.persist(&state);

        Ok(updated)
    }

    pub fn stop(&self, id: &str) -> Result<()> {
        let mut state = self.state.lock();
        let agent = state
            .agents
            .get_mut(id)
            .ok_or_else(|| anyhow!("unknown agent: {id}"))?;

        if let Some(session_id) = agent.session_id.take() {
            let _ = self.manager.remove(&session_id);
        }
        agent.state = AgentState::Idle;
        self.persist(&state);

        Ok(())
    }

    pub fn dismiss(&self, id: &str) -> Result<()> {
        let _ = self.stop(id);
        let mut state = self.state.lock();
        state
            .agents
            .remove(id)
            .ok_or_else(|| anyhow!("unknown agent: {id}"))?;
        self.persist(&state);
        Ok(())
    }
}
