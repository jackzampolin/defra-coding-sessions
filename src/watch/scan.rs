use super::{now_epoch_ms, ScanStats, Watcher, WATCH_CACHE_MAX_IDLE};
use crate::parser::ParsedEntry;
use crate::state::State;
use anyhow::{anyhow, Context, Result};
use std::collections::{BTreeSet, HashSet};

impl Watcher {
    pub async fn scan_once(&mut self) -> Result<ScanStats> {
        let scan_started_epoch_ms = now_epoch_ms();
        let mut next_state = self.state.clone();

        match self.scan_once_inner(&mut next_state).await {
            Ok(stats) => {
                self.record_successful_scan(&mut next_state, scan_started_epoch_ms);
                if let Err(error) = self.persist_state_checkpoint(&mut next_state) {
                    let mut failure_state = self.state.clone();
                    self.record_failed_scan(&mut failure_state, scan_started_epoch_ms, &error);
                    self.persist_state_checkpoint(&mut failure_state).with_context(|| {
                        format!("persisting watch failure after successful scan save failed: {error}")
                    })?;
                    return Err(error);
                }
                Ok(stats)
            }
            Err(error) => {
                let mut failure_state = self.state.clone();
                self.record_failed_scan(&mut failure_state, scan_started_epoch_ms, &error);
                self.persist_state_checkpoint(&mut failure_state)
                    .with_context(|| {
                        format!("persisting watch failure after scan error: {error}")
                    })?;
                Err(error)
            }
        }
    }

    async fn scan_once_inner(&mut self, next_state: &mut State) -> Result<ScanStats> {
        let files = self.source.discover_files(self.source.base_dir())?;
        let visible_files = files.iter().cloned().collect::<HashSet<_>>();
        self.failed_files
            .retain(|path, _| visible_files.contains(path));

        let mut stats = ScanStats::default();
        let now_epoch_ms = now_epoch_ms();

        for path in &files {
            if self.should_skip_failed_file(path, now_epoch_ms) {
                continue;
            }

            let saved_offset = next_state.get_offset(path);
            let chunk = match self
                .source
                .read_entries_from_offset(path, saved_offset)
                .with_context(|| format!("reading {}", path.display()))
            {
                Ok(chunk) => chunk,
                Err(error) => {
                    stats.errors += 1;
                    self.defer_failed_file(path, &error);
                    self.reset_shared_from_state(next_state);
                    continue;
                }
            };

            if chunk.entries.is_empty() {
                self.failed_files.remove(path);
                if chunk.new_offset != saved_offset {
                    next_state.set_offset(path.clone(), chunk.new_offset);
                    self.persist_state_checkpoint(next_state)?;
                }
                continue;
            }

            stats.files_scanned += 1;
            stats.lines_read += chunk.entries.len() as u64;
            let mut sessions_in_file = BTreeSet::new();
            let mut file_error = None;

            for entry in chunk.entries {
                match entry {
                    Ok(ParsedEntry::Skip) => {
                        stats.lines_skipped += 1;
                    }
                    Ok(entry) => match self.shared.process_entry(entry).await {
                        Ok(outcome) => {
                            stats.errors += outcome.batch_result.failed as u64;
                            if outcome.batch_result.failed > 0 {
                                file_error = Some(anyhow!(
                                    "watch file aborted after {} failed mutations",
                                    outcome.batch_result.failed
                                ));
                                break;
                            }

                            if outcome.usage_events_processed > 0 {
                                stats.usage_events_processed += outcome.usage_events_processed;
                                if let Some(session_id) = outcome.session_id {
                                    sessions_in_file.insert(session_id);
                                }
                            } else if outcome.processed {
                                stats.messages_processed += 1;
                                stats.actions_created += outcome.action_count as u64;
                                if let Some(session_id) = outcome.session_id {
                                    sessions_in_file.insert(session_id);
                                }
                            } else if outcome.skipped {
                                stats.lines_skipped += 1;
                            } else if let Some(session_id) = outcome.session_id {
                                sessions_in_file.insert(session_id);
                            }
                        }
                        Err(error) => {
                            stats.errors += 1;
                            file_error = Some(error);
                            break;
                        }
                    },
                    Err(error) => {
                        stats.errors += 1;
                        tracing::warn!(
                            error = %error,
                            path = %path.display(),
                            source = self.source.source_label(),
                            "failed to process line"
                        );
                    }
                }
            }

            if let Some(error) = file_error {
                self.defer_failed_file(path, &error);
                self.reset_shared_from_state(next_state);
                continue;
            }

            let batch_result = match self.shared.flush_batch().await {
                Ok(result) => result,
                Err(error) => {
                    self.defer_failed_file(path, &error);
                    self.reset_shared_from_state(next_state);
                    continue;
                }
            };
            stats.errors += batch_result.failed as u64;
            if batch_result.failed > 0 {
                let error = anyhow!(
                    "watch file aborted after {} failed mutations in batch flush",
                    batch_result.failed
                );
                self.defer_failed_file(path, &error);
                self.reset_shared_from_state(next_state);
                continue;
            }

            let mut snapshot_failed = false;
            for session_id in sessions_in_file {
                let snapshot = match self.shared.snapshot_session(&session_id).await {
                    Ok(snapshot) => snapshot,
                    Err(error) => {
                        self.defer_failed_file(path, &error);
                        self.reset_shared_from_state(next_state);
                        snapshot_failed = true;
                        break;
                    }
                };
                stats.errors += snapshot.batch_result.failed as u64;
                if snapshot.batch_result.failed > 0 {
                    let error = anyhow!(
                        "watch file aborted after {} failed mutations while snapshotting session {}",
                        snapshot.batch_result.failed,
                        session_id
                    );
                    self.defer_failed_file(path, &error);
                    self.reset_shared_from_state(next_state);
                    snapshot_failed = true;
                    break;
                }
                if snapshot.updated {
                    stats.sessions_finalized += 1;
                }
            }

            if snapshot_failed {
                continue;
            }

            next_state.set_offset(path.clone(), chunk.new_offset);
            next_state.replace_session_snapshots(self.shared.export_session_snapshots());
            self.failed_files.remove(path);
            self.persist_state_checkpoint(next_state)?;
        }

        let eviction = self.shared.evict_stale(WATCH_CACHE_MAX_IDLE);
        if eviction.sessions_evicted > 0 || eviction.projects_evicted > 0 {
            next_state.replace_session_snapshots(self.shared.export_session_snapshots());
            self.persist_state_checkpoint(next_state)?;
        }

        if eviction.sessions_evicted > 0 || eviction.projects_evicted > 0 {
            tracing::info!(
                sessions_evicted = eviction.sessions_evicted,
                projects_evicted = eviction.projects_evicted,
                max_idle_secs = WATCH_CACHE_MAX_IDLE.as_secs(),
                source = self.source.source_label(),
                "evicted stale watch cache entries"
            );
        }

        Ok(stats)
    }
}
