use super::enrichment::epoch_ms_to_rfc3339;
use super::project::session_id_from_chat_history_path;
use crate::backfill::read_lines_from_offset;
use crate::parser::{ParsedEntry, ParsedUsageEvent, TokenUsage};
use crate::source::ReadEntriesResult;
use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

pub(super) fn read_update_usage_entries(path: &Path, offset: u64) -> Result<ReadEntriesResult> {
    let session_id = session_id_from_chat_history_path(path)
        .with_context(|| format!("unable to derive grok session id from {}", path.display()))?;
    let (lines, new_offset) = read_lines_from_offset(path, offset)?;
    let mut events = BTreeMap::<String, ParsedUsageEvent>::new();
    let mut parse_errors = Vec::new();

    for line in lines {
        let value: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(error) => {
                parse_errors.push(Err(error.into()));
                continue;
            }
        };
        let Some(event) = parse_usage_event(&session_id, &value) else {
            continue;
        };
        events
            .entry(event.usage_id.clone())
            .and_modify(|current| {
                if event.timestamp > current.timestamp {
                    *current = event.clone();
                }
            })
            .or_insert(event);
    }

    parse_errors.extend(
        events
            .into_values()
            .map(|event| Ok(ParsedEntry::Usage(Box::new(event)))),
    );
    Ok(ReadEntriesResult {
        entries: parse_errors,
        new_offset,
    })
}

fn parse_usage_event(session_id: &str, value: &Value) -> Option<ParsedUsageEvent> {
    let params = value.get("params")?;
    let update = params.get("update")?;
    let kind = update.get("sessionUpdate")?.as_str()?;
    if !matches!(kind, "tool_call" | "agent_message_chunk") {
        return None;
    }

    let meta = params.get("_meta").or_else(|| update.get("_meta"))?;
    let prompt_id = meta.get("promptId")?.as_str()?.trim();
    let total_tokens = meta.get("totalTokens")?.as_u64()?;
    let timestamp_ms = meta.get("agentTimestampMs")?.as_u64()?;
    if prompt_id.is_empty() || total_tokens == 0 {
        return None;
    }

    // totalTokens is a context-size observation for one completed model call.
    // Grok repeats it across streaming/update records, so prompt+counter is the
    // stable call identity and prevents those repeats from being summed.
    let usage_id = format!("grok:{session_id}:{prompt_id}:{total_tokens}");
    Some(ParsedUsageEvent {
        usage_id,
        session_id: session_id.to_string(),
        model: None,
        usage: TokenUsage {
            total_tokens: Some(total_tokens),
            ..Default::default()
        },
        timestamp: epoch_ms_to_rfc3339(timestamp_ms),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn repeated_stream_records_collapse_to_one_usage_id() {
        let first: Value = serde_json::from_str(
            r#"{"params":{"update":{"sessionUpdate":"agent_message_chunk"},"_meta":{"promptId":"prompt-1","totalTokens":42000,"agentTimestampMs":1785520689000}}}"#,
        )
        .unwrap();
        let repeated: Value = serde_json::from_str(
            r#"{"params":{"update":{"sessionUpdate":"tool_call"},"_meta":{"promptId":"prompt-1","totalTokens":42000,"agentTimestampMs":1785520690000}}}"#,
        )
        .unwrap();

        let first = parse_usage_event("session-1", &first).unwrap();
        let repeated = parse_usage_event("session-1", &repeated).unwrap();
        assert_eq!(first.usage_id, repeated.usage_id);
        assert_eq!(first.usage.total_tokens, Some(42_000));
    }

    #[test]
    fn ignores_thought_chunks_and_missing_counters() {
        let thought: Value = serde_json::from_str(
            r#"{"params":{"update":{"sessionUpdate":"agent_thought_chunk"},"_meta":{"promptId":"prompt-1","totalTokens":42000,"agentTimestampMs":1785520689000}}}"#,
        )
        .unwrap();
        let missing: Value = serde_json::from_str(
            r#"{"params":{"update":{"sessionUpdate":"tool_call"},"_meta":{"promptId":"prompt-1","agentTimestampMs":1785520689000}}}"#,
        )
        .unwrap();

        assert!(parse_usage_event("session-1", &thought).is_none());
        assert!(parse_usage_event("session-1", &missing).is_none());
    }

    #[test]
    fn journal_replay_is_deduplicated_and_offset_safe() {
        let temp = TempDir::new().unwrap();
        let session_dir = temp.path().join("session-1");
        fs::create_dir_all(&session_dir).unwrap();
        let path = session_dir.join("updates.jsonl");
        let repeated = [
            r#"{"params":{"update":{"sessionUpdate":"agent_message_chunk"},"_meta":{"promptId":"prompt-1","totalTokens":42000,"agentTimestampMs":1785520689000}}}"#,
            r#"{"params":{"update":{"sessionUpdate":"tool_call"},"_meta":{"promptId":"prompt-1","totalTokens":42000,"agentTimestampMs":1785520690000}}}"#,
        ]
        .join("\n");
        fs::write(&path, format!("{repeated}\n")).unwrap();

        let first = read_update_usage_entries(&path, 0).unwrap();
        assert_eq!(first.entries.len(), 1);
        let offset = first.new_offset;
        let replay = read_update_usage_entries(&path, offset).unwrap();
        assert!(replay.entries.is_empty());
        assert_eq!(replay.new_offset, offset);
    }
}
