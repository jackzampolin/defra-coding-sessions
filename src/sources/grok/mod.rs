mod discovery;
mod enrichment;
mod parse;
mod project;
mod reader;
mod types;
mod usage;

#[cfg(test)]
mod tests;

use crate::parser::ParsedEntry;
use crate::source::{ReadEntriesResult, SourceParser};
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

pub struct GrokSource {
    base_dir: PathBuf,
}

impl GrokSource {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }
}

impl SourceParser for GrokSource {
    fn parse_line(&self, line: &str) -> Result<ParsedEntry> {
        // Grok primary ingest is path-aware (session id + cwd from filesystem).
        // Line-only parsing is used by generic fallbacks; emit Skip with a clear error
        // only when the line is not valid JSON so callers still get parse failures.
        let value: serde_json::Value = serde_json::from_str(line)?;
        let message_type = value
            .get("type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        if matches!(
            message_type,
            "user" | "assistant" | "reasoning" | "system" | "tool_result"
        ) {
            // Without session context we cannot build stable IDs/cwd; callers that
            // need full fidelity must use read_entries_from_offset.
            bail!("grok parse_line requires session context; use read_entries_from_offset");
        }
        Ok(ParsedEntry::Skip)
    }

    fn discover_files(&self, base_dir: &Path) -> Result<Vec<PathBuf>> {
        discovery::discover_session_files(base_dir)
    }

    fn read_entries_from_offset(&self, path: &Path, offset: u64) -> Result<ReadEntriesResult> {
        if discovery::is_chat_history_file(path) {
            reader::read_chat_history_entries(&self.base_dir, path, offset)
        } else if discovery::is_updates_file(path) {
            usage::read_update_usage_entries(path, offset)
        } else {
            // Unknown file type under grok home — nothing to ingest.
            Ok(ReadEntriesResult {
                entries: Vec::new(),
                new_offset: offset,
            })
        }
    }

    fn source_label(&self) -> &'static str {
        "grok"
    }

    fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    fn matches_project_filter(&self, path: &Path, base_dir: &Path, filter: &str) -> bool {
        let filter = filter.to_ascii_lowercase();
        if let Some(cwd) = project::cwd_from_chat_history_path(base_dir, path) {
            if cwd.to_ascii_lowercase().contains(&filter) {
                return true;
            }
        }
        path.to_string_lossy()
            .to_ascii_lowercase()
            .contains(&filter)
    }
}
