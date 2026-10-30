pub(super) use crate::client::GraphqlClient;
pub(super) use crate::mutations::deterministic_message_id;
pub(super) use crate::parser::{ParsedMessage, Role, TokenUsage, ToolAction};
pub(super) use crate::shared::{
    BatchResult, CachedProject, SessionFlushResult, SessionMeta, SharedContext,
};
pub(super) use crate::source::Source;
pub(super) use crate::test_support::{spawn_sequence_server, spawn_server};
pub(super) use serde_json::json;
pub(super) use std::time::SystemTime;

pub(super) const TEST_CWD: &str = "/tmp/repo";
pub(super) const TEST_SESSION_ID: &str = "session-1";
pub(super) const TEST_SESSION_DOC_ID: &str = "session-doc-1";
pub(super) const TEST_TIMESTAMP: &str = "2026-03-23T18:21:26Z";

pub(super) fn context_with_session(
    client: GraphqlClient,
    dry_run: bool,
    batch_size: usize,
    msg_seq: u64,
) -> SharedContext {
    let mut context =
        SharedContext::new(client, dry_run, batch_size, Source::Claude.ingest_label());
    insert_session(&mut context, msg_seq);
    context
}

pub(super) fn insert_session(context: &mut SharedContext, msg_seq: u64) {
    context.session_cache.insert(
        TEST_SESSION_ID.to_string(),
        SessionMeta {
            doc_id: TEST_SESSION_DOC_ID.to_string(),
            msg_seq,
            ..Default::default()
        },
    );
}

pub(super) fn cache_test_project(context: &mut SharedContext) {
    context.project_cache.insert(
        TEST_CWD.to_string(),
        CachedProject {
            doc_id: "project-doc-1".to_string(),
            last_touched: SystemTime::now(),
        },
    );
}

pub(super) fn parsed_message(role: Role, content: &str) -> ParsedMessage {
    ParsedMessage {
        session_id: Some(TEST_SESSION_ID.to_string()),
        message_id: None,
        transient_dedupe_id: None,
        role,
        cwd: Some(TEST_CWD.to_string()),
        git_branch: Some("main".to_string()),
        model: Some("claude-sonnet".to_string()),
        claude_version: Some("1.0.0".to_string()),
        title: None,
        archived: None,
        git_sha: None,
        git_origin_url: None,
        agent_role: None,
        reasoning_effort: None,
        content: content.to_string(),
        thinking: None,
        tool_uses: Vec::new(),
        files_referenced: Vec::new(),
        usage: TokenUsage {
            input_tokens: Some(3),
            output_tokens: Some(5),
            ..Default::default()
        },
        timestamp: Some(TEST_TIMESTAMP.to_string()),
        actions: Vec::new(),
    }
}

pub(super) fn assistant_message(content: &str) -> ParsedMessage {
    let mut message = parsed_message(Role::Assistant, content);
    message.tool_uses = vec!["Read".to_string()];
    message
}

pub(super) fn read_action_message() -> ParsedMessage {
    let mut message = assistant_message("one");
    message.files_referenced = vec![format!("{TEST_CWD}/file.rs")];
    message.actions = vec![ToolAction {
        call_id: Some("call-1".to_string()),
        action_type: "Read".to_string(),
        target: format!("{TEST_CWD}/file.rs"),
        command: format!("sed -n '1,10p' {TEST_CWD}/file.rs"),
    }];
    message
}

pub(super) fn queue_message(context: &mut SharedContext, message: &ParsedMessage) {
    let sequence = context.reserve_message_sequence(TEST_SESSION_ID).unwrap();
    context
        .queue_message(
            TEST_SESSION_ID,
            message,
            &deterministic_message_id(TEST_SESSION_ID, sequence),
            sequence,
        )
        .unwrap();
}
