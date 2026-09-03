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
    /// How this engine is told which model to run. Left out where we do not
    /// know the flag rather than guessed at.
    pub model_flag: Option<&'static str>,
    /// How this engine is told how much to do without asking. Only Claude Code's
    /// is known first-hand; the rest are left out rather than invented.
    pub permission_flag: Option<&'static str>,
    /// How this engine is handed the crew's own tools. Passing the file is the
    /// only way that holds: leaving the engine to discover `.mcp.json` in the
    /// worktree makes the tools depend on an approval nobody sees, which is
    /// silently lost whenever the file changes — measured, twice, as a whole
    /// commander session that looked healthy and could not call a single tool.
    pub mcp_flags: &'static [&'static str],
    pub prompt_style: PromptStyle,
    pub installed: bool,
    pub version: Option<String>,
}

/// `--strict-mcp-config` goes with `--mcp-config`: an agent's tools are then
/// exactly the ones Agentland gave it, and the human's own connectors — mail,
/// calendar, drive — stay out of a crew pane entirely.
const CLAUDE_MCP: &[&str] = &["--mcp-config", "--strict-mcp-config"];

const CATALOG: &[(&str, &str, &str, Option<&str>, Option<&str>, Option<&str>, &[&str], PromptStyle)] = &[
    ("claude", "Claude Code", "claude", Some("--continue"), Some("--model"), Some("--permission-mode"), CLAUDE_MCP, PromptStyle::Positional),
    ("codex", "Codex CLI", "codex", Some("resume"), Some("--model"), None, &[], PromptStyle::Positional),
    ("gemini", "Gemini CLI", "gemini", None, Some("-m"), None, &[], PromptStyle::Flag("-p")),
    ("opencode", "OpenCode", "opencode", None, Some("--model"), None, &[], PromptStyle::Positional),
    ("crush", "Crush", "crush", None, None, None, &[], PromptStyle::Positional),
    ("goose", "Goose", "goose", Some("--resume"), None, None, &[], PromptStyle::None),
    ("qwen", "Qwen Code", "qwen", None, Some("-m"), None, &[], PromptStyle::Positional),
    ("cursor-agent", "Cursor Agent", "cursor-agent", None, Some("--model"), None, &[], PromptStyle::Positional),
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
        .map(|(id, name, command, resume_flag, model_flag, permission_flag, mcp_flags, prompt_style)| {
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
                mcp_flags,
                model_flag: *model_flag,
                permission_flag: *permission_flag,
                prompt_style: *prompt_style,
                installed: version.is_some(),
                version,
            }
        })
        .collect()
}

/// How an engine is handed a settings file.
///
/// Only Claude Code's is known first-hand, so the rest are left out rather than
/// invented — an engine handed a flag it does not have refuses to start, and a
/// crew that cannot start is worse than one that asks too often.
pub fn settings_flag(engine_id: &str) -> Option<&'static str> {
    match engine_id {
        "claude" => Some("--settings"),
        _ => None,
    }
}

/// How an engine is handed a standing instruction — the house rules — for
/// every turn rather than once in a brief.
///
/// A file, not an argument: a page of rules on a command line is a page of
/// rules in every process listing. Only Claude Code's flag is known first-hand;
/// the rest are told at the top of their brief instead.
pub fn standing_flag(engine_id: &str) -> Option<&'static str> {
    match engine_id {
        "claude" => Some("--append-system-prompt-file"),
        _ => None,
    }
}

fn engine(id: &str) -> Option<Engine> {
    engines().into_iter().find(|entry| entry.id == id)
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentState {
    Idle,
    Working,
    Done,
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
    /// What the commander decided about this agent: which model it runs on, what
    /// its pane is called, and the colour it is known by. None means nobody has
    /// decided, and the engine's own default stands.
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub colour: Option<String>,
    /// How much this agent may do without asking. None means the role's default.
    #[serde(default)]
    pub permissions: Option<String>,
    /// Which login on this engine it spends from, when there is more than one.
    ///
    /// Nothing can read this off a pane — a status line says how much of the
    /// week is gone and never whose week — so it is a person's to say, and it
    /// only matters when they have more than one subscription to the same
    /// engine.
    #[serde(default)]
    pub account: Option<String>,
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
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub colour: Option<String>,
    #[serde(default)]
    pub permissions: Option<String>,
    #[serde(default)]
    pub account: Option<String>,
}

/// A change to how an agent is set up. A field left out is left alone; a field
/// set to an empty string is cleared back to the engine's own default.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Shaping {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub colour: Option<String>,
    #[serde(default)]
    pub permissions: Option<String>,
    /// Set by the core when a human has approved this exact raise; never by the
    /// commander, which is why it is not in the tool it calls.
    #[serde(default, skip)]
    pub approved_raise: bool,
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
    /// What a person has already said each project may run without asking.
    /// Handed in rather than read here: the crew starts panes, it does not own
    /// the record of what somebody agreed to.
    learned: Mutex<BTreeMap<String, Vec<String>>>,
    /// The house rules on disk, handed to every engine that can take them.
    standing: Mutex<Option<std::path::PathBuf>>,
    manager: Arc<PtyManager>,
    state: Mutex<State>,
    data_dir: PathBuf,
    endpoint: Mutex<Option<(u16, String)>>,
    /// Who was mid-work when the app last went down. Read once, at startup.
    interrupted: Mutex<Vec<String>>,
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
        let state = crate::db::load_state(&data_dir, "crew");

        let crew = Arc::new(Self {
            learned: Mutex::new(BTreeMap::new()),
            standing: Mutex::new(None),
            manager,
            state: Mutex::new(state),
            data_dir,
            endpoint: Mutex::new(None),
            interrupted: Mutex::new(Vec::new()),
        });

        crew.reconcile();
        crew
    }

    fn reconcile(&self) {
        let mut state = self.state.lock();
        let mut changed = false;

        for agent in state.agents.values_mut() {
            let alive = agent
                .session_id
                .as_ref()
                .map(|id| self.manager.get(id).is_some())
                .unwrap_or(false);

            if !alive && (agent.session_id.is_some() || agent.state == AgentState::Working) {
                // Its pane died with the app rather than because the work
                // finished, so remember it: the crew comes back on its own, and
                // a person should not have to restart each agent by hand. Having
                // a pane is the condition, not being mid-turn — an agent resting
                // at its prompt between steps is exactly the one whose context is
                // worth keeping, and stopping an agent clears its pane, so anyone
                // stopped by hand stays stopped.
                if agent.session_id.is_some() {
                    self.interrupted.lock().push(agent.id.clone());
                }

                agent.session_id = None;
                agent.state = AgentState::Done;
                changed = true;
            }
        }

        if changed {
            self.persist(&state);
        }
    }

    /// Who was working when the app went down, taken once.
    ///
    /// Taken rather than read: bringing the crew back is a startup act, and a
    /// second caller asking again would start the same panes twice.
    pub fn take_the_interrupted(&self) -> Vec<Agent> {
        let names: Vec<String> = self.interrupted.lock().drain(..).collect();
        let state = self.state.lock();

        names
            .into_iter()
            .filter_map(|id| state.agents.get(&id).cloned())
            .collect()
    }

    /// Tell the crew what has been agreed, so the next pane starts with it.
    pub fn set_standing(&self, file: Option<std::path::PathBuf>) {
        *self.standing.lock() = file;
    }

    pub fn set_learned(&self, learned: BTreeMap<String, Vec<String>>) {
        *self.learned.lock() = learned;
    }

    pub fn set_endpoint(&self, port: u16, token: String) {
        *self.endpoint.lock() = Some((port, token));
    }

    fn persist(&self, state: &State) {
        crate::db::save_state(&self.data_dir, "crew", state);
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

        let wanted = slugify(&request.name);
        if wanted.is_empty() {
            bail!("name must contain letters or digits");
        }

        let mut state = self.state.lock();

        // The same name in another project is another agent, not a clash. Every
        // project has an X commanding it, and a crew where the second one had to
        // be called X2 is a crew where the name stops meaning the job. The id
        // stays unique by carrying the project; the name a person reads does not.
        let id = if state.agents.contains_key(&wanted) {
            let held = state
                .agents
                .get(&wanted)
                .map(|agent| agent.repository_id.clone())
                .unwrap_or_default();

            if held == request.repository_id {
                bail!(
                    "{} already has an agent called {}",
                    request.repository_id,
                    request.name
                );
            }

            format!("{wanted}-{}", slugify(&request.repository_id))
        } else {
            wanted
        };

        if state.agents.contains_key(&id) {
            bail!("an agent named {id} already exists");
        }

        let model = request
            .model
            .or_else(|| model_for_role(&request.role).map(str::to_owned));

        let taken: Vec<String> = state
            .agents
            .values()
            .filter_map(|held| held.colour.clone())
            .collect();
        let colour = request
            .colour
            .filter(|value| !value.trim().is_empty())
            .or_else(|| Some(free_colour(&taken).to_owned()));

        let agent = Agent {
            id: id.clone(),
            name: request.name,
            role: request.role,
            engine_id: request.engine_id,
            repository_id: request.repository_id,
            worktree: request.worktree,
            session_id: None,
            state: AgentState::Idle,
            model,
            title: request.title,
            colour,
            permissions: request.permissions,
            account: request.account,
        };

        state.agents.insert(id, agent.clone());
        self.persist(&state);

        Ok(agent)
    }

    /// What the commander decided about an agent. Only the fields it names change;
    /// the rest keep whatever they had, so one decision at a time is possible.
    pub fn shape(&self, id: &str, wanted: Shaping) -> Result<Agent> {
        let mut state = self.state.lock();
        let agent = state
            .agents
            .get_mut(id)
            .ok_or_else(|| anyhow!("unknown agent: {id}"))?;

        if let Some(model) = wanted.model {
            let trimmed = model.trim().to_owned();
            agent.model = (!trimmed.is_empty()).then_some(trimmed);
        }

        if let Some(title) = wanted.title {
            let trimmed = title.trim().to_owned();
            agent.title = (!trimmed.is_empty()).then_some(trimmed);
        }

        if let Some(colour) = wanted.colour {
            let trimmed = colour.trim().to_owned();
            agent.colour = (!trimmed.is_empty()).then_some(trimmed);
        }

        if let Some(mode) = wanted.permissions {
            let trimmed = mode.trim().to_owned();

            if trimmed.is_empty() {
                agent.permissions = None;
            } else {
                let held = agent
                    .permissions
                    .clone()
                    .unwrap_or_else(|| permission_for_role(&agent.role).to_owned());

                // Lowering is the commander's to make. Raising hands out more
                // rope on someone else's machine, so it is not.
                if is_a_raise(Some(&held), &trimmed) && !wanted.approved_raise {
                    bail!(
                        "raising {} from {held} to {trimmed} needs the human's approval — ask with request_approval and shape it again once they say yes",
                        agent.id
                    );
                }

                if permission_rung(&trimmed).is_none() {
                    bail!("no such permission mode: {trimmed}");
                }

                agent.permissions = Some(trimmed);
            }
        }

        let shaped = agent.clone();
        self.persist(&state);
        Ok(shaped)
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

        // A pane opened at a folder that is not there does not fail: it opens in
        // the home folder instead. That put an agent running with permissions
        // accepted at the top of somebody's home directory, asking whether it
        // could be trusted with all of it. Refusing here says what is actually
        // wrong — the worktree it was hired into is gone.
        if !worktree_path.is_dir() {
            bail!(
                "{id} was hired into {} and that worktree is gone — recreate it or dismiss the agent",
                worktree_path.display()
            );
        }

        let engine = engine(&agent.engine_id)
            .ok_or_else(|| anyhow!("unknown engine: {}", agent.engine_id))?;
        if !engine.installed {
            bail!("{} is not on PATH", engine.command);
        }

        let mut args = Vec::new();

        // The crew's own tools, handed over rather than discovered.
        let tools = worktree_path.join(".mcp.json");
        if tools.exists() {
            for flag in engine.mcp_flags {
                args.push((*flag).to_owned());
                if *flag == "--mcp-config" {
                    args.push(tools.to_string_lossy().into_owned());
                }
            }
        }

        // What this role may run without stopping to ask. Written into
        // Agentland's own folder and pointed at, rather than into the worktree:
        // the worktree is a checkout of somebody's repository, and a settings
        // file left in it is a file they did not write showing up in their diff.
        if let Some(flag) = settings_flag(&agent.engine_id) {
            let folder = self.data_dir.join("permits");

            // Per role and per project: what a role may do, plus what this
            // project says about how it is tested. `bash tests/run.sh` is a
            // rule only ccdo gets, because only ccdo keeps that file.
            let mut declared = crate::permits::declared_in(worktree_path);
            declared.extend(self.learned.lock().get(&agent.repository_id).cloned().unwrap_or_default());
            let file = folder.join(format!(
                "{}-{}.json",
                slugify(&agent.role),
                slugify(&agent.repository_id)
            ));

            if fs::create_dir_all(&folder).is_ok()
                && fs::write(&file, crate::permits::settings_for(&agent.role, &declared)).is_ok()
            {
                args.push((*flag).to_owned());
                args.push(file.to_string_lossy().into_owned());
            }
        }

        // The house rules, for every turn rather than once at the start of one.
        if let Some(flag) = standing_flag(&agent.engine_id) {
            if let Some(file) = self.standing.lock().clone() {
                if file.is_file() {
                    args.push((*flag).to_owned());
                    args.push(file.to_string_lossy().into_owned());
                }
            }
        }

        if let Some(flag) = engine.permission_flag {
            let mode = agent
                .permissions
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| permission_for_role(&agent.role));

            args.push(flag.to_owned());
            args.push(mode.to_owned());
        }

        if let (Some(flag), Some(model)) = (engine.model_flag, agent.model.as_deref()) {
            let wanted = model.trim();
            if !wanted.is_empty() {
                args.push(flag.to_owned());
                args.push(wanted.to_owned());
            }
        }

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

    /// Say whether an agent is mid-turn, from what its pane is actually doing.
    ///
    /// Starting an agent used to mean "working" until someone stopped it, so an
    /// agent that finished its step and sat at a prompt still held a slot in the
    /// engine's concurrency cap. Measured on the /version plan: four agents were
    /// "working" with nobody working, the dispatcher queued everything, and the
    /// commander could only ask a person to close panes by hand.
    pub fn mark_busy(&self, id: &str, busy: bool) -> bool {
        let mut state = self.state.lock();
        let Some(agent) = state.agents.get_mut(id) else {
            return false;
        };

        if agent.session_id.is_none() {
            return false;
        }

        let wanted = if busy { AgentState::Working } else { AgentState::Idle };
        if agent.state == wanted {
            return false;
        }

        agent.state = wanted;
        self.persist(&state);
        true
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

/// How much an agent may do without asking.
///
/// These are the engine's own words (Claude Code's `--permission-mode`), ordered
/// by how much rope they hand out. Ordering them is the point: the commander may
/// lower an agent whenever it likes, and raising one is a decision with a cost,
/// so it goes to the human.
pub const PERMISSION_LADDER: &[&str] = &["plan", "default", "acceptEdits", "bypassPermissions"];

pub fn permission_rung(mode: &str) -> Option<usize> {
    PERMISSION_LADDER.iter().position(|known| *known == mode)
}

/// Whether changing from one mode to another hands out more rope.
pub fn is_a_raise(from: Option<&str>, to: &str) -> bool {
    let Some(wanted) = permission_rung(to) else {
        return true; // a mode nobody knows is treated as the most dangerous one
    };

    match from.and_then(permission_rung) {
        Some(held) => wanted > held,
        None => wanted > permission_rung(DEFAULT_PERMISSION).unwrap_or(1),
    }
}

pub const DEFAULT_PERMISSION: &str = "default";

/// What a role may do without asking, before anyone decides otherwise.
///
/// Nothing is born with `bypassPermissions`: an agent that never asks is a
/// decision, not a default. A reviewer reads and reports, so it plans and does
/// not edit; an implementer works inside its own worktree, so it edits and asks
/// before it runs anything.
pub fn permission_for_role(role: &str) -> &'static str {
    match role {
        "reviewer" => "plan",
        "implementer" | "ops" | "commander" => "acceptEdits",
        _ => DEFAULT_PERMISSION,
    }
}

/// The colours a crew is known by.
///
/// Chosen to stay apart at a glance and on the island's green: a human learns
/// six people by colour long before they learn six names. The commander picks
/// from here rather than inventing hexes, so two agents never arrive nearly the
/// same shade.
pub const PALETTE: &[&str] = &[
    "#e0c05a", // sand gold
    "#7fb8c4", // shallow water
    "#c98b6b", // terracotta
    "#8fbf7d", // palm
    "#b48ec9", // dusk
    "#e5705f", // coral
    "#6fa8a0", // lagoon
    "#d0d0c0", // driftwood
];

/// The next colour nobody is wearing, so a new hire is told apart from the crew
/// it joins. When every colour is taken the palette starts again — eight agents
/// on one island is already more than a person can watch.
pub fn free_colour(taken: &[String]) -> &'static str {
    PALETTE
        .iter()
        .find(|colour| !taken.iter().any(|held| held.eq_ignore_ascii_case(colour)))
        .copied()
        .unwrap_or(PALETTE[0])
}

/// What a role should run on when nobody has decided.
///
/// The commander reads the whole board, writes the plan and judges the evidence;
/// that is the work worth the strongest model. An implementer works inside one
/// worktree against a brief someone else wrote, and a smaller model does that at
/// a fraction of the cost. These are only defaults — the commander overrules them
/// agent by agent, which is the point of having one.
pub fn model_for_role(role: &str) -> Option<&'static str> {
    match role {
        "commander" => Some("opus"),
        "reviewer" | "ops" => Some("sonnet"),
        "implementer" => Some("haiku"),
        _ => None,
    }
}

#[cfg(test)]
mod model_tests {
    use super::{engine, free_colour, model_for_role, PALETTE};

    #[test]
    fn claude_is_handed_the_crews_tools_rather_than_left_to_find_them() {
        let claude = engine("claude").expect("claude is in the catalog");

        assert_eq!(claude.mcp_flags, &["--mcp-config", "--strict-mcp-config"]);
    }

    #[test]
    fn an_engine_we_have_not_learned_is_left_to_its_own_tools() {
        let codex = engine("codex").expect("codex is in the catalog");

        assert!(codex.mcp_flags.is_empty(), "nothing is guessed at");
    }

    #[test]
    fn a_role_is_born_with_the_rope_its_work_needs_and_no_more() {
        use super::{permission_for_role, DEFAULT_PERMISSION};

        assert_eq!(permission_for_role("reviewer"), "plan", "a reviewer reads");
        assert_eq!(permission_for_role("implementer"), "acceptEdits");
        assert_eq!(permission_for_role("gardener"), DEFAULT_PERMISSION);

        for role in ["commander", "implementer", "reviewer", "ops", "gardener"] {
            assert_ne!(
                permission_for_role(role),
                "bypassPermissions",
                "nothing is born never asking",
            );
        }
    }

    #[test]
    fn lowering_is_free_and_raising_is_not() {
        use super::is_a_raise;

        assert!(is_a_raise(Some("plan"), "acceptEdits"));
        assert!(is_a_raise(Some("acceptEdits"), "bypassPermissions"));
        assert!(!is_a_raise(Some("bypassPermissions"), "plan"));
        assert!(!is_a_raise(Some("acceptEdits"), "acceptEdits"));
    }

    #[test]
    fn a_mode_nobody_knows_is_treated_as_the_dangerous_one() {
        use super::is_a_raise;

        assert!(is_a_raise(Some("bypassPermissions"), "yolo"));
        assert!(is_a_raise(None, "yolo"));
    }

    #[test]
    fn a_new_hire_takes_a_colour_nobody_is_wearing() {
        let taken = vec![PALETTE[0].to_owned(), PALETTE[1].to_owned()];
        assert_eq!(free_colour(&taken), PALETTE[2]);
    }

    #[test]
    fn a_colour_written_differently_still_counts_as_taken() {
        let taken = vec![PALETTE[0].to_uppercase()];
        assert_ne!(free_colour(&taken), PALETTE[0]);
    }

    #[test]
    fn a_crew_larger_than_the_palette_starts_it_again_rather_than_going_blank() {
        let everything: Vec<String> = PALETTE.iter().map(|colour| (*colour).to_owned()).collect();
        assert_eq!(free_colour(&everything), PALETTE[0]);
    }

    #[test]
    fn the_commander_gets_the_strongest_model_and_the_others_do_not() {
        assert_eq!(model_for_role("commander"), Some("opus"));
        assert_ne!(model_for_role("implementer"), model_for_role("commander"));
        assert_eq!(model_for_role("implementer"), Some("haiku"));
    }

    #[test]
    fn a_role_nobody_has_an_opinion_about_keeps_the_engine_default() {
        assert_eq!(model_for_role("gardener"), None);
    }

    #[test]
    fn the_engines_we_know_how_to_tell_carry_the_flag() {
        assert_eq!(engine("claude").unwrap().model_flag, Some("--model"));
        assert_eq!(engine("gemini").unwrap().model_flag, Some("-m"));
        // Crush is not guessed at.
        assert_eq!(engine("crush").unwrap().model_flag, None);
    }
}

#[cfg(test)]
mod pane_tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("agentland-crew-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn an_x(repository: &str) -> HireRequest {
        HireRequest {
            name: "X".into(),
            role: "commander".into(),
            engine_id: "claude".into(),
            repository_id: repository.into(),
            worktree: "desk".into(),
            model: None,
            title: None,
            colour: None,
            permissions: None,
            account: None,
        }
    }

    #[test]
    fn an_agent_whose_worktree_is_gone_is_not_started_in_the_home_folder() {
        let dir = scratch("missing-worktree");
        let manager = Arc::new(crate::pty::PtyManager::with_log_dir(dir.join("sessions")));
        let crew = Crew::new(manager, dir.clone());

        let Ok(hired) = crew.hire(an_x("svc-demo")) else {
            return;
        };

        let refused = crew
            .start(&hired.id, &dir.join("worktrees/gone"), false, None)
            .expect_err("a folder that is not there is not a place to work");

        assert!(
            refused.to_string().contains("worktree is gone"),
            "it should say what is wrong rather than opening somewhere else: {refused}"
        );
    }

    #[test]
    fn every_project_gets_its_own_x() {
        let dir = scratch("two-projects");
        let manager = Arc::new(crate::pty::PtyManager::with_log_dir(dir.join("sessions")));
        let crew = Crew::new(manager, dir);

        let Ok(first) = crew.hire(an_x("svc-demo")) else {
            // The engine has to be installed to hire on it, and this is a unit
            // test rather than a statement about the machine it runs on.
            return;
        };
        let second = crew.hire(an_x("the-site")).expect("the second project's X");

        assert_eq!(first.name, "X");
        assert_eq!(second.name, "X", "the name is the job, not a number");
        assert_ne!(first.id, second.id, "and the id still tells them apart");
        assert_eq!(second.id, "x-the-site");

        assert!(
            crew.hire(an_x("svc-demo")).is_err(),
            "the same name twice in one project is still a clash"
        );
    }

    fn crew_with(where_it_lives: &str, agent: Agent) -> Arc<Crew> {
        let dir = scratch(where_it_lives);
        let manager = Arc::new(crate::pty::PtyManager::with_log_dir(dir.join("sessions")));
        let crew = Crew::new(manager, dir);
        crew.state.lock().agents.insert(agent.id.clone(), agent);
        crew
    }

    fn held(crew: &Arc<Crew>) -> Agent {
        crew.list().into_iter().find(|agent| agent.id == "ada").unwrap()
    }

    fn an_agent(session: Option<&str>, state: AgentState) -> Agent {
        Agent {
            id: "ada".to_owned(),
            name: "Ada".to_owned(),
            role: "implementer".to_owned(),
            engine_id: "claude".to_owned(),
            repository_id: "svc".to_owned(),
            worktree: "ada-tree".to_owned(),
            session_id: session.map(str::to_owned),
            state,
            model: None,
            title: None,
            colour: None,
            permissions: None,
            account: None,
        }
    }

    /// A crew as it is found on disk when the app starts: the agent was written
    /// there by a previous run, and its pane died with that run.
    fn crew_found_on_disk(where_it_lives: &str, agent: Agent) -> Arc<Crew> {
        let dir = scratch(where_it_lives);
        let mut state = State::default();
        state.agents.insert(agent.id.clone(), agent);
        crate::db::save_state(&dir, "crew", &state);

        let manager = Arc::new(crate::pty::PtyManager::with_log_dir(dir.join("sessions")));
        Crew::new(manager, dir)
    }

    #[test]
    fn whoever_had_a_pane_when_the_app_went_down_is_remembered() {
        let crew = crew_found_on_disk("interrupted", an_agent(Some("pane-gone"), AgentState::Idle));

        let waiting = crew.take_the_interrupted();

        assert_eq!(waiting.len(), 1, "an agent resting at its prompt still counts");
        assert_eq!(waiting[0].id, "ada");
        assert!(crew.take_the_interrupted().is_empty(), "taken once, not twice");
    }

    #[test]
    fn an_agent_nobody_started_is_not_brought_back() {
        let crew = crew_found_on_disk("never-started", an_agent(None, AgentState::Idle));

        assert!(crew.take_the_interrupted().is_empty());
    }

    #[test]
    fn a_pane_that_is_not_mid_turn_gives_its_slot_back() {
        let crew = crew_with("gives-back", an_agent(Some("pane-1"), AgentState::Working));

        assert!(crew.mark_busy("ada", false), "the state changed");
        assert_eq!(held(&crew).state, AgentState::Idle);

        assert!(!crew.mark_busy("ada", false), "saying it twice changes nothing");
    }

    #[test]
    fn a_pane_in_the_middle_of_a_turn_is_working_again() {
        let crew = crew_with("working-again", an_agent(Some("pane-1"), AgentState::Idle));

        assert!(crew.mark_busy("ada", true));
        assert_eq!(held(&crew).state, AgentState::Working);
    }

    #[test]
    fn an_agent_with_no_pane_is_left_where_it_is() {
        let crew = crew_with("no-pane", an_agent(None, AgentState::Done));

        assert!(!crew.mark_busy("ada", true));
        assert_eq!(held(&crew).state, AgentState::Done);
    }
}
