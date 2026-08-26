use std::io::{self, BufRead, Write};

use serde_json::{json, Value};

const PROTOCOL_VERSION: &str = "2025-06-18";

struct Core {
    base: String,
    token: String,
    client: reqwest::blocking::Client,
}

impl Core {
    fn from_env() -> Self {
        let port = std::env::var("AGENTLAND_PORT").unwrap_or_else(|_| "9470".to_owned());
        Self {
            base: format!("http://127.0.0.1:{port}"),
            token: std::env::var("AGENTLAND_TOKEN").unwrap_or_default(),
            client: reqwest::blocking::Client::new(),
        }
    }

    fn call(&self, method: &str, path: &str, body: Option<Value>) -> Result<Value, String> {
        let url = format!("{}{path}", self.base);
        let mut request = match method {
            "POST" => self.client.post(&url),
            "DELETE" => self.client.delete(&url),
            _ => self.client.get(&url),
        }
        .header("x-auth-token", &self.token);

        if let Some(payload) = body {
            request = request.json(&payload);
        }

        let response = request.send().map_err(|error| error.to_string())?;
        let status = response.status();
        let text = response.text().unwrap_or_default();

        if !status.is_success() {
            return Err(format!("{status}: {text}"));
        }

        if text.trim().is_empty() {
            return Ok(json!({ "ok": true }));
        }

        serde_json::from_str(&text).map_err(|error| error.to_string())
    }
}

fn tools() -> Value {
    json!([
        {
            "name": "task_list",
            "description": "List every card on the board with its column, assignee and branch.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "task_create",
            "description": "Put a new card on the board. Use this instead of keeping work in your head.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "title": { "type": "string" },
                    "body": { "type": "string" },
                    "repository_id": { "type": "string" }
                },
                "required": ["title", "repository_id"]
            }
        },
        {
            "name": "task_move",
            "description": "Move a card to backlog, assigned, working, review or done.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "column": { "type": "string" }
                },
                "required": ["id", "column"]
            }
        },
        {
            "name": "crew_list",
            "description": "List the crew: name, role, engine, worktree and current state.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "crew_delegate",
            "description": "Hand a card to X, who picks a free agent within the concurrency caps and explains the choice. Returns the decision and its reason.",
            "inputSchema": {
                "type": "object",
                "properties": { "task_id": { "type": "string" } },
                "required": ["task_id"]
            }
        },
        {
            "name": "repo_list",
            "description": "List registered repositories with their remotes and default branch.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "repo_worktrees",
            "description": "List a repository's worktrees with branch, allocated port and uncommitted count.",
            "inputSchema": {
                "type": "object",
                "properties": { "repository_id": { "type": "string" } },
                "required": ["repository_id"]
            }
        },
        {
            "name": "crew_message",
            "description": "Send a message to another agent by id. Refused when messaging is paused or the grant is missing.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "to": { "type": "string" },
                    "text": { "type": "string" }
                },
                "required": ["to", "text"]
            }
        },
        {
            "name": "memory_propose",
            "description": "Propose something the crew should remember. It is masked for secrets and stays unused until a human approves it.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "text": { "type": "string" },
                    "scope": { "type": "string", "enum": ["workspace", "repository", "agent"] },
                    "scope_id": { "type": "string" }
                },
                "required": ["text"]
            }
        },
        {
            "name": "integration_list",
            "description": "List connected services. Their credentials stay on the app's side and never reach you.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "integration_call",
            "description": "Ask a connected service for data. Agentland makes the call and returns the result; you never handle the token.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "integration_id": { "type": "string" },
                    "operation": { "type": "string" },
                    "params": { "type": "object" }
                },
                "required": ["integration_id", "operation"]
            }
        },
        {
            "name": "request_approval",
            "description": "Ask the human to approve something before you do it. Returns an approval id; poll approval_status for the answer.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "summary": { "type": "string" },
                    "detail": { "type": "string" }
                },
                "required": ["summary"]
            }
        },
        {
            "name": "approval_status",
            "description": "Read every approval and its verdict: pending, approved or rejected.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "repo_review",
            "description": "Read the diff for a worktree: committed range, working tree and untracked files.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "repository_id": { "type": "string" },
                    "worktree": { "type": "string" }
                },
                "required": ["repository_id", "worktree"]
            }
        }
    ])
}

fn call_tool(core: &Core, name: &str, arguments: &Value) -> Result<Value, String> {
    let text = |key: &str| -> Result<String, String> {
        arguments
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| format!("{key} is required"))
    };

    match name {
        "task_list" => core.call("GET", "/tasks", None),
        "task_create" => core.call(
            "POST",
            "/tasks",
            Some(json!({
                "title": text("title")?,
                "body": arguments.get("body").and_then(Value::as_str).unwrap_or_default(),
                "repository_id": text("repository_id")?,
            })),
        ),
        "task_move" => core.call(
            "POST",
            &format!("/tasks/{}/move", text("id")?),
            Some(json!({ "column": text("column")? })),
        ),
        "crew_list" => core.call("GET", "/agents", None),
        "crew_delegate" => core.call("POST", &format!("/dispatch/tasks/{}", text("task_id")?), None),
        "repo_list" => core.call("GET", "/repos", None),
        "crew_message" => core.call(
            "POST",
            "/mail",
            Some(json!({
                "from": std::env::var("AGENTLAND_AGENT").unwrap_or_else(|_| "unknown".to_owned()),
                "to": text("to")?,
                "text": text("text")?,
            })),
        ),
        "memory_propose" => core.call(
            "POST",
            "/memories",
            Some(json!({
                "text": text("text")?,
                "scope": arguments.get("scope").and_then(Value::as_str).unwrap_or("workspace"),
                "scope_id": arguments.get("scope_id").and_then(Value::as_str).unwrap_or_default(),
                "proposed_by": std::env::var("AGENTLAND_AGENT").unwrap_or_else(|_| "unknown".to_owned()),
            })),
        ),
        "integration_list" => core.call("GET", "/integrations", None),
        "request_approval" => core.call(
            "POST",
            "/approvals",
            Some(json!({
                "summary": text("summary")?,
                "detail": arguments.get("detail").and_then(Value::as_str).unwrap_or_default(),
                "requested_by": std::env::var("AGENTLAND_AGENT").unwrap_or_else(|_| "unknown".to_owned()),
            })),
        ),
        "approval_status" => core.call("GET", "/approvals", None),
        "integration_call" => core.call(
            "POST",
            "/integrations/call",
            Some(json!({
                "integration_id": text("integration_id")?,
                "operation": text("operation")?,
                "params": arguments.get("params").cloned().unwrap_or_else(|| json!({})),
            })),
        ),
        "repo_worktrees" => core.call(
            "GET",
            &format!("/repos/{}/worktrees", text("repository_id")?),
            None,
        ),
        "repo_review" => core.call(
            "GET",
            &format!(
                "/repos/{}/worktrees/{}/review",
                text("repository_id")?,
                text("worktree")?
            ),
            None,
        ),
        other => Err(format!("unknown tool: {other}")),
    }
}

fn respond(id: Option<&Value>, result: Value) {
    let Some(id) = id else {
        return;
    };

    let message = json!({ "jsonrpc": "2.0", "id": id, "result": result });
    let mut stdout = io::stdout().lock();
    let _ = writeln!(stdout, "{message}");
    let _ = stdout.flush();
}

fn main() {
    let core = Core::from_env();
    let stdin = io::stdin();

    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }

        let Ok(request): Result<Value, _> = serde_json::from_str(&line) else {
            continue;
        };

        let method = request.get("method").and_then(Value::as_str).unwrap_or("");
        let id = request.get("id");

        match method {
            "initialize" => respond(
                id,
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "agentland", "version": env!("CARGO_PKG_VERSION") }
                }),
            ),
            "tools/list" => respond(id, json!({ "tools": tools() })),
            "tools/call" => {
                let params = request.get("params").cloned().unwrap_or_else(|| json!({}));
                let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
                let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));

                let result = match call_tool(&core, name, &arguments) {
                    Ok(value) => json!({
                        "content": [{ "type": "text", "text": serde_json::to_string_pretty(&value).unwrap_or_default() }],
                        "isError": false
                    }),
                    Err(message) => json!({
                        "content": [{ "type": "text", "text": message }],
                        "isError": true
                    }),
                };

                respond(id, result);
            }
            "ping" => respond(id, json!({})),
            _ => {
                if id.is_some() {
                    respond(id, json!({}));
                }
            }
        }
    }
}
