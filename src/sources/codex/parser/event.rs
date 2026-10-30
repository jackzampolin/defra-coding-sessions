use super::message::build_message;
use super::normalize::{
    normalize_timestamp, optional_session_id, string_field_from_map, total_from_parts,
};
use super::tools::maybe_push_file;
use super::types::{ParsedEntry, Role, TokenUsage};
use anyhow::Result;
use serde_json::{Map, Value};
use std::collections::BTreeSet;

pub(super) fn parse_event_message(value: &Value, raw_line: &str) -> Result<ParsedEntry> {
    let Some(payload) = value.get("payload").and_then(Value::as_object) else {
        return Ok(ParsedEntry::Skip);
    };

    match payload.get("type").and_then(Value::as_str) {
        Some("user_message") => Ok(build_message(
            optional_session_id(value, payload),
            Role::User,
            string_field_from_map(payload, "message").unwrap_or_default(),
            None,
            Vec::new(),
            extract_user_message_files(payload),
            TokenUsage::default(),
            Vec::new(),
            normalize_timestamp(value.get("timestamp").and_then(Value::as_str)),
            raw_line,
        )),
        Some("agent_message") => Ok(build_message(
            optional_session_id(value, payload),
            Role::Assistant,
            string_field_from_map(payload, "message").unwrap_or_default(),
            None,
            Vec::new(),
            Vec::new(),
            TokenUsage::default(),
            Vec::new(),
            normalize_timestamp(value.get("timestamp").and_then(Value::as_str)),
            raw_line,
        )),
        Some("agent_reasoning") => {
            let thinking = string_field_from_map(payload, "text").unwrap_or_default();
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
        Some("token_count") => {
            let usage = extract_token_usage(payload);
            if usage == TokenUsage::default() {
                match classify_empty_token_count(payload) {
                    EmptyTokenCount::NoInfo => {
                        tracing::trace!("skipping token_count event with no token info");
                    }
                    EmptyTokenCount::InfoPresentButEmpty => {
                        tracing::warn!(
                            payload = ?payload,
                            "token_count event has info but yielded no token usage \
                            - Codex token schema may have changed"
                        );
                    }
                }
                return Ok(ParsedEntry::Skip);
            }

            Ok(build_message(
                optional_session_id(value, payload),
                Role::Assistant,
                String::new(),
                None,
                Vec::new(),
                Vec::new(),
                usage,
                Vec::new(),
                normalize_timestamp(value.get("timestamp").and_then(Value::as_str)),
                raw_line,
            ))
        }
        _ => Ok(ParsedEntry::Skip),
    }
}

fn extract_user_message_files(payload: &Map<String, Value>) -> Vec<String> {
    let mut files = BTreeSet::new();

    if let Some(items) = payload.get("local_images").and_then(Value::as_array) {
        for item in items {
            if let Some(path) = item.as_str() {
                maybe_push_file(&mut files, path);
            }
        }
    }

    if let Some(items) = payload.get("images").and_then(Value::as_array) {
        for item in items {
            if let Some(path) = item.as_str() {
                maybe_push_file(&mut files, path);
            }
        }
    }

    files.into_iter().collect()
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum EmptyTokenCount {
    /// `info` is absent or null - expected session-start/marker events.
    NoInfo,
    /// `info` is present but produced no usage - likely a Codex schema change.
    InfoPresentButEmpty,
}

pub(super) fn classify_empty_token_count(payload: &Map<String, Value>) -> EmptyTokenCount {
    match payload.get("info") {
        None | Some(Value::Null) => EmptyTokenCount::NoInfo,
        Some(_) => EmptyTokenCount::InfoPresentButEmpty,
    }
}

fn extract_token_usage(payload: &Map<String, Value>) -> TokenUsage {
    let last = payload
        .get("info")
        .and_then(Value::as_object)
        .and_then(|info| info.get("last_token_usage"))
        .and_then(Value::as_object);
    let total = payload
        .get("info")
        .and_then(Value::as_object)
        .and_then(|info| info.get("total_token_usage"))
        .and_then(Value::as_object);

    let input_tokens = token_usage_field(last, total, "input_tokens");
    let output_tokens = token_usage_field(last, total, "output_tokens");
    let cache_read_input_tokens = token_usage_field(last, total, "cached_input_tokens");
    let reasoning_output_tokens = token_usage_field(last, total, "reasoning_output_tokens");

    TokenUsage {
        input_tokens,
        output_tokens,
        cache_read_input_tokens,
        reasoning_output_tokens,
        total_tokens: token_usage_field(last, total, "total_tokens")
            .or_else(|| total_from_parts([input_tokens, output_tokens])),
    }
}

fn token_usage_field(
    last: Option<&Map<String, Value>>,
    total: Option<&Map<String, Value>>,
    key: &str,
) -> Option<u64> {
    last.and_then(|usage| usage.get(key))
        .and_then(Value::as_u64)
        .or_else(|| {
            total
                .and_then(|usage| usage.get(key))
                .and_then(Value::as_u64)
        })
}
