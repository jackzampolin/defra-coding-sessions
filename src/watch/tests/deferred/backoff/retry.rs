use super::super::super::common::*;

#[tokio::test]
async fn scan_once_retries_failed_file_immediately_after_it_changes() {
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

    let state = State::load(tempdir.path().join("state.json")).unwrap();
    let client = GraphqlClient::new("http://127.0.0.1:1/").unwrap();
    let shared = SharedContext::new(client, true, 50, Source::Gemini.ingest_label());
    let source = Source::Gemini.build_parser(gemini_dir.clone());
    let mut watcher = Watcher::new(source, state, shared, 5);

    let first = watcher.scan_once().await.unwrap();
    assert_eq!(first.errors, 1);
    let retry_at_epoch_ms = watcher.failed_files.get(&file).unwrap().next_retry_epoch_ms;

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

    let second = watcher.scan_once().await.unwrap();

    assert_eq!(second.errors, 0);
    assert_eq!(second.messages_processed, 1);
    assert!(!watcher.failed_files.contains_key(&file));
    assert!(retry_at_epoch_ms > super::super::super::super::now_epoch_ms());
}
