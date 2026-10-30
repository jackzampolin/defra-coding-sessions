use super::common::*;

#[tokio::test]
async fn ensure_project_retries_transaction_conflicts() {
    let server = spawn_sequence_server(vec![
        json!({
            "errors": [{ "message": "storage error: transaction conflict" }]
        }),
        json!({
            "data": {
                "upsert_CodingProject": { "_docID": "project-doc-1" }
            }
        }),
    ])
    .await;

    let client = GraphqlClient::new(format!("http://{}/", server.address)).unwrap();
    let mut context = SharedContext::new(client, false, 50, Source::Claude.ingest_label());

    let doc_id = context.ensure_project("/tmp/repo").await.unwrap();

    assert_eq!(doc_id, "project-doc-1");
    assert_eq!(server.requests.load(std::sync::atomic::Ordering::SeqCst), 2);
}
