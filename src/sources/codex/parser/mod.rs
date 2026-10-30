mod event;
mod message;
mod metadata;
mod normalize;
mod tools;
mod types;

#[cfg(test)]
mod tests;

use anyhow::Result;
use serde_json::Value;

#[cfg(test)]
pub(crate) use normalize::{compute_dedupe_key, MESSAGE_ID_VERSION};
pub(super) use normalize::{
    compute_dedupe_key_with_turn, compute_message_id, REPLAY_DEDUPE_VERSION,
};
pub use types::*;

pub fn parse_line(line: &str) -> Result<ParsedEntry> {
    let value: Value = serde_json::from_str(line)?;
    parse_value(&value, line)
}

pub fn parse_value(value: &Value, raw_line: &str) -> Result<ParsedEntry> {
    match value.get("type").and_then(Value::as_str) {
        Some("session_meta") => metadata::parse_session_meta(value),
        Some("turn_context") => metadata::parse_turn_context(value),
        Some("event_msg") => event::parse_event_message(value, raw_line),
        Some("response_item") => message::parse_response_item(value, raw_line),
        Some("compacted") => Ok(ParsedEntry::Skip),
        _ => Ok(ParsedEntry::Skip),
    }
}
