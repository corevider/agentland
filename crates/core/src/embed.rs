use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct EmbedderSettings {
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_floor")]
    pub min_similarity: f32,
}

fn default_floor() -> f32 {
    0.5
}

fn default_model() -> String {
    "nomic-embed-text".to_owned()
}

impl Default for EmbedderSettings {
    fn default() -> Self {
        Self {
            endpoint: None,
            model: default_model(),
            min_similarity: default_floor(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct EmbedderReport {
    pub settings: EmbedderSettings,
    pub reachable: bool,
    pub dimensions: usize,
    pub detail: String,
}

pub fn load(data_dir: &Path) -> EmbedderSettings {
    crate::db::load_state(data_dir, "embedder")
}

pub fn save(data_dir: &PathBuf, settings: &EmbedderSettings) {
    crate::db::save_state(data_dir, "embedder", settings);
}

pub fn embed(settings: &EmbedderSettings, text: &str) -> Result<Vec<f32>> {
    let Some(endpoint) = settings.endpoint.as_deref().map(str::trim).filter(|value| !value.is_empty())
    else {
        bail!("no embedder is configured");
    };

    if endpoint.starts_with("https://") {
        bail!("this build speaks plain HTTP only, so the embedder must be a local endpoint");
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()?;

    let response = client
        .post(endpoint)
        .json(&serde_json::json!({
            "model": settings.model,
            "input": text,
            "prompt": text,
        }))
        .send()
        .with_context(|| format!("cannot reach the embedder at {endpoint}"))?;

    let status = response.status();
    let body: serde_json::Value = response
        .json()
        .with_context(|| format!("the embedder at {endpoint} did not answer with JSON"))?;

    if !status.is_success() {
        bail!("the embedder answered {status}: {body}");
    }

    read_vector(&body)
}

pub fn read_vector(body: &serde_json::Value) -> Result<Vec<f32>> {
    let numbers = body
        .get("data")
        .and_then(|data| data.get(0))
        .and_then(|first| first.get("embedding"))
        .or_else(|| body.get("embedding"))
        .or_else(|| body.get("embeddings").and_then(|value| value.get(0)));

    let Some(serde_json::Value::Array(values)) = numbers else {
        bail!("the answer carried no embedding");
    };

    let vector: Vec<f32> = values
        .iter()
        .filter_map(|value| value.as_f64().map(|number| number as f32))
        .collect();

    if vector.len() != values.len() || vector.is_empty() {
        bail!("the embedding was not a list of numbers");
    }

    Ok(vector)
}

pub fn cosine(left: &[f32], right: &[f32]) -> f32 {
    if left.len() != right.len() || left.is_empty() {
        return 0.0;
    }

    let mut dot = 0.0;
    let mut left_norm = 0.0;
    let mut right_norm = 0.0;

    for (a, b) in left.iter().zip(right) {
        dot += a * b;
        left_norm += a * a;
        right_norm += b * b;
    }

    if left_norm == 0.0 || right_norm == 0.0 {
        return 0.0;
    }

    dot / (left_norm.sqrt() * right_norm.sqrt())
}

pub fn probe(settings: &EmbedderSettings) -> EmbedderReport {
    if settings.endpoint.as_deref().map(str::trim).unwrap_or("").is_empty() {
        return EmbedderReport {
            settings: settings.clone(),
            reachable: false,
            dimensions: 0,
            detail: "no embedder is configured; recall is lexical only".to_owned(),
        };
    }

    match embed(settings, "agentland embedder probe") {
        Ok(vector) => EmbedderReport {
            settings: settings.clone(),
            reachable: true,
            dimensions: vector.len(),
            detail: format!("answered with {} dimensions", vector.len()),
        },
        Err(error) => EmbedderReport {
            settings: settings.clone(),
            reachable: false,
            dimensions: 0,
            detail: format!("{error}"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_reads_the_openai_shape() {
        let body = serde_json::json!({ "data": [{ "embedding": [0.5, -0.25, 1.0] }] });
        assert_eq!(read_vector(&body).expect("vector"), vec![0.5, -0.25, 1.0]);
    }

    #[test]
    fn it_reads_the_ollama_shape() {
        let body = serde_json::json!({ "embedding": [1.0, 2.0] });
        assert_eq!(read_vector(&body).expect("vector"), vec![1.0, 2.0]);

        let batched = serde_json::json!({ "embeddings": [[3.0, 4.0]] });
        assert_eq!(read_vector(&batched).expect("vector"), vec![3.0, 4.0]);
    }

    #[test]
    fn an_answer_without_an_embedding_is_an_error_rather_than_an_empty_vector() {
        assert!(read_vector(&serde_json::json!({ "error": "no model" })).is_err());
        assert!(read_vector(&serde_json::json!({ "embedding": [] })).is_err());
        assert!(read_vector(&serde_json::json!({ "embedding": ["nope"] })).is_err());
    }

    #[test]
    fn cosine_ranks_the_closer_vector_higher() {
        let query = [1.0, 0.0, 0.0];
        let near = [0.9, 0.1, 0.0];
        let far = [0.0, 1.0, 0.0];

        assert!(cosine(&query, &near) > cosine(&query, &far));
        assert!((cosine(&query, &query) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn vectors_of_different_lengths_score_zero_rather_than_panicking() {
        assert_eq!(cosine(&[1.0, 2.0], &[1.0]), 0.0);
        assert_eq!(cosine(&[], &[]), 0.0);
        assert_eq!(cosine(&[0.0, 0.0], &[1.0, 1.0]), 0.0);
    }

    #[test]
    fn a_missing_endpoint_is_refused_before_any_request() {
        let settings = EmbedderSettings::default();
        assert!(embed(&settings, "anything").is_err());

        let report = probe(&settings);
        assert!(!report.reachable);
        assert!(report.detail.contains("lexical only"));
    }

    #[test]
    fn an_https_endpoint_is_refused_with_the_reason() {
        let settings = EmbedderSettings {
            endpoint: Some("https://api.example.com/v1/embeddings".into()),
            model: "whatever".into(),
            min_similarity: 0.5,
        };

        let error = embed(&settings, "text").expect_err("should refuse");
        assert!(error.to_string().contains("local endpoint"), "{error}");
    }
}
