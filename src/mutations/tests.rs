use super::*;
use crate::parser::{TokenUsage, ToolAction};
use serde_json::json;

#[test]
fn deterministic_message_id_matches_shape() {
    let message_id = deterministic_message_id("abcd1234-efgh5678", 2);
    assert!(message_id.starts_with("abcd1234-msg-"));
    assert_eq!(message_id.len(), "abcd1234-msg-".len() + 16);
}

#[test]
fn deterministic_action_id_uses_call_id_when_present() {
    let action = ToolAction {
        call_id: Some("call-1".to_string()),
        action_type: "command".to_string(),
        target: "target".to_string(),
        command: "ls".to_string(),
    };

    assert_eq!(deterministic_action_id("session-1", &action).len(), 24);
}

#[test]
fn escape_graphql_escapes_control_characters() {
    let escaped = escape_graphql("quote\" slash\\ line\n");
    assert_eq!(escaped, "quote\\\" slash\\\\ line\\n");
}

#[test]
fn serde_json_array_literals_are_embedded_verbatim() {
    let payload = json!(["Read", "Edit"]).to_string();
    assert_eq!(payload, r#"["Read","Edit"]"#);
}

#[test]
fn token_usage_mutations_are_idempotent_by_usage_id() {
    let input = TokenUsageMutationInput {
        usage_id: "grok:session-1:prompt-1:42000".to_string(),
        session_id: "session-1".to_string(),
        source: "grok".to_string(),
        source_host: "host-a".to_string(),
        model: Some("grok-code-fast-1".to_string()),
        usage: TokenUsage {
            total_tokens: Some(42_000),
            ..Default::default()
        },
        created_at: "2026-07-31T18:00:00Z".to_string(),
    };

    let create = create_token_usage_mutation(&input);
    assert!(create.contains("create_CodingTokenUsage"));
    assert!(create.contains(r#"usage_id: "grok:session-1:prompt-1:42000""#));
    assert!(create.contains("total_tokens: 42000"));

    let update = update_token_usage_mutation(&input);
    assert!(update.contains(r#"usage_id: { _eq: "grok:session-1:prompt-1:42000" }"#));
    assert!(update.contains(r#"created_at: "2026-07-31T18:00:00Z""#));
}
