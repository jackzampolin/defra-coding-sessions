use super::enrichment::MessageEnrichment;
use super::types::{GrokChatMessage, GrokSignals, GrokSubagentMeta, GrokSummary};
use crate::parser::{ParsedEntry, ParsedMessage, ParsedSessionMeta, Role, TokenUsage, ToolAction};
use serde_json::Value;

mod extract;
mod ids;

use extract::{extract_actions, extract_files_referenced, extract_tool_uses};
use ids::{grok_message_id, non_empty, normalize_timestamp};

/// Session fields stamped onto every message for Claude/Codex parity.
#[derive(Debug, Clone, Default)]
pub(super) struct SessionStamp {
    pub(super) cwd: Option<String>,
    pub(super) git_branch: Option<String>,
    pub(super) git_sha: Option<String>,
    pub(super) git_origin_url: Option<String>,
    pub(super) model: Option<String>,
    pub(super) claude_version: Option<String>,
    pub(super) agent_role: Option<String>,
    pub(super) reasoning_effort: Option<String>,
}

pub(super) fn build_session_meta(
    session_id: &str,
    summary: Option<&GrokSummary>,
    signals: Option<&GrokSignals>,
    subagent: Option<&GrokSubagentMeta>,
    path_cwd: Option<String>,
    cli_version: Option<String>,
) -> ParsedEntry {
    let cwd = summary
        .and_then(|summary| summary.info.as_ref())
        .and_then(|info| non_empty(info.cwd.as_deref()))
        .or(path_cwd);

    let title = summary.and_then(|summary| {
        non_empty(summary.generated_title.as_deref())
            .or_else(|| non_empty(summary.session_summary.as_deref()))
    });

    let model = summary
        .and_then(|summary| non_empty(summary.current_model_id.as_deref()))
        .or_else(|| signals.and_then(|signals| non_empty(signals.primary_model_id.as_deref())));

    let agent_role = subagent
        .and_then(|meta| non_empty(meta.subagent_type.as_deref()))
        .or_else(|| summary.and_then(|summary| non_empty(summary.agent_name.as_deref())));

    let git_origin_url = summary.and_then(|summary| {
        summary
            .git_remotes
            .iter()
            .find_map(|remote| non_empty(Some(remote.as_str())))
    });

    ParsedEntry::SessionMeta(Box::new(ParsedSessionMeta {
        session_id: session_id.to_string(),
        cwd,
        git_branch: summary.and_then(|summary| non_empty(summary.head_branch.as_deref())),
        model,
        claude_version: cli_version,
        title,
        archived: None,
        git_sha: summary.and_then(|summary| non_empty(summary.head_commit.as_deref())),
        git_origin_url,
        agent_role,
        reasoning_effort: summary
            .and_then(|summary| non_empty(summary.reasoning_effort.as_deref())),
        timestamp: summary.and_then(|summary| {
            normalize_timestamp(
                summary
                    .created_at
                    .as_deref()
                    .or(summary.updated_at.as_deref()),
            )
        }),
        total_tokens: signals.and_then(|signals| signals.context_tokens_used),
        tools_used: signals
            .map(|signals| signals.tools_used.clone())
            .unwrap_or_default(),
    }))
}

pub(super) fn parse_chat_message(
    session_id: &str,
    line_index: u64,
    message: GrokChatMessage,
    stamp: &SessionStamp,
    pending_thinking: &mut Option<String>,
    enrichment: Option<&MessageEnrichment>,
) -> ParsedEntry {
    match message.message_type.as_str() {
        "system" | "tool_result" => ParsedEntry::Skip,
        "backend_tool_call" => {
            parse_backend_tool_call(session_id, line_index, &message, stamp, enrichment)
        }
        "reasoning" => {
            let thinking = extract_reasoning_summary(&message.summary);
            if thinking.trim().is_empty() {
                return ParsedEntry::Skip;
            }
            match pending_thinking {
                Some(existing) => {
                    existing.push('\n');
                    existing.push_str(&thinking);
                }
                None => *pending_thinking = Some(thinking),
            }
            ParsedEntry::Skip
        }
        "user" => {
            // Drop synthetic system reminders / skill dumps.
            if message.synthetic_reason.is_some() {
                return ParsedEntry::Skip;
            }
            // New user turn — discard unattached thinking.
            *pending_thinking = None;

            let content = flatten_content(&message.content);
            if content.trim().is_empty() {
                return ParsedEntry::Skip;
            }

            ParsedEntry::Message(Box::new(ParsedMessage {
                session_id: Some(session_id.to_string()),
                message_id: Some(grok_message_id(
                    session_id,
                    "user",
                    line_index,
                    enrichment.and_then(|value| value.prompt_id.as_deref()),
                    &content,
                    None,
                )),
                transient_dedupe_id: None,
                role: Role::User,
                cwd: stamp.cwd.clone(),
                git_branch: stamp.git_branch.clone(),
                model: None,
                claude_version: stamp.claude_version.clone(),
                title: None,
                archived: None,
                git_sha: stamp.git_sha.clone(),
                git_origin_url: stamp.git_origin_url.clone(),
                agent_role: stamp.agent_role.clone(),
                reasoning_effort: None,
                content,
                thinking: None,
                tool_uses: Vec::new(),
                files_referenced: Vec::new(),
                usage: TokenUsage::default(),
                timestamp: enrichment.and_then(|value| value.timestamp.clone()),
                actions: Vec::new(),
            }))
        }
        "assistant" => {
            let thinking = pending_thinking.take();
            let content = flatten_content(&message.content);
            let tool_uses = extract_tool_uses(&message.tool_calls);
            let files_referenced = extract_files_referenced(&message.tool_calls);
            let actions = extract_actions(&message.tool_calls);
            let stable_hint = message
                .tool_calls
                .first()
                .and_then(|tool_call| tool_call.id.as_deref())
                .or_else(|| enrichment.and_then(|value| value.prompt_id.as_deref()));

            if content.trim().is_empty()
                && thinking
                    .as_ref()
                    .map(|value| value.trim().is_empty())
                    .unwrap_or(true)
                && actions.is_empty()
            {
                return ParsedEntry::Skip;
            }

            ParsedEntry::Message(Box::new(ParsedMessage {
                session_id: Some(session_id.to_string()),
                message_id: Some(grok_message_id(
                    session_id,
                    "assistant",
                    line_index,
                    stable_hint,
                    &content,
                    thinking.as_deref(),
                )),
                transient_dedupe_id: None,
                role: Role::Assistant,
                cwd: stamp.cwd.clone(),
                git_branch: stamp.git_branch.clone(),
                model: non_empty(message.model_id.as_deref()).or_else(|| stamp.model.clone()),
                claude_version: stamp.claude_version.clone(),
                title: None,
                archived: None,
                git_sha: stamp.git_sha.clone(),
                git_origin_url: stamp.git_origin_url.clone(),
                agent_role: stamp.agent_role.clone(),
                reasoning_effort: non_empty(message.reasoning_effort.as_deref())
                    .or_else(|| stamp.reasoning_effort.clone()),
                content,
                thinking,
                tool_uses,
                files_referenced,
                usage: TokenUsage::default(),
                timestamp: enrichment.and_then(|value| value.timestamp.clone()),
                actions,
            }))
        }
        _ => ParsedEntry::Skip,
    }
}

fn parse_backend_tool_call(
    session_id: &str,
    line_index: u64,
    message: &GrokChatMessage,
    stamp: &SessionStamp,
    enrichment: Option<&MessageEnrichment>,
) -> ParsedEntry {
    let kind = message.kind.as_object();
    let tool_type = kind
        .and_then(|map| map.get("tool_type"))
        .and_then(Value::as_str)
        .unwrap_or("backend_tool")
        .to_string();
    let action_obj = kind.and_then(|map| map.get("action"));
    let call_id = kind
        .and_then(|map| map.get("id"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let stable_hint = call_id.as_deref();
    let target = action_obj
        .and_then(|action| {
            action
                .get("query")
                .or_else(|| action.get("url"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .unwrap_or_default();
    let command = action_obj
        .map(|action| serde_json::to_string(action).unwrap_or_else(|_| "{}".to_string()))
        .unwrap_or_else(|| "{}".to_string());

    ParsedEntry::Message(Box::new(ParsedMessage {
        session_id: Some(session_id.to_string()),
        message_id: Some(grok_message_id(
            session_id,
            "backend_tool",
            line_index,
            stable_hint,
            &tool_type,
            Some(&target),
        )),
        transient_dedupe_id: None,
        role: Role::Assistant,
        cwd: stamp.cwd.clone(),
        git_branch: stamp.git_branch.clone(),
        model: stamp.model.clone(),
        claude_version: stamp.claude_version.clone(),
        title: None,
        archived: None,
        git_sha: stamp.git_sha.clone(),
        git_origin_url: stamp.git_origin_url.clone(),
        agent_role: stamp.agent_role.clone(),
        reasoning_effort: stamp.reasoning_effort.clone(),
        content: String::new(),
        thinking: None,
        tool_uses: vec![tool_type.clone()],
        files_referenced: Vec::new(),
        usage: TokenUsage::default(),
        timestamp: enrichment.and_then(|value| value.timestamp.clone()),
        actions: vec![ToolAction {
            call_id,
            action_type: tool_type,
            target,
            command,
        }],
    }))
}

fn extract_reasoning_summary(summary: &Value) -> String {
    match summary {
        Value::Array(parts) => parts
            .iter()
            .filter_map(|part| {
                part.as_object().and_then(|map| {
                    if map.get("type").and_then(Value::as_str) == Some("summary_text") {
                        map.get("text").and_then(Value::as_str).map(str::to_string)
                    } else {
                        None
                    }
                })
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Value::String(text) => text.clone(),
        _ => String::new(),
    }
}

pub(super) fn flatten_content(content: &Value) -> String {
    match content {
        Value::String(text) => text.trim().to_string(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|part| match part {
                Value::String(text) => Some(text.trim().to_string()),
                Value::Object(map) => map
                    .get("text")
                    .and_then(Value::as_str)
                    .map(|text| text.trim().to_string()),
                _ => None,
            })
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}
