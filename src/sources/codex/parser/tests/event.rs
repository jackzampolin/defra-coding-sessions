use super::super::event::{classify_empty_token_count, EmptyTokenCount};
use super::super::*;
use serde_json::json;

#[test]
fn classifies_empty_token_count_events() {
    // Session-start markers carry no `info` - expected, not a problem.
    let null_info = json!({ "type": "token_count", "info": null });
    assert_eq!(
        classify_empty_token_count(null_info.as_object().unwrap()),
        EmptyTokenCount::NoInfo
    );
    let missing_info = json!({ "type": "token_count" });
    assert_eq!(
        classify_empty_token_count(missing_info.as_object().unwrap()),
        EmptyTokenCount::NoInfo
    );
    // `info` present but yielding no usage means Codex changed the schema.
    let drifted = json!({ "type": "token_count", "info": { "unexpected": 1 } });
    assert_eq!(
        classify_empty_token_count(drifted.as_object().unwrap()),
        EmptyTokenCount::InfoPresentButEmpty
    );
}

#[test]
fn parse_old_and_new_reasoning_formats() {
    let old_line = json!({
        "timestamp": "2026-02-03T15:49:27.622Z",
        "type": "event_msg",
        "payload": {
            "type": "agent_reasoning",
            "text": "Preparing to inspect the repo"
        }
    })
    .to_string();

    let ParsedEntry::Message(old_message) = parse_line(&old_line).unwrap() else {
        panic!("expected old reasoning message");
    };
    assert_eq!(old_message.role, Role::Assistant);
    assert_eq!(old_message.content, "");
    assert_eq!(
        old_message.thinking.as_deref(),
        Some("Preparing to inspect the repo")
    );

    let new_line = json!({
        "timestamp": "2025-09-15T21:04:13.821Z",
        "type": "response_item",
        "payload": {
            "type": "reasoning",
            "summary": [
                { "type": "summary_text", "text": "Opening CLAUDE.md" }
            ],
            "content": null
        }
    })
    .to_string();

    let ParsedEntry::Message(new_message) = parse_line(&new_line).unwrap() else {
        panic!("expected new reasoning message");
    };
    assert_eq!(new_message.role, Role::Assistant);
    assert_eq!(new_message.content, "");
    assert_eq!(new_message.thinking.as_deref(), Some("Opening CLAUDE.md"));
}

#[test]
fn token_count_uses_last_token_usage() {
    let line = json!({
        "timestamp": "2025-09-15T21:04:15.745Z",
        "type": "event_msg",
        "payload": {
            "type": "token_count",
            "info": {
                "total_token_usage": {
                    "input_tokens": 5233,
                    "cached_input_tokens": 4864,
                    "output_tokens": 104,
                    "reasoning_output_tokens": 0,
                    "total_tokens": 5337
                },
                "last_token_usage": {
                    "input_tokens": 2694,
                    "cached_input_tokens": 2432,
                    "output_tokens": 54,
                    "reasoning_output_tokens": 0,
                    "total_tokens": 2748
                }
            }
        }
    })
    .to_string();

    let ParsedEntry::Message(message) = parse_line(&line).unwrap() else {
        panic!("expected token_count message");
    };
    assert_eq!(
        message.usage,
        TokenUsage {
            input_tokens: Some(2694),
            output_tokens: Some(54),
            cache_read_input_tokens: Some(2432),
            reasoning_output_tokens: Some(0),
            total_tokens: Some(2748),
        }
    );
}
