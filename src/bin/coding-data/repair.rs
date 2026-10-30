use anyhow::{Context, Result};
use coding_data::backfill::{BackfillOptions, Backfiller};
use coding_data::client::GraphqlClient;
use coding_data::config::Config;
use coding_data::mutations::query_session_message_gaps;
use coding_data::shared::SharedContext;
use coding_data::state::State;

pub(super) async fn run(
    config: Config,
    session_filter: Option<String>,
    dry_run: bool,
) -> Result<()> {
    let client = GraphqlClient::new(config.graphql_endpoint.clone())?;

    tracing::info!("querying store for session message gaps");
    let response = client.execute(&query_session_message_gaps(200)).await;
    let sessions = response
        .data
        .as_ref()
        .and_then(|d| d.get("CodingSession"))
        .and_then(|v| v.as_array())
        .context("failed to query sessions from store")?;

    let mut gap_sessions: Vec<(String, u64, u64, String)> = Vec::new();
    for session in sessions {
        let session_id = session["session_id"].as_str().unwrap_or_default();
        let expected = session["message_count"].as_u64().unwrap_or(0);
        let stored = session["messages"]
            .as_array()
            .map(|a| a.len() as u64)
            .unwrap_or(0);
        let source = session["source"].as_str().unwrap_or("unknown");

        if let Some(ref filter) = session_filter {
            if session_id != filter {
                continue;
            }
        }

        if expected > 0 && stored < expected {
            gap_sessions.push((session_id.to_string(), expected, stored, source.to_string()));
        }
    }

    if gap_sessions.is_empty() {
        tracing::info!("no sessions with message gaps found - nothing to repair");
        return Ok(());
    }

    tracing::info!(
        gaps = gap_sessions.len(),
        "found sessions with message gaps"
    );
    for (session_id, expected, stored, source) in &gap_sessions {
        tracing::info!(
            session_id,
            expected,
            stored,
            missing = expected - stored,
            source,
            "gap detected"
        );
    }

    if dry_run {
        tracing::info!("dry run - not modifying state or re-ingesting");
        return Ok(());
    }

    let source_parser = config.source.build_parser(config.source_dir.clone());
    let base_dir = source_parser.base_dir().to_path_buf();
    let all_files = source_parser.discover_files(&base_dir)?;

    let gap_ids: std::collections::HashSet<&str> =
        gap_sessions.iter().map(|(id, ..)| id.as_str()).collect();

    let mut files_to_reset: Vec<std::path::PathBuf> = Vec::new();
    for path in &all_files {
        let filename = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        let matched = gap_ids.iter().any(|id| filename.contains(id));
        if matched {
            files_to_reset.push(path.clone());
        }
    }

    tracing::info!(
        files = files_to_reset.len(),
        "matched local files for gap sessions"
    );

    let mut state = State::load(config.state_path())?;
    for path in &files_to_reset {
        let old_offset = state.get_offset(path);
        state.set_offset(path.clone(), 0);
        tracing::info!(
            path = %path.display(),
            old_offset,
            "reset file offset to 0"
        );
    }
    for (session_id, ..) in &gap_sessions {
        state.sessions.remove(session_id);
    }
    for path in &files_to_reset {
        state.watch.deferred_files.remove(path);
    }
    state.save()?;

    tracing::info!(
        "starting re-ingest backfill for {} files",
        files_to_reset.len()
    );
    let mut shared = SharedContext::new(
        GraphqlClient::new(config.graphql_endpoint.clone())?,
        false,
        config.batch_size,
        config.source.ingest_label(),
    )
    .with_source_host(config.source_host.clone());
    let state = State::load(config.state_path())?;
    shared.import_session_snapshots(state.session_snapshots().clone());
    let mut backfiller = Backfiller::new(
        source_parser,
        state,
        shared,
        BackfillOptions {
            dry_run: false,
            project_filter: None,
            worker_count: config.backfill_workers,
        },
    );

    let report = backfiller.run().await?;
    tracing::info!(
        files_processed = report.files_processed,
        lines_read = report.lines_read,
        messages_processed = report.messages_processed,
        usage_events_processed = report.usage_events_processed,
        actions_created = report.actions_created,
        sessions_finalized = report.sessions_finalized,
        errors = report.errors,
        "repair backfill complete"
    );

    Ok(())
}
