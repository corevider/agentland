//! Fetch unresolved inline discussions with their original file and line.
use anyhow::{Context, Result};
use serde_json::Value;
use std::path::Path;

pub fn read(cwd: &Path, url: &str, expected_head: &str) -> Result<Vec<String>> {
    let parts: Vec<&str> = url.trim_end_matches('/').split('/').collect();
    anyhow::ensure!(
        parts.len() >= 7 && parts[parts.len() - 2] == "pull",
        "not a pull-request URL"
    );
    let owner = parts[parts.len() - 4];
    let repo = parts[parts.len() - 3];
    let number: u64 = parts[parts.len() - 1].parse()?;
    let mut cursor: Option<String> = None;
    let mut result = Vec::new();
    loop {
        let query = r#"query($owner:String!,$repo:String!,$number:Int!,$after:String) {
          repository(owner:$owner,name:$repo) { pullRequest(number:$number) {
            headRefOid reviewThreads(first:100,after:$after) {
              nodes { id isResolved isOutdated path line
                comments(last:20) { nodes { id body url updatedAt } }
              } pageInfo { hasNextPage endCursor }
            }
          } }
        }"#;
        let mut command = crate::exec::command("gh");
        command
            .args([
                "api",
                "graphql",
                "-f",
                &format!("query={query}"),
                "-f",
                &format!("owner={owner}"),
                "-f",
                &format!("repo={repo}"),
                "-F",
                &format!("number={number}"),
            ])
            .current_dir(cwd);
        if let Some(after) = &cursor {
            command.args(["-f", &format!("after={after}")]);
        }
        let output = command.output()?;
        anyhow::ensure!(
            output.status.success(),
            "cannot read review threads: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let response: Value = serde_json::from_slice(&output.stdout)?;
        anyhow::ensure!(
            response.get("errors").is_none(),
            "forge returned incomplete review threads"
        );
        let pull = &response["data"]["repository"]["pullRequest"];
        anyhow::ensure!(
            pull["headRefOid"].as_str() == Some(expected_head),
            "pull request changed while reading threads"
        );
        let threads = &pull["reviewThreads"];
        result.extend(render(threads));
        if threads["pageInfo"]["hasNextPage"].as_bool() != Some(true) {
            break;
        }
        let next = threads["pageInfo"]["endCursor"]
            .as_str()
            .context("missing next page cursor")?
            .to_owned();
        anyhow::ensure!(
            cursor.as_ref() != Some(&next),
            "forge repeated a review page"
        );
        cursor = Some(next);
    }
    result.sort();
    Ok(result)
}

fn render(threads: &Value) -> Vec<String> {
    threads["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|thread| thread["isResolved"].as_bool() == Some(false))
        .filter_map(|thread| {
            let id = thread["id"].as_str()?;
            let path = thread["path"].as_str().unwrap_or("unknown file");
            let line = thread["line"]
                .as_u64()
                .map_or("original location".into(), |n| n.to_string());
            let comments: Vec<String> = thread["comments"]["nodes"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|comment| {
                    format!(
                        "{} [{}] {}\n{}",
                        comment["id"].as_str().unwrap_or(""),
                        comment["updatedAt"].as_str().unwrap_or(""),
                        comment["url"].as_str().unwrap_or(""),
                        comment["body"].as_str().unwrap_or("")
                    )
                })
                .collect();
            Some(format!(
                "Thread {id} at {path}:{line} (outdated={}):\n{}",
                thread["isOutdated"].as_bool().unwrap_or(false),
                comments.join("\n")
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_location_identity_and_outdated_comments_but_skips_resolved() {
        let input = serde_json::json!({"nodes":[
            {"id":"t1","isResolved":false,"isOutdated":true,"path":"src/a.rs","line":12,"comments":{"nodes":[{"id":"c1","body":"check overflow","updatedAt":"now"}]}},
            {"id":"t2","isResolved":true,"path":"done","comments":{"nodes":[]}}
        ]});
        let output = render(&input);
        assert_eq!(output.len(), 1);
        assert!(output[0].contains("t1 at src/a.rs:12"));
        assert!(output[0].contains("c1 [now]"));
        assert!(output[0].contains("check overflow"));
    }
}
