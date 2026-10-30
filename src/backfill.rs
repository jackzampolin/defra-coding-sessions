use crate::parser::ParsedEntry;
use crate::shared::SharedContext;
use crate::source::SourceParser;
use crate::state::{SessionCheckpoint, State};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

mod io;
mod run;
mod worker;

#[cfg(test)]
mod tests;

pub use io::read_lines_from_offset;

const DEFAULT_BACKFILL_WORKERS: usize = 4;
const WORKER_CHANNEL_CAPACITY: usize = 256;
const STATE_SAVE_INTERVAL_FILES: usize = 10;

#[derive(Debug, Clone)]
pub struct BackfillOptions {
    pub dry_run: bool,
    pub project_filter: Option<String>,
    pub worker_count: usize,
}

impl Default for BackfillOptions {
    fn default() -> Self {
        Self {
            dry_run: false,
            project_filter: None,
            worker_count: DEFAULT_BACKFILL_WORKERS,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct BackfillReport {
    pub dry_run: bool,
    pub files_total: usize,
    pub files_processed: usize,
    pub lines_read: u64,
    pub lines_skipped: u64,
    pub messages_processed: u64,
    pub usage_events_processed: u64,
    pub actions_created: u64,
    pub sessions_finalized: u64,
    pub errors: u64,
}

pub struct Backfiller {
    source: Arc<dyn SourceParser>,
    state: State,
    shared: SharedContext,
    options: BackfillOptions,
}

#[derive(Debug)]
struct BackfillWorker {
    sender: mpsc::Sender<WorkerMessage>,
    handle: tokio::task::JoinHandle<BackfillReport>,
}

#[derive(Debug)]
enum WorkerMessage {
    Process(ParsedEntry),
    Flush {
        response: oneshot::Sender<anyhow::Result<WorkerCheckpoint>>,
    },
    FinalizeSession {
        session_id: String,
        response: oneshot::Sender<anyhow::Result<bool>>,
    },
    Shutdown,
}

#[derive(Debug)]
struct WorkerCheckpoint {
    failures: u64,
    failure_message: Option<String>,
    sessions: HashMap<String, SessionCheckpoint>,
}
