use anyhow::Result;
use coding_data::backfill::{BackfillOptions, Backfiller};
use coding_data::client::GraphqlClient;
use coding_data::config::Config;
use coding_data::shared::SharedContext;
use coding_data::source::Source;
use coding_data::sources::CodexReconciler;
use coding_data::state::State;

pub(super) async fn run(config: Config, project: Option<String>, dry_run: bool) -> Result<()> {
    let client = GraphqlClient::new(config.graphql_endpoint.clone())?;
    let state = State::load(config.state_path())?;
    let mut shared = SharedContext::new(
        client,
        dry_run,
        config.batch_size,
        config.source.ingest_label(),
    )
    .with_source_host(config.source_host.clone());
    shared.import_session_snapshots(state.session_snapshots().clone());
    let source = config.source.build_parser(config.source_dir.clone());
    let mut backfiller = Backfiller::new(
        source,
        state,
        shared,
        BackfillOptions {
            dry_run,
            project_filter: project,
            worker_count: config.backfill_workers,
        },
    );

    let report = backfiller.run().await?;
    let sqlite_threads_reconciled = if config.source == Source::Codex {
        if let (Some(sqlite_path), Some(reconcile_interval_secs)) =
            (config.sqlite_path.clone(), config.reconcile_interval_secs)
        {
            CodexReconciler::new(
                GraphqlClient::new(config.graphql_endpoint.clone())?,
                sqlite_path,
                reconcile_interval_secs,
                dry_run,
            )
            .reconcile_once()
            .await?
            .sessions_updated
        } else {
            0
        }
    } else {
        0
    };

    if config.source == Source::Codex {
        tracing::info!(
            source = config.source.as_str(),
            files_total = report.files_total,
            lines_read = report.lines_read,
            messages_processed = report.messages_processed,
            usage_events_processed = report.usage_events_processed,
            actions_created = report.actions_created,
            sessions_finalized = report.sessions_finalized,
            sqlite_threads_reconciled,
            dry_run = report.dry_run,
            "backfill complete"
        );
    } else {
        tracing::info!(
            source = config.source.as_str(),
            files_total = report.files_total,
            lines_read = report.lines_read,
            messages_processed = report.messages_processed,
            usage_events_processed = report.usage_events_processed,
            actions_created = report.actions_created,
            sessions_finalized = report.sessions_finalized,
            dry_run = report.dry_run,
            "backfill complete"
        );
    }
    Ok(())
}
