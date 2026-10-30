use super::super::common::*;

#[tokio::test]
async fn flush_batch_retries_conflicted_actions_individually() {
    let action_conflict = json!({
        "data": {
            "m0": null
        },
        "errors": [
            { "message": "storage error: transaction conflict", "path": ["m0"] }
        ]
    });
    let server = spawn_sequence_server(vec![
        json!({
            "data": {
                "m0": { "_docID": "message-doc-1" }
            }
        }),
        action_conflict.clone(),
        action_conflict.clone(),
        action_conflict.clone(),
        action_conflict,
        json!({
            "data": {
                "create_CodingAction": { "_docID": "action-doc-1" }
            }
        }),
    ])
    .await;

    let client = GraphqlClient::new(format!("http://{}/", server.address)).unwrap();
    let mut context = context_with_session(client, false, 50, 0);

    queue_message(&mut context, &read_action_message());

    let result = context.flush_batch().await.unwrap();

    assert_eq!(
        result,
        BatchResult {
            created: 2,
            skipped: 0,
            failed: 0,
        }
    );
    assert_eq!(server.requests.load(std::sync::atomic::Ordering::SeqCst), 6);
}
