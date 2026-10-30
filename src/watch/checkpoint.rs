use super::{
    now_epoch_ms, Watcher, FAILED_FILE_FLAPPING_THRESHOLD, SCAN_FAILURE_FLAPPING_THRESHOLD,
};
use crate::shared::SharedContext;
use crate::state::{State, WatchHealth};
use anyhow::Result;

impl Watcher {
    pub(super) fn reset_shared_from_state(&mut self, state: &State) {
        let mut shared = SharedContext::new(
            self.shared.client.clone(),
            self.shared.is_dry_run(),
            self.shared.batch_size(),
            self.shared.source_label().to_string(),
        );
        if let Some(metrics) = self.shared.metrics.clone() {
            shared = shared.with_metrics(metrics);
        }
        shared.import_session_snapshots(state.session_snapshots().clone());
        self.shared = shared;
    }

    pub(super) fn persist_state_checkpoint(&mut self, state: &mut State) -> Result<()> {
        self.sync_watch_checkpoint(state);
        if let Err(error) = state.save() {
            let persisted_state = self.state.clone();
            self.reset_shared_from_state(&persisted_state);
            return Err(error);
        }
        self.state = state.clone();
        Ok(())
    }

    fn sync_watch_checkpoint(&self, state: &mut State) {
        let mut checkpoint = state.watch_checkpoint().clone();
        checkpoint.deferred_files = self.failed_files.clone();
        checkpoint.health = self.watch_health(checkpoint.consecutive_failed_scans);
        state.replace_watch_checkpoint(checkpoint);
    }

    pub(super) fn record_successful_scan(&self, state: &mut State, scan_started_epoch_ms: u64) {
        let scan_completed_epoch_ms = now_epoch_ms();
        let mut checkpoint = state.watch_checkpoint().clone();
        checkpoint.deferred_files = self.failed_files.clone();
        checkpoint.last_scan_started_epoch_ms = Some(scan_started_epoch_ms);
        checkpoint.last_scan_completed_epoch_ms = Some(scan_completed_epoch_ms);
        checkpoint.last_successful_scan_epoch_ms = Some(scan_completed_epoch_ms);
        checkpoint.consecutive_failed_scans = 0;
        checkpoint.last_scan_error = None;
        checkpoint.health = self.watch_health(0);
        state.replace_watch_checkpoint(checkpoint);
    }

    pub(super) fn record_failed_scan(
        &self,
        state: &mut State,
        scan_started_epoch_ms: u64,
        error: &anyhow::Error,
    ) {
        let scan_completed_epoch_ms = now_epoch_ms();
        let mut checkpoint = state.watch_checkpoint().clone();
        let consecutive_failed_scans = checkpoint.consecutive_failed_scans.saturating_add(1);
        checkpoint.deferred_files = self.failed_files.clone();
        checkpoint.last_scan_started_epoch_ms = Some(scan_started_epoch_ms);
        checkpoint.last_scan_completed_epoch_ms = Some(scan_completed_epoch_ms);
        checkpoint.consecutive_failed_scans = consecutive_failed_scans;
        checkpoint.last_scan_error = Some(error.to_string());
        checkpoint.health = self.watch_health(consecutive_failed_scans);
        state.replace_watch_checkpoint(checkpoint);
    }

    fn watch_health(&self, consecutive_failed_scans: u32) -> WatchHealth {
        let max_attempts = self
            .failed_files
            .values()
            .map(|failed| failed.attempts)
            .max()
            .unwrap_or_default();

        if consecutive_failed_scans >= SCAN_FAILURE_FLAPPING_THRESHOLD
            || max_attempts >= FAILED_FILE_FLAPPING_THRESHOLD
        {
            WatchHealth::Flapping
        } else if consecutive_failed_scans > 0 || !self.failed_files.is_empty() {
            WatchHealth::Degraded
        } else {
            WatchHealth::Healthy
        }
    }
}
