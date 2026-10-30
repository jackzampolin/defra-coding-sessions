use super::normalize::{normalize_timestamp, optional_session_id, string_field_from_map};
use super::tools::{
    extract_custom_tool_action, extract_function_call_action, extract_patch_file_paths,
    maybe_push_action_target, maybe_push_file,
};
use super::types::{ParsedEntry, ParsedMessage, Role, TokenUsage, ToolAction};
use anyhow::Result;
use serde_json::{Map, Value};
use std::collections::BTreeSet;

pub(super) fn parse_response_item(value: &Value, raw_line: &str) -> Result<ParsedEntry> {
    let Some(payload) = value.get("payload").and_then(Value::as_object) else {
        return Ok(ParsedEntry::Skip);
    };

    match payload.get("type").and_then(Value::as_str) {
        Some("message") => {
            let role = match payload.get("role").and_then(Value::as_str) {
                Some("user") => Role::User,
                Some("assistant") => Role::Assistant,
                _ => return Ok(ParsedEntry::Skip),
            };

            let (content, files_referenced) = flatten_content_blocks(payload.get("content"));
            Ok(build_message(
                optional_session_id(value, payload),
                role,
                content,
                None,
                Vec::new(),
                files_referenced,
                TokenUsage::default(),
                Vec::new(),
                normalize_timestamp(value.get("timestamp").and_then(Value::as_str)),
                raw_line,
            ))
        }
        Some("reasoning") => {
            let thinking = extract_reasoning_text(payload);
            if thinking.trim().is_empty() {
                return Ok(ParsedEntry::Skip);
            }

            Ok(build_message(
                optional_session_id(value, payload),
                Role::Assistant,
                String::new(),
                Some(thinking),
                Vec::new(),
                Vec::new(),
                TokenUsage::default(),
                Vec::new(),
                normalize_timestamp(value.get("timestamp").and_then(Value::as_str)),
                raw_line,
            ))
        }
        Some("function_call") => {
            let action = extract_function_call_action(payload);
            let mut files_referenced = BTreeSet::new();
            maybe_push_action_target(&mut files_referenced, &action.target);

            Ok(build_message(
                optional_session_id(value, payload),
                Role::Assistant,
                String::new(),
                None,
                vec![action.action_type.clone()],
                files_referenced.into_iter().collect(),
                TokenUsage::default(),
                vec![action],
                normalize_timestamp(value.get("timestamp").and_then(Value::as_str)),
                raw_line,
            ))
        }
        Some("custom_tool_call") => {
            let action = extract_custom_tool_action(payload);
            let files_referenced = extract_patch_file_paths(
                payload
                    .get("input")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            );

            Ok(build_message(
                optional_session_id(value, payload),
                Role::Assistant,
                String::new(),
                None,
                vec![action.action_type.clone()],
                files_referenced,
                TokenUsage::default(),
                vec![action],
                normalize_timestamp(value.get("timestamp").and_then(Value::as_str)),
                raw_line,
            ))
        }
        Some("web_search_call") => {
            let action = ToolAction {
                call_id: string_field_from_map(payload, "call_id"),
                action_type: "web_search".to_string(),
                target: string_field_from_map(payload, "query").unwrap_or_default(),
                command: serde_json::to_string(payload).unwrap_or_else(|_| "{}".to_string()),
            };

            Ok(build_message(
                optional_session_id(value, payload),
                Role::Assistant,
                String::new(),
                None,
                vec![action.action_type.clone()],
                Vec::new(),
                TokenUsage::default(),
                vec![action],
                normalize_timestamp(value.get("timestamp").and_then(Value::as_str)),
                raw_line,
            ))
        }
        _ => Ok(ParsedEntry::Skip),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn build_message(
    session_id: Option<String>,
    role: Role,
    content: String,
    thinking: Option<String>,
    tool_uses: Vec<String>,
    files_referenced: Vec<String>,
    usage: TokenUsage,
    actions: Vec<ToolAction>,
    timestamp: Option<String>,
    raw_line: &str,
) -> ParsedEntry {
    ParsedEntry::Message(Box::new(ParsedMessage {
        session_id,
        role,
        content,
        thinking,
        tool_uses,
        files_referenced,
        usage,
        timestamp,
        actions,
        raw_line: raw_line.to_string(),
    }))
}

fn flatten_content_blocks(content: Option<&Value>) -> (String, Vec<String>) {
    let mut text_parts = Vec::new();
    let mut files = BTreeSet::new();

    let Some(blocks) = content.and_then(Value::as_array) else {
        return (String::new(), Vec::new());
    };

    for block in blocks {
        let Some(map) = block.as_object() else {
            continue;
        };

        match map.get("type").and_then(Value::as_str) {
            Some("input_text") | Some("output_text") => {
                if let Some(text) = map.get("text").and_then(Value::as_str) {
                    text_parts.push(text.to_string());
                }
            }
            Some("input_image") => {
                if let Some(image_url) = map.get("image_url").and_then(Value::as_str) {
                    maybe_push_file(&mut files, image_url);
                }
            }
            _ => {}
        }
    }

    (text_parts.join("\n"), files.into_iter().collect())
}

fn extract_reasoning_text(payload: &Map<String, Value>) -> String {
    if let Some(content) = payload.get("content").and_then(Value::as_str) {
        return content.to_string();
    }

    let Some(summary) = payload.get("summary").and_then(Value::as_array) else {
        return String::new();
    };

    summary
        .iter()
        .filter_map(|item| item.as_object())
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("summary_text"))
        .filter_map(|item| item.get("text").and_then(Value::as_str))
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>()
        .join("\n")
}
