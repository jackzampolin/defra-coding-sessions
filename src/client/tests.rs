use super::*;
use crate::test_support::spawn_status_server;
use serde_json::json;

#[tokio::test]
async fn execute_returns_data_on_success() {
    let address = spawn_status_server(
        json!({
            "data": {
                "CodingSession": [{ "_docID": "session-doc-1" }]
            }
        }),
        axum::http::StatusCode::OK,
    )
    .await;

    let client = GraphqlClient::new(format!("http://{address}/")).unwrap();
    let response = client.execute("query { CodingSession { _docID } }").await;

    assert!(!response.has_errors());
    assert_eq!(
        response
            .data
            .as_ref()
            .and_then(|data| data.get("CodingSession"))
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(|item| item.get("_docID"))
            .and_then(Value::as_str),
        Some("session-doc-1")
    );
}

#[tokio::test]
async fn execute_surfaces_http_failures_as_errors() {
    let address = spawn_status_server(
        json!({
            "error": "bad request"
        }),
        axum::http::StatusCode::BAD_REQUEST,
    )
    .await;

    let client = GraphqlClient::new(format!("http://{address}/")).unwrap();
    let response = client.execute("query { CodingSession { _docID } }").await;

    assert!(response.has_errors());
    assert!(response.errors[0]
        .get("message")
        .and_then(Value::as_str)
        .unwrap()
        .contains("400"));
}
