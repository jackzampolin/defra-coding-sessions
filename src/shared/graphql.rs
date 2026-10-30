use super::INITIAL_RETRY_BACKOFF_MS;
use crate::client::{GraphqlClient, QueryResponse};
use crate::metrics::IngestMetrics;
use anyhow::{bail, Result};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone)]
pub(super) enum AliasOutcome {
    Created(Option<String>),
    Skipped,
    Failed(String),
}

pub(super) fn ensure_success(response: &QueryResponse, action: &str) -> Result<()> {
    if response.has_errors() {
        bail!("{action} failed: {:?}", response.errors);
    }
    Ok(())
}

pub(super) async fn execute_with_retry(
    client: &GraphqlClient,
    query: &str,
    max_retries: u32,
    metrics: Option<&Arc<IngestMetrics>>,
) -> QueryResponse {
    for attempt in 0..=max_retries {
        if attempt > 0 {
            if let Some(m) = metrics {
                m.record_graphql_retry();
            }
            tokio::time::sleep(Duration::from_millis(retry_backoff_ms(attempt))).await;
        }

        let response = client.execute(query).await;
        if !has_transaction_conflict_error(&response) || attempt == max_retries {
            return response;
        }
    }

    unreachable!("retry loop always returns a response")
}

/// Exponential backoff with equal jitter. Concurrent writers that conflict on
/// the same commit collide again if they all sleep the same deterministic
/// duration, so each attempt sleeps a random duration between half and all of
/// the exponential step.
pub(super) fn retry_backoff_ms(attempt: u32) -> u64 {
    use rand::Rng;
    let cap_ms = INITIAL_RETRY_BACKOFF_MS * (1_u64 << (attempt - 1));
    rand::rng().random_range(cap_ms / 2..=cap_ms)
}

pub(super) fn is_already_exists_error(response: &QueryResponse) -> bool {
    response.errors.iter().any(is_already_exists_message)
}

pub(super) fn extract_doc_id(data: &Option<Value>, field: &str) -> Option<String> {
    let data = data.as_ref()?;
    match data.get(field) {
        Some(Value::Array(items)) => items.first().and_then(extract_doc_id_from_value),
        Some(value) => extract_doc_id_from_value(value),
        None => None,
    }
}

pub(super) fn extract_first_item<'a>(data: &'a Option<Value>, field: &str) -> Option<&'a Value> {
    let data = data.as_ref()?;
    match data.get(field) {
        Some(Value::Array(items)) => items.first(),
        Some(value) => Some(value),
        None => None,
    }
}

pub(super) fn extract_doc_id_from_value(value: &Value) -> Option<String> {
    value
        .get("_docID")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

pub(super) fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

pub(super) fn integer_field(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

pub(super) fn array_field(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn alias_outcome(response: &QueryResponse, index: usize) -> AliasOutcome {
    let alias = format!("m{index}");
    if let Some(doc_id) = response
        .data
        .as_ref()
        .and_then(Value::as_object)
        .and_then(|data| data.get(&alias))
        .filter(|value| !value.is_null())
        .map(extract_doc_id_from_alias_value)
    {
        return AliasOutcome::Created(doc_id);
    }

    let mut alias_errors = Vec::new();
    let mut global_errors = Vec::new();

    for error in &response.errors {
        let message = error_message(error);
        match error_alias(error).as_deref() {
            Some(path_alias) if path_alias == alias => alias_errors.push(message),
            None => global_errors.push(message),
            _ => {}
        }
    }

    if !alias_errors.is_empty() {
        if alias_errors
            .iter()
            .all(|message| is_already_exists_text(message))
        {
            AliasOutcome::Skipped
        } else {
            AliasOutcome::Failed(alias_errors.join("; "))
        }
    } else if !global_errors.is_empty() {
        AliasOutcome::Failed(global_errors.join("; "))
    } else {
        AliasOutcome::Failed("mutation returned no data".to_string())
    }
}

fn extract_doc_id_from_alias_value(value: &Value) -> Option<String> {
    match value {
        Value::Array(items) => items.first().and_then(extract_doc_id_from_value),
        _ => extract_doc_id_from_value(value),
    }
}

pub(super) fn has_transaction_conflict_error(response: &QueryResponse) -> bool {
    response.errors.iter().any(is_transaction_conflict_error)
}

fn error_alias(error: &Value) -> Option<String> {
    error
        .get("path")
        .and_then(Value::as_array)
        .and_then(|path| path.first())
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn error_message(error: &Value) -> String {
    error
        .get("message")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| error.to_string())
}

fn is_already_exists_message(error: &Value) -> bool {
    error
        .get("message")
        .and_then(Value::as_str)
        .is_some_and(is_already_exists_text)
}

fn is_transaction_conflict_error(error: &Value) -> bool {
    error
        .get("message")
        .and_then(Value::as_str)
        .is_some_and(is_transaction_conflict_text)
}

pub(super) fn is_transaction_conflict_text(message: &str) -> bool {
    message.contains("transaction conflict")
}

pub(super) fn is_already_exists_text(message: &str) -> bool {
    message.contains("already exists") || message.contains("violates unique index")
}

/// Check if a single error message string indicates a document already exists.
/// Used in batch mutation handling where we have the raw error text.
pub(super) fn is_already_exists_error_text(message: &str) -> bool {
    is_already_exists_text(message)
}
