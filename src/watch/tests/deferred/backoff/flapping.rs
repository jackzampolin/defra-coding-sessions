use super::super::super::common::*;

#[tokio::test]
async fn scan_once_marks_repeated_failures_as_flapping() {
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
    let shared = SharedContext::new(client, true, 50, Source::Gemini.ingest_label());
    let source = Source::Gemini.build_parser(gemini_dir);
    let mut watcher = Watcher::new(source, state, shared, 5);

    for expected_attempts in 1..=3 {
        let stats = watcher.scan_once().await.unwrap();
        assert_eq!(stats.errors, 1);
        assert_eq!(
            watcher.failed_files.get(&file).unwrap().attempts,
            expected_attempts
        );
        if expected_attempts < 3 {
            watcher
                .failed_files
                .get_mut(&file)
                .unwrap()
                .next_retry_epoch_ms = super::super::super::super::now_epoch_ms();
        }
    }

    let persisted = State::load(state_path).unwrap();

    assert_eq!(
        persisted
            .watch_checkpoint()
            .deferred_files
            .get(&file)
            .unwrap()
            .attempts,
        3
    );
    assert_eq!(persisted.watch_checkpoint().health, WatchHealth::Flapping);
}
