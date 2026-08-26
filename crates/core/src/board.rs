use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{anyhow, bail, Result};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Column {
    Backlog,
    Assigned,
    Working,
    Review,
    Done,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Evidence {
    Commit { sha: String, subject: String },
    Diff { files: usize, insertions: u32, deletions: u32 },
    PullRequest { url: String },
    Note { text: String },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub body: String,
    pub column: Column,
    pub repository_id: String,
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub worktree: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CreateTask {
    pub title: String,
    #[serde(default)]
    pub body: String,
    pub repository_id: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MoveTask {
    pub column: Column,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct State {
    #[serde(default)]
    tasks: BTreeMap<String, Task>,
    #[serde(default)]
    next_number: u32,
}

pub struct Board {
    state: Mutex<State>,
    data_dir: PathBuf,
}

impl Board {
    pub fn new(data_dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&data_dir);
        let data_dir = fs::canonicalize(&data_dir).unwrap_or(data_dir);
        let state = crate::db::load_state(&data_dir, "board");

        Self {
            state: Mutex::new(state),
            data_dir,
        }
    }

    fn persist(&self, state: &State) {
        crate::db::save_state(&self.data_dir, "board", state);
    }

    pub fn list(&self) -> Vec<Task> {
        self.state.lock().tasks.values().cloned().collect()
    }

    pub fn get(&self, id: &str) -> Option<Task> {
        self.state.lock().tasks.get(id).cloned()
    }

    pub fn create(&self, request: CreateTask) -> Result<Task> {
        if request.title.trim().is_empty() {
            bail!("a task needs a title");
        }

        let mut state = self.state.lock();
        state.next_number += 1;
        let id = format!("t{}", state.next_number);

        let task = Task {
            id: id.clone(),
            title: request.title,
            body: request.body,
            column: Column::Backlog,
            repository_id: request.repository_id,
            assignee: None,
            worktree: None,
            branch: None,
            evidence: Vec::new(),
        };

        state.tasks.insert(id, task.clone());
        self.persist(&state);
        Ok(task)
    }

    pub fn move_to(&self, id: &str, column: Column) -> Result<Task> {
        let mut state = self.state.lock();
        let task = state
            .tasks
            .get_mut(id)
            .ok_or_else(|| anyhow!("unknown task: {id}"))?;
        task.column = column;
        let updated = task.clone();
        self.persist(&state);
        Ok(updated)
    }

    pub fn attach(&self, id: &str, evidence: Evidence) -> Result<Task> {
        let mut state = self.state.lock();
        let task = state
            .tasks
            .get_mut(id)
            .ok_or_else(|| anyhow!("unknown task: {id}"))?;
        task.evidence.push(evidence);
        let updated = task.clone();
        self.persist(&state);
        Ok(updated)
    }

    pub fn record_assignment(
        &self,
        id: &str,
        assignee: &str,
        worktree: &str,
        branch: &str,
    ) -> Result<Task> {
        let mut state = self.state.lock();
        let task = state
            .tasks
            .get_mut(id)
            .ok_or_else(|| anyhow!("unknown task: {id}"))?;

        task.assignee = Some(assignee.to_owned());
        task.worktree = Some(worktree.to_owned());
        task.branch = Some(branch.to_owned());
        task.column = Column::Working;
        let updated = task.clone();
        self.persist(&state);
        Ok(updated)
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let mut state = self.state.lock();
        state
            .tasks
            .remove(id)
            .ok_or_else(|| anyhow!("unknown task: {id}"))?;
        self.persist(&state);
        Ok(())
    }
}
