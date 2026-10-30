use axum::{extract::State, routing::post, Json, Router};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

#[derive(Clone)]
struct StaticResponseState {
    body: Value,
    status: axum::http::StatusCode,
}

#[derive(Clone)]
struct SequenceResponseState {
    bodies: Arc<Mutex<VecDeque<Value>>>,
    requests: Arc<AtomicUsize>,
}

pub(crate) struct SequenceServer {
    pub address: SocketAddr,
    pub requests: Arc<AtomicUsize>,
}

async fn static_handler(
    State(state): State<StaticResponseState>,
) -> (axum::http::StatusCode, Json<Value>) {
    (state.status, Json(state.body))
}

async fn sequence_handler(State(state): State<SequenceResponseState>) -> Json<Value> {
    state.requests.fetch_add(1, Ordering::SeqCst);
    let body = state
        .bodies
        .lock()
        .await
        .pop_front()
        .unwrap_or_else(|| json!({ "data": {} }));
    Json(body)
}

pub(crate) async fn spawn_server(body: Value) -> SocketAddr {
    spawn_status_server(body, axum::http::StatusCode::OK).await
}

pub(crate) async fn spawn_status_server(body: Value, status: axum::http::StatusCode) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = Router::new()
        .route("/", post(static_handler))
        .with_state(StaticResponseState { body, status });
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    address
}

pub(crate) async fn spawn_sequence_server(bodies: Vec<Value>) -> SequenceServer {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let requests = Arc::new(AtomicUsize::new(0));
    let app = Router::new()
        .route("/", post(sequence_handler))
        .with_state(SequenceResponseState {
            bodies: Arc::new(Mutex::new(VecDeque::from(bodies))),
            requests: requests.clone(),
        });
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    SequenceServer { address, requests }
}
