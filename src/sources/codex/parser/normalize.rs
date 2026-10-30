use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

const DEDUPE_HEX_LEN: usize = 24;
pub(crate) const MESSAGE_ID_VERSION: &str = "codex-raw-line/v1";
pub(crate) const REPLAY_DEDUPE_VERSION: &str = "codex-replay/v1";

pub(crate) fn compute_message_id(session_id: &str, raw_line: &[u8]) -> String {
    match MESSAGE_ID_VERSION {
        "codex-raw-line/v1" => truncated_hash(session_id, raw_line),
        _ => unreachable!("unsupported Codex message-id version"),
    }
}

#[cfg(test)]
pub fn compute_dedupe_key(session_id: &str, raw_line: &[u8]) -> String {
    compute_dedupe_key_with_turn(session_id, raw_line, None)
}

pub(crate) fn compute_dedupe_key_with_turn(
    session_id: &str,
    raw_line: &[u8],
    turn_id: Option<&str>,
) -> String {
    let stable_record = stable_record_bytes(raw_line, turn_id);
    let stable = stable_record.as_deref().unwrap_or(raw_line);
    truncated_hash(session_id, stable)
}

fn truncated_hash(session_id: &str, bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(session_id.as_bytes());
    hasher.update(bytes);
    let digest = hasher.finalize();
    format!("{digest:x}")[..DEDUPE_HEX_LEN].to_string()
}

fn stable_record_bytes(raw_line: &[u8], turn_id: Option<&str>) -> Option<Vec<u8>> {
    let value = serde_json::from_slice::<Value>(raw_line).ok()?;
    let payload = value.get("payload")?;

    match value.get("type").and_then(Value::as_str) {
        Some("response_item") => {
            if let Some(identity) = response_message_identity(payload, turn_id) {
                return Some(identity);
            }

            response_item_identity(payload, turn_id)
        }
        Some("event_msg")
            if payload.get("type").and_then(Value::as_str) == Some("agent_message") =>
        {
            let turn_id = turn_id?;
            let content = payload.get("message").and_then(Value::as_str)?;
            canonical_message_identity("assistant", turn_id, content)
        }
        _ => None,
    }
}

fn response_item_identity(payload: &Value, turn_id: Option<&str>) -> Option<Vec<u8>> {
    let kind = payload.get("type").and_then(Value::as_str)?;
    let intrinsic_id = ["call_id", "id", "encrypted_content"]
        .into_iter()
        .find_map(|field| {
            payload
                .get(field)
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
        });

    if let Some(intrinsic_id) = intrinsic_id {
        return serde_json::to_vec(&("response_item", kind, intrinsic_id, payload)).ok();
    }
    let turn_id = turn_id?;
    serde_json::to_vec(&("response_item", kind, turn_id, payload)).ok()
}

fn response_message_identity(payload: &Value, fallback_turn_id: Option<&str>) -> Option<Vec<u8>> {
    if payload.get("type").and_then(Value::as_str) != Some("message") {
        return None;
    }
    let role = payload.get("role").and_then(Value::as_str)?;
    if !matches!(role, "user" | "assistant") {
        return None;
    }
    let turn_id = payload
        .get("internal_chat_message_metadata_passthrough")
        .and_then(|metadata| metadata.get("turn_id"))
        .and_then(Value::as_str)
        .or(fallback_turn_id)?;
    let content = payload.get("content")?;
    if role == "user" {
        return serde_json::to_vec(&("message", role, turn_id, content)).ok();
    }
    let text = content
        .as_array()?
        .iter()
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");

    canonical_message_identity(role, turn_id, &text)
}

fn canonical_message_identity(role: &str, turn_id: &str, content: &str) -> Option<Vec<u8>> {
    serde_json::to_vec(&("message", role, turn_id, content)).ok()
}

pub(super) fn optional_session_id(value: &Value, payload: &Map<String, Value>) -> Option<String> {
    value
        .get("session_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| string_field_from_map(payload, "session_id"))
        .or_else(|| string_field_from_map(payload, "sessionId"))
}

pub(super) fn string_field_from_map(map: &Map<String, Value>, key: &str) -> Option<String> {
    map.get(key).and_then(Value::as_str).map(ToOwned::to_owned)
}

pub(super) fn string_or_json(value: &Value) -> Option<String> {
    match value {
        Value::String(text) if !text.is_empty() => Some(text.clone()),
        Value::Null => None,
        other => serde_json::to_string(other).ok(),
    }
}

pub(super) fn normalize_timestamp(raw: Option<&str>) -> Option<String> {
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

pub(super) fn value_to_plain_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

pub(super) fn total_from_parts(parts: impl IntoIterator<Item = Option<u64>>) -> Option<u64> {
    let total = parts.into_iter().flatten().sum::<u64>();
    (total > 0).then_some(total)
}
