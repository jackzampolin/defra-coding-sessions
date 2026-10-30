use super::discovery::session_dir_for_chat_history;
use super::enrichment::{GrokEnrichmentIndex, MessageEnrichment};
use super::parse::{build_session_meta, flatten_content, parse_chat_message, SessionStamp};
use super::project::{cwd_from_chat_history_path, session_id_from_chat_history_path};
use super::types::{GrokChatMessage, GrokSignals, GrokSubagentMeta, GrokSummary};
use crate::backfill::read_lines_from_offset;
use crate::source::ReadEntriesResult;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

pub(super) fn read_chat_history_entries(
    base_dir: &Path,
    path: &Path,
    offset: u64,
) -> Result<ReadEntriesResult> {
    let session_dir = session_dir_for_chat_history(path)
        .with_context(|| format!("chat_history path has no session dir: {}", path.display()))?;
    let session_id = session_id_from_chat_history_path(path)
        .with_context(|| format!("unable to derive grok session id from {}", path.display()))?;

    let path_cwd = cwd_from_chat_history_path(base_dir, path);
    let summary = load_summary(session_dir);
    let signals = load_signals(session_dir);
    let subagent = load_subagent_meta(session_dir, &session_id);
    let cli_version = load_cli_version(base_dir);
    let enrichments = load_enrichment_index(session_dir);
    let matched_users = enrichments.match_user_lines(&chat_user_lines(path));

    let (lines, new_offset) = read_lines_from_offset(path, offset)?;
    let start_line_index = if offset == 0 {
        0
    } else {
        count_complete_lines_before(path, offset).unwrap_or(0)
    };

    let cwd = summary
        .as_ref()
        .and_then(|summary| summary.info.as_ref())
        .and_then(|info| info.cwd.clone())
        .or_else(|| path_cwd.clone());

    let stamp = SessionStamp {
        cwd: cwd.clone(),
        git_branch: summary
            .as_ref()
            .and_then(|s| s.head_branch.clone())
            .filter(|s| !s.is_empty()),
        git_sha: summary
            .as_ref()
            .and_then(|s| s.head_commit.clone())
            .filter(|s| !s.is_empty()),
        git_origin_url: summary
            .as_ref()
            .and_then(|s| s.git_remotes.iter().find(|r| !r.trim().is_empty()).cloned()),
        model: summary
            .as_ref()
            .and_then(|s| s.current_model_id.clone())
            .or_else(|| signals.as_ref().and_then(|s| s.primary_model_id.clone()))
            .filter(|s| !s.is_empty()),
        claude_version: cli_version.clone(),
        agent_role: subagent
            .as_ref()
            .and_then(|m| m.subagent_type.clone())
            .or_else(|| summary.as_ref().and_then(|s| s.agent_name.clone()))
            .filter(|s| !s.is_empty()),
        reasoning_effort: summary
            .as_ref()
            .and_then(|s| s.reasoning_effort.clone())
            .filter(|s| !s.is_empty()),
    };

    let mut entries = Vec::new();
    if offset == 0 {
        entries.push(Ok(build_session_meta(
            &session_id,
            summary.as_ref(),
            signals.as_ref(),
            subagent.as_ref(),
            cwd.clone(),
            cli_version,
        )));
    }

    let mut pending_thinking = None;
    let mut current_turn_enrichment: Option<MessageEnrichment> = matched_users
        .iter()
        .filter(|(line_index, _)| **line_index < start_line_index)
        .max_by_key(|(line_index, _)| **line_index)
        .map(|(_, enrichment)| enrichment.clone());

    for (relative_index, line) in lines.into_iter().enumerate() {
        let line_index = start_line_index + relative_index as u64;
        let message: GrokChatMessage = match serde_json::from_str(&line) {
            Ok(message) => message,
            Err(error) => {
                entries.push(Err(error.into()));
                continue;
            }
        };

        let enrichment = match message.message_type.as_str() {
            "user" if message.synthetic_reason.is_none() => {
                current_turn_enrichment = matched_users.get(&line_index).cloned();
                current_turn_enrichment.clone()
            }
            "assistant" => {
                let tool_enrichment = enrichments.for_tool_calls(
                    message
                        .tool_calls
                        .iter()
                        .filter_map(|tool_call| tool_call.id.as_deref()),
                );
                if let Some(tool_enrichment) = tool_enrichment {
                    current_turn_enrichment = Some(tool_enrichment.clone());
                    Some(tool_enrichment)
                } else {
                    enrichments
                        .for_prompt(
                            current_turn_enrichment
                                .as_ref()
                                .and_then(|value| value.prompt_id.as_deref()),
                        )
                        .or_else(|| current_turn_enrichment.clone())
                }
            }
            "backend_tool_call" => current_turn_enrichment.clone(),
            _ => None,
        };

        entries.push(Ok(parse_chat_message(
            &session_id,
            line_index,
            message,
            &stamp,
            &mut pending_thinking,
            enrichment.as_ref(),
        )));
    }

    Ok(ReadEntriesResult {
        entries,
        new_offset,
    })
}

fn load_summary(session_dir: &Path) -> Option<GrokSummary> {
    let path = session_dir.join("summary.json");
    let contents = fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

fn load_signals(session_dir: &Path) -> Option<GrokSignals> {
    let path = session_dir.join("signals.json");
    let contents = fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

fn load_cli_version(base_dir: &Path) -> Option<String> {
    let path = base_dir.join("version.json");
    let contents = fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&contents).ok()?;
    value
        .get("version")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
}

fn load_enrichment_index(session_dir: &Path) -> GrokEnrichmentIndex {
    let path = session_dir.join("updates.jsonl");
    let Ok(contents) = fs::read_to_string(path) else {
        return GrokEnrichmentIndex::default();
    };
    GrokEnrichmentIndex::parse(&contents)
}

fn chat_user_lines(path: &Path) -> Vec<(u64, String)> {
    use std::io::{BufRead, BufReader};

    let Ok(file) = fs::File::open(path) else {
        return Vec::new();
    };
    BufReader::new(file)
        .lines()
        .enumerate()
        .filter_map(|(line_index, line)| {
            let line = line.ok()?;
            let message = serde_json::from_str::<GrokChatMessage>(&line).ok()?;
            (message.message_type == "user" && message.synthetic_reason.is_none())
                .then(|| (line_index as u64, flatten_content(&message.content)))
        })
        .collect()
}

/// Subagent meta lives on the parent session: `subagents/<child-id>/meta.json`.
fn load_subagent_meta(session_dir: &Path, session_id: &str) -> Option<GrokSubagentMeta> {
    let project_dir = session_dir.parent()?;
    for entry in fs::read_dir(project_dir).ok()? {
        let entry = entry.ok()?;
        let meta_path = entry
            .path()
            .join("subagents")
            .join(session_id)
            .join("meta.json");
        if meta_path.is_file() {
            let contents = fs::read_to_string(meta_path).ok()?;
            return serde_json::from_str(&contents).ok();
        }
    }
    None
}

fn count_complete_lines_before(path: &Path, offset: u64) -> Result<u64> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = fs::File::open(path)?;
    let size = file.metadata()?.len().min(offset);
    let mut buf = vec![0u8; size as usize];
    file.seek(SeekFrom::Start(0))?;
    file.read_exact(&mut buf)?;
    Ok(buf.iter().filter(|byte| **byte == b'\n').count() as u64)
}
