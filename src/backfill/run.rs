use super::io::{normalize_worker_count, worker_index};
use super::worker::{checkpoint_workers, merge_reports, spawn_workers};
use super::{
    BackfillOptions, BackfillReport, Backfiller, WorkerMessage, STATE_SAVE_INTERVAL_FILES,
};
use crate::parser::ParsedEntry;
use anyhow::{anyhow, Context, Result};
use std::collections::BTreeSet;
use std::sync::Arc;

impl Backfiller {
    pub fn new(
        source: Arc<dyn crate::source::SourceParser>,
        state: crate::state::State,
        shared: crate::shared::SharedContext,
        options: BackfillOptions,
    ) -> Self {
        Self {
            source,
            state,
            shared,
            options,
        }
    }

    pub async fn run(&mut self) -> Result<BackfillReport> {
        let base_dir = self.source.base_dir().to_path_buf();
        let mut files = self.source.discover_files(&base_dir)?;
        if let Some(filter) = self.options.project_filter.as_deref() {
            files.retain(|path| self.source.matches_project_filter(path, &base_dir, filter));
        }

        let worker_count = normalize_worker_count(self.options.worker_count);
        let mut report = BackfillReport {
            dry_run: self.options.dry_run,
            files_total: files.len(),
            ..Default::default()
        };
        let mut workers = spawn_workers(&self.shared, worker_count);
        let result: Result<()> = async {
            let mut sessions_to_finalize = self
                .state
                .session_snapshots()
                .keys()
                .cloned()
                .collect::<BTreeSet<_>>();
            let mut pending_offsets = Vec::with_capacity(files.len());

            tracing::info!(
                workers = worker_count,
                files_total = files.len(),
                source = self.source.source_label(),
                "backfill worker pool started"
            );

            for (index, path) in files.iter().enumerate() {
                let saved_offset = self.state.get_offset(path);
                let source = self.source.clone();
                let path_for_read = path.clone();
                let chunk = tokio::task::spawn_blocking(move || {
                    source.read_entries_from_offset(&path_for_read, saved_offset)
                })
                .await
                .with_context(|| format!("joining read task for {}", path.display()))?
                .with_context(|| format!("reading {}", path.display()))?;

                tracing::info!(
                    file_index = index + 1,
                    total_files = files.len(),
                    offset = saved_offset,
                    lines = chunk.entries.len(),
                    workers = worker_count,
                    path = %path.display(),
                    source = self.source.source_label(),
                    "dispatching file"
                );

                report.files_processed += 1;
                report.lines_read += chunk.entries.len() as u64;

                for entry in chunk.entries {
                    match entry {
                        Ok(ParsedEntry::Skip) => {
                            report.lines_skipped += 1;
                        }
                        Ok(entry) => {
                            let Some(session_id) = entry_session_id(&entry).map(ToOwned::to_owned)
                            else {
                                report.lines_skipped += 1;
                                continue;
                            };
                            let worker_index = worker_index(&session_id, worker_count);
                            workers[worker_index]
                                .sender
                                .send(WorkerMessage::Process(entry))
                                .await
                                .map_err(|_| {
                                    anyhow!("backfill worker {worker_index} terminated")
                                })?;
                            sessions_to_finalize.insert(session_id);
                        }
                        Err(error) => {
                            report.errors += 1;
                            tracing::warn!(
                                error = %error,
                                path = %path.display(),
                                source = self.source.source_label(),
                                "failed to parse line"
                            );
                        }
                    }
                }

                pending_offsets.push((path.clone(), chunk.new_offset));

                if !self.options.dry_run && pending_offsets.len() >= STATE_SAVE_INTERVAL_FILES {
                    checkpoint_workers(&workers, &mut self.state, &mut pending_offsets, false)
                        .await?;
                }
            }

            for session_id in sessions_to_finalize {
                let worker_index = worker_index(&session_id, worker_count);
                let (response_tx, response_rx) = tokio::sync::oneshot::channel();
                workers[worker_index]
                    .sender
                    .send(WorkerMessage::FinalizeSession {
                        session_id,
                        response: response_tx,
                    })
                    .await
                    .map_err(|_| anyhow!("backfill worker {worker_index} terminated"))?;

                let _ = response_rx
                    .await
                    .context("waiting for backfill finalize response")??;
            }

            if !self.options.dry_run {
                checkpoint_workers(&workers, &mut self.state, &mut pending_offsets, true).await?;
            }

            Ok(())
        }
        .await;

        for worker in &workers {
            let _ = worker.sender.send(WorkerMessage::Shutdown).await;
        }

        for worker in workers.drain(..) {
            let worker_report = worker
                .handle
                .await
                .context("joining backfill worker task")?;
            if result.is_ok() {
                merge_reports(&mut report, worker_report);
            }
        }

        result?;
        Ok(report)
    }
}

fn entry_session_id(entry: &ParsedEntry) -> Option<&str> {
    match entry {
        ParsedEntry::Skip => None,
        ParsedEntry::SessionMeta(meta) => Some(meta.session_id.as_str()),
        ParsedEntry::TurnContext(context) => context.session_id.as_deref(),
        ParsedEntry::Message(message) => message.session_id.as_deref(),
        ParsedEntry::Usage(event) => Some(event.session_id.as_str()),
    }
}
