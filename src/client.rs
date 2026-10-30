use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct GraphqlClient {
    endpoint: String,
    http: reqwest::Client,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct QueryResponse {
    pub data: Option<Value>,
    pub errors: Vec<Value>,
}

impl QueryResponse {
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

impl GraphqlClient {
    pub fn new(endpoint: impl Into<String>) -> Result<Self> {
        let endpoint = endpoint.into();
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(120))
            .build()
            .context("building coding-data GraphQL HTTP client")?;
        Ok(Self { endpoint, http })
    }

    pub async fn execute(&self, query: &str) -> QueryResponse {
        let response = match self
            .http
            .post(&self.endpoint)
            .json(&GraphqlRequest { query })
            .send()
            .await
        {
            Ok(response) => response,
            Err(error) => {
                return error_response(format!(
                    "sending GraphQL request to {}: {error}",
                    self.endpoint
                ))
            }
        };

        let status = response.status();
        let body = match response.text().await {
            Ok(body) => body,
            Err(error) => {
                return error_response(format!(
                    "reading GraphQL response body from {}: {error}",
                    self.endpoint
                ))
            }
        };

        if !status.is_success() {
            return error_response(format!(
                "GraphQL request failed with {}: {}",
                status.as_u16(),
                truncate_text(body.trim(), 1_000)
            ));
        }

        match serde_json::from_str::<GraphqlResponse<Value>>(&body) {
            Ok(parsed) => QueryResponse {
                data: parsed.data,
                errors: parsed.errors.unwrap_or_default(),
            },
            Err(error) => error_response(format!("parsing GraphQL response JSON: {error}")),
        }
    }
}

fn truncate_text(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }

    value.chars().take(max_chars).collect()
}

fn error_response(message: String) -> QueryResponse {
    QueryResponse {
        data: None,
        errors: vec![json!({ "message": message })],
    }
}

#[derive(Debug, Serialize)]
struct GraphqlRequest<'a> {
    query: &'a str,
}

#[derive(Debug, Deserialize)]
struct GraphqlResponse<T> {
    data: Option<T>,
    errors: Option<Vec<Value>>,
}

#[cfg(test)]
mod tests;
