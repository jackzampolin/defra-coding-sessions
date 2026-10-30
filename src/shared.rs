use crate::batch::MutationBatch;
use crate::client::GraphqlClient;
use crate::metrics::IngestMetrics;
use crate::mutations::ActionMutationInput;
use crate::parser::ParsedMessage;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;
use std::time::SystemTime;

mod checkpoints;
mod context;
mod graphql;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Default)]
pub struct SessionMeta {
    pub doc_id: String,
    pub project_doc_id: Option<String>,
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    pub git_origin_url: Option<String>,
    pub current_model: Option<String>,
    pub claude_version: Option<String>,
    pub msg_seq: u64,
    pub model_counts: HashMap<String, u64>,
    pub created_at: Option<String>,
    pub last_ts: Option<String>,
    pub message_count: u64,
    pub user_message_count: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub reasoning_output_tokens: u64,
    pub total_tokens: u64,
    pub tools_used: BTreeSet<String>,
    pub first_prompt: Option<String>,
    pub title: Option<String>,
    pub archived: Option<bool>,
    pub git_sha: Option<String>,
    pub agent_role: Option<String>,
    pub reasoning_effort: Option<String>,
    pub last_touched: Option<SystemTime>,
    /// Durable message ids and transient parser dedupe ids already queued this
    /// run. Codex durable ids hash the raw JSONL line, so byte-identical repeats
    /// collide; transient ids suppress repeated source records that lack a
    /// durable identity. Queueing either again would burn a sequence slot and
    /// can create a duplicate `message_id` row for the store to arbitrate. Not
    /// persisted: cross-run duplicates are handled by the store's unique-index
    /// "already exists" response.
    pub seen_message_ids: HashSet<String>,
}

#[derive(Debug, Clone)]
pub struct CachedProject {
    pub doc_id: String,
    pub last_touched: SystemTime,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BatchResult {
    pub created: usize,
    pub skipped: usize,
    pub failed: usize,
}

impl BatchResult {
    fn merge(&mut self, other: Self) {
        self.created += other.created;
        self.skipped += other.skipped;
        self.failed += other.failed;
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionFlushResult {
    pub updated: bool,
    pub batch_result: BatchResult,
}

#[derive(Debug, Clone, Default)]
pub struct ProcessLineResult {
    pub session_id: Option<String>,
    pub processed: bool,
    pub skipped: bool,
    pub action_count: usize,
    pub usage_events_processed: u64,
    pub batch_result: BatchResult,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EvictionResult {
    pub sessions_evicted: usize,
    pub projects_evicted: usize,
}

pub struct SharedContext {
    pub client: GraphqlClient,
    pub project_cache: HashMap<String, CachedProject>,
    pub session_cache: HashMap<String, SessionMeta>,
    batch: MutationBatch,
    pending_messages: Vec<PendingMessage>,
    dry_run: bool,
    source_label: String,
    source_host: String,
    pub(crate) metrics: Option<Arc<IngestMetrics>>,
}

const MAX_MUTATION_RETRIES: u32 = 3;
const INITIAL_RETRY_BACKOFF_MS: u64 = 50;

#[derive(Debug, Clone)]
struct PendingMessage {
    session_id: String,
    session_doc_id: String,
    message_id: String,
    sequence: u64,
    message: ParsedMessage,
}

#[derive(Debug, Clone)]
struct PendingAction {
    session_id: String,
    message_id: String,
    action_id_tag: String,
    mutation: ActionMutationInput,
}
