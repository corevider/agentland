use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, bail, Context, Result};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::ports::{PortRegistry, SharedPorts};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Remote {
    pub name: String,
    pub url: String,
    pub host: Option<String>,
    pub owner: Option<String>,
    pub repo: Option<String>,
    pub provider: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Repository {
    pub id: String,
    pub name: String,
    pub primary_path: PathBuf,
    pub default_branch: String,
    #[serde(default)]
    pub remotes: Vec<Remote>,
    #[serde(default)]
    pub origin: Option<String>,
}

pub fn parse_remote(name: &str, url: &str) -> Remote {
    let trimmed = url.trim();
    let (host, path) = if let Some(rest) = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .or_else(|| trimmed.strip_prefix("ssh://"))
        .or_else(|| trimmed.strip_prefix("git://"))
    {
        let rest = rest.split_once('@').map_or(rest, |(_, tail)| tail);
        match rest.split_once('/') {
            Some((authority, tail)) => (
                Some(authority.split_once(':').map_or(authority, |(h, _)| h).to_owned()),
                Some(tail.to_owned()),
            ),
            None => (Some(rest.to_owned()), None),
        }
    } else if let Some((authority, tail)) = trimmed.split_once(':') {
        if trimmed.starts_with('/') || trimmed.starts_with('.') || trimmed.starts_with("file") {
            (None, None)
        } else {
            let host = authority.split_once('@').map_or(authority, |(_, h)| h);
            (Some(host.to_owned()), Some(tail.to_owned()))
        }
    } else {
        (None, None)
    };

    let path = path.map(|value| value.trim_end_matches('/').trim_end_matches(".git").to_owned());
    let (owner, repo) = match path.as_deref() {
        Some(value) => match value.rsplit_once('/') {
            Some((owner, repo)) => (Some(owner.to_owned()), Some(repo.to_owned())),
            None => (None, Some(value.to_owned())),
        },
        None => (None, None),
    };

    let provider = match host.as_deref() {
        Some(value) if value.contains("github") => "github",
        Some(value) if value.contains("gitlab") => "gitlab",
        Some(value) if value.contains("bitbucket") => "bitbucket",
        Some(value) if value.contains("codeberg") => "codeberg",
        Some(_) => "git",
        None => "local",
    }
    .to_owned();

    Remote {
        name: name.to_owned(),
        url: trimmed.to_owned(),
        host,
        owner,
        repo,
        provider,
    }
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

/// Who git should say wrote a commit, when the machine has nobody to say.
///
/// Git refuses to commit without a name to put on it, and a machine that has
/// never had `user.email` set is not a broken machine — it is a fresh one. Its
/// first commit should not be the thing that fails, and on a CI runner it is
/// exactly the thing that did.
///
/// Only when nothing is configured. A person who has set their own identity
/// keeps it: a commit attributed to the tool rather than to them is a lie about
/// who wrote it.
fn who_is_committing(at: &Path) -> Vec<String> {
    let configured = git(&["config", "user.email"], Some(at))
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);

    if configured {
        return Vec::new();
    }

    vec![
        "-c".to_owned(),
        "user.name=Agentland".to_owned(),
        "-c".to_owned(),
        "user.email=agentland@localhost".to_owned(),
    ]
}

/// Run `git commit` with somebody's name on it, whoever that turns out to be.
fn commit_as_somebody(args: &[&str], at: &Path) -> Result<String> {
    let mut all = who_is_committing(at);
    all.extend(args.iter().map(|piece| (*piece).to_owned()));

    let borrowed: Vec<&str> = all.iter().map(String::as_str).collect();
    git(&borrowed, Some(at))
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

        for worktree in state.worktrees.values() {
            if worktree.path.exists() {
                write_mcp_config(&worktree.path, &data_dir);
            }
        }

        Self {
            state: Mutex::new(state),
            data_dir,
            ports,
        }
    }

    fn load(data_dir: &Path) -> State {
        crate::db::load_state(data_dir, "repositories")
    }

    fn persist(&self, state: &State) {
        crate::db::save_state(&self.data_dir, "repositories", state);
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

        let git_dir = git(&["rev-parse", "--absolute-git-dir"], Some(&primary_path))?;
        let common_dir = git(&["rev-parse", "--path-format=absolute", "--git-common-dir"], Some(&primary_path))
            .unwrap_or_else(|_| git_dir.clone());
        if git_dir != common_dir {
            bail!(
                "{} is a worktree of {}; register the main checkout instead",
                primary_path.display(),
                PathBuf::from(&common_dir).parent().unwrap_or(Path::new(&common_dir)).display()
            );
        }

        let name = primary_path
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .ok_or_else(|| anyhow!("repository has no directory name"))?;
        let id = slugify(&name);

        let default_branch = git(&["rev-parse", "--abbrev-ref", "HEAD"], Some(&primary_path))
            .unwrap_or_else(|_| "main".to_owned());

        let remotes: Vec<Remote> = git(&["remote"], Some(&primary_path))
            .unwrap_or_default()
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|remote_name| {
                let url = git(
                    &["remote", "get-url", remote_name.trim()],
                    Some(&primary_path),
                )
                .ok()?;
                Some(parse_remote(remote_name.trim(), &url))
            })
            .collect();

        let origin = remotes
            .iter()
            .find(|remote| remote.name == "origin")
            .or_else(|| remotes.first())
            .map(|remote| remote.url.clone());

        let repository = Repository {
            id: id.clone(),
            name,
            primary_path,
            default_branch,
            remotes,
            origin,
        };

        let mut state = self.state.lock();
        state.repositories.insert(id, repository.clone());
        self.persist(&state);

        Ok(repository)
    }

    /// Take a plain folder as a project, starting a git repository in it.
    ///
    /// Not everything a person wants the crew to work on is a checkout yet — a
    /// folder of notes, a sketch of an app, something they made this morning.
    /// Agentland gives each agent its own worktree, and a worktree needs a repo
    /// with at least one commit behind it, so adopting a folder means `git init`
    /// and a first commit. That writes to somebody's folder, so it is never
    /// automatic: the panel asks first, and this only runs on a yes.
    pub fn adopt(&self, path: &Path) -> Result<Repository> {
        let canonical = path
            .canonicalize()
            .with_context(|| format!("path does not exist: {}", path.display()))?;

        if !canonical.is_dir() {
            bail!("{} is not a folder", canonical.display());
        }

        if git(&["rev-parse", "--show-toplevel"], Some(&canonical)).is_ok() {
            return self.register(&canonical);
        }

        git(&["init", "-b", "main"], Some(&canonical))?;
        git(&["add", "-A"], Some(&canonical))?;

        // An empty folder has nothing to commit, and a repository with no HEAD
        // cannot have a worktree cut from it — so it gets an empty commit rather
        // than a repository the crew cannot work in.
        let staged = git(&["diff", "--cached", "--name-only"], Some(&canonical)).unwrap_or_default();
        let message = "chore: start tracking this folder with Agentland";
        if staged.trim().is_empty() {
            commit_as_somebody(&["commit", "--allow-empty", "-m", message], &canonical)?;
        } else {
            commit_as_somebody(&["commit", "-m", message], &canonical)?;
        }

        self.register(&canonical)
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

    /// Stop tracking a project. The folder on disk is not touched.
    ///
    /// Opening the wrong folder is a normal mistake and has to be undoable, but
    /// a project with worktrees cut from it is not forgotten by accident: those
    /// are branches someone is working on, so they have to go first.
    pub fn forget(&self, id: &str) -> Result<()> {
        let mut state = self.state.lock();
        if !state.repositories.contains_key(id) {
            bail!("unknown repository: {id}");
        }

        let held: Vec<String> = state
            .worktrees
            .values()
            .filter(|worktree| worktree.repository_id == id)
            .map(|worktree| worktree.name.clone())
            .collect();

        if !held.is_empty() {
            bail!(
                "{id} still has {} worktree(s) — remove {} first",
                held.len(),
                held.join(", ")
            );
        }

        state.repositories.remove(id);
        self.persist(&state);
        Ok(())
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
        write_mcp_config(&path, &self.data_dir);
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

#[derive(Clone, Debug, Serialize)]
pub struct CommitInfo {
    pub sha: String,
    pub subject: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Review {
    pub base: String,
    pub branch: String,
    pub files: usize,
    pub insertions: u32,
    pub deletions: u32,
    pub commits: Vec<CommitInfo>,
    pub untracked: Vec<String>,
    pub uncommitted: bool,
    pub patch: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Commit {
    pub sha: String,
    pub branch: String,
    pub files: usize,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PullRequest {
    pub url: String,
    pub created: bool,
    pub detail: String,
}

const UNTRACKED_PATCH_LIMIT: usize = 40;

fn diff_untracked(worktree: &Path, file: &str) -> Option<String> {
    let output = Command::new("git")
        .args(["diff", "--no-index", "--", "/dev/null", file])
        .current_dir(worktree)
        .output()
        .ok()?;

    let rendered = String::from_utf8_lossy(&output.stdout).into_owned();
    if rendered.trim().is_empty() {
        None
    } else {
        Some(rendered)
    }
}

const TOOL_NAME: &str = "agentland-mcp";

fn built_tool() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(TOOL_NAME)))
        .filter(|candidate| candidate.exists())
}

fn is_current(built: &Path, kept: &Path) -> bool {
    let (Ok(built), Ok(kept)) = (fs::metadata(built), fs::metadata(kept)) else {
        return false;
    };

    built.len() == kept.len()
        && match (built.modified(), kept.modified()) {
            (Ok(built), Ok(kept)) => kept >= built,
            _ => false,
        }
}

/// Keep the crew's tool program somewhere a rebuild cannot reach.
///
/// A working agent holds an open pipe to this program. Building Agentland
/// overwrites the copy under `target/`, and every agent mid-turn loses its
/// tools and stops to ask what happened. Copying it beside the data and
/// putting it in place by rename leaves running agents on the file they
/// already opened, while the next agent to start picks up the new one.
fn kept_tool(built: &Path, data_dir: &Path) -> Option<PathBuf> {
    let shelf = data_dir.join("bin");
    fs::create_dir_all(&shelf).ok()?;
    let kept = shelf.join(TOOL_NAME);

    if is_current(built, &kept) {
        return Some(kept);
    }

    let arriving = shelf.join(format!("{TOOL_NAME}.arriving"));
    fs::copy(built, &arriving).ok()?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&arriving, fs::Permissions::from_mode(0o755));
    }

    match fs::rename(&arriving, &kept) {
        Ok(()) => Some(kept),
        Err(_) => {
            let _ = fs::remove_file(&arriving);
            None
        }
    }
}

fn mcp_binary(data_dir: &Path) -> String {
    built_tool()
        .and_then(|built| kept_tool(&built, data_dir).or(Some(built)))
        .map(|candidate| candidate.to_string_lossy().into_owned())
        .unwrap_or_else(|| TOOL_NAME.to_owned())
}

fn write_mcp_config(worktree: &Path, data_dir: &Path) {
    let config = serde_json::json!({
        "mcpServers": {
            "agentland": {
                "command": mcp_binary(data_dir),
                "args": [],
                "env": {
                    "AGENTLAND_PORT": "${AGENTLAND_PORT}",
                    "AGENTLAND_TOKEN": "${AGENTLAND_TOKEN}"
                }
            }
        }
    });

    if let Ok(rendered) = serde_json::to_string_pretty(&config) {
        let _ = fs::write(worktree.join(".mcp.json"), rendered);
    }

    exclude_from_git(worktree, ".mcp.json");
    trust_our_own_tools(worktree);
}

/// Say, in the worktree, that the tools Agentland put there are wanted.
///
/// The engine asks a person before loading a project's MCP servers, and it ties
/// that answer to what the file says. Agentland writes the file itself, so the
/// question is one nobody can usefully answer — and when the tool's path moved,
/// every earlier answer was silently invalidated and whole sessions came up with
/// no tools while looking perfectly healthy. The crew's own config is trusted by
/// the app that wrote it.
fn trust_our_own_tools(worktree: &Path) {
    let settings = worktree.join(".claude");
    if fs::create_dir_all(&settings).is_err() {
        return;
    }

    let file = settings.join("settings.local.json");
    let mut held: serde_json::Value = fs::read_to_string(&file)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_else(|| serde_json::json!({}));

    if let Some(map) = held.as_object_mut() {
        map.insert("enableAllProjectMcpServers".to_owned(), serde_json::Value::Bool(true));
    }

    if let Ok(rendered) = serde_json::to_string_pretty(&held) {
        let _ = fs::write(&file, rendered);
    }

    exclude_from_git(worktree, ".claude/settings.local.json");
}

fn exclude_from_git(worktree: &Path, pattern: &str) {
    let Ok(common_dir) = git(
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        Some(worktree),
    ) else {
        return;
    };

    let info = PathBuf::from(common_dir).join("info");
    if fs::create_dir_all(&info).is_err() {
        return;
    }

    let path = info.join("exclude");
    let existing = fs::read_to_string(&path).unwrap_or_default();

    if existing.lines().any(|line| line.trim() == pattern) {
        return;
    }

    let separator = if existing.is_empty() || existing.ends_with('\n') {
        ""
    } else {
        "\n"
    };

    let _ = fs::write(&path, format!("{existing}{separator}{pattern}\n"));
}

fn numstat_totals(output: &str) -> (usize, u32, u32) {
    let mut files = 0;
    let mut insertions = 0;
    let mut deletions = 0;

    for line in output.lines().filter(|line| !line.trim().is_empty()) {
        let mut parts = line.split_whitespace();
        let added = parts.next().unwrap_or("0").parse::<u32>().unwrap_or(0);
        let removed = parts.next().unwrap_or("0").parse::<u32>().unwrap_or(0);
        files += 1;
        insertions += added;
        deletions += removed;
    }

    (files, insertions, deletions)
}

impl RepoRegistry {
    fn locate(&self, repository_id: &str, worktree_name: &str) -> Result<(Repository, Worktree)> {
        let state = self.state.lock();
        let repository = state
            .repositories
            .get(repository_id)
            .cloned()
            .ok_or_else(|| anyhow!("unknown repository: {repository_id}"))?;
        let worktree = state
            .worktrees
            .get(&format!("{repository_id}/{worktree_name}"))
            .cloned()
            .ok_or_else(|| anyhow!("unknown worktree: {repository_id}/{worktree_name}"))?;
        Ok((repository, worktree))
    }

    pub fn review(&self, repository_id: &str, worktree_name: &str) -> Result<Review> {
        let (repository, worktree) = self.locate(repository_id, worktree_name)?;
        let base = repository.default_branch.clone();
        let range = format!("{base}...HEAD");

        let committed = git(&["diff", "--numstat", &range], Some(&worktree.path)).unwrap_or_default();
        let pending = git(&["diff", "--numstat"], Some(&worktree.path)).unwrap_or_default();
        let (files, insertions, deletions) = numstat_totals(&format!("{committed}\n{pending}"));

        let commits = git(
            &["log", "--format=%h\u{1f}%s", &format!("{base}..HEAD")],
            Some(&worktree.path),
        )
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let (sha, subject) = line.split_once('\u{1f}')?;
            Some(CommitInfo {
                sha: sha.to_owned(),
                subject: subject.to_owned(),
            })
        })
        .collect();

        let mut patch = git(&["diff", &range], Some(&worktree.path)).unwrap_or_default();
        let working = git(&["diff"], Some(&worktree.path)).unwrap_or_default();
        if !working.trim().is_empty() {
            patch.push_str("\n");
            patch.push_str(&working);
        }

        let untracked: Vec<String> = git(
            &["ls-files", "--others", "--exclude-standard"],
            Some(&worktree.path),
        )
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::to_owned)
        .collect();

        let mut untracked_insertions = 0;
        for file in untracked.iter().take(UNTRACKED_PATCH_LIMIT) {
            if let Some(rendered) = diff_untracked(&worktree.path, file) {
                untracked_insertions += rendered
                    .lines()
                    .filter(|line| line.starts_with('+') && !line.starts_with("+++"))
                    .count() as u32;
                patch.push_str("\n");
                patch.push_str(&rendered);
            }
        }

        Ok(Review {
            base,
            branch: worktree.branch,
            files: files + untracked.len(),
            insertions: insertions + untracked_insertions,
            deletions,
            commits,
            uncommitted: !working.trim().is_empty() || !untracked.is_empty(),
            untracked,
            patch,
        })
    }

    pub fn commit(
        &self,
        repository_id: &str,
        worktree_name: &str,
        message: &str,
    ) -> Result<Commit> {
        let message = message.trim();
        if message.is_empty() {
            bail!("a commit needs a message");
        }

        let (_, worktree) = self.locate(repository_id, worktree_name)?;
        git(&["add", "-A"], Some(&worktree.path))?;

        let staged = git(&["diff", "--cached", "--name-only"], Some(&worktree.path))?;
        if staged.trim().is_empty() {
            bail!("there is nothing to commit in {worktree_name}");
        }

        commit_as_somebody(&["commit", "-q", "-m", message], &worktree.path)?;

        let sha = git(&["rev-parse", "--short", "HEAD"], Some(&worktree.path))?
            .trim()
            .to_owned();

        Ok(Commit {
            sha,
            branch: worktree.branch,
            files: staged.lines().filter(|line| !line.is_empty()).count(),
            message: message.to_owned(),
        })
    }

    /// What the forge says about the pull request on this worktree's branch.
    ///
    /// `Ok(None)` when there is no pull request, which is different from an
    /// error: a branch nobody has opened one for is an ordinary state, and a
    /// card should not be told its checks failed because `gh` is not installed.
    pub fn pull_request_state(
        &self,
        repository_id: &str,
        worktree_name: &str,
    ) -> Result<Option<crate::pulls::PullState>> {
        let (_, worktree) = self.locate(repository_id, worktree_name)?;

        let output = Command::new("gh")
            .args([
                "pr",
                "view",
                &worktree.branch,
                "--json",
                "number,url,state,mergeable,mergeStateStatus,baseRefName,reviewDecision,statusCheckRollup",
            ])
            .current_dir(&worktree.path)
            .output()
            .context("gh could not be run")?;

        if !output.status.success() {
            let said = String::from_utf8_lossy(&output.stderr);
            if said.contains("no pull requests found") || said.contains("no open pull requests") {
                return Ok(None);
            }
            bail!("gh could not read the pull request: {}", said.trim());
        }

        Ok(Some(serde_json::from_slice(&output.stdout)?))
    }

    /// Which files would conflict if this branch were merged into its base.
    ///
    /// Asked of `merge-tree`, which computes the merge and writes nothing: the
    /// worktree the agent is standing in is not a place to run a trial merge.
    /// The base is fetched first, because the conflict is with what the base is
    /// now and not with the copy this machine last saw.
    pub fn conflicting_files(
        &self,
        repository_id: &str,
        worktree_name: &str,
        base: &str,
    ) -> Result<Vec<String>> {
        let (_, worktree) = self.locate(repository_id, worktree_name)?;
        let base = base.to_owned();

        let against = if git(&["fetch", "origin", &base], Some(&worktree.path)).is_ok() {
            format!("origin/{base}")
        } else {
            base
        };

        let output = Command::new("git")
            .args(["merge-tree", "--write-tree", "--name-only", &against, &worktree.branch])
            .current_dir(&worktree.path)
            .output()
            .context("git could not be run")?;

        // A clean merge exits zero and names nothing; a conflict exits one and
        // names the files. Anything else is git failing, not a conflict.
        if output.status.code() == Some(0) {
            return Ok(Vec::new());
        }

        if output.status.code() != Some(1) {
            bail!(
                "git could not work the merge out: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }

        Ok(crate::pulls::merge_tree_conflicts(
            &String::from_utf8_lossy(&output.stdout),
        ))
    }

    /// What the failing checks on this branch actually said.
    ///
    /// `--log-failed` is the whole point: a run's full log is tens of thousands
    /// of lines of setup, and only the steps that failed are the reason.
    pub fn failing_check_log(&self, repository_id: &str, worktree_name: &str) -> Result<Option<String>> {
        let (_, worktree) = self.locate(repository_id, worktree_name)?;

        let listed = Command::new("gh")
            .args([
                "run",
                "list",
                "--branch",
                &worktree.branch,
                "--limit",
                "1",
                "--json",
                "databaseId,conclusion",
            ])
            .current_dir(&worktree.path)
            .output()
            .context("gh could not be run")?;

        if !listed.status.success() {
            return Ok(None);
        }

        #[derive(serde::Deserialize)]
        struct Run {
            #[serde(rename = "databaseId")]
            id: u64,
            #[serde(default)]
            conclusion: String,
        }

        let runs: Vec<Run> = serde_json::from_slice(&listed.stdout).unwrap_or_default();
        let Some(run) = runs.into_iter().find(|run| run.conclusion == "failure") else {
            return Ok(None);
        };

        let log = Command::new("gh")
            .args(["run", "view", &run.id.to_string(), "--log-failed"])
            .current_dir(&worktree.path)
            .output()
            .context("gh could not be run")?;

        if !log.status.success() {
            return Ok(None);
        }

        let text = String::from_utf8_lossy(&log.stdout).into_owned();
        Ok(if text.trim().is_empty() { None } else { Some(text) })
    }

    /// Leave a review on this branch's pull request.
    ///
    /// Always a comment, never an approval: every agent here pushes as the same
    /// GitHub account, and an account cannot approve its own pull request. The
    /// verdict is Agentland's to keep; this is so the people reading the pull
    /// request see it too.
    pub fn comment_on_pull_request(
        &self,
        repository_id: &str,
        worktree_name: &str,
        body: &str,
    ) -> Result<()> {
        let (_, worktree) = self.locate(repository_id, worktree_name)?;

        let output = Command::new("gh")
            .args(["pr", "review", &worktree.branch, "--comment", "--body", body])
            .current_dir(&worktree.path)
            .output()
            .context("gh could not be run")?;

        if !output.status.success() {
            bail!(
                "the review could not be posted: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }

        Ok(())
    }

    /// Merge the pull request on this worktree's branch.
    ///
    /// Squashed, because a card is one piece of work and its branch is the
    /// workings. The branch is left alone: deleting it is destroying something
    /// and belongs to whoever decides to.
    pub fn merge_pull_request(&self, repository_id: &str, worktree_name: &str) -> Result<String> {
        let (_, worktree) = self.locate(repository_id, worktree_name)?;

        let output = Command::new("gh")
            .args(["pr", "merge", &worktree.branch, "--squash"])
            .current_dir(&worktree.path)
            .output()
            .context("gh could not be run")?;

        if !output.status.success() {
            bail!(
                "the merge was refused: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }

    pub fn open_pull_request(
        &self,
        repository_id: &str,
        worktree_name: &str,
        title: &str,
        body: &str,
    ) -> Result<PullRequest> {
        let (repository, worktree) = self.locate(repository_id, worktree_name)?;

        let pending = git(&["status", "--porcelain"], Some(&worktree.path))?;
        if !pending.trim().is_empty() {
            let count = pending.lines().filter(|line| !line.is_empty()).count();
            bail!("commit the work first: {count} file(s) in {worktree_name} are not committed");
        }

        let base = repository.default_branch.clone();
        let ahead = git(
            &["rev-list", "--count", &format!("{base}..HEAD")],
            Some(&worktree.path),
        )?;
        if ahead.trim() == "0" {
            bail!("{worktree_name} has no commits on top of {base} to open a pull request with");
        }

        let origin = repository
            .remotes
            .iter()
            .find(|remote| remote.name == "origin")
            .or_else(|| repository.remotes.first())
            .ok_or_else(|| anyhow!("this repository has no remote to open a pull request against"))?;

        git(
            &["push", "-u", &origin.name, &worktree.branch],
            Some(&worktree.path),
        )?;

        let gh_available = Command::new("gh")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);

        if origin.provider == "github" && gh_available {
            let url = git_command(
                "gh",
                &[
                    "pr",
                    "create",
                    "--title",
                    title,
                    "--body",
                    body,
                    "--head",
                    &worktree.branch,
                    "--base",
                    &repository.default_branch,
                ],
                Some(&worktree.path),
            )?;

            return Ok(PullRequest {
                url: url.lines().last().unwrap_or_default().to_owned(),
                created: true,
                detail: "created with gh".to_owned(),
            });
        }

        let (owner, name) = match (origin.owner.as_deref(), origin.repo.as_deref()) {
            (Some(owner), Some(name)) => (owner, name),
            _ => {
                return Ok(PullRequest {
                    url: String::new(),
                    created: false,
                    detail: format!(
                        "{} is pushed to {}, which has no web address to open a pull request on",
                        worktree.branch, origin.name
                    ),
                })
            }
        };
        let host = origin.host.as_deref().unwrap_or("github.com");

        let url = match origin.provider.as_str() {
            "gitlab" => format!(
                "https://{host}/{owner}/{name}/-/merge_requests/new?merge_request[source_branch]={}",
                worktree.branch
            ),
            _ => format!(
                "https://{host}/{owner}/{name}/compare/{}...{}?expand=1",
                repository.default_branch, worktree.branch
            ),
        };

        Ok(PullRequest {
            url,
            created: false,
            detail: "branch pushed; open the link to finish".to_owned(),
        })
    }
}

fn git_command(program: &str, args: &[&str], cwd: Option<&Path>) -> Result<String> {
    let mut command = Command::new(program);
    command.args(args);
    if let Some(dir) = cwd {
        command.current_dir(dir);
    }

    let output = command.output()?;
    if !output.status.success() {
        bail!(
            "{program} {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("agentland-repo-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn a_folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("agentland-adopt-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn a_registry(name: &str) -> RepoRegistry {
        RepoRegistry::new(std::env::temp_dir().join(format!("agentland-adopt-data-{name}")))
    }

    #[test]
    fn a_machine_with_nobody_configured_is_still_given_somebody() {
        let dir = std::env::temp_dir().join("agentland-identity-probe");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        git(&["init", "-b", "main"], Some(&dir)).unwrap();

        // An empty local value answers for the machine whatever the machine
        // running this test has configured for itself.
        git(&["config", "--local", "user.email", ""], Some(&dir)).unwrap();

        let given = who_is_committing(&dir);
        assert!(
            given.iter().any(|piece| piece.starts_with("user.email=")),
            "git was given nobody to blame: {given:?}"
        );

        git(&["config", "--local", "user.email", "someone@example.com"], Some(&dir)).unwrap();
        assert!(
            who_is_committing(&dir).is_empty(),
            "somebody who has a name of their own keeps it"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_plain_folder_becomes_a_project_with_something_to_branch_from() {
        let dir = a_folder("with-files");
        fs::write(dir.join("notes.md"), "a sketch\n").unwrap();

        let repository = a_registry("with-files").adopt(&dir).unwrap();

        assert_eq!(repository.default_branch, "main");
        assert!(dir.join(".git").exists(), "it is a repository now");
        let log = git(&["log", "--oneline"], Some(&dir)).unwrap();
        assert!(log.contains("start tracking this folder"), "and it has a commit: {log}");
    }

    #[test]
    fn an_empty_folder_still_gets_a_commit_to_branch_from() {
        let dir = a_folder("empty");

        a_registry("empty").adopt(&dir).unwrap();

        let log = git(&["log", "--oneline"], Some(&dir)).unwrap();
        assert_eq!(log.lines().count(), 1, "one empty commit: {log}");
    }

    #[test]
    fn a_folder_that_is_already_a_repository_is_left_exactly_as_it_is() {
        let dir = a_folder("already");
        git(&["init", "-b", "trunk"], Some(&dir)).unwrap();
        fs::write(dir.join("thing.txt"), "x\n").unwrap();
        git(&["add", "-A"], Some(&dir)).unwrap();
        git(&["-c", "user.email=t@e", "-c", "user.name=t", "commit", "-m", "first"], Some(&dir)).unwrap();

        let repository = a_registry("already").adopt(&dir).unwrap();

        assert_eq!(repository.default_branch, "trunk", "nothing was re-initialised");
        let log = git(&["log", "--oneline"], Some(&dir)).unwrap();
        assert_eq!(log.lines().count(), 1, "no extra commit was made");
    }

    #[test]
    fn a_project_can_be_forgotten_without_touching_the_folder() {
        let dir = a_folder("forget");
        fs::write(dir.join("notes.md"), "x\n").unwrap();
        let registry = a_registry("forget");
        let repository = registry.adopt(&dir).unwrap();

        registry.forget(&repository.id).unwrap();

        assert!(dir.exists(), "the folder is still there");
        assert!(registry.repositories().is_empty());
        assert!(registry.forget(&repository.id).is_err(), "and it is gone from the registry");
    }

    #[test]
    fn a_worktree_is_told_the_tools_agentland_wrote_are_wanted() {
        let dir = a_folder("trust");

        write_mcp_config(&dir, &dir.join("data"));

        let settings: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.join(".claude/settings.local.json")).unwrap())
                .unwrap();

        assert_eq!(settings["enableAllProjectMcpServers"], serde_json::Value::Bool(true));
        assert!(dir.join(".mcp.json").exists());
    }

    #[test]
    fn whatever_else_is_in_the_settings_is_left_alone() {
        let dir = a_folder("trust-existing");
        fs::create_dir_all(dir.join(".claude")).unwrap();
        fs::write(
            dir.join(".claude/settings.local.json"),
            r#"{"permissions":{"allow":["Bash(npm test)"]}}"#,
        )
        .unwrap();

        write_mcp_config(&dir, &dir.join("data"));

        let settings: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.join(".claude/settings.local.json")).unwrap())
                .unwrap();

        assert_eq!(settings["enableAllProjectMcpServers"], serde_json::Value::Bool(true));
        assert!(settings["permissions"]["allow"].is_array(), "somebody else's setting survived");
    }

    #[test]
    fn the_tool_is_copied_out_of_the_build_directory() {
        let dir = scratch("kept");
        let built = dir.join("built");
        fs::write(&built, b"first").unwrap();

        let kept = kept_tool(&built, &dir).expect("a copy");

        assert_eq!(kept, dir.join("bin").join(TOOL_NAME));
        assert_eq!(fs::read(&kept).unwrap(), b"first");
        assert_ne!(kept, built);
    }

    #[test]
    fn a_rebuilt_tool_replaces_the_copy_without_touching_the_old_file() {
        let dir = scratch("replaced");
        let built = dir.join("built");
        fs::write(&built, b"first").unwrap();
        let kept = kept_tool(&built, &dir).expect("a copy");
        let opened = fs::File::open(&kept).unwrap();

        fs::write(&built, b"second build").unwrap();
        let again = kept_tool(&built, &dir).expect("a fresh copy");

        assert_eq!(again, kept);
        assert_eq!(fs::read(&kept).unwrap(), b"second build");

        let mut held = String::new();
        use std::io::Read;
        let mut opened = opened;
        opened.read_to_string(&mut held).unwrap();
        assert_eq!(held, "first", "a running agent keeps the file it opened");
    }

    #[test]
    fn an_unchanged_tool_is_left_alone() {
        let dir = scratch("unchanged");
        let built = dir.join("built");
        fs::write(&built, b"same").unwrap();
        let kept = kept_tool(&built, &dir).expect("a copy");
        let stamped = fs::metadata(&kept).unwrap().modified().unwrap();

        let again = kept_tool(&built, &dir).expect("the same copy");

        assert_eq!(again, kept);
        assert_eq!(fs::metadata(&kept).unwrap().modified().unwrap(), stamped);
    }
}
