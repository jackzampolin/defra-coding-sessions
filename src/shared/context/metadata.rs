use super::super::{SessionMeta, SharedContext};
use crate::parser::{ParsedMessage, ParsedSessionMeta, ParsedTurnContext, Role};
use std::time::SystemTime;

impl SharedContext {
    pub(super) fn apply_session_metadata(&mut self, session_id: &str, meta: &ParsedSessionMeta) {
        let cache = self
            .session_cache
            .entry(session_id.to_string())
            .or_default();
        if let Some(cwd) = meta.cwd.as_ref().filter(|value| !value.is_empty()) {
            cache.cwd = Some(cwd.clone());
        }
        if let Some(git_branch) = meta.git_branch.as_ref().filter(|value| !value.is_empty()) {
            cache.git_branch = Some(git_branch.clone());
        }
        if let Some(model) = meta.model.as_ref().filter(|value| !value.is_empty()) {
            cache.current_model = Some(model.clone());
        }
        if let Some(claude_version) = meta
            .claude_version
            .as_ref()
            .filter(|value| !value.is_empty())
        {
            cache.claude_version = Some(claude_version.clone());
        }
        if let Some(title) = meta.title.as_ref().filter(|value| !value.is_empty()) {
            cache.title = Some(title.clone());
        }
        if let Some(archived) = meta.archived {
            cache.archived = Some(archived);
        }
        if let Some(git_sha) = meta.git_sha.as_ref().filter(|value| !value.is_empty()) {
            cache.git_sha = Some(git_sha.clone());
        }
        if let Some(git_origin_url) = meta
            .git_origin_url
            .as_ref()
            .filter(|value| !value.is_empty())
        {
            cache.git_origin_url = Some(git_origin_url.clone());
        }
        if let Some(agent_role) = meta.agent_role.as_ref().filter(|value| !value.is_empty()) {
            cache.agent_role = Some(agent_role.clone());
        }
        if let Some(reasoning_effort) = meta
            .reasoning_effort
            .as_ref()
            .filter(|value| !value.is_empty())
        {
            cache.reasoning_effort = Some(reasoning_effort.clone());
        }
        if let Some(total_tokens) = meta.total_tokens {
            cache.total_tokens = cache.total_tokens.max(total_tokens);
        }
        cache.tools_used.extend(meta.tools_used.iter().cloned());
        cache.last_touched = Some(SystemTime::now());
        update_created_at(cache, meta.timestamp.as_deref());
        update_last_ts(cache, meta.timestamp.as_deref());
    }

    pub(super) fn apply_turn_context(&mut self, session_id: &str, context: &ParsedTurnContext) {
        let cache = self
            .session_cache
            .entry(session_id.to_string())
            .or_default();
        if let Some(cwd) = context.cwd.as_ref().filter(|value| !value.is_empty()) {
            cache.cwd = Some(cwd.clone());
        }
        if let Some(model) = context.model.as_ref().filter(|value| !value.is_empty()) {
            cache.current_model = Some(model.clone());
        }
        if let Some(reasoning_effort) = context
            .reasoning_effort
            .as_ref()
            .filter(|value| !value.is_empty())
        {
            cache.reasoning_effort = Some(reasoning_effort.clone());
        }
        cache.last_touched = Some(SystemTime::now());
        update_created_at(cache, context.timestamp.as_deref());
        update_last_ts(cache, context.timestamp.as_deref());
    }

    pub(super) fn apply_message_metadata(&mut self, session_id: &str, message: &ParsedMessage) {
        let cache = self
            .session_cache
            .entry(session_id.to_string())
            .or_default();
        if let Some(cwd) = message.cwd.as_ref().filter(|value| !value.is_empty()) {
            cache.cwd = Some(cwd.clone());
        }
        if let Some(git_branch) = message
            .git_branch
            .as_ref()
            .filter(|value| !value.is_empty())
        {
            cache.git_branch = Some(git_branch.clone());
        }
        if let Some(model) = message.model.as_ref().filter(|value| !value.is_empty()) {
            cache.current_model = Some(model.clone());
        }
        if let Some(claude_version) = message
            .claude_version
            .as_ref()
            .filter(|value| !value.is_empty())
        {
            cache.claude_version = Some(claude_version.clone());
        }
        if let Some(title) = message.title.as_ref().filter(|value| !value.is_empty()) {
            cache.title = Some(title.clone());
        }
        if let Some(archived) = message.archived {
            cache.archived = Some(archived);
        }
        if let Some(git_sha) = message.git_sha.as_ref().filter(|value| !value.is_empty()) {
            cache.git_sha = Some(git_sha.clone());
        }
        if let Some(git_origin_url) = message
            .git_origin_url
            .as_ref()
            .filter(|value| !value.is_empty())
        {
            cache.git_origin_url = Some(git_origin_url.clone());
        }
        if let Some(agent_role) = message
            .agent_role
            .as_ref()
            .filter(|value| !value.is_empty())
        {
            cache.agent_role = Some(agent_role.clone());
        }
        if let Some(reasoning_effort) = message
            .reasoning_effort
            .as_ref()
            .filter(|value| !value.is_empty())
        {
            cache.reasoning_effort = Some(reasoning_effort.clone());
        }
        cache.last_touched = Some(SystemTime::now());
        update_created_at(cache, message.timestamp.as_deref());
        update_last_ts(cache, message.timestamp.as_deref());
    }

    pub(super) fn touch_session(&mut self, session_id: &str) {
        if let Some(meta) = self.session_cache.get_mut(session_id) {
            meta.last_touched = Some(SystemTime::now());
        }
    }

    pub(super) fn record_message(&mut self, session_id: &str, message: &ParsedMessage) {
        let meta = self
            .session_cache
            .get_mut(session_id)
            .expect("session metadata should exist before recording");

        meta.last_touched = Some(SystemTime::now());
        meta.message_count += 1;
        if matches!(message.role, Role::User) {
            meta.user_message_count += 1;
            if meta.first_prompt.is_none() && !message.content.trim().is_empty() {
                meta.first_prompt = Some(message.content.chars().take(500).collect());
            }
        }
        if let Some(model) = meta
            .current_model
            .as_ref()
            .filter(|model| !model.is_empty())
        {
            *meta.model_counts.entry(model.clone()).or_insert(0) += 1;
        }
        update_created_at(meta, message.timestamp.as_deref());
        update_last_ts(meta, message.timestamp.as_deref());
        meta.input_tokens += message.usage.input_tokens.unwrap_or(0);
        meta.output_tokens += message.usage.output_tokens.unwrap_or(0);
        meta.cache_creation_input_tokens += message.usage.cache_creation_input_tokens.unwrap_or(0);
        meta.cache_read_input_tokens += message.usage.cache_read_input_tokens.unwrap_or(0);
        meta.reasoning_output_tokens += message.usage.reasoning_output_tokens.unwrap_or(0);
        // Context-window snapshots (total_tokens only, no usage breakdown) must not
        // inflate the session total — those are applied via SessionMeta instead.
        let has_usage_breakdown = message.usage.input_tokens.is_some()
            || message.usage.output_tokens.is_some()
            || message.usage.cache_creation_input_tokens.is_some()
            || message.usage.cache_read_input_tokens.is_some()
            || message.usage.reasoning_output_tokens.is_some();
        if has_usage_breakdown {
            meta.total_tokens += message.usage.total_tokens.unwrap_or(0);
        }
        meta.tools_used.extend(message.tool_uses.iter().cloned());
    }
}

pub(super) fn merge_session_cache(target: &mut SessionMeta, source: &SessionMeta) {
    if target.doc_id.is_empty() {
        target.doc_id = source.doc_id.clone();
    }
    if target.project_doc_id.is_none() {
        target.project_doc_id = source.project_doc_id.clone();
    }
    if target.cwd.is_none() {
        target.cwd = source.cwd.clone();
    }
    if target.git_branch.is_none() {
        target.git_branch = source.git_branch.clone();
    }
    if target.git_origin_url.is_none() {
        target.git_origin_url = source.git_origin_url.clone();
    }
    if target.current_model.is_none() {
        target.current_model = source.current_model.clone();
    }
    if target.claude_version.is_none() {
        target.claude_version = source.claude_version.clone();
    }
    if target.created_at.is_none() {
        target.created_at = source.created_at.clone();
    }
    if target.last_ts.is_none() {
        target.last_ts = source.last_ts.clone();
    }
    if target.first_prompt.is_none() {
        target.first_prompt = source.first_prompt.clone();
    }
    if target.title.is_none() {
        target.title = source.title.clone();
    }
    if target.archived.is_none() {
        target.archived = source.archived;
    }
    if target.git_sha.is_none() {
        target.git_sha = source.git_sha.clone();
    }
    if target.agent_role.is_none() {
        target.agent_role = source.agent_role.clone();
    }
    if target.reasoning_effort.is_none() {
        target.reasoning_effort = source.reasoning_effort.clone();
    }
    target.msg_seq = target.msg_seq.max(source.msg_seq);
    target.message_count = target.message_count.max(source.message_count);
    target.user_message_count = target.user_message_count.max(source.user_message_count);
    target.input_tokens = target.input_tokens.max(source.input_tokens);
    target.output_tokens = target.output_tokens.max(source.output_tokens);
    target.cache_creation_input_tokens = target
        .cache_creation_input_tokens
        .max(source.cache_creation_input_tokens);
    target.cache_read_input_tokens = target
        .cache_read_input_tokens
        .max(source.cache_read_input_tokens);
    target.reasoning_output_tokens = target
        .reasoning_output_tokens
        .max(source.reasoning_output_tokens);
    target.total_tokens = target.total_tokens.max(source.total_tokens);
    for (model, count) in &source.model_counts {
        target
            .model_counts
            .entry(model.clone())
            .and_modify(|existing| *existing = (*existing).max(*count))
            .or_insert(*count);
    }
    target.tools_used.extend(source.tools_used.iter().cloned());
}

pub(super) fn update_created_at(meta: &mut SessionMeta, candidate: Option<&str>) {
    let Some(candidate) = candidate else {
        return;
    };

    if meta
        .created_at
        .as_deref()
        .is_none_or(|current| current.is_empty() || candidate < current)
    {
        meta.created_at = Some(candidate.to_string());
    }
}

pub(super) fn update_last_ts(meta: &mut SessionMeta, candidate: Option<&str>) {
    if let Some(candidate) =
        candidate.filter(|value| is_newer_timestamp(meta.last_ts.as_deref(), Some(value)))
    {
        meta.last_ts = Some(candidate.to_string());
    }
}

fn is_newer_timestamp(current: Option<&str>, candidate: Option<&str>) -> bool {
    let Some(candidate) = candidate else {
        return false;
    };
    let Some(current) = current else {
        return true;
    };

    match (
        chrono::DateTime::parse_from_rfc3339(current),
        chrono::DateTime::parse_from_rfc3339(candidate),
    ) {
        (Ok(current), Ok(candidate)) => candidate >= current,
        _ => candidate >= current,
    }
}
