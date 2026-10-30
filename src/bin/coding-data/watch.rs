use anyhow::{Context, Result};
use axum::extract::State as AxumState;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use coding_data::client::GraphqlClient;
use coding_data::config::Config;
use coding_data::metrics::IngestMetrics;
use coding_data::shared::SharedContext;
use coding_data::source::Source;
use coding_data::sources::CodexReconciler;
use coding_data::state::State;
use coding_data::watch::Watcher;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::task::JoinSet;

const DEFAULT_METRICS_PORT: u16 = 9196;

pub(super) async fn run(config: Config, requested_sources: Vec<Source>) -> Result<()> {
    let watch_sources = normalize_sources(config.source, requested_sources);
    let metrics = Arc::new(IngestMetrics::new()?);
    let metrics_port = metrics_port_from_env();

    tokio::select! {
        result = run_group(config, watch_sources, Arc::clone(&metrics)) => result?,
        result = run_metrics_server(metrics_port, Arc::clone(&metrics)) => result?,
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("received ctrl-c, shutting down");
        }
    }

    Ok(())
}

async fn run_group(
    base_config: Config,
    sources: Vec<Source>,
    metrics: Arc<IngestMetrics>,
) -> Result<()> {
    let mut tasks = JoinSet::new();
    for source in sources {
        let config = base_config.for_runtime_source(source);
        let metrics = Arc::clone(&metrics);
        tasks.spawn(async move { run_source(config, metrics).await });
    }

    while let Some(result) = tasks.join_next().await {
        result.context("joining watch source task")??;
    }

    Ok(())
}

async fn run_source(config: Config, metrics: Arc<IngestMetrics>) -> Result<()> {
    let client = GraphqlClient::new(config.graphql_endpoint.clone())?;
    let state = State::load(config.state_path())?;
    let mut shared = SharedContext::new(
        client.clone(),
        false,
        config.batch_size,
        config.source.ingest_label(),
    )
    .with_source_host(config.source_host.clone())
    .with_metrics(Arc::clone(&metrics));
    shared.import_session_snapshots(state.session_snapshots().clone());
    let source = config.source.build_parser(config.source_dir.clone());
    let mut watcher =
        Watcher::new(source, state, shared, config.scan_interval_secs).with_metrics(metrics);
    if config.source == Source::Codex {
        if let (Some(sqlite_path), Some(reconcile_interval_secs)) =
            (config.sqlite_path.clone(), config.reconcile_interval_secs)
        {
            let reconciler =
                CodexReconciler::new(client, sqlite_path, reconcile_interval_secs, false);
            tokio::select! {
                result = watcher.run() => result,
                result = reconciler.run() => result,
            }
        } else {
            watcher.run().await
        }
    } else {
        watcher.run().await
    }
}

async fn run_metrics_server(port: u16, metrics: Arc<IngestMetrics>) -> Result<()> {
    let app = Router::new()
        .route("/metrics", get(metrics_handler))
        .with_state(metrics);

    let listener = TcpListener::bind(format!("0.0.0.0:{port}")).await?;
    tracing::info!(port, "metrics server listening");

    axum::serve(listener, app)
        .await
        .context("metrics server failed")
}

async fn metrics_handler(AxumState(metrics): AxumState<Arc<IngestMetrics>>) -> impl IntoResponse {
    metrics.encode()
}

fn metrics_port_from_env() -> u16 {
    std::env::var("CODING_DATA_METRICS_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_METRICS_PORT)
}

fn normalize_sources(default_source: Source, requested: Vec<Source>) -> Vec<Source> {
    if requested.is_empty() {
        return vec![default_source];
    }

    let mut sources = Vec::new();
    for source in requested {
        if !sources.contains(&source) {
            sources.push(source);
        }
    }
    sources
}
