use super::discovery::{chat_session_ids, discover_chat_files, normalize_offset};
use super::parse::{build_chat_session_meta, parse_chat_message, parse_log_entry};
use super::project::project_root_for_path;
use super::types::{GeminiChatFile, GeminiLogEntry};
use crate::parser::ParsedEntry;
use crate::source::ReadEntriesResult;
use anyhow::Result;
use serde_json::Value;
use std::fs;
use std::path::Path;

pub(super) fn read_chat_entries(
    base_dir: &Path,
    path: &Path,
    offset: u64,
) -> Result<ReadEntriesResult> {
    let chat: GeminiChatFile = serde_json::from_str(&fs::read_to_string(path)?)?;
    let message_count = chat.messages.len();
    let start = normalize_offset(message_count, offset);
    let cwd = project_root_for_path(base_dir, path, chat.project_hash.as_deref())?;
    if cwd.is_none() {
        return Ok(ReadEntriesResult {
            entries: Vec::new(),
            new_offset: message_count as u64,
        });
    }
    let mut entries =
        Vec::with_capacity(message_count.saturating_sub(start) + usize::from(start == 0));

    if start == 0 && !chat.session_id.trim().is_empty() {
        entries.push(Ok(build_chat_session_meta(&chat, cwd.clone())));
    }

    for message in chat.messages.into_iter().skip(start) {
        entries.push(Ok(parse_chat_message(
            &chat.session_id,
            message,
            cwd.clone(),
        )));
    }

    Ok(ReadEntriesResult {
        entries,
        new_offset: message_count as u64,
    })
}

pub(super) fn read_log_entries(
    base_dir: &Path,
    path: &Path,
    offset: u64,
) -> Result<ReadEntriesResult> {
    let cwd = project_root_for_path(base_dir, path, None)?;
    let contents = fs::read_to_string(path)?;
    let values = serde_json::from_str::<Vec<Value>>(&contents)?;
    let start = normalize_offset(values.len(), offset);
    if cwd.is_none() {
        return Ok(ReadEntriesResult {
            entries: Vec::new(),
            new_offset: values.len() as u64,
        });
    }
    let chat_sessions = chat_session_ids(&discover_chat_files(base_dir)?);
    let entries = values[start..]
        .iter()
        .map(|value| {
            let entry = serde_json::from_value::<GeminiLogEntry>(value.clone())?;
            if chat_sessions.contains(&entry.session_id) {
                return Ok(ParsedEntry::Skip);
            }
            Ok(parse_log_entry(entry, cwd.clone()))
        })
        .collect::<Vec<Result<ParsedEntry>>>();

    Ok(ReadEntriesResult {
        entries,
        new_offset: values.len() as u64,
    })
}
