use super::super::super::common::*;

#[tokio::test]
async fn flush_batch_retries_transaction_conflicts() {
    let server = spawn_sequence_server(vec![
        json!({
            "data": {
                "m0": null
            },
            "errors": [
                { "message": "storage error: transaction conflict", "path": ["m0"] }
            ]
        }),
        json!({
            "data": {
                "m0": { "_docID": "message-doc-1" }
            }
        }),
    ])
    .await;

    let client = GraphqlClient::new(format!("http://{}/", server.address)).unwrap();
    let mut context = context_with_session(client, false, 1, 0);

    queue_message(&mut context, &parsed_message(Role::User, "one"));

    let result = context.flush_batch().await.unwrap();

    assert_eq!(
        result,
        BatchResult {
            created: 1,
            skipped: 0,
            failed: 0,
        }
    );
    assert_eq!(server.requests.load(std::sync::atomic::Ordering::SeqCst), 2);
}

#[tokio::test]
async fn flush_batch_falls_back_to_individual_retries_after_persistent_batch_conflicts() {
    let conflict_response = json!({
        "data": {
            "m0": null,
            "m1": null
        },
        "errors": [
            { "message": "storage error: transaction conflict", "path": ["m0"] },
            { "message": "storage error: transaction conflict", "path": ["m1"] }
        ]
    });
    let server = spawn_sequence_server(vec![
        conflict_response.clone(),
        conflict_response.clone(),
        conflict_response.clone(),
        conflict_response,
        json!({
            "data": {
                "create_CodingMessage": { "_docID": "message-doc-1" }
            }
        }),
        json!({
            "data": {
                "create_CodingMessage": { "_docID": "message-doc-2" }
            }
        }),
    ])
    .await;

    let client = GraphqlClient::new(format!("http://{}/", server.address)).unwrap();
    let mut context = context_with_session(client, false, 50, 0);

    for content in ["one", "two"] {
        queue_message(&mut context, &parsed_message(Role::User, content));
    }

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

#[tokio::test]
async fn flush_batch_individual_retry_looks_up_message_when_mutation_returns_no_data() {
    let conflict_response = json!({
        "data": {
            "m0": null
        },
        "errors": [
            { "message": "storage error: transaction conflict", "path": ["m0"] }
        ]
    });
    let server = spawn_sequence_server(vec![
        conflict_response.clone(),
        conflict_response.clone(),
        conflict_response.clone(),
        conflict_response,
        json!({ "data": {} }),
        json!({
            "data": {
                "CodingMessage": [{ "_docID": "message-doc-1" }]
            }
        }),
    ])
    .await;

    let client = GraphqlClient::new(format!("http://{}/", server.address)).unwrap();
    let mut context = context_with_session(client, false, 1, 0);

    queue_message(&mut context, &parsed_message(Role::User, "one"));

    let result = context.flush_batch().await.unwrap();

    assert_eq!(
        result,
        BatchResult {
            created: 0,
            skipped: 1,
            failed: 0,
        }
    );
    assert_eq!(server.requests.load(std::sync::atomic::Ordering::SeqCst), 6);
}
