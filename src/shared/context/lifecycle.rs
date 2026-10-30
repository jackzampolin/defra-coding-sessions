use super::super::checkpoints::{session_checkpoint_from_meta, session_meta_from_checkpoint};
use super::super::{CachedProject, EvictionResult, SessionMeta, SharedContext};
use crate::state::SessionCheckpoint;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

impl SharedContext {
    pub fn new(
        client: crate::client::GraphqlClient,
        dry_run: bool,
        batch_size: usize,
        source_label: impl Into<String>,
    ) -> Self {
        Self {
            client,
            project_cache: HashMap::new(),
            session_cache: HashMap::new(),
            batch: crate::batch::MutationBatch::new(batch_size),
            pending_messages: Vec::new(),
            dry_run,
            source_label: source_label.into(),
            source_host: crate::config::default_source_host(),
            metrics: None,
        }
    }

    pub fn with_source_host(mut self, source_host: impl Into<String>) -> Self {
        let source_host = source_host.into();
        let source_host = source_host.trim();
        if !source_host.is_empty() {
            self.source_host = source_host.to_string();
        }
        self
    }

    pub fn with_metrics(mut self, metrics: std::sync::Arc<crate::metrics::IngestMetrics>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    pub fn import_session_snapshots<I>(&mut self, snapshots: I)
    where
        I: IntoIterator<Item = (String, SessionCheckpoint)>,
    {
        for (session_id, checkpoint) in snapshots {
            self.session_cache
                .insert(session_id, session_meta_from_checkpoint(checkpoint));
        }
    }

    pub fn export_session_snapshots(&self) -> HashMap<String, SessionCheckpoint> {
        self.session_cache
            .iter()
            .map(|(session_id, meta)| (session_id.clone(), session_checkpoint_from_meta(meta)))
            .collect()
    }

    pub fn evict_stale(&mut self, max_idle: Duration) -> EvictionResult {
        let now = SystemTime::now();
        let mut result = EvictionResult::default();

        self.session_cache.retain(|_, meta: &mut SessionMeta| {
            let keep = meta.last_touched.is_none_or(|last_touched| {
                now.duration_since(last_touched).unwrap_or_default() <= max_idle
            });
            if !keep {
                result.sessions_evicted += 1;
            }
            keep
        });

        self.project_cache.retain(|_, project: &mut CachedProject| {
            let keep = now.duration_since(project.last_touched).unwrap_or_default() <= max_idle;
            if !keep {
                result.projects_evicted += 1;
            }
            keep
        });

        result
    }

    pub fn batch_size(&self) -> usize {
        self.batch.batch_size()
    }

    pub fn is_dry_run(&self) -> bool {
        self.dry_run
    }

    pub fn source_label(&self) -> &str {
        &self.source_label
    }

    pub fn source_host(&self) -> &str {
        &self.source_host
    }
}
