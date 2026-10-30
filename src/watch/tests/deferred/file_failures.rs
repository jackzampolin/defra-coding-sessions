use super::super::common::*;

#[tokio::test]
async fn scan_once_keeps_offsets_after_ingest_failure() {
    let tempdir = TempDir::new().unwrap();
    let claude_dir = tempdir.path().join(".claude");
    let project_dir = claude_dir.join("projects").join("demo-project");
    std::fs::create_dir_all(&project_dir).unwrap();

    let file = project_dir.join("session-a.jsonl");
    std::fs::write(
        &file,
        serde_json::json!({
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
        .to_string()
            + "\n",
    )
    .unwrap();

    let state_path = tempdir.path().join("state.json");
    let state = State::load(state_path.clone()).unwrap();
    let client = GraphqlClient::new("http://127.0.0.1:1/").unwrap();
    let shared = SharedContext::new(client, false, 50, Source::Claude.ingest_label());
    let source = Source::Claude.build_parser(claude_dir.clone());
    let mut watcher = Watcher::new(source, state, shared, 5);

    let stats = watcher.scan_once().await.unwrap();
    let persisted = State::load(state_path).unwrap();

    assert_eq!(stats.errors, 1);
    assert_eq!(watcher.state.get_offset(&file), 0);
    assert!(watcher.failed_files.contains_key(&file));
    assert_eq!(persisted.get_offset(&file), 0);
    assert!(persisted
        .watch_checkpoint()
        .deferred_files
        .contains_key(&file));
    assert_eq!(persisted.watch_checkpoint().health, WatchHealth::Degraded);
    assert!(persisted
        .watch_checkpoint()
        .last_successful_scan_epoch_ms
        .is_some());
    assert!(persisted.watch_checkpoint().last_scan_error.is_none());
}

#[tokio::test]
async fn scan_once_defers_bad_file_and_persists_successful_offsets() {
    let tempdir = TempDir::new().unwrap();
    let codex_dir = tempdir.path().join(".codex");
    let sessions_dir = codex_dir.join("sessions/2026/03/23");
    std::fs::create_dir_all(&sessions_dir).unwrap();

    let bad_file = sessions_dir.join("bad-rollout.jsonl");
    std::fs::write(
        &bad_file,
        format!(
            "{}\n",
            serde_json::json!({
                "timestamp": "2026-03-23T18:21:27Z",
                "session_id": "session-bad",
                "type": "response_item",
                "payload": {
                    "type": "message",
                    "role": "assistant",
                    "content": [{ "type": "output_text", "text": "orphan" }]
                }
            })
        ),
    )
    .unwrap();

    let good_file = sessions_dir.join("good-rollout.jsonl");
    std::fs::write(
        &good_file,
        [
            serde_json::json!({
                "timestamp": "2026-03-23T18:21:26Z",
                "type": "session_meta",
                "payload": {
                    "id": "session-good",
                    "cwd": "/home/user/src/example",
                    "cli_version": "0.117.0"
                }
            })
            .to_string(),
            serde_json::json!({
                "timestamp": "2026-03-23T18:21:27Z",
                "type": "response_item",
                "payload": {
                    "type": "message",
                    "role": "assistant",
                    "content": [{ "type": "output_text", "text": "healthy" }]
                }
            })
            .to_string(),
        ]
        .join("\n")
            + "\n",
    )
    .unwrap();

    let state_path = tempdir.path().join("state.json");
    let state = State::load(state_path.clone()).unwrap();
    let client = GraphqlClient::new("http://127.0.0.1:1/").unwrap();
    let shared = SharedContext::new(client, true, 50, Source::Codex.ingest_label());
    let source = Source::Codex.build_parser(codex_dir.clone());
    let mut watcher = Watcher::new(source, state, shared, 5);

    let stats = watcher.scan_once().await.unwrap();
    let persisted = State::load(state_path).unwrap();

    assert_eq!(stats.errors, 1);
    assert_eq!(
        persisted.get_offset(&good_file),
        std::fs::metadata(&good_file).unwrap().len()
    );
    assert_eq!(persisted.get_offset(&bad_file), 0);
    assert!(persisted
        .watch_checkpoint()
        .deferred_files
        .contains_key(&bad_file));
    assert_eq!(persisted.watch_checkpoint().health, WatchHealth::Degraded);
    assert!(watcher.failed_files.contains_key(&bad_file));
    assert!(!watcher.failed_files.contains_key(&good_file));
}

#[tokio::test]
async fn scan_once_checkpoints_rootless_gemini_file_without_replaying_it() {
    let tempdir = TempDir::new().unwrap();
    let gemini_dir = tempdir.path().join(".gemini");
    let chats_dir = gemini_dir.join("tmp/orphan/chats");
    std::fs::create_dir_all(&chats_dir).unwrap();

    let file = chats_dir.join("session-orphan.json");
    std::fs::write(
        &file,
        serde_json::json!({
            "sessionId": "session-orphan",
            "messages": [{
                "id": "assistant-1",
                "timestamp": "2026-03-19T13:32:00Z",
                "type": "gemini",
                "content": "done"
            }]
        })
        .to_string(),
    )
    .unwrap();

    let state_path = tempdir.path().join("state.json");
    let state = State::load(state_path.clone()).unwrap();
    let client = GraphqlClient::new("http://127.0.0.1:1/").unwrap();
    let shared = SharedContext::new(client, true, 50, Source::Gemini.ingest_label());
    let source = Source::Gemini.build_parser(gemini_dir);
    let mut watcher = Watcher::new(source, state, shared, 5);

    let first = watcher.scan_once().await.unwrap();
    let persisted = State::load(state_path).unwrap();
    let second = watcher.scan_once().await.unwrap();

    assert_eq!(first, ScanStats::default());
    assert_eq!(second, ScanStats::default());
    assert_eq!(persisted.get_offset(&file), 1);
}

#[tokio::test]
async fn scan_once_defers_malformed_gemini_file_without_aborting_good_file() {
    let tempdir = TempDir::new().unwrap();
    let gemini_dir = tempdir.path().join(".gemini");
    let good_dir = gemini_dir.join("tmp/good/chats");
    let bad_dir = gemini_dir.join("tmp/bad/chats");
    std::fs::create_dir_all(&good_dir).unwrap();
    std::fs::create_dir_all(&bad_dir).unwrap();
    std::fs::write(
        gemini_dir.join("tmp/good/.project_root"),
        "/home/user/src/example\n",
    )
    .unwrap();

    let good_file = good_dir.join("session-good.json");
    std::fs::write(
        &good_file,
        serde_json::json!({
            "sessionId": "session-good",
            "messages": [{
                "id": "assistant-1",
                "timestamp": "2026-03-19T13:32:00Z",
                "type": "gemini",
                "content": "done"
            }]
        })
        .to_string(),
    )
    .unwrap();

    let bad_file = bad_dir.join("session-bad.json");
    std::fs::write(&bad_file, "{").unwrap();

    let state_path = tempdir.path().join("state.json");
    let state = State::load(state_path.clone()).unwrap();
    let client = GraphqlClient::new("http://127.0.0.1:1/").unwrap();
    let shared = SharedContext::new(client, true, 50, Source::Gemini.ingest_label());
    let source = Source::Gemini.build_parser(gemini_dir);
    let mut watcher = Watcher::new(source, state, shared, 5);

    let stats = watcher.scan_once().await.unwrap();
    let persisted = State::load(state_path).unwrap();

    assert_eq!(stats.errors, 1);
    assert_eq!(persisted.get_offset(&good_file), 1);
    assert_eq!(persisted.get_offset(&bad_file), 0);
    assert!(persisted
        .watch_checkpoint()
        .deferred_files
        .contains_key(&bad_file));
    assert_eq!(persisted.watch_checkpoint().health, WatchHealth::Degraded);
    assert!(watcher.failed_files.contains_key(&bad_file));
    assert!(!watcher.failed_files.contains_key(&good_file));
}
