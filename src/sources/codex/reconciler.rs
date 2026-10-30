use super::sqlite::{read_threads, ThreadRow};
use crate::client::GraphqlClient;
use crate::mutations::{query_session_checkpoint, update_session_mutation, SessionMutationInput};
use anyhow::{anyhow, Result};
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;

const MAX_MUTATION_RETRIES: u32 = 3;
const INITIAL_RETRY_BACKOFF_MS: u64 = 50;

pub struct Reconciler {
    client: GraphqlClient,
    sqlite_path: PathBuf,
    interval: Duration,
    dry_run: bool,
}

#[cfg(test)]
mod tests;

impl Reconciler {
    pub fn new(
        client: GraphqlClient,
        sqlite_path: PathBuf,
        interval_secs: u64,
        dry_run: bool,
    ) -> Self {
        Self {
            client,
            sqlite_path,
            interval: Duration::from_secs(interval_secs),
            dry_run,
        }
    }

    pub async fn run(&self) -> Result<()> {
        let mut ticker = tokio::time::interval(self.interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            ticker.tick().await;

            match self.reconcile_once().await {
                Ok(stats) => {
                    if stats.sessions_updated > 0 || stats.new_threads_discovered > 0 {
                        tracing::info!(
                            threads_checked = stats.threads_checked,
                            sessions_updated = stats.sessions_updated,
                            new_threads_discovered = stats.new_threads_discovered,
                            "sqlite reconciliation complete"
                        );
                    }
                }
                Err(error) => {
                    tracing::error!(error = %error, "sqlite reconciliation failed");
                }
            }
        }
    }

    pub async fn reconcile_once(&self) -> Result<ReconcileStats> {
        if self.dry_run {
            return Ok(ReconcileStats::default());
        }

        if !self.sqlite_path.exists() {
            return Ok(ReconcileStats::default());
        }

        let threads = read_threads(&self.sqlite_path)?;
        let mut stats = ReconcileStats::default();

        for thread in &threads {
            stats.threads_checked += 1;
            let response = self
                .client
                .execute(&query_session_checkpoint(&thread.id))
                .await;
            ensure_success(&response, "lookup session for reconciliation")?;

            let Some(current) = response
                .data
                .as_ref()
                .and_then(|data| data.get("CodingSession"))
                .and_then(Value::as_array)
                .and_then(|rows| rows.first())
            else {
                stats.new_threads_discovered += 1;
                continue;
            };

            if self.reconcile_loaded_thread(thread, current).await? {
                stats.sessions_updated += 1;
            }
        }

        Ok(stats)
    }

    async fn reconcile_loaded_thread(&self, thread: &ThreadRow, current: &Value) -> Result<bool> {
        if self.dry_run || !session_needs_reconcile(current, thread) {
            return Ok(false);
        }

        let mutation = update_session_mutation(
            &thread.id,
            &SessionMutationInput {
                session_id: thread.id.clone(),
                project_doc_id: String::new(),
                git_branch: thread.git_branch.clone(),
                source: "codex".to_string(),
                source_host: None,
                model_primary: thread.model.clone(),
                claude_version: thread.cli_version.clone(),
                title: thread.title.clone(),
                archived: Some(thread.archived),
                git_sha: thread.git_sha.clone(),
                git_origin_url: thread.git_origin_url.clone(),
                agent_role: thread.agent_role.clone(),
                reasoning_effort: thread.reasoning_effort.clone(),
                created_at: thread.created_at.clone(),
                finished_at: thread.updated_at.clone(),
                message_count: None,
                user_message_count: None,
                input_tokens: None,
                output_tokens: None,
                cache_creation_input_tokens: None,
                cache_read_input_tokens: None,
                reasoning_output_tokens: None,
                total_tokens: None,
                tools_used: Vec::new(),
                first_prompt: None,
                summary: None,
            },
        );

        let response = execute_with_retry(&self.client, &mutation, "reconcile CodingSession").await;
        ensure_success(&response, "reconcile CodingSession")?;
        Ok(true)
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ReconcileStats {
    pub threads_checked: u64,
    pub sessions_updated: u64,
    pub new_threads_discovered: u64,
}

async fn execute_with_retry(
    client: &GraphqlClient,
    query: &str,
    operation: &str,
) -> crate::client::QueryResponse {
    let mut attempt = 0;

    loop {
        let response = client.execute(query).await;
        if attempt >= MAX_MUTATION_RETRIES || !is_retryable_response(&response) {
            return response;
        }

        attempt += 1;
        let backoff_ms = INITIAL_RETRY_BACKOFF_MS.saturating_mul(1_u64 << (attempt - 1));
        tracing::warn!(
            operation = %operation,
            attempt,
            backoff_ms,
            "retrying GraphQL request after retryable failure"
        );
        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
    }
}

fn ensure_success(response: &crate::client::QueryResponse, operation: &str) -> Result<()> {
    if response.has_errors() {
        return Err(anyhow!("{operation} failed: {:?}", response.errors));
    }

    Ok(())
}

fn is_retryable_response(response: &crate::client::QueryResponse) -> bool {
    response
        .errors
        .iter()
        .filter_map(|error| error.get("message").and_then(Value::as_str))
        .any(is_retryable_error_message)
}

fn is_retryable_error_message(message: &str) -> bool {
    if message.contains("transaction conflict") {
        return true;
    }

    let Some(status) = message
        .split("status ")
        .nth(1)
        .and_then(|rest| rest.split(|ch: char| !ch.is_ascii_digit()).next())
    else {
        return false;
    };
    let Ok(status) = status.trim().parse::<u16>() else {
        return false;
    };

    matches!(status, 408 | 425 | 429 | 500..=599)
}

fn session_needs_reconcile(current: &Value, thread: &ThreadRow) -> bool {
    string_field(current, "title") != thread.title
        || bool_field(current, "archived") != Some(thread.archived)
        || string_field(current, "git_sha") != thread.git_sha
        || string_field(current, "git_branch") != thread.git_branch
        || string_field(current, "git_origin_url") != thread.git_origin_url
        || string_field(current, "model_primary") != thread.model
        || string_field(current, "reasoning_effort") != thread.reasoning_effort
        || string_field(current, "agent_role") != thread.agent_role
        || string_field(current, "claude_version") != thread.cli_version
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn bool_field(value: &Value, key: &str) -> Option<bool> {
    value.get(key).and_then(Value::as_bool)
}
