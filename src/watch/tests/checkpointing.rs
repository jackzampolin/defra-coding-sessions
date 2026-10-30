use super::common::*;

#[tokio::test]
async fn scan_once_leaves_incomplete_trailing_line_uncheckpointed_until_completed() {
    let tempdir = TempDir::new().unwrap();
    let claude_dir = tempdir.path().join(".claude");
    let project_dir = claude_dir.join("projects").join("demo-project");
    std::fs::create_dir_all(&project_dir).unwrap();

    let file = project_dir.join("session-a.jsonl");
    let first_line = serde_json::json!({
        "sessionId": "session-1",
        "cwd": "/tmp/repo",
        "gitBranch": "main",
        "version": "1.0.0",
        "timestamp": "2026-03-23T18:21:26Z",
        "message": {
            "role": "user",
            "model": "claude-sonnet",
            "content": "hello"
        }
    })
    .to_string();
    let second_line = serde_json::json!({
        "sessionId": "session-1",
        "cwd": "/tmp/repo",
        "gitBranch": "main",
        "version": "1.0.0",
        "timestamp": "2026-03-23T18:21:27Z",
        "message": {
            "role": "assistant",
            "model": "claude-sonnet",
            "content": "world"
        }
    })
    .to_string();
    std::fs::write(&file, format!("{first_line}\n{second_line}")).unwrap();

    let state_path = tempdir.path().join("state.json");
    let state = State::load(state_path.clone()).unwrap();
    let client = GraphqlClient::new("http://127.0.0.1:1/").unwrap();
    let shared = SharedContext::new(client, true, 50, Source::Claude.ingest_label());
    let source = Source::Claude.build_parser(claude_dir.clone());
    let mut watcher = Watcher::new(source, state, shared, 5);

    let first = watcher.scan_once().await.unwrap();
    let persisted = State::load(state_path.clone()).unwrap();

    assert_eq!(first.messages_processed, 1);
    assert_eq!(persisted.get_offset(&file), (first_line.len() + 1) as u64);

    std::fs::OpenOptions::new()
        .append(true)
        .open(&file)
        .unwrap()
        .write_all(b"\n")
        .unwrap();

    let second = watcher.scan_once().await.unwrap();
    let persisted = State::load(state_path).unwrap();

    assert_eq!(second.messages_processed, 1);
    assert_eq!(
        persisted.get_offset(&file),
        std::fs::metadata(&file).unwrap().len()
    );
}

#[tokio::test]
async fn codex_scan_once_tracks_appended_lines_without_session_ids() {
    let tempdir = TempDir::new().unwrap();
    let codex_dir = tempdir.path().join(".codex");
    let sessions_dir = codex_dir.join("sessions/2026/03/23");
    std::fs::create_dir_all(&sessions_dir).unwrap();

    let file = sessions_dir.join("rollout.jsonl");
    std::fs::write(
        &file,
        [
            serde_json::json!({
                "timestamp": "2026-03-23T18:21:26Z",
                "type": "session_meta",
                "payload": {
                    "id": "session-watch",
                    "cwd": "/home/user/src/example",
                    "cli_version": "0.116.0"
                }
            })
            .to_string(),
            serde_json::json!({
                "timestamp": "2026-03-23T18:21:27Z",
                "type": "response_item",
                "payload": {
                    "type": "message",
                    "role": "assistant",
                    "content": [{ "type": "output_text", "text": "first" }]
                }
            })
            .to_string(),
        ]
        .join("\n")
            + "\n",
    )
    .unwrap();

    let state = State::load(tempdir.path().join("state.json")).unwrap();
    let client = GraphqlClient::new("http://127.0.0.1:1/").unwrap();
    let shared = SharedContext::new(client, true, 50, Source::Codex.ingest_label());
    let source = Source::Codex.build_parser(codex_dir.clone());
    let mut watcher = Watcher::new(source, state, shared, 1);

    let first = watcher.scan_once().await.unwrap();
    assert_eq!(first.messages_processed, 1);
    assert_eq!(first.lines_read, 2);

    std::fs::OpenOptions::new()
        .append(true)
        .open(&file)
        .unwrap()
        .write_all(
            format!(
                "{}\n",
                serde_json::json!({
                    "timestamp": "2026-03-23T18:22:00Z",
                    "type": "response_item",
                    "payload": {
                        "type": "message",
                        "role": "assistant",
                        "content": [{ "type": "output_text", "text": "second" }]
                    }
                })
            )
            .as_bytes(),
        )
        .unwrap();

    let second = watcher.scan_once().await.unwrap();
    assert_eq!(second.messages_processed, 1);
    assert_eq!(second.lines_read, 1);

    let session = watcher.shared.session_cache.get("session-watch").unwrap();
    assert_eq!(session.message_count, 2);
}
