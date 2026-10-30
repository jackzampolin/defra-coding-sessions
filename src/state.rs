use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionCheckpoint {
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
    pub last_touched_epoch_ms: u64,
    pub message_count: u64,
    pub user_message_count: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_creation_input_tokens: u64,
    #[serde(default)]
    pub cache_read_input_tokens: u64,
    #[serde(default)]
    pub reasoning_output_tokens: u64,
    #[serde(default)]
    pub total_tokens: u64,
    pub tools_used: Vec<String>,
    pub first_prompt: Option<String>,
    pub title: Option<String>,
    pub archived: Option<bool>,
    pub git_sha: Option<String>,
    pub agent_role: Option<String>,
    pub reasoning_effort: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WatchHealth {
    #[default]
    Healthy,
    Degraded,
    Flapping,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileFingerprint {
    pub len: u64,
    #[serde(default)]
    pub modified_epoch_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeferredFileCheckpoint {
    pub attempts: u32,
    pub next_retry_epoch_ms: u64,
    pub last_failed_epoch_ms: u64,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub fingerprint: Option<FileFingerprint>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct WatchCheckpoint {
    #[serde(default)]
    pub deferred_files: HashMap<PathBuf, DeferredFileCheckpoint>,
    #[serde(default)]
    pub health: WatchHealth,
    #[serde(default)]
    pub last_scan_started_epoch_ms: Option<u64>,
    #[serde(default)]
    pub last_scan_completed_epoch_ms: Option<u64>,
    #[serde(default)]
    pub last_successful_scan_epoch_ms: Option<u64>,
    #[serde(default)]
    pub consecutive_failed_scans: u32,
    #[serde(default)]
    pub last_scan_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedState {
    offsets: HashMap<PathBuf, u64>,
    sessions: HashMap<String, SessionCheckpoint>,
    #[serde(default)]
    watch: WatchCheckpoint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    pub path: PathBuf,
    pub offsets: HashMap<PathBuf, u64>,
    pub sessions: HashMap<String, SessionCheckpoint>,
    #[serde(default)]
    pub watch: WatchCheckpoint,
}

impl State {
    pub fn load(path: PathBuf) -> Result<Self> {
        if !path.exists() {
            return Ok(Self {
                path,
                offsets: HashMap::new(),
                sessions: HashMap::new(),
                watch: WatchCheckpoint::default(),
            });
        }

        let contents = std::fs::read_to_string(&path)
            .with_context(|| format!("reading ingest state {}", path.display()))?;
        let parsed = serde_json::from_str::<PersistedState>(&contents)
            .with_context(|| format!("parsing ingest state {}", path.display()))?;

        Ok(Self {
            path,
            offsets: parsed.offsets,
            sessions: parsed.sessions,
            watch: parsed.watch,
        })
    }

    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating state directory {}", parent.display()))?;
        }

        let data = serde_json::to_string_pretty(&PersistedState {
            offsets: self.offsets.clone(),
            sessions: self.sessions.clone(),
            watch: self.watch.clone(),
        })?;
        let temp_path = self.temporary_path();
        let mut file = std::fs::File::create(&temp_path)
            .with_context(|| format!("creating temporary ingest state {}", temp_path.display()))?;
        file.write_all(data.as_bytes())
            .with_context(|| format!("writing temporary ingest state {}", temp_path.display()))?;
        file.sync_all()
            .with_context(|| format!("syncing temporary ingest state {}", temp_path.display()))?;
        std::fs::rename(&temp_path, &self.path).with_context(|| {
            format!(
                "renaming temporary ingest state {} to {}",
                temp_path.display(),
                self.path.display()
            )
        })?;
        Ok(())
    }

    pub fn default_path(data_dir: &Path) -> PathBuf {
        data_dir.join("ingest-state.json")
    }

    pub fn get_offset(&self, path: &Path) -> u64 {
        self.offsets.get(path).copied().unwrap_or(0)
    }

    pub fn set_offset(&mut self, path: PathBuf, offset: u64) {
        self.offsets.insert(path, offset);
    }

    pub fn session_snapshots(&self) -> &HashMap<String, SessionCheckpoint> {
        &self.sessions
    }

    pub fn replace_session_snapshots(&mut self, sessions: HashMap<String, SessionCheckpoint>) {
        self.sessions = sessions;
    }

    pub fn clear_session_snapshots(&mut self) {
        self.sessions.clear();
    }

    pub fn watch_checkpoint(&self) -> &WatchCheckpoint {
        &self.watch
    }

    pub fn replace_watch_checkpoint(&mut self, checkpoint: WatchCheckpoint) {
        self.watch = checkpoint;
    }

    fn temporary_path(&self) -> PathBuf {
        let file_name = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("ingest-state.json");
        self.path.with_file_name(format!(".{file_name}.tmp"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn load_rejects_legacy_offsets_only_state() {
        let tempdir = TempDir::new().unwrap();
        let path = tempdir.path().join("state.json");
        std::fs::write(
            &path,
            serde_json::json!({
                "/tmp/a.jsonl": 42
            })
            .to_string(),
        )
        .unwrap();

        let error = State::load(path).unwrap_err();

        assert!(error.to_string().contains("parsing ingest state"));
    }

    #[test]
    fn save_round_trips_sessions_and_offsets() {
        let tempdir = TempDir::new().unwrap();
        let path = tempdir.path().join("state.json");
        let mut state = State::load(path.clone()).unwrap();
        state.set_offset(PathBuf::from("/tmp/a.jsonl"), 42);
        state.replace_session_snapshots(HashMap::from([(
            "session-1".to_string(),
            SessionCheckpoint {
                doc_id: "session-doc-1".to_string(),
                msg_seq: 7,
                model_counts: HashMap::from([("claude".to_string(), 3)]),
                last_ts: Some("2026-03-23T18:21:26Z".to_string()),
                last_touched_epoch_ms: timestamp_epoch_ms("2026-03-23T18:21:26Z"),
                message_count: 3,
                user_message_count: 1,
                input_tokens: 9,
                output_tokens: 5,
                tools_used: vec!["Read".to_string()],
                first_prompt: Some("hello".to_string()),
                ..Default::default()
            },
        )]));
        state.replace_watch_checkpoint(WatchCheckpoint {
            deferred_files: HashMap::from([(
                PathBuf::from("/tmp/bad.jsonl"),
                DeferredFileCheckpoint {
                    attempts: 2,
                    next_retry_epoch_ms: 1234,
                    last_failed_epoch_ms: 1200,
                    last_error: Some("bad file".to_string()),
                    fingerprint: Some(FileFingerprint {
                        len: 99,
                        modified_epoch_ms: Some(777),
                    }),
                },
            )]),
            health: WatchHealth::Degraded,
            last_scan_started_epoch_ms: Some(1000),
            last_scan_completed_epoch_ms: Some(1001),
            last_successful_scan_epoch_ms: Some(999),
            consecutive_failed_scans: 1,
            last_scan_error: Some("bad file".to_string()),
        });

        state.save().unwrap();
        let loaded = State::load(path).unwrap();

        assert_eq!(loaded.offsets, state.offsets);
        assert_eq!(loaded.sessions, state.sessions);
        assert_eq!(loaded.watch, state.watch);
    }

    fn timestamp_epoch_ms(value: &str) -> u64 {
        chrono::DateTime::parse_from_rfc3339(value)
            .unwrap()
            .timestamp_millis() as u64
    }
}
