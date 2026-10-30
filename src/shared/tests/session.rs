use super::common::*;

#[tokio::test]
async fn snapshot_session_keeps_session_state() {
    let client = GraphqlClient::new("http://127.0.0.1:1/").unwrap();
    let mut context = context_with_session(client, true, 50, 7);

    let result = context.snapshot_session(TEST_SESSION_ID).await.unwrap();

    assert_eq!(
        result,
        SessionFlushResult {
            updated: true,
            batch_result: BatchResult::default(),
        }
    );
    assert!(context.session_cache.contains_key(TEST_SESSION_ID));
    assert_eq!(
        context.reserve_message_sequence(TEST_SESSION_ID).unwrap(),
        7
    );
}

#[tokio::test]
async fn finalize_session_failure_preserves_session_state() {
    let address = spawn_server(json!({
        "errors": [{ "message": "write failed" }]
    }))
    .await;

    let client = GraphqlClient::new(format!("http://{address}/")).unwrap();
    let mut context = context_with_session(client, false, 50, 2);

    let error = context.finalize_session(TEST_SESSION_ID).await.unwrap_err();

    assert!(error.to_string().contains("finalize CodingSession failed"));
    assert!(context.session_cache.contains_key(TEST_SESSION_ID));
    assert_eq!(
        context.reserve_message_sequence(TEST_SESSION_ID).unwrap(),
        2
    );
}

#[tokio::test]
async fn process_message_reloads_existing_session_state_before_create_on_cache_miss() {
    let server = spawn_sequence_server(vec![
        json!({
            "data": {
                "CodingSession": [{
                    "_docID": "session-doc-1",
                    "model_primary": "claude-sonnet",
                    "message_count": 4,
                    "user_message_count": 2,
                    "input_tokens": 11,
                    "output_tokens": 7,
                    "tools_used": ["Read"],
                    "first_prompt": "hello"
                }]
            }
        }),
        json!({
            "data": {
                "CodingMessage": [{ "sequence": 3 }]
            }
        }),
        json!({
            "data": {
                "m0": { "_docID": "message-doc-5" }
            }
        }),
    ])
    .await;

    let client = GraphqlClient::new(format!("http://{}/", server.address)).unwrap();
    let mut context = SharedContext::new(client, false, 1, Source::Claude.ingest_label());
    cache_test_project(&mut context);

    let outcome = context
        .process_message(assistant_message("response"))
        .await
        .unwrap();

    assert!(outcome.processed);
    assert_eq!(server.requests.load(std::sync::atomic::Ordering::SeqCst), 3);

    let meta = context.session_cache.get(TEST_SESSION_ID).unwrap();
    assert_eq!(meta.doc_id, TEST_SESSION_DOC_ID);
    assert_eq!(meta.msg_seq, 5);
    assert_eq!(meta.message_count, 5);
    assert_eq!(meta.user_message_count, 2);
    assert_eq!(meta.input_tokens, 14);
    assert_eq!(meta.output_tokens, 12);
    assert!(meta.tools_used.contains("Read"));
    assert_eq!(meta.first_prompt.as_deref(), Some("hello"));
}

#[tokio::test]
async fn process_message_recovers_when_session_create_races_existing_row() {
    let server = spawn_sequence_server(vec![
        json!({
            "data": {
                "CodingSession": []
            }
        }),
        json!({
            "errors": [{
                "message": "can not index a doc's field(s) that violates unique index"
            }]
        }),
        json!({
            "data": {
                "CodingSession": [{
                    "_docID": "session-doc-1",
                    "model_primary": "claude-sonnet",
                    "message_count": 4,
                    "user_message_count": 2,
                    "input_tokens": 11,
                    "output_tokens": 7,
                    "tools_used": ["Read"],
                    "first_prompt": "hello"
                }]
            }
        }),
        json!({
            "data": {
                "CodingMessage": [{ "sequence": 3 }]
            }
        }),
        json!({
            "data": {
                "m0": { "_docID": "message-doc-5" }
            }
        }),
    ])
    .await;

    let client = GraphqlClient::new(format!("http://{}/", server.address)).unwrap();
    let mut context = SharedContext::new(client, false, 1, Source::Claude.ingest_label());
    cache_test_project(&mut context);

    let outcome = context
        .process_message(assistant_message("response"))
        .await
        .unwrap();

    assert!(outcome.processed);
    assert_eq!(server.requests.load(std::sync::atomic::Ordering::SeqCst), 5);

    let meta = context.session_cache.get(TEST_SESSION_ID).unwrap();
    assert_eq!(meta.doc_id, TEST_SESSION_DOC_ID);
    assert_eq!(meta.msg_seq, 5);
    assert_eq!(meta.message_count, 5);
    assert_eq!(meta.user_message_count, 2);
    assert_eq!(meta.input_tokens, 14);
    assert_eq!(meta.output_tokens, 12);
    assert!(meta.tools_used.contains("Read"));
    assert_eq!(meta.first_prompt.as_deref(), Some("hello"));
}
