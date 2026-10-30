use super::parser;
use super::resume::ResumeContext;
use crate::parser::{
    ParsedEntry, ParsedMessage, ParsedSessionMeta, ParsedTurnContext, Role, TokenUsage, ToolAction,
};

pub(super) fn map_entry(entry: parser::ParsedEntry, resume_context: &ResumeContext) -> ParsedEntry {
    match entry {
        parser::ParsedEntry::Skip => ParsedEntry::Skip,
        parser::ParsedEntry::SessionMeta(meta) => {
            ParsedEntry::SessionMeta(Box::new(ParsedSessionMeta {
                session_id: meta.session_id,
                cwd: meta.cwd,
                git_branch: meta.git_branch,
                model: meta.model,
                claude_version: meta.cli_version,
                title: None,
                archived: None,
                git_sha: meta.git_sha,
                git_origin_url: meta.git_origin_url,
                agent_role: meta.agent_role,
                reasoning_effort: None,
                timestamp: meta.timestamp,
                total_tokens: None,
                tools_used: Vec::new(),
            }))
        }
        parser::ParsedEntry::TurnContext(context) => {
            ParsedEntry::TurnContext(Box::new(ParsedTurnContext {
                session_id: context
                    .session_id
                    .or_else(|| resume_context.session_id.clone()),
                cwd: context.cwd.or_else(|| resume_context.cwd.clone()),
                model: context.model.or_else(|| resume_context.model.clone()),
                reasoning_effort: context
                    .effort
                    .or_else(|| resume_context.reasoning_effort.clone()),
                timestamp: context.timestamp,
            }))
        }
        parser::ParsedEntry::Message(message) => {
            let session_id = message
                .session_id
                .or_else(|| resume_context.session_id.clone());
            let message_id = session_id.as_ref().map(|session_id| {
                parser::compute_message_id(session_id, message.raw_line.as_bytes())
            });
            let transient_dedupe_id = session_id.as_ref().map(|session_id| {
                format!(
                    "{}:{}",
                    parser::REPLAY_DEDUPE_VERSION,
                    parser::compute_dedupe_key_with_turn(
                        session_id,
                        message.raw_line.as_bytes(),
                        resume_context.turn_id.as_deref(),
                    )
                )
            });

            ParsedEntry::Message(Box::new(ParsedMessage {
                session_id,
                message_id,
                transient_dedupe_id,
                role: map_role(message.role),
                cwd: resume_context.cwd.clone(),
                git_branch: resume_context.git_branch.clone(),
                model: resume_context.model.clone(),
                claude_version: resume_context.cli_version.clone(),
                title: None,
                archived: None,
                git_sha: resume_context.git_sha.clone(),
                git_origin_url: resume_context.git_origin_url.clone(),
                agent_role: resume_context.agent_role.clone(),
                reasoning_effort: resume_context.reasoning_effort.clone(),
                content: message.content,
                thinking: message.thinking,
                tool_uses: message.tool_uses,
                files_referenced: message.files_referenced,
                usage: TokenUsage {
                    input_tokens: message.usage.input_tokens,
                    output_tokens: message.usage.output_tokens,
                    cache_creation_input_tokens: None,
                    cache_read_input_tokens: message.usage.cache_read_input_tokens,
                    reasoning_output_tokens: message.usage.reasoning_output_tokens,
                    total_tokens: message.usage.total_tokens,
                },
                timestamp: message.timestamp,
                actions: message
                    .actions
                    .into_iter()
                    .map(|action| ToolAction {
                        call_id: action.call_id,
                        action_type: action.action_type,
                        target: action.target,
                        command: action.command,
                    })
                    .collect(),
            }))
        }
    }
}

fn map_role(role: parser::Role) -> Role {
    match role {
        parser::Role::User => Role::User,
        parser::Role::Assistant => Role::Assistant,
    }
}
