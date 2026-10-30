use super::super::Reconciler;
use super::common::{create_sqlite_catalog, spawn_server};
use crate::client::GraphqlClient;
use serde_json::{Map, Value};
use tempfile::TempDir;

#[tokio::test]
async fn reconcile_once_updates_title_and_archived_state() {
    let tempdir = TempDir::new().unwrap();
    let sqlite_path = tempdir.path().join("state_5.sqlite");
    create_sqlite_catalog(&sqlite_path, "Updated title", true);

    let (address, store) = spawn_server().await;
    {
        let mut store = store.lock().await;
        let mut session = Map::new();
        session.insert(
            "_docID".to_string(),
            Value::String("session-doc-main".to_string()),
        );
        session.insert(
            "session_id".to_string(),
            Value::String("session-main".to_string()),
        );
        session.insert("title".to_string(), Value::String("Old title".to_string()));
        session.insert("archived".to_string(), Value::Bool(false));
        store.sessions.insert("session-main".to_string(), session);
    }

    let client = GraphqlClient::new(format!("http://{address}/")).unwrap();
    let reconciler = Reconciler::new(client, sqlite_path, 60, false);
    let stats = reconciler.reconcile_once().await.unwrap();

    assert_eq!(stats.threads_checked, 1);
    assert_eq!(stats.sessions_updated, 1);
    assert_eq!(stats.new_threads_discovered, 0);

    let store = store.lock().await;
    let session = store.sessions.get("session-main").unwrap();
    assert_eq!(
        session.get("title").and_then(Value::as_str),
        Some("Updated title")
    );
    assert_eq!(session.get("archived").and_then(Value::as_bool), Some(true));
}
