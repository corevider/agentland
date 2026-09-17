//! Test programs run against a detached copy of the reviewed commit. This is
//! checkout isolation, not a security sandbox for untrusted repository code.
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Deserialize)]
pub struct Request {
    pub task_id: String,
    pub head_sha: String,
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub directory: String,
    pub by: String,
    #[serde(default)]
    pub checkout: Option<PathBuf>,
}

#[derive(Serialize)]
pub struct Report {
    pub head_sha: String,
    pub passed: bool,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub source_changed: bool,
    pub output: String,
    pub checkout: PathBuf,
}

pub fn allowed(program: &str, args: &[String]) -> bool {
    // Use the explicit, containment-checked directory field to choose a package.
    if args.iter().any(|arg| {
        [
            "--prefix",
            "--cwd",
            "--dir",
            "--directory",
            "--manifest-path",
            "--target-dir",
            "--work-tree",
            "-C",
        ]
        .iter()
        .any(|flag| arg == flag || arg.starts_with(&format!("{flag}=")))
    }) {
        return false;
    }
    let first = args.first().map(String::as_str).unwrap_or("");
    match program {
        "cargo" => matches!(first, "test" | "check" | "clippy" | "build"),
        "npm" => {
            matches!(first, "test" | "ci" | "install")
                || (first == "run"
                    && args.get(1).is_some_and(|s| {
                        matches!(s.as_str(), "test" | "build" | "lint" | "typecheck")
                            || s.starts_with("test:")
                    }))
        }
        "pnpm" | "yarn" => matches!(first, "test" | "install" | "build" | "lint" | "typecheck"),
        "pytest" => true,
        "python" | "python3" => {
            first == "-m"
                && args
                    .get(1)
                    .is_some_and(|s| matches!(s.as_str(), "pytest" | "unittest"))
        }
        "go" => matches!(first, "test" | "vet" | "build"),
        _ => false,
    }
}

pub fn is_test(program: &str, args: &[String]) -> bool {
    let first = args.first().map(String::as_str).unwrap_or("");
    match program {
        "cargo" | "go" | "yarn" | "pnpm" => first == "test",
        "npm" => {
            first == "test"
                || (first == "run"
                    && args
                        .get(1)
                        .is_some_and(|s| s == "test" || s.starts_with("test:")))
        }
        "pytest" => true,
        "python" | "python3" => {
            first == "-m"
                && args
                    .get(1)
                    .is_some_and(|s| matches!(s.as_str(), "pytest" | "unittest"))
        }
        _ => false,
    }
}

pub fn prepare(source: &Path, root: &Path, sha: &str) -> Result<PathBuf> {
    anyhow::ensure!(
        (sha.len() == 40 || sha.len() == 64) && sha.bytes().all(|c| c.is_ascii_hexdigit()),
        "expected a full commit SHA"
    );
    std::fs::create_dir_all(root)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let checkout = root.join(format!(
        "{stamp}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let cloned = crate::exec::command("git")
        .args(["clone", "--no-hardlinks", "--no-checkout", "--"])
        .arg(source)
        .arg(&checkout)
        .output()?;
    anyhow::ensure!(
        cloned.status.success(),
        "cannot isolate checkout: {}",
        String::from_utf8_lossy(&cloned.stderr)
    );
    let checked = crate::exec::command("git")
        .args(["checkout", "--detach", sha])
        .current_dir(&checkout)
        .output()?;
    anyhow::ensure!(
        checked.status.success(),
        "cannot check out reviewed commit: {}",
        String::from_utf8_lossy(&checked.stderr)
    );
    Ok(checkout)
}

pub async fn run(checkout: PathBuf, request: &Request) -> Result<Report> {
    run_with_timeout(checkout, request, Duration::from_secs(300)).await
}

async fn run_with_timeout(
    checkout: PathBuf,
    request: &Request,
    timeout: Duration,
) -> Result<Report> {
    if !allowed(&request.program, &request.args) {
        bail!("use a supported test, build, lint or dependency-install command");
    }
    let directory = checkout
        .join(&request.directory)
        .canonicalize()
        .context("test directory does not exist")?;
    anyhow::ensure!(
        directory.starts_with(checkout.canonicalize()?),
        "test directory must stay inside the isolated checkout"
    );
    // File-backed logs avoid keeping arbitrary test output in the core's RAM.
    let log = checkout.with_extension("log");
    let output = std::fs::File::create(&log)?;
    let mut command = crate::exec::tokio_command(&request.program);
    command
        .env_remove("CARGO_TARGET_DIR")
        .args(&request.args)
        .current_dir(&directory)
        .stdin(std::process::Stdio::null())
        .stdout(output.try_clone()?)
        .stderr(output)
        .kill_on_drop(true);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.as_std_mut().process_group(0);
    }
    let mut child = command.spawn()?;
    let child_id = child.id();
    let status = tokio::time::timeout(timeout, child.wait()).await;
    let (passed, exit_code, timed_out) = match status {
        Ok(result) => {
            let status = result?;
            (status.success(), status.code(), false)
        }
        Err(_) => {
            if let Some(pid) = child_id {
                #[cfg(unix)]
                let _ = crate::exec::tokio_command("kill")
                    .args(["-KILL", "--", &format!("-{pid}")])
                    .output()
                    .await;
                #[cfg(windows)]
                let _ = crate::exec::tokio_command("taskkill")
                    .args(["/PID", &pid.to_string(), "/T", "/F"])
                    .output()
                    .await;
            }
            let _ = child.kill().await;
            (false, None, true)
        }
    };
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(log)?;
    let size = file.metadata()?.len();
    file.seek(SeekFrom::Start(size.saturating_sub(32 * 1024)))?;
    let mut bytes = Vec::new();
    file.take(32 * 1024).read_to_end(&mut bytes)?;
    let unchanged = crate::exec::command("git")
        .args(["diff", "--quiet", &request.head_sha, "--"])
        .current_dir(&checkout)
        .status()?;
    let head = crate::exec::command("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&checkout)
        .output()?;
    let source_changed = !unchanged.success()
        || !head.status.success()
        || String::from_utf8_lossy(&head.stdout).trim() != request.head_sha;
    Ok(Report {
        head_sha: request.head_sha.clone(),
        passed: passed && !source_changed,
        exit_code,
        timed_out,
        source_changed,
        output: String::from_utf8_lossy(&bytes).into_owned(),
        checkout,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_test_runners_without_a_shell() {
        assert!(allowed("cargo", &["test".into(), "--workspace".into()]));
        assert!(allowed("python3", &["-m".into(), "pytest".into()]));
        assert!(!allowed("sh", &["-c".into(), "cargo test".into()]));
        assert!(!allowed("git", &["push".into()]));
        assert!(!allowed("npm", &["publish".into()]));
    }
    #[tokio::test]
    async fn tests_run_on_the_reviewed_commit_without_touching_the_authors_work() {
        let root = std::env::temp_dir().join(format!("proving-{}", crate::generate_token()));
        let source = root.join("source");
        std::fs::create_dir_all(source.join("src")).unwrap();
        let git = |args: &[&str]| {
            let result = crate::exec::command("git")
                .args(args)
                .current_dir(&source)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            String::from_utf8_lossy(&result.stdout).trim().to_owned()
        };
        git(&["init"]);
        std::fs::write(
            source.join("Cargo.toml"),
            "[package]\nname=\"isolated-probe\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
        )
        .unwrap();
        std::fs::write(source.join("src/lib.rs"), "#[test] fn proof() { assert_eq!(std::fs::read_to_string(\"data.txt\").unwrap(), \"reviewed\"); std::fs::write(\"artifact.txt\", \"passed\").unwrap(); }").unwrap();
        std::fs::write(source.join("data.txt"), "reviewed").unwrap();
        git(&["add", "-A"]);
        git(&[
            "-c",
            "user.name=test",
            "-c",
            "user.email=test@local",
            "commit",
            "-m",
            "base",
        ]);
        let sha = git(&["rev-parse", "HEAD"]);
        std::fs::write(source.join("data.txt"), "author's uncommitted change").unwrap();
        let checkout = prepare(&source, &root.join("runs"), &sha).unwrap();
        let request = Request {
            task_id: "t1".into(),
            head_sha: sha,
            program: "cargo".into(),
            args: vec!["test".into(), "--offline".into()],
            directory: String::new(),
            by: "tester".into(),
            checkout: None,
        };
        let report = run(checkout.clone(), &request).await.unwrap();
        assert!(report.passed, "{}", report.output);
        assert!(checkout.join("artifact.txt").is_file());
        assert!(!source.join("artifact.txt").exists());
        assert_eq!(
            std::fs::read_to_string(source.join("data.txt")).unwrap(),
            "author's uncommitted change"
        );
        std::fs::write(checkout.join("data.txt"), "wrong").unwrap();
        let report = run(checkout.clone(), &request).await.unwrap();
        assert!(!report.passed);
        assert!(report.source_changed);
        let report = run_with_timeout(checkout, &request, Duration::from_nanos(1))
            .await
            .unwrap();
        assert!(report.timed_out);
        assert!(!report.passed);
        std::fs::remove_dir_all(root).unwrap();
    }
}
