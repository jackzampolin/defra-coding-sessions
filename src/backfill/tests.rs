use super::*;
use crate::client::GraphqlClient;
use crate::shared::SharedContext;
use crate::source::Source;
use crate::test_support::spawn_sequence_server;
use serde_json::json;
use std::collections::HashMap;
use tempfile::TempDir;

#[tokio::test]
async fn backfill_workers_process_multi_file_session_once() {
    let tempdir = TempDir::new().unwrap();
    let claude_dir = tempdir.path().join(".claude");
    let project_dir = claude_dir.join("projects").join("demo-project");
    std::fs::create_dir_all(&project_dir).unwrap();

    let first_file = project_dir.join("session-a.jsonl");
    let second_file = project_dir.join("session-b.jsonl");
    std::fs::write(
        &first_file,
        format!(
            "{}\n",
            message_line("session-1", "user", "hello", "2026-03-23T18:21:26Z")
        ),
    )
    .unwrap();
    std::fs::write(
        &second_file,
        format!(
            "{}\n{}\n",
            message_line("session-1", "assistant", "hi", "2026-03-23T18:21:27Z"),
            message_line("session-2", "user", "ship it", "2026-03-23T18:21:28Z")
        ),
    )
    .unwrap();

    let state = State::load(tempdir.path().join("state.json")).unwrap();
    let client = GraphqlClient::new("http://127.0.0.1:1/").unwrap();
    let shared = SharedContext::new(client, true, 50, Source::Claude.ingest_label());
    let source = Source::Claude.build_parser(claude_dir.clone());
    let mut backfiller = Backfiller::new(
        source,
        state,
        shared,
        BackfillOptions {
            dry_run: true,
            project_filter: None,
            worker_count: 2,
        },
    );

    let report = backfiller.run().await.unwrap();

    assert_eq!(report.files_total, 2);
    assert_eq!(report.files_processed, 2);
    assert_eq!(report.lines_read, 3);
    assert_eq!(report.messages_processed, 3);
    assert_eq!(report.sessions_finalized, 2);
    assert_eq!(report.errors, 0);
}

#[tokio::test]
async fn backfill_finalizes_checkpointed_sessions_without_new_lines() {
    let tempdir = TempDir::new().unwrap();
    let claude_dir = tempdir.path().join(".claude");
    let project_dir = claude_dir.join("projects").join("demo-project");
    std::fs::create_dir_all(&project_dir).unwrap();

    let file = project_dir.join("session-a.jsonl");
    std::fs::write(
        &file,
        format!(
            "{}\n",
            message_line("session-1", "user", "hello", "2026-03-23T18:21:26Z")
        ),
    )
    .unwrap();
    let file_size = std::fs::metadata(&file).unwrap().len();

    let state_path = tempdir.path().join("state.json");
    let mut state = State::load(state_path.clone()).unwrap();
    state.set_offset(file.clone(), file_size);
    state.replace_session_snapshots(HashMap::from([(
        "session-1".to_string(),
        SessionCheckpoint {
            doc_id: "session-doc-1".to_string(),
            msg_seq: 1,
            model_counts: HashMap::from([("claude-sonnet".to_string(), 1)]),
            last_ts: Some("2026-03-23T18:21:26Z".to_string()),
            last_touched_epoch_ms: timestamp_epoch_ms("2026-03-23T18:21:26Z"),
            message_count: 1,
            user_message_count: 1,
            input_tokens: 3,
            output_tokens: 0,
            tools_used: Vec::new(),
            first_prompt: Some("hello".to_string()),
            ..Default::default()
        },
    )]));
    state.save().unwrap();

    let server = spawn_sequence_server(vec![json!({
        "data": {
            "update_CodingSession": { "_docID": "session-doc-1" }
        }
    })])
    .await;

    let client = GraphqlClient::new(format!("http://{}/", server.address)).unwrap();
    let mut shared = SharedContext::new(client, false, 50, Source::Claude.ingest_label());
    shared.import_session_snapshots(state.session_snapshots().clone());
    let source = Source::Claude.build_parser(claude_dir.clone());
    let mut backfiller = Backfiller::new(
        source,
        state,
        shared,
        BackfillOptions {
            dry_run: false,
            project_filter: None,
            worker_count: 2,
        },
    );

    let report = backfiller.run().await.unwrap();
    let persisted = State::load(state_path).unwrap();

    assert_eq!(report.files_total, 1);
    assert_eq!(report.files_processed, 1);
    assert_eq!(report.lines_read, 0);
    assert_eq!(report.sessions_finalized, 1);
    assert_eq!(report.errors, 0);
    assert!(persisted.sessions.is_empty());
    assert_eq!(persisted.get_offset(&file), file_size);
}

#[tokio::test]
async fn backfill_does_not_checkpoint_offsets_after_worker_failure() {
    let tempdir = TempDir::new().unwrap();
    let claude_dir = tempdir.path().join(".claude");
    let project_dir = claude_dir.join("projects").join("demo-project");
    std::fs::create_dir_all(&project_dir).unwrap();

    let file = project_dir.join("session-a.jsonl");
    std::fs::write(
        &file,
        format!(
            "{}\n",
            message_line("session-1", "user", "hello", "2026-03-23T18:21:26Z")
        ),
    )
    .unwrap();

    let state_path = tempdir.path().join("state.json");
    let state = State::load(state_path.clone()).unwrap();
    let client = GraphqlClient::new("http://127.0.0.1:1/").unwrap();
    let shared = SharedContext::new(client, false, 50, Source::Claude.ingest_label());
    let source = Source::Claude.build_parser(claude_dir.clone());
    let mut backfiller = Backfiller::new(
        source,
        state,
        shared,
        BackfillOptions {
            dry_run: false,
            project_filter: None,
            worker_count: 2,
        },
    );

    let error = backfiller.run().await.unwrap_err();

    assert!(error.to_string().contains("backfill checkpoint aborted"));
    assert!(!state_path.exists());
}

#[test]
fn read_lines_from_offset_only_strips_line_endings() {
    let tempdir = TempDir::new().unwrap();
    let path = tempdir.path().join("fixture.jsonl");
    std::fs::write(&path, "  first  \r\nsecond\t\n\n").unwrap();

    let (lines, offset) = read_lines_from_offset(&path, 0).unwrap();

    assert_eq!(lines, vec!["  first  ".to_string(), "second\t".to_string()]);
    assert_eq!(offset, std::fs::metadata(&path).unwrap().len());
}

#[test]
fn read_lines_from_offset_leaves_incomplete_trailing_line_for_retry() {
    let tempdir = TempDir::new().unwrap();
    let path = tempdir.path().join("fixture.jsonl");
    std::fs::write(&path, "first\nsecond").unwrap();

    let (lines, offset) = read_lines_from_offset(&path, 0).unwrap();

    assert_eq!(lines, vec!["first".to_string()]);
    assert_eq!(offset, "first\n".len() as u64);
}

fn message_line(session_id: &str, role: &str, content: &str, timestamp: &str) -> String {
    serde_json::json!({
        "sessionId": session_id,
        "cwd": "/tmp/repo",
        "gitBranch": "main",
        "version": "1.0.0",
        "timestamp": timestamp,
        "message": {
            "role": role,
            "model": "claude-sonnet",
            "content": content,
            "usage": {
                "input_tokens": 1,
                "output_tokens": 2
            }
        }
    })
    .to_string()
}

fn timestamp_epoch_ms(value: &str) -> u64 {
    chrono::DateTime::parse_from_rfc3339(value)
        .unwrap()
        .timestamp_millis() as u64
}
