use super::super::*;
use serde_json::json;

#[test]
fn function_call_and_custom_tool_call_extract_actions() {
    let function_line = json!({
        "timestamp": "2026-03-23T18:22:04.746Z",
        "type": "response_item",
        "payload": {
            "type": "function_call",
            "name": "exec_command",
            "arguments": "{\"command\":\"sed -n '1,20p' Cargo.toml\",\"workdir\":\"/home/user/src/example\"}",
            "call_id": "call-1"
        }
    })
    .to_string();

    let ParsedEntry::Message(function_message) = parse_line(&function_line).unwrap() else {
        panic!("expected function call message");
    };
    assert_eq!(function_message.tool_uses, vec!["exec_command".to_string()]);
    assert_eq!(function_message.actions.len(), 1);
    assert_eq!(
        function_message.actions[0],
        ToolAction {
            call_id: Some("call-1".to_string()),
            action_type: "exec_command".to_string(),
            target: "sed -n '1,20p' Cargo.toml".to_string(),
            command:
                "{\"command\":\"sed -n '1,20p' Cargo.toml\",\"workdir\":\"/home/user/src/example\"}"
                    .to_string(),
        }
    );

    let custom_line = json!({
        "timestamp": "2026-03-10T18:27:32.693Z",
        "type": "response_item",
        "payload": {
            "type": "custom_tool_call",
            "call_id": "call-2",
            "name": "apply_patch",
            "input": "*** Begin Patch\n*** Update File: /home/user/src/example/src/main.rs\n@@\n-foo\n+bar\n*** End Patch\n"
        }
    })
    .to_string();

    let ParsedEntry::Message(custom_message) = parse_line(&custom_line).unwrap() else {
        panic!("expected custom tool call message");
    };
    assert_eq!(custom_message.tool_uses, vec!["apply_patch".to_string()]);
    assert_eq!(
        custom_message.files_referenced,
        vec!["/home/user/src/example/src/main.rs".to_string()]
    );
    assert_eq!(
        custom_message.actions[0].target,
        "/home/user/src/example/src/main.rs"
    );
    assert!(custom_message.actions[0]
        .command
        .contains("*** Update File"));
}

#[test]
fn nested_web_tool_targets_are_extracted_without_search_query() {
    let line = json!({
        "timestamp": "2026-03-23T18:22:04.746Z",
        "type": "response_item",
        "payload": {
            "type": "function_call",
            "name": "open",
            "arguments": "{\"open\":[{\"ref_id\":\"turn0search0\"}]}",
            "call_id": "call-open-1"
        }
    })
    .to_string();

    let ParsedEntry::Message(message) = parse_line(&line).unwrap() else {
        panic!("expected function call message");
    };

    assert_eq!(message.tool_uses, vec!["open".to_string()]);
    assert_eq!(message.actions.len(), 1);
    assert_eq!(message.actions[0].target, "turn0search0");
}

#[test]
fn developer_response_items_are_skipped() {
    let line = json!({
        "timestamp": "2026-03-23T18:21:57.196Z",
        "type": "response_item",
        "payload": {
            "type": "message",
            "role": "developer",
            "content": [
                { "type": "input_text", "text": "developer prompt" }
            ]
        }
    })
    .to_string();

    assert_eq!(parse_line(&line).unwrap(), ParsedEntry::Skip);
}

#[test]
fn dedupe_key_is_deterministic_for_same_session_and_raw_line() {
    let line = "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"done\"}]}}";
    let a = compute_dedupe_key("session-1", line.as_bytes());
    let b = compute_dedupe_key("session-1", line.as_bytes());
    let c = compute_dedupe_key("session-2", line.as_bytes());
    let d = compute_dedupe_key(
        "session-1",
        "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"other\"}]}}".as_bytes(),
    );

    assert_eq!(a, b);
    assert_ne!(a, c);
    assert_ne!(a, d);
}

#[test]
fn response_item_dedupe_key_ignores_replay_timestamp() {
    let payload = json!({
        "type": "message",
        "role": "user",
        "content": [{ "type": "input_text", "text": "same historical prompt" }],
        "internal_chat_message_metadata_passthrough": {
            "turn_id": "turn-original"
        }
    });
    let first = json!({
        "timestamp": "2026-07-31T21:18:54.976Z",
        "type": "response_item",
        "payload": payload
    })
    .to_string();
    let replay = json!({
        "timestamp": "2026-08-04T23:21:05.810Z",
        "type": "response_item",
        "payload": payload
    })
    .to_string();

    assert_eq!(
        compute_dedupe_key("session-1", first.as_bytes()),
        compute_dedupe_key("session-1", replay.as_bytes())
    );
}

#[test]
fn response_item_dedupe_key_keeps_distinct_turns() {
    let line = |turn_id: &str| {
        json!({
            "timestamp": "2026-08-04T23:21:05.810Z",
            "type": "response_item",
            "payload": {
                "type": "message",
                "role": "user",
                "content": [{ "type": "input_text", "text": "repeatable prompt" }],
                "internal_chat_message_metadata_passthrough": {
                    "turn_id": turn_id
                }
            }
        })
        .to_string()
    };

    assert_ne!(
        compute_dedupe_key("session-1", line("turn-a").as_bytes()),
        compute_dedupe_key("session-1", line("turn-b").as_bytes())
    );
}

#[test]
fn response_item_without_embedded_metadata_uses_observed_turn() {
    let line = |timestamp: &str| {
        json!({
            "timestamp": timestamp,
            "type": "response_item",
            "payload": {
                "type": "message",
                "role": "assistant",
                "content": [{ "type": "output_text", "text": "stable reply" }]
            }
        })
        .to_string()
    };

    assert_eq!(
        compute_dedupe_key_with_turn(
            "session-1",
            line("2026-08-01T00:00:00Z").as_bytes(),
            Some("turn-original")
        ),
        compute_dedupe_key_with_turn(
            "session-1",
            line("2026-08-02T00:00:00Z").as_bytes(),
            Some("turn-original")
        )
    );
}

#[test]
fn records_without_stable_identity_keep_raw_line_identity() {
    let line = |timestamp: &str, kind: &str| {
        json!({
            "timestamp": timestamp,
            "type": "response_item",
            "payload": if kind == "message" {
                json!({
                    "type": "message",
                    "role": "user",
                    "content": [{ "type": "input_text", "text": "continue" }]
                })
            } else {
                json!({ "type": kind, "name": "shell", "arguments": "{}" })
            }
        })
        .to_string()
    };

    for kind in ["message", "reasoning", "function_call", "web_search_call"] {
        assert_ne!(
            compute_dedupe_key("session-1", line("2026-08-01T00:00:00Z", kind).as_bytes()),
            compute_dedupe_key("session-1", line("2026-08-02T00:00:00Z", kind).as_bytes())
        );
    }
}

#[test]
fn durable_message_id_remains_raw_line_v1() {
    let first = br#"{"timestamp":"one","type":"response_item","payload":{}}"#;
    let second = br#"{"timestamp":"two","type":"response_item","payload":{}}"#;
    assert_eq!(MESSAGE_ID_VERSION, "codex-raw-line/v1");
    assert_ne!(
        compute_message_id("session-1", first),
        compute_message_id("session-1", second)
    );
}

#[test]
fn parse_message_without_session_id_defers_dedupe_key() {
    let line = json!({
        "timestamp": "2026-03-23T18:22:12.963Z",
        "type": "response_item",
        "payload": {
            "type": "message",
            "role": "assistant",
            "content": [
                { "type": "output_text", "text": "done" }
            ]
        }
    })
    .to_string();

    let ParsedEntry::Message(message) = parse_line(&line).unwrap() else {
        panic!("expected message");
    };

    assert_eq!(message.session_id, None);
    assert_eq!(message.content, "done");
}
