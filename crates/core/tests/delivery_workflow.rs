//! Managed HTTP delivery must honor independent settings, even for agents.
use agentland_core::{
    auth::{Scope, TokenStore},
    board::CreateTask,
    delivery::Policy,
    project_settings::ProjectSettings,
    Board, PtyManager, RepoRegistry, ServerConfig,
};
use serde_json::{json, Value};
use std::{fs, path::Path, process::Command, sync::Arc, time::Duration};
fn git(path: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().into()
}
async fn call(
    client: &reqwest::Client,
    root: &str,
    token: &str,
    path: &str,
    body: Value,
) -> (u16, Value) {
    let response = client
        .post(format!("{root}{path}"))
        .header("x-auth-token", token)
        .json(&body)
        .send()
        .await
        .unwrap();
    (response.status().as_u16(), response.json().await.unwrap())
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn delivery_stages_and_manual_trigger_are_enforced_at_the_api() {
    let base = std::env::temp_dir().join(format!("agentland-delivery-http-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let project = base.join("demo");
    fs::create_dir_all(&project).unwrap();
    git(&project, &["init", "-q", "-b", "main"]);
    git(&project, &["config", "user.name", "Test"]);
    git(&project, &["config", "user.email", "test@example.com"]);
    fs::write(project.join("README.md"), "demo").unwrap();
    git(&project, &["add", "-A"]);
    git(&project, &["commit", "-qm", "chore: initialize"]);
    let remote = base.join("remote.git");
    git(&base, &["init", "--bare", "-q", remote.to_str().unwrap()]);
    git(
        &project,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    let data = base.join("data");
    let repos = RepoRegistry::new(data.clone());
    repos.register(&project).unwrap();
    let worktree = repos.create_worktree("demo", "work").unwrap();
    let policy = Policy {
        auto_commit: true,
        auto_pr: true,
        ..Default::default()
    };
    repos
        .set_settings(
            "demo",
            ProjectSettings {
                delivery: Some(policy.clone()),
                ..Default::default()
            },
        )
        .unwrap();
    let board = Board::new(data.clone());
    let task = board
        .create(CreateTask {
            title: "fix: repair startup".into(),
            body: "Regression fixed".into(),
            repository_id: "demo".into(),
            worktree: Some("work".into()),
            issue: None,
        })
        .unwrap();
    fs::write(worktree.path.join("change.txt"), "task work").unwrap();
    let primary = "local-test-only";
    let tokens = TokenStore::new(primary.into(), data.clone());
    let agent = tokens
        .issue("test agent".into(), Scope::Agent)
        .secret()
        .to_owned();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let root = format!("http://127.0.0.1:{port}");
    let config = ServerConfig {
        host: "127.0.0.1".into(),
        port,
        token: primary.into(),
        allowed_hosts: vec![format!("127.0.0.1:{port}")],
        allowed_origins: vec![],
        data_dir: data,
    };
    let server = tokio::spawn(agentland_core::serve(Arc::new(PtyManager::new()), config));
    let client = reqwest::Client::new();
    for _ in 0..250 {
        if client
            .get(format!("{root}/tasks"))
            .header("x-auth-token", primary)
            .send()
            .await
            .is_ok()
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let route = format!("/tasks/{}/workflow", task.id);
    let (status, result) = call(&client, &root, &agent, &route, json!({})).await;
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["performed"], json!(["commit"]));
    assert!(
        result["waiting"]
            .as_str()
            .unwrap()
            .contains("push is required"),
        "{result}"
    );
    assert!(git(&remote, &["branch", "--list", "agent/work"]).is_empty());
    assert_eq!(
        git(&worktree.path, &["log", "-1", "--format=%s"]),
        format!("fix: repair startup [{}]", task.id)
    );
    let before = git(&worktree.path, &["rev-parse", "HEAD"]);
    let (_, retry) = call(&client, &root, &agent, &route, json!({})).await;
    assert_eq!(retry["performed"], json!([]));
    assert_eq!(git(&worktree.path, &["rev-parse", "HEAD"]), before);
    let push = "/repos/demo/worktrees/work/push";
    assert_ne!(
        call(&client, &root, &agent, push, json!({"task_id": task.id}))
            .await
            .0,
        200
    );
    assert_eq!(
        call(&client, &root, primary, push, json!({"task_id": task.id}))
            .await
            .0,
        200
    );
    assert_eq!(
        git(&remote, &["rev-parse", "refs/heads/agent/work"]),
        before
    );
    let manual = ProjectSettings {
        delivery: Some(Policy {
            auto_commit: true,
            trigger: "manual".into(),
            ..Default::default()
        }),
        ..Default::default()
    };
    assert_ne!(
        call(
            &client,
            &root,
            &agent,
            "/repos/demo/settings",
            json!(manual)
        )
        .await
        .0,
        200
    );
    assert_ne!(
        call(
            &client,
            &root,
            &agent,
            "/dispatch/merge-policy",
            json!({"merge_when_checks_pass":true})
        )
        .await
        .0,
        200
    );
    assert_eq!(
        call(
            &client,
            &root,
            primary,
            "/repos/demo/settings",
            json!(manual)
        )
        .await
        .0,
        200
    );
    fs::write(worktree.path.join("change.txt"), "another change").unwrap();
    let (_, blocked) = call(&client, &root, &agent, &route, json!({})).await;
    assert!(blocked["waiting"].as_str().unwrap().contains("person"));
    assert_eq!(git(&worktree.path, &["rev-parse", "HEAD"]), before);
    let (_, completed) = call(&client, &root, primary, &route, json!({})).await;
    assert_eq!(completed["performed"], json!(["commit"]));
    assert!(completed["waiting"].is_null());
    assert_eq!(
        git(&remote, &["rev-parse", "refs/heads/agent/work"]),
        before,
        "manual workflow must not enable push"
    );
    server.abort();
}
