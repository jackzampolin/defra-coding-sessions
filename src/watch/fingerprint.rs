use crate::state::FileFingerprint;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn file_fingerprint(path: &Path) -> Option<FileFingerprint> {
    let metadata = std::fs::metadata(path).ok()?;
    Some(FileFingerprint {
        len: metadata.len(),
        modified_epoch_ms: metadata
            .modified()
            .ok()
            .and_then(epoch_millis_from_system_time),
    })
}

pub(super) fn epoch_millis_from_system_time(value: SystemTime) -> Option<u64> {
    value
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
}
