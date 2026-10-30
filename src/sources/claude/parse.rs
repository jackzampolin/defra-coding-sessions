use crate::parser::{ParsedEntry, ParsedMessage, Role, TokenUsage, ToolAction};
use anyhow::Result;
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Map, Value};
use std::collections::BTreeSet;

pub(super) fn parse_value(value: &Value) -> Result<ParsedEntry> {
    let message = match value.get("message").and_then(Value::as_object) {
        Some(message) => message,
        None => return Ok(ParsedEntry::Skip),
    };

    let role = match message.get("role").and_then(Value::as_str) {
        Some("user") => Role::User,
        Some("assistant") => Role::Assistant,
        _ => return Ok(ParsedEntry::Skip),
    };

    let session_id = match value.get("sessionId").and_then(Value::as_str) {
        Some(session_id) if !session_id.is_empty() => session_id.to_string(),
        _ => return Ok(ParsedEntry::Skip),
    };

    let content_value = message.get("content").unwrap_or(&Value::Null);
    let (content, thinking, tool_uses) = flatten_content(content_value);
    let files_referenced = extract_files_referenced(content_value);
    let actions = if matches!(role, Role::Assistant) {
        extract_actions(content_value)
    } else {
        Vec::new()
    };

    let usage = parse_usage(message);

    Ok(ParsedEntry::Message(Box::new(ParsedMessage {
        session_id: Some(session_id),
        message_id: claude_message_id(value),
        transient_dedupe_id: None,
        role,
        cwd: string_field(value, "cwd"),
        git_branch: string_field(value, "gitBranch"),
        model: string_field_from_map(message, "model"),
        claude_version: string_field(value, "version"),
        title: None,
        archived: None,
        git_sha: None,
        git_origin_url: None,
        agent_role: None,
        reasoning_effort: None,
        content,
        thinking,
        tool_uses,
        files_referenced,
        usage,
        timestamp: normalize_timestamp(value.get("timestamp").and_then(Value::as_str)),
        actions,
    })))
}

fn parse_usage(message: &Map<String, Value>) -> TokenUsage {
    let usage = message.get("usage").and_then(Value::as_object);
    let input_tokens = usage_field(usage, "input_tokens");
    let output_tokens = usage_field(usage, "output_tokens");
    let cache_creation_input_tokens = usage_field(usage, "cache_creation_input_tokens")
        .or_else(|| cache_creation_detail_sum(usage));
    let cache_read_input_tokens = usage_field(usage, "cache_read_input_tokens");
    let reasoning_output_tokens = usage_field(usage, "reasoning_output_tokens");
    TokenUsage {
        input_tokens,
        output_tokens,
        cache_creation_input_tokens,
        cache_read_input_tokens,
        reasoning_output_tokens,
        total_tokens: usage_field(usage, "total_tokens").or_else(|| {
            total_from_parts([
                input_tokens,
                output_tokens,
                cache_creation_input_tokens,
                cache_read_input_tokens,
                reasoning_output_tokens,
            ])
        }),
    }
}

fn usage_field(usage: Option<&Map<String, Value>>, key: &str) -> Option<u64> {
    usage
        .and_then(|usage| usage.get(key))
        .and_then(Value::as_u64)
}

fn cache_creation_detail_sum(usage: Option<&Map<String, Value>>) -> Option<u64> {
    let sum = usage
        .and_then(|usage| usage.get("cache_creation"))
        .and_then(Value::as_object)
        .map(|cache_creation| {
            cache_creation
                .values()
                .filter_map(Value::as_u64)
                .sum::<u64>()
        })
        .unwrap_or(0);
    (sum > 0).then_some(sum)
}

fn total_from_parts(parts: impl IntoIterator<Item = Option<u64>>) -> Option<u64> {
    let total = parts.into_iter().flatten().sum::<u64>();
    (total > 0).then_some(total)
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn string_field_from_map(map: &Map<String, Value>, key: &str) -> Option<String> {
    map.get(key).and_then(Value::as_str).map(ToOwned::to_owned)
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

fn flatten_content(content: &Value) -> (String, Option<String>, Vec<String>) {
    match content {
        Value::String(text) => (text.clone(), None, Vec::new()),
        Value::Array(blocks) => {
            let mut text_parts = Vec::new();
            let mut thinking_parts = Vec::new();
            let mut tool_uses = Vec::new();

            for block in blocks {
                match block {
                    Value::Object(map) => match map.get("type").and_then(Value::as_str) {
                        Some("text") => {
                            if let Some(text) = map.get("text").and_then(Value::as_str) {
                                text_parts.push(text.to_string());
                            }
                        }
                        Some("thinking") => {
                            if let Some(thinking) = map.get("thinking").and_then(Value::as_str) {
                                thinking_parts.push(thinking.to_string());
                            }
                        }
                        Some("tool_use") => {
                            tool_uses.push(
                                map.get("name")
                                    .and_then(Value::as_str)
                                    .unwrap_or("unknown")
                                    .to_string(),
                            );
                        }
                        _ => {}
                    },
                    Value::String(text) => text_parts.push(text.clone()),
                    _ => {}
                }
            }

            let thinking = (!thinking_parts.is_empty()).then(|| thinking_parts.join("\n"));
            (text_parts.join("\n"), thinking, tool_uses)
        }
        other => (other.to_string(), None, Vec::new()),
    }
}

fn extract_files_referenced(content: &Value) -> Vec<String> {
    let mut files = BTreeSet::new();
    let Some(blocks) = content.as_array() else {
        return Vec::new();
    };

    for block in blocks {
        let Some(map) = block.as_object() else {
            continue;
        };
        if map.get("type").and_then(Value::as_str) != Some("tool_use") {
            continue;
        }

        let name = map.get("name").and_then(Value::as_str).unwrap_or_default();
        if !matches!(name, "Read" | "Write" | "Edit") {
            continue;
        }

        let Some(input) = map.get("input").and_then(Value::as_object) else {
            continue;
        };
        let file = input
            .get("file_path")
            .or_else(|| input.get("path"))
            .and_then(Value::as_str);
        if let Some(file) = file {
            files.insert(file.to_string());
        }
    }

    files.into_iter().collect()
}

fn extract_actions(content: &Value) -> Vec<ToolAction> {
    let Some(blocks) = content.as_array() else {
        return Vec::new();
    };

    let mut actions = Vec::new();
    for block in blocks {
        let Some(map) = block.as_object() else {
            continue;
        };
        if map.get("type").and_then(Value::as_str) != Some("tool_use") {
            continue;
        }

        let action_type = map
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let input = map.get("input").cloned().unwrap_or_else(empty_object);
        let target = tool_target_key(&action_type)
            .and_then(|key| input.get(key))
            .map(value_to_plain_string)
            .unwrap_or_default();
        let command = serde_json::to_string(&input).unwrap_or_else(|_| "{}".to_string());

        actions.push(ToolAction {
            call_id: None,
            action_type,
            target,
            command,
        });
    }

    actions
}

fn empty_object() -> Value {
    Value::Object(Map::new())
}

fn tool_target_key(tool_name: &str) -> Option<&'static str> {
    match tool_name {
        "Read" => Some("file_path"),
        "Write" => Some("file_path"),
        "Edit" => Some("file_path"),
        "Glob" => Some("pattern"),
        "Grep" => Some("pattern"),
        "Bash" => Some("command"),
        "Agent" => Some("description"),
        "WebSearch" => Some("query"),
        "WebFetch" => Some("url"),
        "NotebookEdit" => Some("notebook_path"),
        "Skill" => Some("skill"),
        _ => None,
    }
}

fn value_to_plain_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn claude_message_id(value: &Value) -> Option<String> {
    string_field(value, "uuid").map(|uuid| format!("claude:{uuid}"))
}
