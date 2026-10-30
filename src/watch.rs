mod checkpoint;
mod deferred;
mod fingerprint;
mod scan;
mod stats;

use crate::metrics::IngestMetrics;
use crate::shared::SharedContext;
use crate::source::SourceParser;
use crate::state::{DeferredFileCheckpoint, State};
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

pub use stats::ScanStats;

const WATCH_CACHE_MAX_IDLE: Duration = Duration::from_secs(6 * 60 * 60);
const FAILED_FILE_BACKOFF_BASE: Duration = Duration::from_secs(15);
const FAILED_FILE_BACKOFF_MAX: Duration = Duration::from_secs(5 * 60);
const FAILED_FILE_FLAPPING_THRESHOLD: u32 = 3;
const SCAN_FAILURE_FLAPPING_THRESHOLD: u32 = 3;

pub struct Watcher {
    source: Arc<dyn SourceParser>,
    state: State,
    shared: SharedContext,
    scan_interval: Duration,
    failed_files: HashMap<PathBuf, DeferredFileCheckpoint>,
    metrics: Option<Arc<IngestMetrics>>,
}

impl Watcher {
    pub fn new(
        source: Arc<dyn SourceParser>,
        state: State,
        shared: SharedContext,
        scan_interval_secs: u64,
    ) -> Self {
        let failed_files = state.watch_checkpoint().deferred_files.clone();
        Self {
            source,
            state,
            shared,
            scan_interval: Duration::from_secs(scan_interval_secs),
            failed_files,
            metrics: None,
        }
    }

    pub fn with_metrics(mut self, metrics: Arc<IngestMetrics>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    pub async fn run(&mut self) -> Result<()> {
        tracing::info!(
            source = self.source.source_label(),
            source_dir = %self.source.base_dir().display(),
            scan_interval_secs = self.scan_interval.as_secs(),
            "watch mode started"
        );

        let mut ticker = tokio::time::interval(self.scan_interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            ticker.tick().await;

            match self.scan_once().await {
                Ok(stats) => {
                    if let Some(m) = &self.metrics {
                        m.record_scan_success();
                        m.set_deferred_file_count(self.failed_files.len());
                    }
                    if stats.lines_read > 0 {
                        tracing::info!(
                            files_scanned = stats.files_scanned,
                            lines_read = stats.lines_read,
                            messages_processed = stats.messages_processed,
                            usage_events_processed = stats.usage_events_processed,
                            actions_created = stats.actions_created,
                            sessions_finalized = stats.sessions_finalized,
                            errors = stats.errors,
                            source = self.source.source_label(),
                            "watch scan complete"
                        );
                    }
                }
                Err(error) => {
                    if let Some(m) = &self.metrics {
                        m.record_scan_failure();
                        m.set_deferred_file_count(self.failed_files.len());
                    }
                    tracing::error!(
                        error = %error,
                        source = self.source.source_label(),
                        "watch scan failed"
                    );
                }
            }
        }
    }
}

fn now_epoch_ms() -> u64 {
    fingerprint::epoch_millis_from_system_time(SystemTime::now()).unwrap_or_default()
}

#[cfg(test)]
mod tests;
