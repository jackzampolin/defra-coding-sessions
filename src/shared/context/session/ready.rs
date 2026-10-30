use super::super::super::graphql::{
    array_field, ensure_success, execute_with_retry, extract_doc_id, extract_doc_id_from_value,
    extract_first_item, integer_field, is_already_exists_error, string_field,
};
use super::super::super::{SessionMeta, SharedContext, MAX_MUTATION_RETRIES};
use super::super::metadata::merge_session_cache;
use crate::mutations::{
    create_session_mutation, query_message_max_sequence, query_session_checkpoint,
    SessionMutationInput,
};
use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::collections::HashSet;
use std::time::SystemTime;

impl SharedContext {
    pub(in crate::shared::context) async fn ensure_session_ready(
        &mut self,
        session_id: &str,
        cwd_hint: Option<&str>,
    ) -> Result<()> {
        self.touch_session(session_id);

        let project_doc_id = if let Some(doc_id) = self
            .session_cache
            .get(session_id)
            .and_then(|meta| meta.project_doc_id.clone())
        {
            doc_id
        } else {
            let cwd = cwd_hint
                .map(ToOwned::to_owned)
                .or_else(|| {
                    self.session_cache
                        .get(session_id)
                        .and_then(|meta| meta.cwd.clone())
                })
                .context("session missing cwd before session creation")?;
            let project_doc_id = self.ensure_project(&cwd).await?;
            let meta = self
                .session_cache
                .entry(session_id.to_string())
                .or_default();
            meta.cwd.get_or_insert(cwd);
            meta.project_doc_id = Some(project_doc_id.clone());
            project_doc_id
        };

        if self
            .session_cache
            .get(session_id)
            .map(|meta| !meta.doc_id.is_empty())
            .unwrap_or(false)
        {
            return Ok(());
        }

        if self.dry_run {
            let meta = self
                .session_cache
                .entry(session_id.to_string())
                .or_default();
            meta.doc_id = format!("dry-run-session-{session_id}");
            meta.project_doc_id = Some(project_doc_id);
            meta.last_touched = Some(SystemTime::now());
            return Ok(());
        }

        let session_input = self
            .session_cache
            .get(session_id)
            .cloned()
            .unwrap_or_default();
        if let Some(loaded_meta) = self.lookup_session_meta(session_id).await? {
            self.adopt_loaded_session_meta(session_id, loaded_meta);
            return Ok(());
        }

        let mutation = create_session_mutation(&SessionMutationInput {
            session_id: session_id.to_string(),
            project_doc_id: project_doc_id.clone(),
            git_branch: session_input.git_branch.clone(),
            source: self.source_label.clone(),
            source_host: Some(self.source_host.clone()),
            model_primary: session_input.current_model.clone(),
            claude_version: session_input.claude_version.clone(),
            title: session_input.title.clone(),
            archived: session_input.archived,
            git_sha: session_input.git_sha.clone(),
            git_origin_url: session_input.git_origin_url.clone(),
            agent_role: session_input.agent_role.clone(),
            reasoning_effort: session_input.reasoning_effort.clone(),
            created_at: session_input.created_at.clone(),
            finished_at: None,
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
        });

        let response = execute_with_retry(
            &self.client,
            &mutation,
            MAX_MUTATION_RETRIES,
            self.metrics.as_ref(),
        )
        .await;
        let loaded_meta = if response.has_errors() {
            if is_already_exists_error(&response) {
                self.load_session_meta(session_id).await?
            } else {
                anyhow::bail!("create_CodingSession failed: {:?}", response.errors);
            }
        } else {
            let doc_id = extract_doc_id(&response.data, "create_CodingSession").unwrap_or_default();
            if doc_id.is_empty() {
                self.load_session_meta(session_id).await?
            } else {
                SessionMeta {
                    doc_id,
                    project_doc_id: Some(project_doc_id),
                    last_touched: Some(SystemTime::now()),
                    ..session_input
                }
            }
        };

        self.adopt_loaded_session_meta(session_id, loaded_meta);
        Ok(())
    }

    async fn load_session_meta(&self, session_id: &str) -> Result<SessionMeta> {
        self.lookup_session_meta(session_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("no CodingSession returned for {}", session_id))
    }

    async fn lookup_session_meta(&self, session_id: &str) -> Result<Option<SessionMeta>> {
        let query = query_session_checkpoint(session_id);
        let response = self.client.execute(&query).await;
        ensure_success(&response, "load session checkpoint")?;

        let Some(session) = extract_first_item(&response.data, "CodingSession") else {
            return Ok(None);
        };
        Ok(Some(
            self.session_meta_from_value(session_id, session).await?,
        ))
    }

    async fn session_meta_from_value(
        &self,
        session_id: &str,
        session: &Value,
    ) -> Result<SessionMeta> {
        let doc_id = extract_doc_id_from_value(session)
            .ok_or_else(|| anyhow::anyhow!("no session doc_id returned for {}", session_id))?;
        let msg_seq = self.load_next_message_sequence(&doc_id).await?;

        let mut model_counts = HashMap::new();
        if let Some(model_primary) =
            string_field(session, "model_primary").filter(|m| !m.is_empty())
        {
            model_counts.insert(
                model_primary,
                integer_field(session, "message_count").max(1),
            );
        }

        Ok(SessionMeta {
            doc_id,
            project_doc_id: None,
            cwd: None,
            git_branch: string_field(session, "git_branch"),
            git_origin_url: string_field(session, "git_origin_url"),
            current_model: string_field(session, "model_primary"),
            claude_version: string_field(session, "claude_version"),
            msg_seq,
            model_counts,
            created_at: string_field(session, "created_at"),
            last_ts: None,
            message_count: integer_field(session, "message_count"),
            user_message_count: integer_field(session, "user_message_count"),
            input_tokens: integer_field(session, "input_tokens"),
            output_tokens: integer_field(session, "output_tokens"),
            cache_creation_input_tokens: integer_field(session, "cache_creation_input_tokens"),
            cache_read_input_tokens: integer_field(session, "cache_read_input_tokens"),
            reasoning_output_tokens: integer_field(session, "reasoning_output_tokens"),
            total_tokens: integer_field(session, "total_tokens"),
            tools_used: array_field(session, "tools_used").into_iter().collect(),
            first_prompt: string_field(session, "first_prompt"),
            title: string_field(session, "title"),
            archived: session.get("archived").and_then(Value::as_bool),
            git_sha: string_field(session, "git_sha"),
            agent_role: string_field(session, "agent_role"),
            reasoning_effort: string_field(session, "reasoning_effort"),
            last_touched: Some(SystemTime::now()),
            seen_message_ids: HashSet::new(),
        })
    }

    fn adopt_loaded_session_meta(&mut self, session_id: &str, loaded_meta: SessionMeta) {
        let loaded_project_doc_id = loaded_meta.project_doc_id.clone();
        let loaded_doc_id = loaded_meta.doc_id.clone();
        let loaded_msg_seq = loaded_meta.msg_seq;
        let mut final_meta = self.session_cache.remove(session_id).unwrap_or_default();

        merge_session_cache(&mut final_meta, &loaded_meta);
        if final_meta.project_doc_id.is_none() {
            final_meta.project_doc_id = loaded_project_doc_id;
        }
        final_meta.doc_id = loaded_doc_id;
        final_meta.msg_seq = loaded_msg_seq;
        final_meta.last_touched = Some(SystemTime::now());

        self.session_cache
            .insert(session_id.to_string(), final_meta);
    }

    async fn load_next_message_sequence(&self, session_doc_id: &str) -> Result<u64> {
        let query = query_message_max_sequence(session_doc_id);
        let response = self.client.execute(&query).await;
        ensure_success(&response, "load message sequence")?;

        Ok(extract_first_item(&response.data, "CodingMessage")
            .and_then(|message| message.get("sequence"))
            .and_then(Value::as_u64)
            .map(|sequence| sequence + 1)
            .unwrap_or(0))
    }
}
