use super::super::super::common::*;

#[tokio::test]
async fn scan_once_restores_deferred_file_state_after_restart() {
    let tempdir = TempDir::new().unwrap();
    let gemini_dir = tempdir.path().join(".gemini");
    let project_dir = gemini_dir.join("tmp/demo/chats");
    std::fs::create_dir_all(&project_dir).unwrap();
    std::fs::write(
        gemini_dir.join("tmp/demo/.project_root"),
        "/home/user/src/example\n",
    )
    .unwrap();

    let file = project_dir.join("session.json");
    std::fs::write(&file, "{").unwrap();

    let state_path = tempdir.path().join("state.json");
    let state = State::load(state_path.clone()).unwrap();
    let client = GraphqlClient::new("http://127.0.0.1:1/").unwrap();
    let shared = SharedContext::new(client.clone(), true, 50, Source::Gemini.ingest_label());
    let source = Source::Gemini.build_parser(gemini_dir.clone());
    let mut watcher = Watcher::new(source, state, shared, 5);

    let first = watcher.scan_once().await.unwrap();
    let persisted = State::load(state_path.clone()).unwrap();
    let persisted_failure = persisted
        .watch_checkpoint()
        .deferred_files
        .get(&file)
        .unwrap()
        .clone();

    assert_eq!(first.errors, 1);

    let mut restarted_shared = SharedContext::new(client, true, 50, Source::Gemini.ingest_label());
    restarted_shared.import_session_snapshots(persisted.session_snapshots().clone());
    let restarted_source = Source::Gemini.build_parser(gemini_dir);
    let restarted = Watcher::new(restarted_source, persisted, restarted_shared, 5);
    let restored_failure = restarted.failed_files.get(&file).unwrap();

    assert_eq!(restored_failure.attempts, 1);
    assert_eq!(
        restored_failure.next_retry_epoch_ms,
        persisted_failure.next_retry_epoch_ms
    );
    assert_eq!(restored_failure.fingerprint, persisted_failure.fingerprint);
}

#[tokio::test]
async fn scan_once_resumes_backoff_after_restart_without_blocking_healthy_siblings() {
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
    std::fs::write(
        gemini_dir.join("tmp/bad/.project_root"),
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
    let shared = SharedContext::new(client.clone(), true, 50, Source::Gemini.ingest_label());
    let source = Source::Gemini.build_parser(gemini_dir.clone());
    let mut watcher = Watcher::new(source, state, shared, 5);

    let first = watcher.scan_once().await.unwrap();
    let persisted = State::load(state_path.clone()).unwrap();
    let retry_epoch_ms = persisted
        .watch_checkpoint()
        .deferred_files
        .get(&bad_file)
        .unwrap()
        .next_retry_epoch_ms;

    assert_eq!(first.errors, 1);

    let mut restarted_shared = SharedContext::new(client, true, 50, Source::Gemini.ingest_label());
    restarted_shared.import_session_snapshots(persisted.session_snapshots().clone());
    let restarted_source = Source::Gemini.build_parser(gemini_dir.clone());
    let mut restarted = Watcher::new(restarted_source, persisted, restarted_shared, 5);

    std::fs::write(
        &good_file,
        serde_json::json!({
            "sessionId": "session-good",
            "messages": [
                {
                    "id": "assistant-1",
                    "timestamp": "2026-03-19T13:32:00Z",
                    "type": "gemini",
                    "content": "done"
                },
                {
                    "id": "assistant-2",
                    "timestamp": "2026-03-19T13:33:00Z",
                    "type": "gemini",
                    "content": "still healthy"
                }
            ]
        })
        .to_string(),
    )
    .unwrap();

    let second = restarted.scan_once().await.unwrap();
    let persisted = State::load(state_path).unwrap();

    assert_eq!(second.errors, 0);
    assert_eq!(second.messages_processed, 1);
    assert_eq!(persisted.get_offset(&good_file), 2);
    assert_eq!(persisted.get_offset(&bad_file), 0);
    assert_eq!(
        restarted
            .failed_files
            .get(&bad_file)
            .unwrap()
            .next_retry_epoch_ms,
        retry_epoch_ms
    );
    assert_eq!(restarted.failed_files.get(&bad_file).unwrap().attempts, 1);
    assert_eq!(persisted.watch_checkpoint().health, WatchHealth::Degraded);
}

#[tokio::test]
async fn scan_once_recovers_deferred_file_after_restart_when_repaired() {
    let tempdir = TempDir::new().unwrap();
    let gemini_dir = tempdir.path().join(".gemini");
    let project_dir = gemini_dir.join("tmp/demo/chats");
    std::fs::create_dir_all(&project_dir).unwrap();
    std::fs::write(
        gemini_dir.join("tmp/demo/.project_root"),
        "/home/user/src/example\n",
    )
    .unwrap();

    let file = project_dir.join("session.json");
    std::fs::write(&file, "{").unwrap();

    let state_path = tempdir.path().join("state.json");
    let state = State::load(state_path.clone()).unwrap();
    let client = GraphqlClient::new("http://127.0.0.1:1/").unwrap();
    let shared = SharedContext::new(client.clone(), true, 50, Source::Gemini.ingest_label());
    let source = Source::Gemini.build_parser(gemini_dir.clone());
    let mut watcher = Watcher::new(source, state, shared, 5);

    let first = watcher.scan_once().await.unwrap();
    let persisted = State::load(state_path.clone()).unwrap();

    assert_eq!(first.errors, 1);
    assert!(persisted
        .watch_checkpoint()
        .deferred_files
        .contains_key(&file));

    let mut restarted_shared = SharedContext::new(client, true, 50, Source::Gemini.ingest_label());
    restarted_shared.import_session_snapshots(persisted.session_snapshots().clone());
    let restarted_source = Source::Gemini.build_parser(gemini_dir.clone());
    let mut restarted = Watcher::new(restarted_source, persisted, restarted_shared, 5);

    std::fs::write(
        &file,
        serde_json::json!({
            "sessionId": "session-1",
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

    let second = restarted.scan_once().await.unwrap();
    let persisted = State::load(state_path).unwrap();

    assert_eq!(second.errors, 0);
    assert_eq!(second.messages_processed, 1);
    assert!(!restarted.failed_files.contains_key(&file));
    assert!(!persisted
        .watch_checkpoint()
        .deferred_files
        .contains_key(&file));
    assert_eq!(persisted.watch_checkpoint().health, WatchHealth::Healthy);
}
