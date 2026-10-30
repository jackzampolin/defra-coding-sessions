use super::super::Reconciler;
use super::common::{
    assistant_message_line, create_sqlite_catalog, session_meta_line, spawn_server,
};
use crate::batch::DEFAULT_BATCH_SIZE;
use crate::client::GraphqlClient;
use crate::shared::SharedContext;
use crate::source::Source;
use crate::state::State;
use crate::watch::Watcher;
use serde_json::{Map, Value};
use tempfile::TempDir;

#[tokio::test]
async fn watcher_and_reconciler_do_not_conflict() {
    let tempdir = TempDir::new().unwrap();
    let codex_dir = tempdir.path().join("codex-home");
    let sessions_dir = codex_dir.join("sessions/2026/03/23");
    std::fs::create_dir_all(&sessions_dir).unwrap();
    std::fs::write(
        sessions_dir.join("rollout.jsonl"),
        [
            session_meta_line("session-main", "2026-03-23T18:21:26Z"),
            assistant_message_line("first", "2026-03-23T18:21:27Z"),
        ]
        .join("\n")
            + "\n",
    )
    .unwrap();

    let sqlite_path = codex_dir.join("state_5.sqlite");
    create_sqlite_catalog(&sqlite_path, "Concurrent title", false);

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

    let watcher = async {
        let state = State::load(tempdir.path().join("watch-state.json")).unwrap();
        let client = GraphqlClient::new("http://127.0.0.1:1/").unwrap();
        let shared = SharedContext::new(
            client,
            true,
            DEFAULT_BATCH_SIZE,
            Source::Codex.ingest_label(),
        );
        let source = Source::Codex.build_parser(codex_dir.clone());
        let mut watcher = Watcher::new(source, state, shared, 1);
        watcher.scan_once().await.unwrap()
    };

    let reconciler = async {
        let client = GraphqlClient::new(format!("http://{address}/")).unwrap();
        let reconciler = Reconciler::new(client, sqlite_path.clone(), 60, false);
        reconciler.reconcile_once().await.unwrap()
    };

    let (watch_stats, reconcile_stats) = tokio::join!(watcher, reconciler);
    assert_eq!(watch_stats.messages_processed, 1);
    assert_eq!(reconcile_stats.sessions_updated, 1);
}
