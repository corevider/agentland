use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, bail, Context, Result};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::ports::{PortRegistry, SharedPorts};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Repository {
    pub id: String,
    pub name: String,
    pub primary_path: PathBuf,
    pub default_branch: String,
    pub remote: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Worktree {
    pub name: String,
    pub repository_id: String,
    pub path: PathBuf,
    pub branch: String,
    pub port: u16,
}

#[derive(Clone, Debug, Serialize)]
pub struct WorktreeStatus {
    #[serde(flatten)]
    pub worktree: Worktree,
    pub dirty_files: usize,
    pub ahead: u32,
    pub missing: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct State {
    #[serde(default)]
    repositories: BTreeMap<String, Repository>,
    #[serde(default)]
    worktrees: BTreeMap<String, Worktree>,
    #[serde(default)]
    ports: PortRegistry,
}

pub struct RepoRegistry {
    state: Mutex<State>,
    data_dir: PathBuf,
    ports: SharedPorts,
}

fn git(args: &[&str], cwd: Option<&Path>) -> Result<String> {
    let mut command = Command::new("git");
    command.args(args);
    if let Some(dir) = cwd {
        command.current_dir(dir);
    }

    let output = command
        .output()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;

    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn slugify(value: &str) -> String {
    let slug: String = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();

    slug.trim_matches('-').replace("--", "-")
}

impl RepoRegistry {
    pub fn new(data_dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&data_dir);
        let data_dir = fs::canonicalize(&data_dir).unwrap_or(data_dir);
        let state = Self::load(&data_dir);
        let ports = SharedPorts::new(state.ports.clone());

        Self {
            state: Mutex::new(state),
            data_dir,
            ports,
        }
    }

    fn state_path(data_dir: &Path) -> PathBuf {
        data_dir.join("repositories.json")
    }

    fn load(data_dir: &Path) -> State {
        fs::read_to_string(Self::state_path(data_dir))
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    fn persist(&self, state: &State) {
        if let Ok(raw) = serde_json::to_string_pretty(state) {
            let _ = fs::write(Self::state_path(&self.data_dir), raw);
        }
    }

    fn worktree_root(&self, repository_id: &str) -> PathBuf {
        self.data_dir.join("worktrees").join(repository_id)
    }

    pub fn register(&self, path: &Path) -> Result<Repository> {
        let canonical = path
            .canonicalize()
            .with_context(|| format!("path does not exist: {}", path.display()))?;

        let top_level = git(&["rev-parse", "--show-toplevel"], Some(&canonical))
            .context("not a git repository")?;
        let primary_path = PathBuf::from(top_level);

        let name = primary_path
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .ok_or_else(|| anyhow!("repository has no directory name"))?;
        let id = slugify(&name);

        let default_branch = git(&["rev-parse", "--abbrev-ref", "HEAD"], Some(&primary_path))
            .unwrap_or_else(|_| "main".to_owned());
        let remote = git(&["remote", "get-url", "origin"], Some(&primary_path)).ok();

        let repository = Repository {
            id: id.clone(),
            name,
            primary_path,
            default_branch,
            remote,
        };

        let mut state = self.state.lock();
        state.repositories.insert(id, repository.clone());
        self.persist(&state);

        Ok(repository)
    }

    pub fn clone_repository(&self, url: &str, into: &Path) -> Result<Repository> {
        let name = url
            .rsplit('/')
            .next()
            .map(|value| value.trim_end_matches(".git"))
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow!("cannot derive a name from {url}"))?;

        let target = into.join(name);
        if target.exists() {
            bail!("{} already exists", target.display());
        }

        fs::create_dir_all(into)?;
        git(&["clone", url, target.to_string_lossy().as_ref()], None)?;
        self.register(&target)
    }

    pub fn repositories(&self) -> Vec<Repository> {
        self.state.lock().repositories.values().cloned().collect()
    }

    pub fn worktrees(&self) -> Vec<WorktreeStatus> {
        let worktrees: Vec<Worktree> = self.state.lock().worktrees.values().cloned().collect();
        worktrees.into_iter().map(|entry| self.status(entry)).collect()
    }

    fn status(&self, worktree: Worktree) -> WorktreeStatus {
        if !worktree.path.exists() {
            return WorktreeStatus {
                worktree,
                dirty_files: 0,
                ahead: 0,
                missing: true,
            };
        }

        let dirty_files = git(&["status", "--porcelain"], Some(&worktree.path))
            .map(|output| output.lines().filter(|line| !line.trim().is_empty()).count())
            .unwrap_or(0);

        let ahead = git(
            &["rev-list", "--count", &format!("{}..HEAD", worktree.branch)],
            Some(&worktree.path),
        )
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);

        WorktreeStatus {
            worktree,
            dirty_files,
            ahead,
            missing: false,
        }
    }

    pub fn create_worktree(&self, repository_id: &str, name: &str) -> Result<Worktree> {
        let repository = self
            .state
            .lock()
            .repositories
            .get(repository_id)
            .cloned()
            .ok_or_else(|| anyhow!("unknown repository: {repository_id}"))?;

        let key = format!("{repository_id}/{name}");
        if self.state.lock().worktrees.contains_key(&key) {
            bail!("worktree already exists: {key}");
        }

        let root = self.worktree_root(repository_id);
        fs::create_dir_all(&root)?;
        let path = root.join(name);
        let branch = format!("agent/{name}");

        let branch_exists = git(
            &["rev-parse", "--verify", &format!("refs/heads/{branch}")],
            Some(&repository.primary_path),
        )
        .is_ok();

        let path_text = path.to_string_lossy().to_string();
        let args: Vec<&str> = if branch_exists {
            vec!["worktree", "add", &path_text, &branch]
        } else {
            vec!["worktree", "add", "-b", &branch, &path_text]
        };

        git(&args, Some(&repository.primary_path))?;

        let port = self.ports.allocate(&key)?;
        let worktree = Worktree {
            name: name.to_owned(),
            repository_id: repository_id.to_owned(),
            path,
            branch,
            port,
        };

        let mut state = self.state.lock();
        state.worktrees.insert(key, worktree.clone());
        state.ports = self.ports.snapshot();
        self.persist(&state);

        Ok(worktree)
    }

    pub fn remove_worktree(&self, repository_id: &str, name: &str, force: bool) -> Result<()> {
        let key = format!("{repository_id}/{name}");
        let worktree = self
            .state
            .lock()
            .worktrees
            .get(&key)
            .cloned()
            .ok_or_else(|| anyhow!("unknown worktree: {key}"))?;

        let repository = self
            .state
            .lock()
            .repositories
            .get(repository_id)
            .cloned()
            .ok_or_else(|| anyhow!("unknown repository: {repository_id}"))?;

        if !force {
            let status = self.status(worktree.clone());
            if status.dirty_files > 0 {
                bail!(
                    "{key} has {} uncommitted file(s); pass force to discard them",
                    status.dirty_files
                );
            }
        }

        let path_text = worktree.path.to_string_lossy().to_string();
        let mut args = vec!["worktree", "remove", path_text.as_str()];
        if force {
            args.push("--force");
        }

        if worktree.path.exists() {
            git(&args, Some(&repository.primary_path))?;
        } else {
            let _ = git(&["worktree", "prune"], Some(&repository.primary_path));
        }

        self.ports.release(&key);

        let mut state = self.state.lock();
        state.worktrees.remove(&key);
        state.ports = self.ports.snapshot();
        self.persist(&state);

        Ok(())
    }

    pub fn ports(&self) -> PortRegistry {
        self.ports.snapshot()
    }
}
