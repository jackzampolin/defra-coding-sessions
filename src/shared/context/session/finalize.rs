use super::super::super::graphql::{ensure_success, execute_with_retry};
use super::super::super::{SessionFlushResult, SharedContext, MAX_MUTATION_RETRIES};
use crate::mutations::{update_session_mutation, SessionMutationInput};
use anyhow::Result;

impl SharedContext {
    pub async fn finalize_session(&mut self, session_id: &str) -> Result<SessionFlushResult> {
        self.flush_session(session_id, true).await
    }

    pub async fn snapshot_session(&mut self, session_id: &str) -> Result<SessionFlushResult> {
        self.flush_session(session_id, false).await
    }

    async fn flush_session(
        &mut self,
        session_id: &str,
        remove_after: bool,
    ) -> Result<SessionFlushResult> {
        self.touch_session(session_id);
        let batch_result = self.flush_batch().await?;
        if batch_result.failed > 0 {
            tracing::warn!(
                session_id,
                failed_mutations = batch_result.failed,
                "updating session after batched mutation failures"
            );
        }

        let Some(mut meta) = self.session_cache.get(session_id).cloned() else {
            return Ok(SessionFlushResult {
                updated: false,
                batch_result,
            });
        };

        if self.dry_run {
            if remove_after {
                self.session_cache.remove(session_id);
            }
            return Ok(SessionFlushResult {
                updated: true,
                batch_result,
            });
        }

        if meta.doc_id.is_empty() {
            if meta.project_doc_id.is_some() {
                self.ensure_session_ready(session_id, meta.cwd.as_deref())
                    .await?;
                if let Some(updated) = self.session_cache.get(session_id) {
                    meta = updated.clone();
                }
            } else {
                if remove_after {
                    self.session_cache.remove(session_id);
                }
                return Ok(SessionFlushResult {
                    updated: false,
                    batch_result,
                });
            }
        }

        let model_primary = meta
            .model_counts
            .iter()
            .max_by_key(|(_, count)| **count)
            .map(|(model, _)| model.clone());
        let tools_used: Vec<String> = meta.tools_used.into_iter().collect();

        let mutation = update_session_mutation(
            session_id,
            &SessionMutationInput {
                session_id: session_id.to_string(),
                project_doc_id: meta.project_doc_id.clone().unwrap_or_default(),
                git_branch: meta.git_branch.clone(),
                source: self.source_label.clone(),
                source_host: Some(self.source_host.clone()),
                model_primary,
                claude_version: meta.claude_version.clone(),
                title: meta.title.clone(),
                archived: meta.archived,
                git_sha: meta.git_sha.clone(),
                git_origin_url: meta.git_origin_url.clone(),
                agent_role: meta.agent_role.clone(),
                reasoning_effort: meta.reasoning_effort.clone(),
                created_at: meta.created_at.clone(),
                finished_at: meta.last_ts.clone(),
                message_count: Some(meta.message_count),
                user_message_count: Some(meta.user_message_count),
                input_tokens: Some(meta.input_tokens),
                output_tokens: Some(meta.output_tokens),
                cache_creation_input_tokens: Some(meta.cache_creation_input_tokens),
                cache_read_input_tokens: Some(meta.cache_read_input_tokens),
                reasoning_output_tokens: Some(meta.reasoning_output_tokens),
                total_tokens: Some(meta.total_tokens),
                tools_used,
                first_prompt: meta.first_prompt,
                summary: None,
            },
        );

        let action = if remove_after {
            "finalize CodingSession"
        } else {
            "snapshot CodingSession"
        };
        let response = execute_with_retry(
            &self.client,
            &mutation,
            MAX_MUTATION_RETRIES,
            self.metrics.as_ref(),
        )
        .await;
        ensure_success(&response, action)?;

        if remove_after {
            self.session_cache.remove(session_id);
        }

        Ok(SessionFlushResult {
            updated: true,
            batch_result,
        })
    }
}
