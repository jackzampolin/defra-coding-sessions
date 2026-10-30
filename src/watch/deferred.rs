use super::fingerprint::file_fingerprint;
use super::{now_epoch_ms, Watcher, FAILED_FILE_BACKOFF_BASE, FAILED_FILE_BACKOFF_MAX};
use crate::state::DeferredFileCheckpoint;
use std::path::Path;

impl Watcher {
    pub(super) fn should_skip_failed_file(&mut self, path: &Path, now_epoch_ms: u64) -> bool {
        let fingerprint = file_fingerprint(path);
        let Some(failed) = self.failed_files.get_mut(path) else {
            return false;
        };

        if failed.fingerprint != fingerprint {
            failed.attempts = 0;
            failed.next_retry_epoch_ms = now_epoch_ms;
            failed.fingerprint = fingerprint;
        }

        failed.next_retry_epoch_ms > now_epoch_ms
    }

    pub(super) fn defer_failed_file(&mut self, path: &Path, error: &anyhow::Error) {
        let fingerprint = file_fingerprint(path);
        let attempts = self
            .failed_files
            .get(path)
            .filter(|failed| failed.fingerprint == fingerprint)
            .map_or(1, |failed| failed.attempts.saturating_add(1));
        let shift = attempts.saturating_sub(1).min(31);
        let multiplier = 1_u32.checked_shl(shift).unwrap_or(u32::MAX);
        let backoff = FAILED_FILE_BACKOFF_BASE
            .checked_mul(multiplier)
            .unwrap_or(FAILED_FILE_BACKOFF_MAX)
            .min(FAILED_FILE_BACKOFF_MAX);
        let now_epoch_ms = now_epoch_ms();
        let retry_in_ms = backoff.as_millis().min(u128::from(u64::MAX)) as u64;

        self.failed_files.insert(
            path.to_path_buf(),
            DeferredFileCheckpoint {
                attempts,
                next_retry_epoch_ms: now_epoch_ms.saturating_add(retry_in_ms),
                last_failed_epoch_ms: now_epoch_ms,
                last_error: Some(error.to_string()),
                fingerprint,
            },
        );

        if let Some(m) = &self.metrics {
            m.record_deferred_file(self.failed_files.len());
        }

        tracing::warn!(
            error = %error,
            path = %path.display(),
            source = self.source.source_label(),
            attempt = attempts,
            retry_in_secs = backoff.as_secs(),
            "deferring failed watch file"
        );
    }
}
