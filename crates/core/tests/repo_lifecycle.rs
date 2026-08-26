use std::fs;
use std::path::PathBuf;
use std::process::Command;

use agentland_core::RepoRegistry;

fn git(args: &[&str], cwd: &PathBuf) {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("git should run");
    assert!(output.status.success(), "git {args:?} failed");
}

fn scratch(name: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!("agentland-test-{name}"));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).expect("scratch dir");
    base
}

#[test]
fn worktree_lifecycle_allocates_and_releases_a_port() {
    let base = scratch("lifecycle");
    let repo_path = base.join("demo");
    fs::create_dir_all(&repo_path).unwrap();

    git(&["init", "-q", "-b", "main"], &repo_path);
    git(&["config", "user.email", "test@example.com"], &repo_path);
    git(&["config", "user.name", "test"], &repo_path);
    fs::write(repo_path.join("README.md"), "demo").unwrap();
    git(&["add", "-A"], &repo_path);
    git(&["commit", "-qm", "init"], &repo_path);

    let registry = RepoRegistry::new(base.join("data"));
    let repository = registry.register(&repo_path).expect("register");
    assert_eq!(repository.id, "demo");
    assert_eq!(repository.default_branch, "main");

    let worktree = registry.create_worktree("demo", "work1").expect("create");
    assert!(worktree.path.join("README.md").exists());
    assert_eq!(worktree.branch, "agent/work1");
    assert!((4100..=4999).contains(&worktree.port));

    let second = registry
        .create_worktree("demo", "work2")
        .expect("create second");
    assert_ne!(worktree.port, second.port);

    fs::write(worktree.path.join("dirty.txt"), "uncommitted").unwrap();
    let statuses = registry.worktrees();
    let dirty = statuses
        .iter()
        .find(|entry| entry.worktree.name == "work1")
        .expect("work1 present");
    assert_eq!(dirty.dirty_files, 1);

    let refused = registry.remove_worktree("demo", "work1", false);
    assert!(
        refused.is_err(),
        "a dirty worktree must not be removed silently"
    );

    registry
        .remove_worktree("demo", "work1", true)
        .expect("forced remove");
    assert!(!worktree.path.exists());
    assert!(registry.ports().assignment("demo/work1").is_none());
    assert!(registry.ports().assignment("demo/work2").is_some());

    let reopened = RepoRegistry::new(base.join("data"));
    assert_eq!(reopened.repositories().len(), 1);
    assert_eq!(reopened.worktrees().len(), 1);
}

#[test]
fn a_pull_request_refuses_work_that_is_not_committed_yet() {
    let base = scratch("commit-gate");
    let repo_path = base.join("demo");
    fs::create_dir_all(&repo_path).unwrap();

    git(&["init", "-q", "-b", "main"], &repo_path);
    git(&["config", "user.email", "test@example.com"], &repo_path);
    git(&["config", "user.name", "test"], &repo_path);
    fs::write(repo_path.join("README.md"), "demo").unwrap();
    git(&["add", "-A"], &repo_path);
    git(&["commit", "-qm", "init"], &repo_path);

    let bare = base.join("origin.git");
    git(&["init", "-q", "--bare", bare.to_str().unwrap()], &base);
    git(&["remote", "add", "origin", bare.to_str().unwrap()], &repo_path);

    let registry = RepoRegistry::new(base.join("data"));
    registry.register(&repo_path).expect("register");
    let worktree = registry.create_worktree("demo", "work1").expect("worktree");

    let empty = registry.commit("demo", "work1", "nothing to see");
    assert!(
        empty.unwrap_err().to_string().contains("nothing to commit"),
        "an empty commit is refused"
    );

    fs::write(worktree.path.join("feature.txt"), "the agent's work").unwrap();

    let refused = registry
        .open_pull_request("demo", "work1", "title", "body")
        .expect_err("uncommitted work cannot become a pull request");
    assert!(refused.to_string().contains("commit the work first"), "{refused}");

    let blank = registry.commit("demo", "work1", "   ");
    assert!(blank.unwrap_err().to_string().contains("needs a message"));

    let commit = registry
        .commit("demo", "work1", "feat(demo): add the feature")
        .expect("commit");
    assert_eq!(commit.files, 1);
    assert_eq!(commit.branch, "agent/work1");
    assert!(!commit.sha.is_empty());

    let opened = registry
        .open_pull_request("demo", "work1", "Add the feature", "body")
        .expect("a committed branch can be pushed");
    assert!(!opened.created, "a local remote hosts no pull requests");
    assert!(
        opened.detail.contains("no web address"),
        "it says why instead of failing: {}",
        opened.detail
    );

    let pushed = std::process::Command::new("git")
        .args(["branch", "--list", "agent/work1"])
        .current_dir(&bare)
        .output()
        .expect("git");
    assert!(
        String::from_utf8_lossy(&pushed.stdout).contains("agent/work1"),
        "the branch reached the remote"
    );
}
