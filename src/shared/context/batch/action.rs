use crate::mutations::{create_action_mutation, deterministic_action_id, ActionMutationInput};

use super::super::super::graphql::{
    alias_outcome, execute_with_retry, extract_doc_id, has_transaction_conflict_error,
    is_already_exists_error, is_transaction_conflict_text, AliasOutcome,
};
use super::super::super::{
    BatchResult, PendingAction, PendingMessage, SharedContext, MAX_MUTATION_RETRIES,
};

impl SharedContext {
    pub(super) fn build_action_mutations(
        &self,
        pending: &PendingMessage,
        message_doc_id: &str,
    ) -> Vec<PendingAction> {
        pending
            .message
            .actions
            .iter()
            .map(|action| {
                let action_id_tag = format!(
                    "action_id:{}",
                    deterministic_action_id(&pending.session_id, action)
                );
                PendingAction {
                    session_id: pending.session_id.clone(),
                    message_id: pending.message_id.clone(),
                    action_id_tag: action_id_tag.clone(),
                    mutation: ActionMutationInput {
                        message_doc_id: message_doc_id.to_string(),
                        session_doc_id: pending.session_doc_id.clone(),
                        action: action.clone(),
                        tags: vec![action_id_tag],
                        created_at: pending.message.timestamp.clone(),
                    },
                }
            })
            .collect()
    }

    async fn execute_action_batch(&mut self, pending_actions: Vec<PendingAction>) -> BatchResult {
        let Some(query) = self.batch.take() else {
            return BatchResult::default();
        };
        if pending_actions.is_empty() {
            return BatchResult::default();
        }

        let response = execute_with_retry(
            &self.client,
            &query,
            MAX_MUTATION_RETRIES,
            self.metrics.as_ref(),
        )
        .await;
        let mut result = BatchResult::default();
        let mut retry_actions = Vec::new();

        for (index, pending) in pending_actions.into_iter().enumerate() {
            match alias_outcome(&response, index) {
                AliasOutcome::Created(_) => result.created += 1,
                AliasOutcome::Skipped => result.skipped += 1,
                AliasOutcome::Failed(message) => {
                    if is_transaction_conflict_text(&message)
                        || message == "mutation returned no data"
                    {
                        retry_actions.push(pending);
                    } else {
                        result.failed += 1;
                        tracing::warn!(
                            alias = %format!("m{index}"),
                            session_id = %pending.session_id,
                            message_id = %pending.message_id,
                            error = %message,
                            "batched CodingAction mutation failed"
                        );
                    }
                }
            }
        }

        for pending in retry_actions {
            result.merge(self.retry_action_mutation(&pending).await);
        }

        result
    }

    async fn retry_action_mutation(&self, pending: &PendingAction) -> BatchResult {
        let response = execute_with_retry(
            &self.client,
            &create_action_mutation(&pending.mutation),
            MAX_MUTATION_RETRIES,
            self.metrics.as_ref(),
        )
        .await;
        let mut result = BatchResult::default();

        if extract_doc_id(&response.data, "create_CodingAction").is_some() {
            result.created += 1;
            return result;
        }

        if !response.has_errors() {
            match self.lookup_action_doc_id(&pending.action_id_tag).await {
                Ok(_) => {
                    result.skipped += 1;
                    return result;
                }
                Err(lookup_error) => {
                    tracing::warn!(
                        session_id = %pending.session_id,
                        message_id = %pending.message_id,
                        action_id_tag = %pending.action_id_tag,
                        error = %lookup_error,
                        "individual CodingAction retry returned no data"
                    );
                }
            }
        }

        if is_already_exists_error(&response) {
            result.skipped += 1;
            return result;
        }

        if has_transaction_conflict_error(&response) {
            match self.lookup_action_doc_id(&pending.action_id_tag).await {
                Ok(_) => {
                    result.skipped += 1;
                    return result;
                }
                Err(lookup_error) => {
                    tracing::warn!(
                        session_id = %pending.session_id,
                        message_id = %pending.message_id,
                        action_id_tag = %pending.action_id_tag,
                        error = %lookup_error,
                        "failed to confirm CodingAction after transaction conflict retry"
                    );
                }
            }

            result.failed += 1;
            tracing::warn!(
                session_id = %pending.session_id,
                message_id = %pending.message_id,
                action_id_tag = %pending.action_id_tag,
                error = ?response.errors,
                "individual CodingAction retry still conflicted"
            );
            return result;
        }

        result.failed += 1;
        tracing::warn!(
            session_id = %pending.session_id,
            message_id = %pending.message_id,
            action_id_tag = %pending.action_id_tag,
            error = ?response.errors,
            "individual CodingAction retry failed"
        );
        result
    }
}

pub(super) async fn flush_action_mutations(
    context: &mut SharedContext,
    mutations: Vec<PendingAction>,
) -> BatchResult {
    let mut result = BatchResult::default();
    let mut pending_actions = Vec::new();

    for mutation in mutations {
        let should_flush = context
            .batch
            .push(create_action_mutation(&mutation.mutation));
        pending_actions.push(mutation);
        if should_flush {
            result.merge(
                context
                    .execute_action_batch(std::mem::take(&mut pending_actions))
                    .await,
            );
        }
    }

    if !context.batch.is_empty() {
        result.merge(context.execute_action_batch(pending_actions).await);
    }

    result
}
