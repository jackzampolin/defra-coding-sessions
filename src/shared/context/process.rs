use super::super::{BatchResult, ProcessLineResult, SharedContext};
use crate::mutations::{
    create_token_usage_mutation, deterministic_message_id, update_token_usage_mutation,
    TokenUsageMutationInput,
};
use crate::parser::{
    ParsedEntry, ParsedMessage, ParsedSessionMeta, ParsedTurnContext, ParsedUsageEvent, Role,
};
use anyhow::Result;

impl SharedContext {
    pub async fn process_entry(&mut self, entry: ParsedEntry) -> Result<ProcessLineResult> {
        match entry {
            ParsedEntry::Skip => Ok(ProcessLineResult {
                skipped: true,
                ..Default::default()
            }),
            ParsedEntry::SessionMeta(meta) => self.process_session_meta(*meta).await,
            ParsedEntry::TurnContext(context) => self.process_turn_context(*context).await,
            ParsedEntry::Message(message) => self.process_message(*message).await,
            ParsedEntry::Usage(event) => self.process_usage_event(*event).await,
        }
    }

    async fn process_usage_event(&mut self, event: ParsedUsageEvent) -> Result<ProcessLineResult> {
        let input = TokenUsageMutationInput {
            usage_id: event.usage_id,
            session_id: event.session_id.clone(),
            source: self.source_label.clone(),
            source_host: self.source_host.clone(),
            model: event.model,
            usage: event.usage,
            created_at: event.timestamp,
        };

        if self.dry_run {
            return Ok(ProcessLineResult {
                session_id: Some(event.session_id),
                processed: true,
                usage_events_processed: 1,
                ..Default::default()
            });
        }

        let create = super::super::graphql::execute_with_retry(
            &self.client,
            &create_token_usage_mutation(&input),
            super::super::MAX_MUTATION_RETRIES,
            self.metrics.as_ref(),
        )
        .await;
        let skipped = if super::super::graphql::is_already_exists_error(&create) {
            let update = super::super::graphql::execute_with_retry(
                &self.client,
                &update_token_usage_mutation(&input),
                super::super::MAX_MUTATION_RETRIES,
                self.metrics.as_ref(),
            )
            .await;
            super::super::graphql::ensure_success(&update, "update CodingTokenUsage")?;
            true
        } else {
            super::super::graphql::ensure_success(&create, "create CodingTokenUsage")?;
            false
        };

        Ok(ProcessLineResult {
            session_id: Some(event.session_id),
            processed: true,
            skipped,
            usage_events_processed: 1,
            ..Default::default()
        })
    }

    pub async fn process_message(&mut self, message: ParsedMessage) -> Result<ProcessLineResult> {
        let Some(session_id) = message.session_id.clone() else {
            return Ok(ProcessLineResult {
                skipped: true,
                ..Default::default()
            });
        };

        self.apply_message_metadata(&session_id, &message);
        self.ensure_session_ready(&session_id, message.cwd.as_deref())
            .await?;

        if let Some(source_id) = [
            message.message_id.as_deref(),
            message.transient_dedupe_id.as_deref(),
        ]
        .into_iter()
        .flatten()
        .find(|source_id| {
            self.session_cache
                .get(&session_id)
                .is_some_and(|meta| meta.seen_message_ids.contains(*source_id))
        }) {
            tracing::debug!(
                session_id = %session_id,
                message_id = %source_id,
                "skipping repeated source message id"
            );
            return Ok(ProcessLineResult {
                session_id: Some(session_id),
                skipped: true,
                ..Default::default()
            });
        }

        let sequence = self.reserve_message_sequence(&session_id)?;
        let message_id = message
            .message_id
            .clone()
            .unwrap_or_else(|| deterministic_message_id(&session_id, sequence));
        if let Some(meta) = self.session_cache.get_mut(&session_id) {
            meta.seen_message_ids.insert(message_id.clone());
            if let Some(dedupe_id) = message.transient_dedupe_id.as_ref() {
                meta.seen_message_ids.insert(dedupe_id.clone());
            }
        }

        let action_count = if matches!(message.role, Role::Assistant) {
            message.actions.len()
        } else {
            0
        };
        let mut batch_result = BatchResult::default();

        if self.dry_run {
            self.record_message(&session_id, &message);
        } else {
            let should_flush = self.queue_message(&session_id, &message, &message_id, sequence)?;
            if should_flush {
                batch_result = self.flush_batch().await?;
            }
        }

        Ok(ProcessLineResult {
            session_id: Some(session_id),
            processed: true,
            skipped: false,
            action_count,
            batch_result,
            ..Default::default()
        })
    }

    async fn process_session_meta(&mut self, meta: ParsedSessionMeta) -> Result<ProcessLineResult> {
        let session_id = meta.session_id.clone();
        self.apply_session_metadata(&session_id, &meta);
        self.ensure_session_ready(&session_id, meta.cwd.as_deref())
            .await?;
        Ok(ProcessLineResult {
            session_id: Some(session_id),
            ..Default::default()
        })
    }

    async fn process_turn_context(
        &mut self,
        context: ParsedTurnContext,
    ) -> Result<ProcessLineResult> {
        let Some(session_id) = context.session_id.clone() else {
            return Ok(ProcessLineResult {
                skipped: true,
                ..Default::default()
            });
        };

        self.apply_turn_context(&session_id, &context);
        self.ensure_session_ready(&session_id, context.cwd.as_deref())
            .await?;
        Ok(ProcessLineResult {
            session_id: Some(session_id),
            ..Default::default()
        })
    }
}
