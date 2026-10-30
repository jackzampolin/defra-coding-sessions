use super::*;
use crate::parser::{ParsedEntry, Role};
use crate::source::SourceParser;
use std::fs;
use tempfile::TempDir;

fn write_fixture_session(root: &std::path::Path) -> std::path::PathBuf {
    let project = root.join("sessions").join("%2Ftmp%2Fexample-project");
    let session = project.join("session-1");
    fs::create_dir_all(session.join("subagents/session-child")).unwrap();
    fs::create_dir_all(project.join("session-child")).unwrap();

    fs::write(
        session.join("summary.json"),
        r#"{
          "info": {"id": "session-1", "cwd": "/tmp/example-project"},
          "session_summary": "Build Grok Adapter",
          "created_at": "2026-07-09T16:24:29.695338Z",
          "updated_at": "2026-07-09T16:30:00Z",
          "current_model_id": "grok-4.5",
          "git_remotes": ["https://github.com/example/repo.git"],
          "head_commit": "abc123",
          "head_branch": "main",
          "generated_title": "Build Grok Adapter",
          "agent_name": "grok-build-plan",
          "reasoning_effort": "high"
        }"#,
    )
    .unwrap();

    fs::write(
        session.join("signals.json"),
        r#"{
          "contextTokensUsed": 12000,
          "toolsUsed": ["read_file", "run_terminal_command"],
          "primaryModelId": "grok-4.5"
        }"#,
    )
    .unwrap();

    // Real Grok places `_meta` under `params` (sibling of `update`), not only nested.
    fs::write(
        session.join("updates.jsonl"),
        r#"{"timestamp":1783614286,"method":"session/update","params":{"sessionId":"session-1","update":{"sessionUpdate":"user_message_chunk","content":{"type":"text","text":"build the adapter"}},"_meta":{"agentTimestampMs":1783614286000,"eventId":"u1"}}}
{"timestamp":1783614287,"method":"session/update","params":{"sessionId":"session-1","update":{"sessionUpdate":"tool_call","toolCallId":"call-1"},"_meta":{"totalTokens":4000,"promptId":"prompt-1","agentTimestampMs":1783614287000,"eventId":"t1"}}}
{"timestamp":1783614288,"method":"session/update","params":{"sessionId":"session-1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Done."}},"_meta":{"totalTokens":5000,"promptId":"prompt-1","agentTimestampMs":1783614288000,"eventId":"e1"}}}
"#,
    )
    .unwrap();

    fs::write(
        session.join("chat_history.jsonl"),
        r#"{"type":"system","content":"You are Grok."}
{"type":"user","content":[{"type":"text","text":"system reminder"}],"synthetic_reason":"system_reminder"}
{"type":"user","content":[{"type":"text","text":"<user_query>\nbuild the adapter\n</user_query>"}]}
{"type":"reasoning","id":"rs_1","summary":[{"type":"summary_text","text":"Need to inspect sources"}],"status":"completed"}
{"type":"assistant","content":"I'll start by exploring.","tool_calls":[{"id":"call-1","name":"read_file","arguments":"{\"target_file\":\"src/source.rs\"}"}],"model_id":"grok-4.5","reasoning_effort":"high"}
{"type":"tool_result","tool_call_id":"call-1","content":"fn main() {}"}
{"type":"assistant","content":"Done.","model_id":"grok-4.5","reasoning_effort":"high"}
{"type":"backend_tool_call","kind":{"tool_type":"web_search","action":{"type":"search","query":"coding-data grok adapter"},"id":"ws_1","status":"completed"}}
"#,
    )
    .unwrap();

    // Child/subagent session with full history + parent pointer.
    fs::write(
        project.join("session-1/subagents/session-child/meta.json"),
        r#"{
          "subagent_id": "session-child",
          "parent_session_id": "session-1",
          "child_session_id": "session-child",
          "subagent_type": "explore",
          "description": "scan sources"
        }"#,
    )
    .unwrap();
    fs::write(
        project.join("session-child/summary.json"),
        r#"{
          "info": {"id": "session-child", "cwd": "/tmp/example-project"},
          "created_at": "2026-07-09T16:25:00Z",
          "current_model_id": "grok-4.5",
          "generated_title": "scan sources"
        }"#,
    )
    .unwrap();
    fs::write(
        project.join("session-child/chat_history.jsonl"),
        r#"{"type":"user","content":"explore the repo"}
{"type":"assistant","content":"looking","model_id":"grok-4.5"}
"#,
    )
    .unwrap();

    fs::write(root.join("version.json"), r#"{"version":"0.2.93"}"#).unwrap();

    session.join("chat_history.jsonl")
}

#[test]
fn discovers_primary_and_subagent_chat_histories() {
    let temp = TempDir::new().unwrap();
    write_fixture_session(temp.path());
    let source = GrokSource::new(temp.path().to_path_buf());
    let files = source.discover_files(temp.path()).unwrap();
    assert_eq!(files.len(), 3);
    assert_eq!(
        files
            .iter()
            .filter(|path| path.file_name().and_then(|name| name.to_str())
                == Some("chat_history.jsonl"))
            .count(),
        2
    );
    assert_eq!(
        files
            .iter()
            .filter(|path| path.file_name().and_then(|name| name.to_str()) == Some("updates.jsonl"))
            .count(),
        1
    );
}

#[test]
fn reads_session_meta_thinking_actions_and_skips_noise() {
    let temp = TempDir::new().unwrap();
    let chat_path = write_fixture_session(temp.path());
    let source = GrokSource::new(temp.path().to_path_buf());
    let result = source.read_entries_from_offset(&chat_path, 0).unwrap();

    let mut session_meta = None;
    let mut users = Vec::new();
    let mut assistants = Vec::new();
    let mut skips = 0usize;

    for entry in result.entries {
        match entry.unwrap() {
            ParsedEntry::SessionMeta(meta) => session_meta = Some(meta),
            ParsedEntry::Message(message) => match message.role {
                Role::User => users.push(message),
                Role::Assistant => assistants.push(message),
            },
            ParsedEntry::Skip => skips += 1,
            ParsedEntry::TurnContext(_) => {}
            ParsedEntry::Usage(_) => {}
        }
    }

    let meta = session_meta.expect("session meta");
    assert_eq!(meta.session_id, "session-1");
    assert_eq!(meta.cwd.as_deref(), Some("/tmp/example-project"));
    assert_eq!(meta.model.as_deref(), Some("grok-4.5"));
    assert_eq!(meta.title.as_deref(), Some("Build Grok Adapter"));
    assert_eq!(meta.git_branch.as_deref(), Some("main"));
    assert_eq!(meta.git_sha.as_deref(), Some("abc123"));
    assert_eq!(
        meta.git_origin_url.as_deref(),
        Some("https://github.com/example/repo.git")
    );
    assert_eq!(meta.agent_role.as_deref(), Some("grok-build-plan"));
    assert_eq!(meta.reasoning_effort.as_deref(), Some("high"));
    assert_eq!(meta.claude_version.as_deref(), Some("0.2.93"));
    assert_eq!(meta.total_tokens, Some(12_000));

    assert_eq!(users.len(), 1);
    assert!(users[0].content.contains("build the adapter"));

    assert_eq!(assistants.len(), 3);
    assert_eq!(
        assistants[0].thinking.as_deref(),
        Some("Need to inspect sources")
    );
    assert_eq!(assistants[0].content, "I'll start by exploring.");
    assert_eq!(assistants[0].tool_uses, vec!["read_file".to_string()]);
    assert_eq!(
        assistants[0].files_referenced,
        vec!["src/source.rs".to_string()]
    );
    assert_eq!(assistants[0].actions.len(), 1);
    assert_eq!(assistants[0].actions[0].action_type, "read_file");
    assert_eq!(assistants[0].actions[0].target, "src/source.rs");
    assert_eq!(assistants[0].usage.total_tokens, None);
    assert!(assistants[0].timestamp.is_some());

    assert_eq!(assistants[1].content, "Done.");
    assert!(assistants[1].thinking.is_none());
    assert_eq!(assistants[1].usage.total_tokens, None);

    assert_eq!(assistants[2].tool_uses, vec!["web_search".to_string()]);
    assert_eq!(assistants[2].actions[0].action_type, "web_search");
    assert_eq!(assistants[2].actions[0].target, "coding-data grok adapter");

    // system + synthetic user + reasoning + tool_result
    assert!(skips >= 4);
}

#[test]
fn subagent_session_gets_agent_role_from_parent_meta() {
    let temp = TempDir::new().unwrap();
    write_fixture_session(temp.path());
    let source = GrokSource::new(temp.path().to_path_buf());
    let child_path = temp
        .path()
        .join("sessions/%2Ftmp%2Fexample-project/session-child/chat_history.jsonl");
    let result = source.read_entries_from_offset(&child_path, 0).unwrap();

    let meta = result
        .entries
        .into_iter()
        .find_map(|entry| match entry.unwrap() {
            ParsedEntry::SessionMeta(meta) => Some(meta),
            ParsedEntry::Usage(_) => None,
            _ => None,
        })
        .expect("child session meta");

    assert_eq!(meta.session_id, "session-child");
    assert_eq!(meta.agent_role.as_deref(), Some("explore"));
    assert_eq!(meta.cwd.as_deref(), Some("/tmp/example-project"));
}

#[test]
fn project_filter_matches_decoded_cwd() {
    let temp = TempDir::new().unwrap();
    let chat_path = write_fixture_session(temp.path());
    let source = GrokSource::new(temp.path().to_path_buf());
    assert!(source.matches_project_filter(&chat_path, temp.path(), "example-project"));
    assert!(!source.matches_project_filter(&chat_path, temp.path(), "other-repo"));
}

#[test]
fn offset_resume_does_not_replay_session_meta() {
    let temp = TempDir::new().unwrap();
    let chat_path = write_fixture_session(temp.path());
    let source = GrokSource::new(temp.path().to_path_buf());
    let first = source.read_entries_from_offset(&chat_path, 0).unwrap();
    let second = source
        .read_entries_from_offset(&chat_path, first.new_offset)
        .unwrap();
    assert!(second.entries.is_empty());
    assert_eq!(second.new_offset, first.new_offset);
}
