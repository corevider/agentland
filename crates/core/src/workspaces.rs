use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{bail, Result};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub repository_ids: Vec<String>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct State {
    #[serde(default)]
    workspaces: BTreeMap<String, Workspace>,
    #[serde(default)]
    active: Option<String>,
    #[serde(default)]
    next_number: u32,
}

#[derive(Debug, Deserialize)]
pub struct CreateWorkspace {
    pub name: String,
    #[serde(default)]
    pub repository_ids: Vec<String>,
}

pub struct Workspaces {
    state: Mutex<State>,
    data_dir: PathBuf,
}

impl Workspaces {
    pub fn new(data_dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&data_dir);
        let data_dir = crate::exec::settled(&data_dir);
        let state = crate::db::load_state(&data_dir, "workspaces");

        Self {
            state: Mutex::new(state),
            data_dir,
        }
    }

    fn persist(&self, state: &State) {
        crate::db::save_state(&self.data_dir, "workspaces", state);
    }

    pub fn list(&self) -> Vec<Workspace> {
        self.state.lock().workspaces.values().cloned().collect()
    }

    pub fn active(&self) -> Option<Workspace> {
        let state = self.state.lock();
        state
            .active
            .as_ref()
            .and_then(|id| state.workspaces.get(id))
            .cloned()
    }

    pub fn create(&self, request: CreateWorkspace) -> Result<Workspace> {
        let name = request.name.trim().to_owned();
        if name.is_empty() {
            bail!("a workspace needs a name");
        }

        let mut state = self.state.lock();
        state.next_number += 1;

        let workspace = Workspace {
            id: format!("ws{}", state.next_number),
            name,
            repository_ids: request.repository_ids,
        };

        state.workspaces.insert(workspace.id.clone(), workspace.clone());
        if state.active.is_none() {
            state.active = Some(workspace.id.clone());
        }
        self.persist(&state);

        Ok(workspace)
    }

    /// Stand in a workspace.
    ///
    /// There is no standing nowhere while there is somewhere to stand. Every
    /// question the rest of the app asks — which project a folder just opened
    /// belongs to, which workspace's folder a note goes in — is answered by
    /// where you are, and "nowhere" is not an answer to any of them: a project
    /// opened that way joined no workspace, and what its crew learned was filed
    /// under a vault folder literally called "workspace".
    pub fn activate(&self, id: Option<&str>) -> Result<Option<Workspace>> {
        let mut state = self.state.lock();

        match id {
            Some(id) => {
                if !state.workspaces.contains_key(id) {
                    bail!("there is no workspace called {id}");
                }
                state.active = Some(id.to_owned());
            }
            None if !state.workspaces.is_empty() => {
                bail!("open a workspace and work from there — there is nowhere else to stand")
            }
            None => state.active = None,
        }

        self.persist(&state);
        Ok(state.active.as_ref().and_then(|id| state.workspaces.get(id)).cloned())
    }

    pub fn set_repositories(&self, id: &str, repository_ids: Vec<String>) -> Result<Workspace> {
        let mut state = self.state.lock();
        let Some(workspace) = state.workspaces.get_mut(id) else {
            bail!("there is no workspace called {id}");
        };

        workspace.repository_ids = repository_ids;
        let updated = workspace.clone();
        self.persist(&state);

        Ok(updated)
    }

    /// Put a project into a workspace, unless it is already there.
    ///
    /// A folder opened while a workspace is active belongs to that workspace —
    /// otherwise it lands nowhere and the rail keeps saying the workspace holds
    /// nothing while the project sits in the list below it.
    pub fn include(&self, id: &str, repository_id: &str) -> Result<Workspace> {
        let mut state = self.state.lock();
        let Some(workspace) = state.workspaces.get_mut(id) else {
            bail!("there is no workspace called {id}");
        };

        if !workspace.repository_ids.iter().any(|held| held == repository_id) {
            workspace.repository_ids.push(repository_id.to_owned());
        }

        let updated = workspace.clone();
        self.persist(&state);

        Ok(updated)
    }

    pub fn remove(&self, id: &str) -> Result<()> {
        let mut state = self.state.lock();
        if state.workspaces.remove(id).is_none() {
            bail!("there is no workspace called {id}");
        }

        // Standing somewhere is the whole idea: deleting the workspace you were
        // in moves you to another one rather than to nowhere. Nowhere is only
        // where you are when there is nothing left to stand in.
        if state.active.as_deref() == Some(id) {
            state.active = state.workspaces.keys().next().cloned();
        }

        self.persist(&state);
        Ok(())
    }

    pub fn forget_repository(&self, repository_id: &str) {
        let mut state = self.state.lock();
        let mut touched = false;

        for workspace in state.workspaces.values_mut() {
            let before = workspace.repository_ids.len();
            workspace.repository_ids.retain(|id| id != repository_id);
            touched |= workspace.repository_ids.len() != before;
        }

        if touched {
            self.persist(&state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspaces(name: &str) -> Workspaces {
        let dir = std::env::temp_dir().join(format!("agentland-ws-{name}"));
        let _ = fs::remove_dir_all(&dir);
        Workspaces::new(dir)
    }

    #[test]
    fn the_first_workspace_becomes_the_active_one() {
        let store = workspaces("first");
        assert_eq!(store.active(), None);

        let created = store
            .create(CreateWorkspace {
                name: "Product".into(),
                repository_ids: vec!["agentland".into()],
            })
            .expect("create");

        assert_eq!(store.active().map(|entry| entry.id), Some(created.id.clone()));

        store
            .create(CreateWorkspace {
                name: "Infra".into(),
                repository_ids: vec![],
            })
            .expect("create a second");

        assert_eq!(
            store.active().map(|entry| entry.id),
            Some(created.id),
            "a later workspace does not steal focus"
        );
    }

    #[test]
    fn a_workspace_without_a_name_is_refused() {
        let store = workspaces("unnamed");
        let error = store
            .create(CreateWorkspace {
                name: "   ".into(),
                repository_ids: vec![],
            })
            .expect_err("should refuse");
        assert!(error.to_string().contains("needs a name"));
    }

    #[test]
    fn there_is_no_standing_nowhere_while_there_is_somewhere_to_stand() {
        let store = workspaces("nowhere");
        store
            .create(CreateWorkspace {
                name: "Product".into(),
                repository_ids: vec!["agentland".into()],
            })
            .expect("create");

        let refused = store.activate(None).expect_err("nowhere is not a place");
        assert!(refused.to_string().contains("open a workspace"), "{refused}");
        assert!(store.active().is_some(), "and it left you where you were");
    }

    #[test]
    fn removing_the_workspace_you_are_in_moves_you_to_another_one() {
        let store = workspaces("remove");
        let first = store
            .create(CreateWorkspace {
                name: "Product".into(),
                repository_ids: vec![],
            })
            .expect("create");
        let second = store
            .create(CreateWorkspace {
                name: "Infra".into(),
                repository_ids: vec![],
            })
            .expect("create a second");

        assert_eq!(store.active().map(|entry| entry.id), Some(first.id.clone()));

        store.remove(&first.id).expect("remove");
        assert_eq!(
            store.active().map(|entry| entry.id),
            Some(second.id.clone()),
            "you are moved, not stranded"
        );

        store.remove(&second.id).expect("remove the last one");
        assert_eq!(store.active(), None, "nowhere, only when there is nothing left");
        assert!(store.remove(&second.id).is_err(), "removing it twice is an error");
    }

    #[test]
    fn a_repository_that_is_gone_leaves_every_workspace() {
        let store = workspaces("forget");
        let created = store
            .create(CreateWorkspace {
                name: "Product".into(),
                repository_ids: vec!["agentland".into(), "ccdo".into()],
            })
            .expect("create");

        store.forget_repository("ccdo");

        let held = store.list().into_iter().find(|entry| entry.id == created.id).expect("still there");
        assert_eq!(held.repository_ids, vec!["agentland".to_owned()]);
    }

    #[test]
    fn repositories_can_be_replaced_wholesale() {
        let store = workspaces("replace");
        let created = store
            .create(CreateWorkspace {
                name: "Product".into(),
                repository_ids: vec!["a".into()],
            })
            .expect("create");

        let updated = store
            .set_repositories(&created.id, vec!["b".into(), "c".into()])
            .expect("replace");
        assert_eq!(updated.repository_ids, vec!["b".to_owned(), "c".to_owned()]);
        assert!(store.set_repositories("nope", vec![]).is_err());
    }

    #[test]
    fn a_project_opened_here_joins_this_workspace_once() {
        let held = workspaces("include");
        let made = held
            .create(CreateWorkspace {
                name: "test".to_owned(),
                repository_ids: Vec::new(),
            })
            .unwrap();

        held.include(&made.id, "svc-demo").unwrap();
        let twice = held.include(&made.id, "svc-demo").unwrap();

        assert_eq!(twice.repository_ids, vec!["svc-demo".to_owned()]);
    }

    #[test]
    fn including_into_a_workspace_that_is_gone_says_so() {
        assert!(workspaces("include-missing").include("nope", "svc-demo").is_err());
    }
}
