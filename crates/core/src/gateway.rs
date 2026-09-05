use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{anyhow, bail, Result};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const KEYCHAIN_SERVICE: &str = "agentland";

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    Dev,
    Prod,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Integration {
    pub id: String,
    pub service: String,
    pub environment: Environment,
    pub source: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ConnectRequest {
    pub service: String,
    #[serde(default = "dev")]
    pub environment: Environment,
    #[serde(default)]
    pub token: Option<String>,
}

fn dev() -> Environment {
    Environment::Dev
}

#[derive(Clone, Debug, Deserialize)]
pub struct CallRequest {
    pub integration_id: String,
    pub operation: String,
    #[serde(default)]
    pub params: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct State {
    #[serde(default)]
    integrations: BTreeMap<String, Integration>,
}

pub struct Gateway {
    state: Mutex<State>,
    data_dir: PathBuf,
    client: reqwest::Client,
}

fn env_variable(id: &str) -> String {
    format!("AGENTLAND_SECRET_{}", id.to_uppercase().replace('-', "_"))
}

fn store_secret(id: &str, token: &str) -> Result<String> {
    match keyring::Entry::new(KEYCHAIN_SERVICE, id) {
        Ok(entry) => match entry.set_password(token) {
            Ok(()) => Ok("keychain".to_owned()),
            Err(error) => bail!(
                "the OS keychain refused to store this credential ({error}); set {} in the environment instead",
                env_variable(id)
            ),
        },
        Err(error) => bail!(
            "no OS keychain is available ({error}); set {} in the environment instead",
            env_variable(id)
        ),
    }
}

fn read_secret(integration: &Integration) -> Result<String> {
    if integration.source == "keychain" {
        if let Ok(entry) = keyring::Entry::new(KEYCHAIN_SERVICE, &integration.id) {
            if let Ok(password) = entry.get_password() {
                return Ok(password);
            }
        }
    }

    std::env::var(env_variable(&integration.id)).map_err(|_| {
        anyhow!(
            "no credential for {}; set {} or reconnect it",
            integration.id,
            env_variable(&integration.id)
        )
    })
}

impl Gateway {
    pub fn new(data_dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&data_dir);
        let data_dir = crate::exec::settled(&data_dir);
        let state = crate::db::load_state(&data_dir, "integrations");

        Self {
            state: Mutex::new(state),
            data_dir,
            client: reqwest::Client::new(),
        }
    }

    fn persist(&self, state: &State) {
        crate::db::save_state(&self.data_dir, "integrations", state);
    }

    pub fn list(&self) -> Vec<Integration> {
        self.state.lock().integrations.values().cloned().collect()
    }

    pub fn connect(&self, request: ConnectRequest) -> Result<Integration> {
        let service = request.service.to_lowercase();
        if !matches!(service.as_str(), "github" | "sentry") {
            bail!("unsupported service: {service}");
        }

        let environment = request.environment;
        let id = format!(
            "{service}-{}",
            match environment {
                Environment::Dev => "dev",
                Environment::Prod => "prod",
            }
        );

        let source = match request.token {
            Some(token) if !token.trim().is_empty() => store_secret(&id, token.trim())?,
            _ => {
                if std::env::var(env_variable(&id)).is_err() {
                    bail!(
                        "no token given and {} is not set",
                        env_variable(&id)
                    );
                }
                "environment".to_owned()
            }
        };

        let integration = Integration {
            id: id.clone(),
            service,
            environment,
            source,
        };

        let mut state = self.state.lock();
        state.integrations.insert(id, integration.clone());
        self.persist(&state);
        Ok(integration)
    }

    pub fn disconnect(&self, id: &str) -> Result<()> {
        let mut state = self.state.lock();
        state
            .integrations
            .remove(id)
            .ok_or_else(|| anyhow!("unknown integration: {id}"))?;
        self.persist(&state);

        if let Ok(entry) = keyring::Entry::new(KEYCHAIN_SERVICE, id) {
            let _ = entry.delete_credential();
        }

        Ok(())
    }

    pub async fn call(&self, request: CallRequest) -> Result<Value> {
        let integration = self
            .state
            .lock()
            .integrations
            .get(&request.integration_id)
            .cloned()
            .ok_or_else(|| anyhow!("unknown integration: {}", request.integration_id))?;

        let token = read_secret(&integration)?;

        let (url, header_name, header_value) = match (integration.service.as_str(), request.operation.as_str()) {
            ("github", "list_issues") => {
                let repo = request
                    .params
                    .get("repo")
                    .ok_or_else(|| anyhow!("repo is required, as owner/name"))?;
                (
                    format!("https://api.github.com/repos/{repo}/issues?per_page=20"),
                    "authorization",
                    format!("Bearer {token}"),
                )
            }
            ("github", "list_pulls") => {
                let repo = request
                    .params
                    .get("repo")
                    .ok_or_else(|| anyhow!("repo is required, as owner/name"))?;
                (
                    format!("https://api.github.com/repos/{repo}/pulls?per_page=20"),
                    "authorization",
                    format!("Bearer {token}"),
                )
            }
            ("sentry", "list_issues") => {
                let project = request
                    .params
                    .get("project")
                    .ok_or_else(|| anyhow!("project is required, as organisation/project"))?;
                (
                    format!("https://sentry.io/api/0/projects/{project}/issues/?limit=20"),
                    "authorization",
                    format!("Bearer {token}"),
                )
            }
            (service, operation) => bail!("{service} has no operation called {operation}"),
        };

        let response = self
            .client
            .get(&url)
            .header(header_name, header_value)
            .header("user-agent", "agentland")
            .header("accept", "application/vnd.github+json")
            .send()
            .await?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        if !status.is_success() {
            bail!("{} replied {status}", integration.service);
        }

        Ok(serde_json::from_str(&body).unwrap_or(Value::String(body)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_environment_variable_name_is_predictable() {
        assert_eq!(env_variable("github-prod"), "AGENTLAND_SECRET_GITHUB_PROD");
    }

    #[test]
    fn an_integration_record_never_carries_the_token() {
        let integration = Integration {
            id: "github-dev".to_owned(),
            service: "github".to_owned(),
            environment: Environment::Dev,
            source: "keychain".to_owned(),
        };

        let rendered = serde_json::to_string(&integration).expect("serialise");
        assert!(!rendered.contains("token"), "{rendered}");
        assert!(!rendered.contains("secret"), "{rendered}");
    }

    #[test]
    fn unsupported_services_are_refused_by_name() {
        let dir = std::env::temp_dir().join("agentland-gateway-test");
        let _ = fs::remove_dir_all(&dir);
        let gateway = Gateway::new(dir);

        let error = gateway
            .connect(ConnectRequest {
                service: "pastebin".to_owned(),
                environment: Environment::Dev,
                token: Some("x".to_owned()),
            })
            .unwrap_err()
            .to_string();

        assert!(error.contains("unsupported service"), "{error}");
    }
}
