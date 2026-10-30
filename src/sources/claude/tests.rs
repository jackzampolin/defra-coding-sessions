use super::*;
use crate::parser::{ParsedEntry, Role};
use serde_json::json;

#[test]
fn parse_assistant_message_extracts_actions() {
    let entry = json!({
        "uuid": "msg-uuid-1234",
        "sessionId": "session-1234",
        "cwd": "/home/user/src/github.com/octo-org/example-repo",
        "gitBranch": "main",
        "timestamp": "2026-03-13T12:00:00Z",
        "message": {
            "role": "assistant",
            "model": "claude-3.7-sonnet",
            "usage": {
                "input_tokens": 5,
                "output_tokens": 9,
                "cache_creation_input_tokens": 2,
                "cache_read_input_tokens": 3
            },
            "content": [
                { "type": "text", "text": "done" },
                { "type": "tool_use", "name": "Read", "input": { "file_path": "src/lib.rs" } },
                { "type": "thinking", "thinking": "considering" }
            ]
        }
    });

    let parsed = parse::parse_value(&entry).unwrap();
    let ParsedEntry::Message(message) = parsed else {
        panic!("expected message");
    };

    assert_eq!(message.role, Role::Assistant);
    assert_eq!(message.message_id.as_deref(), Some("claude:msg-uuid-1234"));
    assert_eq!(message.tool_uses, vec!["Read"]);
    assert_eq!(message.files_referenced, vec!["src/lib.rs"]);
    assert_eq!(message.actions.len(), 1);
    assert_eq!(message.actions[0].target, "src/lib.rs");
    assert_eq!(message.content, "done");
    assert_eq!(message.thinking.as_deref(), Some("considering"));
    assert_eq!(message.usage.cache_creation_input_tokens, Some(2));
    assert_eq!(message.usage.cache_read_input_tokens, Some(3));
    assert_eq!(message.usage.total_tokens, Some(19));
}

#[test]
fn non_existent_claude_dir_returns_empty() {
    let root = std::env::temp_dir().join("coding-data-missing-claude-dir");
    let source = ClaudeSource::new(root.clone());
    let files = source.discover_files(&root).unwrap();
    assert!(files.is_empty());
}
