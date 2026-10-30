use super::super::super::common::*;

#[tokio::test]
async fn flush_batch_handles_mixed_message_results() {
    let address = spawn_server(json!({
        "data": {
            "m0": { "_docID": "message-doc-1" },
            "m1": null,
            "m2": null
        },
        "errors": [
            { "message": "already exists", "path": ["m1"] },
            { "message": "storage error: transaction conflict", "path": ["m2"] }
        ]
    }))
    .await;

    let client = GraphqlClient::new(format!("http://{address}/")).unwrap();
    let mut context = context_with_session(client, false, 50, 0);

    for content in ["one", "two", "three"] {
        queue_message(&mut context, &parsed_message(Role::User, content));
    }

    let result = context.flush_batch().await.unwrap();

    assert_eq!(
        result,
        BatchResult {
            created: 1,
            skipped: 1,
            failed: 1,
        }
    );

    let meta = context.session_cache.get(TEST_SESSION_ID).unwrap();
    assert_eq!(meta.message_count, 1);
    assert_eq!(meta.user_message_count, 1);
    assert_eq!(meta.input_tokens, 3);
    assert_eq!(meta.output_tokens, 5);
}
