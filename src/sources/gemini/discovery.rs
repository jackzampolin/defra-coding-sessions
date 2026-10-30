use super::types::{GeminiChatFile, GeminiLogEntry};
use anyhow::Result;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn discover_chat_files(base_dir: &Path) -> Result<Vec<PathBuf>> {
    let tmp_dir = base_dir.join("tmp");
    if !tmp_dir.exists() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    for entry in fs::read_dir(&tmp_dir)? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                tracing::warn!(error = %error, path = %tmp_dir.display(), "failed to read gemini tmp entry");
                continue;
            }
        };

        let chats_dir = entry.path().join("chats");
        if !chats_dir.is_dir() {
            continue;
        }

        for chat_entry in fs::read_dir(&chats_dir)? {
            let chat_entry = match chat_entry {
                Ok(chat_entry) => chat_entry,
                Err(error) => {
                    tracing::warn!(error = %error, path = %chats_dir.display(), "failed to read gemini chats entry");
                    continue;
                }
            };
            let path = chat_entry.path();
            if path.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("json") {
                files.push(path);
            }
        }
    }

    Ok(files)
}

pub(super) fn discover_log_files(base_dir: &Path) -> Result<Vec<PathBuf>> {
    let tmp_dir = base_dir.join("tmp");
    if !tmp_dir.exists() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    for entry in fs::read_dir(&tmp_dir)? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                tracing::warn!(error = %error, path = %tmp_dir.display(), "failed to read gemini tmp entry");
                continue;
            }
        };

        let logs_path = entry.path().join("logs.json");
        if logs_path.is_file() {
            files.push(logs_path);
        }
    }

    Ok(files)
}

pub(super) fn chat_session_ids(files: &[PathBuf]) -> BTreeSet<String> {
    let mut session_ids = BTreeSet::new();
    for file in files {
        let Ok(contents) = fs::read_to_string(file) else {
            continue;
        };
        let Ok(chat) = serde_json::from_str::<GeminiChatFile>(&contents) else {
            continue;
        };
        if !chat.session_id.trim().is_empty() {
            session_ids.insert(chat.session_id);
        }
    }
    session_ids
}

pub(super) fn log_contains_unmatched_sessions(
    path: &Path,
    chat_sessions: &BTreeSet<String>,
) -> Result<bool> {
    let contents = fs::read_to_string(path)?;
    let entries = serde_json::from_str::<Vec<GeminiLogEntry>>(&contents)?;
    Ok(entries.iter().any(|entry| {
        !entry.session_id.trim().is_empty() && !chat_sessions.contains(&entry.session_id)
    }))
}

pub(super) fn normalize_offset(len: usize, offset: u64) -> usize {
    let offset = offset as usize;
    if len < offset {
        0
    } else {
        offset
    }
}

pub(super) fn is_chat_file(path: &Path) -> bool {
    path.parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        == Some("chats")
}
