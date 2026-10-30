use super::common::*;

#[tokio::test]
async fn scan_once_isolates_transaction_conflicts_to_the_failed_file() {
    let tempdir = TempDir::new().unwrap();
    let claude_dir = tempdir.path().join(".claude");
    let project_dir = claude_dir.join("projects").join("demo-project");
    std::fs::create_dir_all(&project_dir).unwrap();

    let good_file = project_dir.join("a-good.jsonl");
    std::fs::write(
        &good_file,
        json!({
            "sessionId": "session-good",
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

    let bad_file = project_dir.join("b-bad.jsonl");
    std::fs::write(
        &bad_file,
        json!({
            "sessionId": "session-bad",
            "cwd": "/tmp/repo",
            "gitBranch": "main",
            "version": "1.0.0",
            "timestamp": "2026-03-23T18:21:27Z",
            "message": {
                "role": "user",
                "model": "claude-sonnet",
                "content": "conflict"
            }
        })
        .to_string()
            + "\n",
    )
    .unwrap();

    let conflict = json!({
        "data": { "m0": null },
        "errors": [
            { "message": "storage error: transaction conflict", "path": ["m0"] }
        ]
    });
    let server = spawn_sequence_server(vec![
        json!({
            "data": { "upsert_CodingProject": { "_docID": "project-doc-1" } }
        }),
        json!({
            "data": { "CodingSession": [] }
        }),
        json!({
            "data": { "create_CodingSession": { "_docID": "session-doc-1" } }
        }),
        json!({
            "data": { "m0": { "_docID": "message-doc-1" } }
        }),
        json!({
            "data": { "update_CodingSession": { "_docID": "session-doc-1" } }
        }),
        json!({
            "data": { "CodingSession": [] }
        }),
        json!({
            "data": { "create_CodingSession": { "_docID": "session-doc-2" } }
        }),
        conflict.clone(),
        conflict.clone(),
        conflict.clone(),
        conflict,
    ])
    .await;

    let state_path = tempdir.path().join("state.json");
    let state = State::load(state_path.clone()).unwrap();
    let client = GraphqlClient::new(format!("http://{}/", server.address)).unwrap();
    let shared = SharedContext::new(client, false, 50, Source::Claude.ingest_label());
    let source = Source::Claude.build_parser(claude_dir);
    let mut watcher = Watcher::new(source, state, shared, 5);

    let stats = watcher.scan_once().await.unwrap();
    let persisted = State::load(state_path).unwrap();

    assert_eq!(stats.errors, 1);
    assert_eq!(
        persisted.get_offset(&good_file),
        std::fs::metadata(&good_file).unwrap().len()
    );
    assert_eq!(persisted.get_offset(&bad_file), 0);
    assert!(watcher.failed_files.contains_key(&bad_file));
    assert!(!watcher.failed_files.contains_key(&good_file));
}
