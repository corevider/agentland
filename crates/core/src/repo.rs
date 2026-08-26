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

    pub fn open_pull_request(
        &self,
        repository_id: &str,
        worktree_name: &str,
        title: &str,
        body: &str,
    ) -> Result<PullRequest> {
        let (repository, worktree) = self.locate(repository_id, worktree_name)?;

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
            _ => bail!("branch pushed, but this remote has no web address to open"),
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
