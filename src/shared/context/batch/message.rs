use super::action::flush_action_mutations;
use crate::mutations::{create_message_mutation, MessageMutationInput};
use crate::parser::ParsedMessage;

use super::super::super::graphql::{
    alias_outcome, execute_with_retry, extract_doc_id, has_transaction_conflict_error,
    is_already_exists_error, is_already_exists_error_text, is_transaction_conflict_text,
    AliasOutcome,
};
use super::super::super::{
    BatchResult, PendingAction, PendingMessage, SharedContext, MAX_MUTATION_RETRIES,
};
use anyhow::{Context, Result};
use std::time::SystemTime;

impl SharedContext {
    pub async fn flush_batch(&mut self) -> Result<BatchResult> {
        let pending_messages = std::mem::take(&mut self.pending_messages);
        let Some(query) = self.batch.take() else {
            return Ok(BatchResult::default());
        };
        if pending_messages.is_empty() {
            return Ok(BatchResult::default());
        }

        let response = execute_with_retry(
            &self.client,
            &query,
            MAX_MUTATION_RETRIES,
            self.metrics.as_ref(),
        )
        .await;
        let mut result = BatchResult::default();
        let mut action_mutations = Vec::new();
        let mut retry_messages = Vec::new();

        for (index, pending) in pending_messages.into_iter().enumerate() {
            match alias_outcome(&response, index) {
                AliasOutcome::Created(doc_id) => {
                    result.created += 1;
                    self.record_message(&pending.session_id, &pending.message);

                    if pending.message.actions.is_empty() {
                        continue;
                    }

                    match self
                        .resolve_message_doc_id(&pending.message_id, doc_id)
                        .await
                    {
                        Ok(message_doc_id) => {
                            self.append_action_mutations(
                                &pending,
                                &message_doc_id,
                                &mut action_mutations,
                            );
                        }
                        Err(error) => {
                            result.failed += pending.message.actions.len();
                            tracing::warn!(
                                session_id = %pending.session_id,
                                message_id = %pending.message_id,
                                error = %error,
                                "failed to resolve created message doc_id for batched actions"
                            );
                        }
                    }
                }
                AliasOutcome::Skipped => {
                    result.skipped += 1;

                    if pending.message.actions.is_empty() {
                        continue;
                    }

                    match self.lookup_message_doc_id(&pending.message_id).await {
                        Ok(message_doc_id) => {
                            self.append_action_mutations(
                                &pending,
                                &message_doc_id,
                                &mut action_mutations,
                            );
                        }
                        Err(error) => {
                            result.failed += pending.message.actions.len();
                            tracing::warn!(
                                session_id = %pending.session_id,
                                message_id = %pending.message_id,
                                error = %error,
                                "failed to resolve existing message doc_id for batched actions"
                            );
                        }
                    }
                }
                AliasOutcome::Failed(message) => {
                    if is_transaction_conflict_text(&message) {
                        retry_messages.push(pending);
                    } else if is_already_exists_error_text(&message) {
                        tracing::debug!(
                            session_id = %pending.session_id,
                            message_id = %pending.message_id,
                            "CodingMessage already exists, skipping"
                        );
                    } else {
                        result.failed += 1;
                        tracing::warn!(
                            session_id = %pending.session_id,
                            message_id = %pending.message_id,
                            error = %message,
                            "batched CodingMessage mutation failed"
                        );
                    }
                }
            }
        }

        for pending in retry_messages {
            let (retry_result, retry_actions) = self.retry_message_mutation(&pending).await;
            result.merge(retry_result);
            action_mutations.extend(retry_actions);
        }

        result.merge(flush_action_mutations(self, action_mutations).await);
        Ok(result)
    }

    pub(in crate::shared) fn queue_message(
        &mut self,
        session_id: &str,
        message: &ParsedMessage,
        message_id: &str,
        sequence: u64,
    ) -> Result<bool> {
        let session_doc_id = self
            .session_cache
            .get(session_id)
            .map(|meta| meta.doc_id.clone())
            .context("session metadata missing before message creation")?;

        let is_full =
            self.batch
                .push(self.message_mutation(message_id, &session_doc_id, sequence, message));
        self.pending_messages.push(PendingMessage {
            session_id: session_id.to_string(),
            session_doc_id,
            message_id: message_id.to_string(),
            sequence,
            message: message.clone(),
        });
        Ok(is_full)
    }

    pub(in crate::shared) fn reserve_message_sequence(&mut self, session_id: &str) -> Result<u64> {
        let meta = self
            .session_cache
            .get_mut(session_id)
            .context("session metadata missing before message sequencing")?;
        meta.last_touched = Some(SystemTime::now());
        let sequence = meta.msg_seq;
        meta.msg_seq += 1;
        Ok(sequence)
    }

    async fn resolve_message_doc_id(
        &self,
        message_id: &str,
        doc_id: Option<String>,
    ) -> Result<String> {
        match doc_id.filter(|value| !value.is_empty()) {
            Some(doc_id) => Ok(doc_id),
            None => self.lookup_message_doc_id(message_id).await,
        }
    }

    async fn retry_message_mutation(
        &mut self,
        pending: &PendingMessage,
    ) -> (BatchResult, Vec<PendingAction>) {
        let mutation = self.message_mutation(
            &pending.message_id,
            &pending.session_doc_id,
            pending.sequence,
            &pending.message,
        );
        let response = execute_with_retry(
            &self.client,
            &mutation,
            MAX_MUTATION_RETRIES,
            self.metrics.as_ref(),
        )
        .await;
        let mut result = BatchResult::default();
        let mut action_mutations = Vec::new();

        if let Some(doc_id) = extract_doc_id(&response.data, "create_CodingMessage") {
            result.created += 1;
            self.record_message(&pending.session_id, &pending.message);
            if !pending.message.actions.is_empty() {
                match self
                    .resolve_message_doc_id(&pending.message_id, Some(doc_id))
                    .await
                {
                    Ok(message_doc_id) => {
                        self.append_action_mutations(
                            pending,
                            &message_doc_id,
                            &mut action_mutations,
                        );
                    }
                    Err(error) => {
                        result.failed += pending.message.actions.len();
                        tracing::warn!(
                            session_id = %pending.session_id,
                            message_id = %pending.message_id,
                            error = %error,
                            "failed to resolve created message doc_id after retry"
                        );
                    }
                }
            }
            return (result, action_mutations);
        }

        if !response.has_errors() {
            match self.lookup_message_doc_id(&pending.message_id).await {
                Ok(message_doc_id) => {
                    result.skipped += 1;
                    self.append_action_mutations(pending, &message_doc_id, &mut action_mutations);
                    return (result, action_mutations);
                }
                Err(lookup_error) => {
                    tracing::warn!(
                        session_id = %pending.session_id,
                        message_id = %pending.message_id,
                        error = %lookup_error,
                        "individual CodingMessage retry returned no data"
                    );
                }
            }
        }

        if has_transaction_conflict_error(&response) {
            match self.lookup_message_doc_id(&pending.message_id).await {
                Ok(message_doc_id) => {
                    result.skipped += 1;
                    self.append_action_mutations(pending, &message_doc_id, &mut action_mutations);
                    return (result, action_mutations);
                }
                Err(lookup_error) => {
                    tracing::warn!(
                        session_id = %pending.session_id,
                        message_id = %pending.message_id,
                        error = %lookup_error,
                        "failed to confirm CodingMessage after transaction conflict retry"
                    );
                }
            }

            result.failed += 1;
            tracing::warn!(
                session_id = %pending.session_id,
                message_id = %pending.message_id,
                error = ?response.errors,
                "individual CodingMessage retry still conflicted"
            );
            return (result, action_mutations);
        }

        if is_already_exists_error(&response) {
            result.skipped += 1;
            if !pending.message.actions.is_empty() {
                match self.lookup_message_doc_id(&pending.message_id).await {
                    Ok(message_doc_id) => {
                        self.append_action_mutations(
                            pending,
                            &message_doc_id,
                            &mut action_mutations,
                        );
                    }
                    Err(error) => {
                        result.failed += pending.message.actions.len();
                        tracing::warn!(
                            session_id = %pending.session_id,
                            message_id = %pending.message_id,
                            error = %error,
                            "failed to resolve existing message doc_id after retry"
                        );
                    }
                }
            }
            return (result, action_mutations);
        }

        result.failed += 1;
        tracing::warn!(
            session_id = %pending.session_id,
            message_id = %pending.message_id,
            error = ?response.errors,
            "individual CodingMessage retry failed"
        );
        (result, action_mutations)
    }

    fn message_mutation(
        &self,
        message_id: &str,
        session_doc_id: &str,
        sequence: u64,
        message: &ParsedMessage,
    ) -> String {
        create_message_mutation(&MessageMutationInput {
            message_id: message_id.to_string(),
            session_doc_id: session_doc_id.to_string(),
            sequence,
            role: message.role.as_str().to_string(),
            source_host: Some(self.source_host.clone()),
            model: message.model.clone(),
            content: Some(message.content.clone()),
            thinking: message.thinking.clone(),
            tool_uses: message.tool_uses.clone(),
            files_referenced: message.files_referenced.clone(),
            usage: message.usage.clone(),
            created_at: message.timestamp.clone(),
        })
    }

    fn append_action_mutations(
        &self,
        pending: &PendingMessage,
        message_doc_id: &str,
        action_mutations: &mut Vec<PendingAction>,
    ) {
        if pending.message.actions.is_empty() {
            return;
        }

        action_mutations.extend(self.build_action_mutations(pending, message_doc_id));
    }
}
