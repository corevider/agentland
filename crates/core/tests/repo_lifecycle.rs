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

#[test]
fn a_checkout_that_is_gone_says_so_instead_of_failing_in_git() {
    let base = scratch("gone-checkout");
    let repo_path = base.join("demo");
    fs::create_dir_all(&repo_path).unwrap();
    git(&["init", "-q", "-b", "main"], &repo_path);
    git(&["config", "user.email", "test@example.com"], &repo_path);
    git(&["config", "user.name", "test"], &repo_path);
    fs::write(repo_path.join("README.md"), "demo").unwrap();
    git(&["add", "-A"], &repo_path);
    git(&["commit", "-qm", "init"], &repo_path);

    let registry = RepoRegistry::new(base.join("data"));
    registry.register(&repo_path).expect("register");
    fs::remove_dir_all(&repo_path).unwrap();

    let refused = registry
        .create_worktree("demo", "work1")
        .expect_err("no checkout, no worktree");

    assert!(refused.to_string().contains("is gone"), "{refused}");
}

fn rust_project(base: &PathBuf) -> PathBuf {
    let repo_path = base.join("demo");
    fs::create_dir_all(&repo_path).unwrap();
    git(&["init", "-q", "-b", "main"], &repo_path);
    git(&["config", "user.email", "test@example.com"], &repo_path);
    git(&["config", "user.name", "test"], &repo_path);
    fs::write(
        repo_path.join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    git(&["add", "-A"], &repo_path);
    git(&["commit", "-qm", "init"], &repo_path);
    repo_path
}

#[test]
fn worktrees_of_one_project_compile_into_one_build_directory() {
    let base = scratch("shared-build-cache");
    let repo_path = rust_project(&base);

    let registry = RepoRegistry::new(base.join("data"));
    registry.register(&repo_path).expect("register");
    let first = registry.create_worktree("demo", "work1").expect("first");
    let second = registry.create_worktree("demo", "work2").expect("second");

    let config = |worktree: &PathBuf| {
        fs::read_to_string(worktree.join(".cargo").join("config.toml")).expect("cargo config")
    };

    let written = config(&first.path);
    assert_eq!(
        written,
        config(&second.path),
        "both worktrees are pointed at the same build directory"
    );

    let target = written
        .split('"')
        .nth(1)
        .expect("the config names a directory")
        .to_owned();
    assert!(
        PathBuf::from(&target).is_dir(),
        "the shared build directory is made, not merely named: {target}"
    );
    assert!(
        target.contains("caches") && target.ends_with("cargo-target"),
        "it lives in Agentland's own cache, away from either checkout: {target}"
    );

    let status = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(&first.path)
        .output()
        .expect("git");
    assert!(
        String::from_utf8_lossy(&status.stdout).trim().is_empty(),
        "the config Agentland wrote is not work the agent has to explain"
    );

    registry.remove_worktree("demo", "work1", true).expect("remove");
    registry.remove_worktree("demo", "work2", true).expect("remove");
    registry.forget("demo").expect("forget");
    assert!(
        !PathBuf::from(&target).exists(),
        "a forgotten project leaves no gigabytes behind"
    );
}

#[test]
fn a_project_that_chose_its_own_build_directory_keeps_it() {
    let base = scratch("own-cargo-config");
    let repo_path = rust_project(&base);
    let theirs = "[build]\ntarget-dir = \"somewhere-they-picked\"\n";
    fs::create_dir_all(repo_path.join(".cargo")).unwrap();
    fs::write(repo_path.join(".cargo").join("config.toml"), theirs).unwrap();
    git(&["add", "-A"], &repo_path);
    git(&["commit", "-qm", "cargo config"], &repo_path);

    let registry = RepoRegistry::new(base.join("data"));
    registry.register(&repo_path).expect("register");
    let worktree = registry.create_worktree("demo", "work1").expect("worktree");

    assert_eq!(
        fs::read_to_string(worktree.path.join(".cargo").join("config.toml")).unwrap(),
        theirs,
        "a config the project ships is not overwritten"
    );
}

#[test]
fn a_project_with_no_cargo_manifest_is_left_as_it_is() {
    let base = scratch("no-cargo");
    let repo_path = base.join("demo");
    fs::create_dir_all(&repo_path).unwrap();
    git(&["init", "-q", "-b", "main"], &repo_path);
    git(&["config", "user.email", "test@example.com"], &repo_path);
    git(&["config", "user.name", "test"], &repo_path);
    fs::write(repo_path.join("package.json"), "{}").unwrap();
    git(&["add", "-A"], &repo_path);
    git(&["commit", "-qm", "init"], &repo_path);

    let registry = RepoRegistry::new(base.join("data"));
    registry.register(&repo_path).expect("register");
    let worktree = registry.create_worktree("demo", "work1").expect("worktree");

    assert!(
        !worktree.path.join(".cargo").exists(),
        "nothing here builds with cargo, so nothing here is configured for it"
    );
}
