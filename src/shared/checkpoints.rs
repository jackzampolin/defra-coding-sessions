use super::SessionMeta;
use crate::state::SessionCheckpoint;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(super) fn session_meta_from_checkpoint(checkpoint: SessionCheckpoint) -> SessionMeta {
    SessionMeta {
        doc_id: checkpoint.doc_id,
        project_doc_id: checkpoint.project_doc_id,
        cwd: checkpoint.cwd,
        git_branch: checkpoint.git_branch,
        git_origin_url: checkpoint.git_origin_url,
        current_model: checkpoint.current_model,
        claude_version: checkpoint.claude_version,
        msg_seq: checkpoint.msg_seq,
        model_counts: checkpoint.model_counts,
        created_at: checkpoint.created_at,
        last_ts: checkpoint.last_ts,
        last_touched: Some(system_time_from_epoch_millis(
            checkpoint.last_touched_epoch_ms,
        )),
        message_count: checkpoint.message_count,
        user_message_count: checkpoint.user_message_count,
        input_tokens: checkpoint.input_tokens,
        output_tokens: checkpoint.output_tokens,
        cache_creation_input_tokens: checkpoint.cache_creation_input_tokens,
        cache_read_input_tokens: checkpoint.cache_read_input_tokens,
        reasoning_output_tokens: checkpoint.reasoning_output_tokens,
        total_tokens: checkpoint.total_tokens,
        tools_used: checkpoint.tools_used.into_iter().collect(),
        first_prompt: checkpoint.first_prompt,
        title: checkpoint.title,
        archived: checkpoint.archived,
        git_sha: checkpoint.git_sha,
        agent_role: checkpoint.agent_role,
        reasoning_effort: checkpoint.reasoning_effort,
        seen_message_ids: std::collections::HashSet::new(),
    }
}

pub(super) fn session_checkpoint_from_meta(meta: &SessionMeta) -> SessionCheckpoint {
    SessionCheckpoint {
        doc_id: meta.doc_id.clone(),
        project_doc_id: meta.project_doc_id.clone(),
        cwd: meta.cwd.clone(),
        git_branch: meta.git_branch.clone(),
        git_origin_url: meta.git_origin_url.clone(),
        current_model: meta.current_model.clone(),
        claude_version: meta.claude_version.clone(),
        msg_seq: meta.msg_seq,
        model_counts: meta.model_counts.clone(),
        created_at: meta.created_at.clone(),
        last_ts: meta.last_ts.clone(),
        last_touched_epoch_ms: epoch_millis_from_system_time(
            meta.last_touched.unwrap_or_else(SystemTime::now),
        ),
        message_count: meta.message_count,
        user_message_count: meta.user_message_count,
        input_tokens: meta.input_tokens,
        output_tokens: meta.output_tokens,
        cache_creation_input_tokens: meta.cache_creation_input_tokens,
        cache_read_input_tokens: meta.cache_read_input_tokens,
        reasoning_output_tokens: meta.reasoning_output_tokens,
        total_tokens: meta.total_tokens,
        tools_used: meta.tools_used.iter().cloned().collect(),
        first_prompt: meta.first_prompt.clone(),
        title: meta.title.clone(),
        archived: meta.archived,
        git_sha: meta.git_sha.clone(),
        agent_role: meta.agent_role.clone(),
        reasoning_effort: meta.reasoning_effort.clone(),
    }
}

fn system_time_from_epoch_millis(value: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(value)
}

fn epoch_millis_from_system_time(value: SystemTime) -> u64 {
    value.duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}
