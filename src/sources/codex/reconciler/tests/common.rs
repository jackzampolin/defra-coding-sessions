use axum::{extract::State as AxumState, routing::post, Json, Router};
use rusqlite::params;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

#[derive(Clone, Default)]
pub(super) struct SessionStore {
    pub(super) sessions: HashMap<String, Map<String, Value>>,
}

async fn handler(
    AxumState(store): AxumState<Arc<Mutex<SessionStore>>>,
    Json(body): Json<Value>,
) -> Json<Value> {
    let query = body
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let mut store = store.lock().await;
    Json(handle_query(&mut store, &query))
}

pub(super) async fn spawn_server() -> (SocketAddr, Arc<Mutex<SessionStore>>) {
    let store = Arc::new(Mutex::new(SessionStore::default()));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = Router::new()
        .route("/", post(handler))
        .with_state(store.clone());
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (address, store)
}

pub(super) fn create_sqlite_catalog(path: &std::path::Path, title: &str, archived: bool) {
    let connection = rusqlite::Connection::open(path).unwrap();
    connection
        .execute_batch(
            r#"
            CREATE TABLE threads (
                id TEXT PRIMARY KEY,
                rollout_path TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                source TEXT NOT NULL,
                model_provider TEXT NOT NULL,
                cwd TEXT NOT NULL,
                title TEXT NOT NULL,
                sandbox_policy TEXT NOT NULL,
                approval_mode TEXT NOT NULL,
                tokens_used INTEGER NOT NULL DEFAULT 0,
                has_user_event INTEGER NOT NULL DEFAULT 0,
                archived INTEGER NOT NULL DEFAULT 0,
                archived_at INTEGER,
                git_sha TEXT,
                git_branch TEXT,
                git_origin_url TEXT,
                cli_version TEXT NOT NULL DEFAULT '',
                first_user_message TEXT NOT NULL DEFAULT '',
                agent_nickname TEXT,
                agent_role TEXT,
                memory_mode TEXT NOT NULL DEFAULT 'enabled',
                model TEXT,
                reasoning_effort TEXT
            );
            "#,
        )
        .unwrap();
    connection
        .execute(
            r#"
            INSERT INTO threads (
                id, rollout_path, created_at, updated_at, source, model_provider, cwd, title,
                sandbox_policy, approval_mode, archived, git_sha, git_branch, git_origin_url,
                cli_version, first_user_message, model, reasoning_effort
            ) VALUES (
                'session-main', '/home/user/.codex/sessions/rollout.jsonl', 1774290086,
                1774290470, 'cli', 'openai', '/home/user/src/example', ?1,
                'danger-full-access', 'never', ?2, 'abc123', 'main',
                'git@example.com:example/repo.git', '0.116.0', 'Build codex-data',
                'gpt-5.4', 'xhigh'
            )
            "#,
            params![title, if archived { 1 } else { 0 }],
        )
        .unwrap();
}

pub(super) fn session_meta_line(session_id: &str, timestamp: &str) -> String {
    json!({
        "timestamp": timestamp,
        "type": "session_meta",
        "payload": {
            "id": session_id,
            "timestamp": timestamp,
            "cwd": "/home/user/src/example",
            "cli_version": "0.116.0",
            "source": "cli"
        }
    })
    .to_string()
}

pub(super) fn assistant_message_line(text: &str, timestamp: &str) -> String {
    json!({
        "timestamp": timestamp,
        "type": "response_item",
        "payload": {
            "type": "message",
            "role": "assistant",
            "content": [
                { "type": "output_text", "text": text }
            ]
        }
    })
    .to_string()
}

fn handle_query(store: &mut SessionStore, query: &str) -> Value {
    if query.contains("update_CodingSession(") {
        let session_id = extract_filter_value(query, "session_id").unwrap();
        let session = store.sessions.entry(session_id.clone()).or_insert_with(|| {
            let mut map = Map::new();
            map.insert(
                "_docID".to_string(),
                Value::String(format!("session-doc-{session_id}")),
            );
            map.insert("session_id".to_string(), Value::String(session_id.clone()));
            map
        });
        if let Some(title) = extract_string_field(query, "title") {
            session.insert("title".to_string(), Value::String(title));
        }
        if let Some(archived) = extract_bool_field(query, "archived") {
            session.insert("archived".to_string(), Value::Bool(archived));
        }
        return json!({ "data": { "update_CodingSession": { "_docID": session.get("_docID").cloned().unwrap() } } });
    }

    if query.contains("CodingSession(") {
        let session_id = extract_filter_value(query, "session_id").unwrap();
        let rows = store
            .sessions
            .get(&session_id)
            .cloned()
            .map(|row| vec![Value::Object(row)])
            .unwrap_or_default();
        return json!({ "data": { "CodingSession": rows } });
    }

    json!({ "errors": [{ "message": format!("unsupported query: {query}") }] })
}

fn extract_filter_value(query: &str, field: &str) -> Option<String> {
    let needle = format!(r#"{field}: {{ _eq: ""#);
    let start = query.find(&needle)? + needle.len();
    parse_quoted(&query[start..]).map(|(value, _)| value)
}

fn extract_string_field(query: &str, field: &str) -> Option<String> {
    let needle = format!(r#"{field}: ""#);
    let start = query.find(&needle)? + needle.len();
    parse_quoted(&query[start..]).map(|(value, _)| value)
}

fn extract_bool_field(query: &str, field: &str) -> Option<bool> {
    let needle = format!("{field}: ");
    let start = query.find(&needle)? + needle.len();
    let rest = &query[start..];
    if rest.starts_with("true") {
        Some(true)
    } else if rest.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

fn parse_quoted(input: &str) -> Option<(String, usize)> {
    let mut escaped = false;
    let mut raw = String::new();
    let mut consumed = 0usize;

    for ch in input.chars() {
        consumed += ch.len_utf8();
        if escaped {
            raw.push(ch);
            escaped = false;
            continue;
        }

        match ch {
            '\\' => {
                raw.push(ch);
                escaped = true;
            }
            '"' => {
                let value: String = serde_json::from_str(&format!(r#""{}""#, raw)).ok()?;
                return Some((value, consumed));
            }
            _ => raw.push(ch),
        }
    }

    None
}
