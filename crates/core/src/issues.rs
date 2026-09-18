//! GitHub issues, as the cards they can become.
//!
//! A project's issues are where work gets asked for while nobody is looking at
//! the board. They are read with `gh` in the project's own checkout, so the
//! repository and the login are whatever `gh` already uses there. A card made
//! from an issue keeps its number, and the card's pull request closes it.

use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::board::{CreateTask, Issue};

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Label {
    #[serde(default)]
    pub name: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Author {
    #[serde(default)]
    pub login: String,
}

/// One issue, as `gh issue list --json` and `gh issue view --json` give it.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GitHubIssue {
    pub number: u64,
    pub title: String,
    #[serde(default)]
    pub body: String,
    pub url: String,
    #[serde(default)]
    pub labels: Vec<Label>,
    #[serde(default)]
    pub author: Author,
    #[serde(default, rename = "updatedAt")]
    pub updated_at: String,
}

const FIELDS: &str = "number,title,body,url,labels,author,updatedAt";
const MOST: usize = 100;

/// Whether a project's issues can be read at all: it needs a remote on GitHub.
pub fn on_github(repository: &crate::repo::Repository) -> bool {
    repository.remotes.iter().any(|remote| remote.provider == "github")
}

pub fn read_issues(json: &[u8]) -> Result<Vec<GitHubIssue>> {
    serde_json::from_slice(json).context("gh answered with something that is not a list of issues")
}

async fn gh(checkout: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let answer = crate::exec::tokio_command("gh")
        .args(args)
        .current_dir(checkout)
        .output()
        .await
        .context("gh could not be started — the GitHub CLI reads the issues")?;
    if !answer.status.success() {
        bail!("gh: {}", String::from_utf8_lossy(&answer.stderr).trim());
    }
    Ok(answer.stdout)
}

/// The project's open issues, newest activity first as `gh` lists them.
pub async fn open_issues(checkout: &Path) -> Result<Vec<GitHubIssue>> {
    let most = MOST.to_string();
    let listed = gh(checkout, &["issue", "list", "--state", "open", "--limit", &most, "--json", FIELDS]).await?;
    read_issues(&listed)
}

pub async fn issue(checkout: &Path, number: u64) -> Result<GitHubIssue> {
    let number = number.to_string();
    let viewed = gh(checkout, &["issue", "view", &number, "--json", FIELDS]).await?;
    serde_json::from_slice(&viewed).context("gh answered with something that is not an issue")
}

/// The card an issue becomes: its title, its words, and where it came from.
pub fn card_from(issue: &GitHubIssue, repository_id: &str) -> CreateTask {
    let body = issue.body.trim();
    let from = format!("From GitHub issue #{}: {}", issue.number, issue.url);

    CreateTask {
        title: issue.title.trim().to_owned(),
        body: if body.is_empty() { from } else { format!("{body}\n\n{from}") },
        repository_id: repository_id.to_owned(),
        worktree: None,
        issue: Some(Issue {
            number: issue.number,
            url: issue.url.clone(),
            labels: issue.labels.iter().map(|label| label.name.clone()).collect(),
        }),
    }
}

const CLOSING_WORDS: [&str; 9] = [
    "close", "closes", "closed", "fix", "fixes", "fixed", "resolve", "resolves", "resolved",
];

/// Whether a pull request body already closes issue `number` the way GitHub
/// reads it: one of its closing words, then `#number` and nothing more of a
/// number after it.
fn already_closes(body: &str, number: u64) -> bool {
    let lower = body.to_lowercase();
    let target = format!("#{number}");

    lower.match_indices(&target).any(|(at, _)| {
        if lower[at + target.len()..].starts_with(|next: char| next.is_ascii_digit()) {
            return false;
        }
        let before = lower[..at].trim_end().trim_end_matches(':').trim_end();
        CLOSING_WORDS.iter().any(|word| {
            before
                .strip_suffix(word)
                .is_some_and(|rest| !rest.ends_with(|previous: char| previous.is_alphanumeric()))
        })
    })
}

/// A pull request body that closes the issue its card came from, unless it
/// already says so: GitHub closes an issue when a pull request whose body says
/// "Closes #N" is merged.
pub fn closing(body: &str, number: u64) -> String {
    if already_closes(body, number) {
        return body.to_owned();
    }
    let body = body.trim_end();
    if body.is_empty() {
        format!("Closes #{number}")
    } else {
        format!("{body}\n\nCloses #{number}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issue(number: u64, body: &str) -> GitHubIssue {
        GitHubIssue {
            number,
            title: "  Cart total is off by one  ".to_owned(),
            body: body.to_owned(),
            url: format!("https://github.com/shop/web/issues/{number}"),
            labels: vec![Label { name: "bug".to_owned() }],
            author: Author { login: "ada".to_owned() },
            updated_at: "2026-09-13T10:00:00Z".to_owned(),
        }
    }

    #[test]
    fn issues_are_read_the_way_gh_writes_them() {
        let listed = br#"[{"number":12,"title":"Cart total","body":"It is off.","url":"https://github.com/shop/web/issues/12","labels":[{"id":"L1","name":"bug","color":"d73a4a"}],"author":{"login":"ada","is_bot":false},"updatedAt":"2026-09-13T10:00:00Z"}]"#;

        let read = read_issues(listed).unwrap();

        assert_eq!(read.len(), 1);
        assert_eq!(read[0].number, 12);
        assert_eq!(read[0].labels[0].name, "bug");
        assert_eq!(read[0].author.login, "ada");
        assert!(read_issues(b"{\"message\":\"Not Found\"}").is_err());
    }

    #[test]
    fn an_issue_becomes_a_card_that_remembers_it() {
        let card = card_from(&issue(12, "It is off by one.\n"), "web");

        assert_eq!(card.title, "Cart total is off by one");
        assert_eq!(card.body, "It is off by one.\n\nFrom GitHub issue #12: https://github.com/shop/web/issues/12");
        assert_eq!(card.repository_id, "web");
        assert_eq!(card.issue.map(|held| held.number), Some(12));
    }

    #[test]
    fn an_issue_with_no_words_still_says_where_it_came_from() {
        assert_eq!(card_from(&issue(7, "  "), "web").body, "From GitHub issue #7: https://github.com/shop/web/issues/7");
    }

    #[test]
    fn a_pull_request_closes_the_issue_its_card_came_from() {
        assert_eq!(closing("Adds the missing item.\n", 12), "Adds the missing item.\n\nCloses #12");
        assert_eq!(closing("", 12), "Closes #12");
    }

    #[test]
    fn a_body_that_already_closes_it_is_left_alone() {
        assert_eq!(closing("Fixes #12", 12), "Fixes #12");
        assert_eq!(closing("This resolves: #12.", 12), "This resolves: #12.");
        assert_eq!(closing("closed #12", 12), "closed #12");
    }

    #[test]
    fn a_mention_that_does_not_close_it_is_not_taken_for_one() {
        assert_eq!(closing("Fixes #123", 12), "Fixes #123\n\nCloses #12", "another issue");
        assert_eq!(closing("See #12", 12), "See #12\n\nCloses #12", "a mention, not a closing word");
        assert_eq!(closing("prefix #12", 12), "prefix #12\n\nCloses #12", "fix inside another word");
    }
}
