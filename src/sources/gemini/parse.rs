use super::types::{GeminiChatFile, GeminiChatMessage, GeminiLogEntry, GeminiToolCall};
use crate::parser::{ParsedEntry, ParsedMessage, ParsedSessionMeta, Role, TokenUsage, ToolAction};
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::Value;
use std::collections::BTreeSet;

pub(super) fn parse_chat_message(
    session_id: &str,
    message: GeminiChatMessage,
    cwd: Option<String>,
) -> ParsedEntry {
    let role = match message.message_type.as_str() {
        "user" => Role::User,
        "gemini" => Role::Assistant,
        _ => return ParsedEntry::Skip,
    };

    let mut content = flatten_content(&message.content);
    if content.is_empty() {
        if let Some(display_content) = message.display_content.as_ref() {
            content = flatten_content(display_content);
        }
    }

    let tool_uses = extract_tool_uses(&message.tool_calls);
    let files_referenced = extract_files_referenced(&message.tool_calls);
    let actions = extract_actions(&message.tool_calls);
    let input_tokens = message.tokens.as_ref().and_then(|tokens| tokens.input);
    let output_tokens = message.tokens.as_ref().and_then(|tokens| tokens.output);

    ParsedEntry::Message(Box::new(ParsedMessage {
        session_id: Some(session_id.to_string()),
        message_id: message
            .id
            .as_deref()
            .filter(|value| !value.is_empty())
            .map(|value| gemini_message_id(session_id, value)),
        transient_dedupe_id: None,
        role,
        cwd,
        git_branch: None,
        model: non_empty(message.model.as_deref()),
        claude_version: None,
        title: None,
        archived: None,
        git_sha: None,
        git_origin_url: None,
        agent_role: None,
        reasoning_effort: None,
        content,
        thinking: None,
        tool_uses,
        files_referenced,
        usage: TokenUsage {
            input_tokens,
            output_tokens,
            cache_creation_input_tokens: None,
            cache_read_input_tokens: None,
            reasoning_output_tokens: None,
            total_tokens: total_from_parts([input_tokens, output_tokens]),
        },
        timestamp: normalize_timestamp(message.timestamp.as_deref()),
        actions,
    }))
}

pub(super) fn parse_log_entry(entry: GeminiLogEntry, cwd: Option<String>) -> ParsedEntry {
    let role = match entry.entry_type.as_str() {
        "user" => Role::User,
        "assistant" | "model" => Role::Assistant,
        _ => return ParsedEntry::Skip,
    };

    if entry.session_id.trim().is_empty() {
        return ParsedEntry::Skip;
    }

    let mut content = entry.message.trim().to_string();
    if content.is_empty() {
        if let Some(message_id) = entry.message_id {
            content = format!("message-{message_id}");
        } else {
            return ParsedEntry::Skip;
        }
    }

    ParsedEntry::Message(Box::new(ParsedMessage {
        session_id: Some(entry.session_id.clone()),
        message_id: entry
            .message_id
            .map(|message_id| gemini_message_id(&entry.session_id, &format!("log-{message_id}"))),
        transient_dedupe_id: None,
        role,
        cwd,
        git_branch: None,
        model: None,
        claude_version: None,
        title: None,
        archived: None,
        git_sha: None,
        git_origin_url: None,
        agent_role: None,
        reasoning_effort: None,
        content,
        thinking: None,
        tool_uses: Vec::new(),
        files_referenced: Vec::new(),
        usage: TokenUsage::default(),
        timestamp: normalize_timestamp(entry.timestamp.as_deref()),
        actions: Vec::new(),
    }))
}

pub(super) fn build_chat_session_meta(chat: &GeminiChatFile, cwd: Option<String>) -> ParsedEntry {
    ParsedEntry::SessionMeta(Box::new(ParsedSessionMeta {
        session_id: chat.session_id.clone(),
        cwd,
        git_branch: None,
        model: chat
            .messages
            .iter()
            .find_map(|message| non_empty(message.model.as_deref())),
        claude_version: None,
        title: non_empty(chat.summary.as_deref()),
        archived: None,
        git_sha: None,
        git_origin_url: None,
        agent_role: chat
            .kind
            .as_deref()
            .filter(|kind| !kind.is_empty() && *kind != "main")
            .map(ToOwned::to_owned),
        reasoning_effort: None,
        timestamp: normalize_timestamp(chat.start_time.as_deref().or(chat.last_updated.as_deref())),
        total_tokens: None,
        tools_used: Vec::new(),
    }))
}

fn total_from_parts(parts: impl IntoIterator<Item = Option<u64>>) -> Option<u64> {
    let total = parts.into_iter().flatten().sum::<u64>();
    (total > 0).then_some(total)
}

fn gemini_message_id(session_id: &str, raw_id: &str) -> String {
    format!("gemini:{session_id}:{raw_id}")
}

fn flatten_content(content: &Value) -> String {
    match content {
        Value::String(text) => text.trim().to_string(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(flatten_content_part)
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn flatten_content_part(part: &Value) -> Option<String> {
    match part {
        Value::String(text) => Some(text.trim().to_string()),
        Value::Object(map) => map
            .get("text")
            .and_then(Value::as_str)
            .map(|text| text.trim().to_string()),
        _ => None,
    }
}

fn extract_tool_uses(tool_calls: &[GeminiToolCall]) -> Vec<String> {
    let mut tool_uses = Vec::new();
    for tool_call in tool_calls {
        if !tool_uses.contains(&tool_call.name) {
            tool_uses.push(tool_call.name.clone());
        }
    }
    tool_uses
}

fn extract_files_referenced(tool_calls: &[GeminiToolCall]) -> Vec<String> {
    let mut files = BTreeSet::new();
    for tool_call in tool_calls {
        if let Value::Object(args) = &tool_call.args {
            for key in ["file_path", "dir_path", "plan_path"] {
                if let Some(path) = args.get(key).and_then(Value::as_str) {
                    files.insert(path.to_string());
                }
            }
        }
    }
    files.into_iter().collect()
}

fn extract_actions(tool_calls: &[GeminiToolCall]) -> Vec<ToolAction> {
    tool_calls
        .iter()
        .map(|tool_call| ToolAction {
            call_id: tool_call.id.clone(),
            action_type: tool_call.name.clone(),
            target: extract_tool_target(&tool_call.name, &tool_call.args).unwrap_or_default(),
            command: serde_json::to_string(&tool_call.args).unwrap_or_else(|_| "null".to_string()),
        })
        .collect()
}

fn extract_tool_target(tool_name: &str, args: &Value) -> Option<String> {
    let Value::Object(map) = args else {
        return (!args.is_null()).then(|| value_to_plain_string(args));
    };

    let keys = match tool_name {
        "run_shell_command" => &["command", "description"][..],
        "read_file" | "write_file" | "replace" => &["file_path"][..],
        "grep_search" => &["pattern", "dir_path"][..],
        "google_web_search" => &["query"][..],
        "list_directory" => &["dir_path"][..],
        "web_fetch" => &["prompt"][..],
        "enter_plan_mode" => &["reason"][..],
        "exit_plan_mode" => &["plan_path"][..],
        "ask_user" => &["questions"][..],
        _ => &[][..],
    };

    for key in keys {
        if let Some(value) = map.get(*key) {
            return Some(value_to_plain_string(value));
        }
    }

    (!map.is_empty()).then(|| Value::Object(map.clone()).to_string())
}

fn value_to_plain_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn normalize_timestamp(raw: Option<&str>) -> Option<String> {
    raw.map(|timestamp| {
        DateTime::parse_from_rfc3339(timestamp)
            .map(|parsed| {
                parsed
                    .with_timezone(&Utc)
                    .to_rfc3339_opts(SecondsFormat::Secs, true)
            })
            .unwrap_or_else(|_| timestamp.to_string())
    })
}

fn non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}
