use super::common::*;

#[tokio::test]
async fn scan_once_persists_evicted_sessions_as_removed() {
    let tempdir = TempDir::new().unwrap();
    let state_path = tempdir.path().join("state.json");
    let mut state = State::load(state_path.clone()).unwrap();
    let stale_last_touched =
        SystemTime::now() - super::super::WATCH_CACHE_MAX_IDLE - Duration::from_secs(1);
    state.replace_session_snapshots(HashMap::from([(
        "session-1".to_string(),
        SessionCheckpoint {
            doc_id: "session-doc-1".to_string(),
            msg_seq: 4,
            model_counts: HashMap::new(),
            last_ts: None,
            last_touched_epoch_ms: system_time_epoch_ms(stale_last_touched),
            message_count: 4,
            user_message_count: 2,
            input_tokens: 0,
            output_tokens: 0,
            tools_used: Vec::new(),
            first_prompt: Some("hello".to_string()),
            ..Default::default()
        },
    )]));

    let client = GraphqlClient::new("http://127.0.0.1:1/").unwrap();
    let mut shared = SharedContext::new(client, false, 50, Source::Claude.ingest_label());
    shared.import_session_snapshots(state.session_snapshots().clone());
    let source = Source::Claude.build_parser(tempdir.path().join(".claude"));
    let mut watcher = Watcher::new(source, state, shared, 5);

    let stats = watcher.scan_once().await.unwrap();
    let persisted = State::load(state_path).unwrap();

    assert_eq!(stats, ScanStats::default());
    assert!(persisted.sessions.is_empty());
    assert!(watcher.shared.session_cache.is_empty());
}
