use super::io::worker_index;
use super::{
    BackfillReport, BackfillWorker, WorkerCheckpoint, WorkerMessage, WORKER_CHANNEL_CAPACITY,
};
use crate::shared::{SessionFlushResult, SharedContext};
use crate::state::State;
use anyhow::{anyhow, bail, Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::sync::{mpsc, oneshot};

pub(super) async fn checkpoint_workers(
    workers: &[BackfillWorker],
    state: &mut State,
    pending_offsets: &mut Vec<(PathBuf, u64)>,
    force_save: bool,
) -> Result<()> {
    if pending_offsets.is_empty() && !force_save {
        return Ok(());
    }

    let mut responses = Vec::with_capacity(workers.len());
    for (worker_index, worker) in workers.iter().enumerate() {
        let (response_tx, response_rx) = oneshot::channel();
        worker
            .sender
            .send(WorkerMessage::Flush {
                response: response_tx,
            })
            .await
            .map_err(|_| anyhow!("backfill worker {worker_index} terminated"))?;
        responses.push(response_rx);
    }

    let mut snapshots = HashMap::new();
    let mut failures = 0_u64;
    let mut failure_message = None;
    for response in responses {
        let checkpoint = response
            .await
            .context("waiting for backfill checkpoint response")??;
        failures += checkpoint.failures;
        if failure_message.is_none() {
            failure_message = checkpoint.failure_message;
        }
        snapshots.extend(checkpoint.sessions);
    }

    if failures > 0 {
        let detail = failure_message
            .as_deref()
            .unwrap_or("worker ingest failure before checkpoint");
        bail!("backfill checkpoint aborted after {failures} worker failures: {detail}");
    }

    for (path, offset) in pending_offsets.drain(..) {
        state.set_offset(path, offset);
    }
    state.replace_session_snapshots(snapshots);
    state.save()?;
    Ok(())
}

pub(super) fn spawn_workers(template: &SharedContext, worker_count: usize) -> Vec<BackfillWorker> {
    let batch_size = template.batch_size();
    let dry_run = template.is_dry_run();
    let client = template.client.clone();
    let source_label = template.source_label().to_string();
    let source_host = template.source_host().to_string();
    let snapshots = template.export_session_snapshots();
    let mut worker_snapshots = (0..worker_count)
        .map(|_| HashMap::new())
        .collect::<Vec<_>>();

    for (session_id, checkpoint) in snapshots {
        let worker_index = worker_index(&session_id, worker_count);
        worker_snapshots[worker_index].insert(session_id, checkpoint);
    }

    (0..worker_count)
        .map(|worker_id| {
            let (sender, receiver) = mpsc::channel(WORKER_CHANNEL_CAPACITY);
            let mut shared =
                SharedContext::new(client.clone(), dry_run, batch_size, source_label.clone())
                    .with_source_host(source_host.clone());
            shared.import_session_snapshots(worker_snapshots[worker_id].clone());
            let handle = tokio::spawn(run_worker(worker_id, shared, receiver));
            BackfillWorker { sender, handle }
        })
        .collect()
}

async fn run_worker(
    worker_id: usize,
    mut shared: SharedContext,
    mut receiver: mpsc::Receiver<WorkerMessage>,
) -> BackfillReport {
    let mut report = BackfillReport::default();
    let mut checkpoint_failures = 0_u64;
    let mut checkpoint_failure_message = None;

    while let Some(message) = receiver.recv().await {
        match message {
            WorkerMessage::Process(entry) => match shared.process_entry(entry).await {
                Ok(outcome) => {
                    report.errors += outcome.batch_result.failed as u64;
                    if outcome.batch_result.failed > 0 {
                        note_checkpoint_failure(
                            &mut checkpoint_failures,
                            &mut checkpoint_failure_message,
                            outcome.batch_result.failed as u64,
                            format!(
                                "worker {worker_id} saw {} failed mutations while processing a message",
                                outcome.batch_result.failed
                            ),
                        );
                    }
                    if outcome.usage_events_processed > 0 {
                        report.usage_events_processed += outcome.usage_events_processed;
                    } else if outcome.processed {
                        report.messages_processed += 1;
                        report.actions_created += outcome.action_count as u64;
                    } else if outcome.skipped {
                        report.lines_skipped += 1;
                    }
                }
                Err(error) => {
                    report.errors += 1;
                    note_checkpoint_failure(
                        &mut checkpoint_failures,
                        &mut checkpoint_failure_message,
                        1,
                        format!("worker {worker_id} failed to process message: {error}"),
                    );
                    tracing::warn!(
                        worker_id,
                        error = %error,
                        "backfill worker failed to process message"
                    );
                }
            },
            WorkerMessage::Flush { response } => {
                let checkpoint = match shared.flush_batch().await {
                    Ok(batch_result) => {
                        if batch_result.failed > 0 {
                            note_checkpoint_failure(
                                &mut checkpoint_failures,
                                &mut checkpoint_failure_message,
                                batch_result.failed as u64,
                                format!(
                                    "worker {worker_id} saw {} failed mutations while flushing a checkpoint batch",
                                    batch_result.failed
                                ),
                            );
                        }

                        let checkpoint = WorkerCheckpoint {
                            failures: checkpoint_failures,
                            failure_message: checkpoint_failure_message.take(),
                            sessions: shared.export_session_snapshots(),
                        };
                        checkpoint_failures = 0;
                        Ok(checkpoint)
                    }
                    Err(error) => Err(error),
                };
                let _ = response.send(checkpoint);
            }
            WorkerMessage::FinalizeSession {
                session_id,
                response,
            } => {
                let finalize = shared.finalize_session(&session_id).await.map(
                    |SessionFlushResult {
                         updated,
                         batch_result,
                     }| {
                        report.errors += batch_result.failed as u64;
                        if batch_result.failed > 0 {
                            note_checkpoint_failure(
                                &mut checkpoint_failures,
                                &mut checkpoint_failure_message,
                                batch_result.failed as u64,
                                format!(
                                    "worker {worker_id} saw {} failed mutations while finalizing session {session_id}",
                                    batch_result.failed
                                ),
                            );
                        }
                        if updated {
                            report.sessions_finalized += 1;
                        }
                        updated
                    },
                );

                if let Err(error) = &finalize {
                    report.errors += 1;
                    note_checkpoint_failure(
                        &mut checkpoint_failures,
                        &mut checkpoint_failure_message,
                        1,
                        format!(
                            "worker {worker_id} failed to finalize session {session_id}: {error}"
                        ),
                    );
                    tracing::warn!(
                        worker_id,
                        session_id = %session_id,
                        error = %error,
                        "backfill worker failed to finalize session"
                    );
                }
                let _ = response.send(finalize);
            }
            WorkerMessage::Shutdown => {
                match shared.flush_batch().await {
                    Ok(batch_result) => {
                        report.errors += batch_result.failed as u64;
                    }
                    Err(error) => {
                        report.errors += 1;
                        tracing::warn!(
                            worker_id,
                            error = %error,
                            "backfill worker failed to flush pending batch"
                        );
                    }
                }
                break;
            }
        }
    }

    report
}

fn note_checkpoint_failure(
    failures: &mut u64,
    failure_message: &mut Option<String>,
    count: u64,
    message: String,
) {
    *failures += count;
    if failure_message.is_none() {
        *failure_message = Some(message);
    }
}

pub(super) fn merge_reports(report: &mut BackfillReport, worker_report: BackfillReport) {
    report.lines_skipped += worker_report.lines_skipped;
    report.messages_processed += worker_report.messages_processed;
    report.usage_events_processed += worker_report.usage_events_processed;
    report.actions_created += worker_report.actions_created;
    report.sessions_finalized += worker_report.sessions_finalized;
    report.errors += worker_report.errors;
}
