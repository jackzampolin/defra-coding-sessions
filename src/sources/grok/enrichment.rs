use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};

#[derive(Debug, Clone, Default)]
pub(super) struct MessageEnrichment {
    pub(super) prompt_id: Option<String>,
    pub(super) timestamp: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct TurnEnrichment {
    prompt_id: Option<String>,
    user_content: String,
    user_timestamp: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct GrokEnrichmentIndex {
    turns: Vec<TurnEnrichment>,
    prompt_final: HashMap<String, MessageEnrichment>,
    tool_calls: HashMap<String, MessageEnrichment>,
}

impl GrokEnrichmentIndex {
    pub(super) fn parse(contents: &str) -> Self {
        let mut index = Self::default();
        let mut pending_turns = VecDeque::new();
        let mut assigned_prompts = HashMap::<String, usize>::new();

        for line in contents
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
        {
            let Ok(value) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            let params = value.get("params").cloned().unwrap_or(Value::Null);
            let update = params.get("update").cloned().unwrap_or(Value::Null);
            let kind = update
                .get("sessionUpdate")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let meta = params
                .get("_meta")
                .or_else(|| update.get("_meta"))
                .cloned()
                .unwrap_or(Value::Null);
            let timestamp = event_timestamp(&value, &meta);

            if kind == "user_message_chunk" {
                let turn_index = index.turns.len();
                index.turns.push(TurnEnrichment {
                    prompt_id: None,
                    user_content: normalized_update_content(update.get("content")),
                    user_timestamp: timestamp,
                });
                pending_turns.push_back(turn_index);
                continue;
            }

            let Some(prompt_id) = meta
                .get("promptId")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(ToOwned::to_owned)
            else {
                continue;
            };

            if !assigned_prompts.contains_key(&prompt_id) {
                if let Some(turn_index) = pending_turns.pop_front() {
                    index.turns[turn_index].prompt_id = Some(prompt_id.clone());
                    assigned_prompts.insert(prompt_id.clone(), turn_index);
                }
            }

            let prompt_enrichment = MessageEnrichment {
                prompt_id: Some(prompt_id.clone()),
                timestamp: timestamp.clone(),
            };
            index
                .prompt_final
                .entry(prompt_id.clone())
                .and_modify(|current| update_if_newer(current, &prompt_enrichment))
                .or_insert_with(|| prompt_enrichment.clone());

            if matches!(kind, "tool_call" | "tool_call_update") {
                if let Some(tool_call_id) = update
                    .get("toolCallId")
                    .or_else(|| update.get("tool_call_id"))
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                {
                    index
                        .tool_calls
                        .entry(tool_call_id.to_string())
                        .and_modify(|current| update_if_newer(current, &prompt_enrichment))
                        .or_insert_with(|| prompt_enrichment.clone());
                }
            }
        }

        index
    }

    pub(super) fn match_user_lines(
        &self,
        chat_users: &[(u64, String)],
    ) -> HashMap<u64, MessageEnrichment> {
        let mut matches = HashMap::new();
        let mut before = self.turns.len();
        for (line_index, content) in chat_users.iter().rev() {
            let normalized = normalize_chat_user_content(content);
            let Some(turn_index) = (0..before)
                .rev()
                .find(|index| self.turns[*index].user_content == normalized)
            else {
                continue;
            };
            let turn = &self.turns[turn_index];
            matches.insert(
                *line_index,
                MessageEnrichment {
                    prompt_id: turn.prompt_id.clone(),
                    timestamp: turn.user_timestamp.clone(),
                },
            );
            before = turn_index;
        }
        matches
    }

    pub(super) fn for_tool_calls<'a>(
        &self,
        tool_call_ids: impl IntoIterator<Item = &'a str>,
    ) -> Option<MessageEnrichment> {
        tool_call_ids
            .into_iter()
            .find_map(|tool_call_id| self.tool_calls.get(tool_call_id).cloned())
    }

    pub(super) fn for_prompt(&self, prompt_id: Option<&str>) -> Option<MessageEnrichment> {
        prompt_id.and_then(|prompt_id| self.prompt_final.get(prompt_id).cloned())
    }
}

pub(super) fn epoch_ms_to_rfc3339(ms: u64) -> String {
    DateTime::<Utc>::from_timestamp_millis(ms as i64)
        .map(|dt| dt.to_rfc3339_opts(SecondsFormat::Secs, true))
        .unwrap_or_else(|| ms.to_string())
}

fn event_timestamp(value: &Value, meta: &Value) -> Option<String> {
    meta.get("agentTimestampMs")
        .and_then(Value::as_u64)
        .map(epoch_ms_to_rfc3339)
        .or_else(|| {
            value
                .get("timestamp")
                .and_then(Value::as_u64)
                .map(|secs| epoch_ms_to_rfc3339(secs.saturating_mul(1000)))
        })
}

fn update_if_newer(current: &mut MessageEnrichment, candidate: &MessageEnrichment) {
    if candidate.timestamp > current.timestamp {
        *current = candidate.clone();
    }
}

fn normalized_update_content(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(text)) => text.trim().to_string(),
        Some(Value::Object(map)) => map
            .get("text")
            .or_else(|| map.get("content"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| match part {
                Value::String(text) => Some(text.as_str()),
                Value::Object(map) => map.get("text").and_then(Value::as_str),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string(),
        _ => String::new(),
    }
}

fn normalize_chat_user_content(content: &str) -> String {
    let trimmed = content.trim();
    trimmed
        .strip_prefix("<user_query>")
        .and_then(|value| value.strip_suffix("</user_query>"))
        .unwrap_or(trimmed)
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compacted_chat_users_match_latest_update_turns_by_content() {
        let updates = r#"
{"timestamp":100,"params":{"update":{"sessionUpdate":"user_message_chunk","content":{"type":"text","text":"old"}},"_meta":{"agentTimestampMs":100000}}}
{"timestamp":101,"params":{"update":{"sessionUpdate":"agent_message_chunk"},"_meta":{"promptId":"prompt-old","agentTimestampMs":101000}}}
{"timestamp":200,"params":{"update":{"sessionUpdate":"user_message_chunk","content":{"type":"text","text":"retained"}},"_meta":{"agentTimestampMs":200000}}}
{"timestamp":201,"params":{"update":{"sessionUpdate":"agent_message_chunk"},"_meta":{"promptId":"prompt-retained","agentTimestampMs":201000}}}
"#;
        let index = GrokEnrichmentIndex::parse(updates);
        let matches =
            index.match_user_lines(&[(7, "<user_query>\nretained\n</user_query>".into())]);
        let matched = matches.get(&7).unwrap();
        assert_eq!(matched.prompt_id.as_deref(), Some("prompt-retained"));
        assert_eq!(matched.timestamp.as_deref(), Some("1970-01-01T00:03:20Z"));
    }
}
