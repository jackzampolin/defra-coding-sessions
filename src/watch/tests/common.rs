pub(super) use crate::client::GraphqlClient;
pub(super) use crate::shared::SharedContext;
pub(super) use crate::source::Source;
pub(super) use crate::state::{SessionCheckpoint, State, WatchHealth};
pub(super) use crate::test_support::spawn_sequence_server;
pub(super) use crate::watch::{ScanStats, Watcher};
pub(super) use serde_json::json;
pub(super) use std::collections::HashMap;
pub(super) use std::io::Write;
pub(super) use std::time::{Duration, SystemTime};
pub(super) use tempfile::TempDir;

pub(super) fn system_time_epoch_ms(value: SystemTime) -> u64 {
    value
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
