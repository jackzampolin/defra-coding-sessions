mod discovery;
mod parse;
mod project;
mod reader;
mod types;

#[cfg(test)]
mod tests;

use crate::parser::ParsedEntry;
use crate::source::{ReadEntriesResult, SourceParser};
use anyhow::Result;
use std::path::{Path, PathBuf};

pub struct GeminiSource {
    base_dir: PathBuf,
}

impl GeminiSource {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }
}

impl SourceParser for GeminiSource {
    fn parse_line(&self, line: &str) -> Result<ParsedEntry> {
        let entry: types::GeminiLogEntry = serde_json::from_str(line)?;
        Ok(parse::parse_log_entry(entry, None))
    }

    fn discover_files(&self, base_dir: &Path) -> Result<Vec<PathBuf>> {
        let chat_files = discovery::discover_chat_files(base_dir)?;
        let chat_sessions = discovery::chat_session_ids(&chat_files);
        let mut files = chat_files;

        for log_file in discovery::discover_log_files(base_dir)? {
            if discovery::log_contains_unmatched_sessions(&log_file, &chat_sessions).unwrap_or(true)
            {
                files.push(log_file);
            }
        }

        files.sort();
        Ok(files)
    }

    fn read_entries_from_offset(&self, path: &Path, offset: u64) -> Result<ReadEntriesResult> {
        if discovery::is_chat_file(path) {
            reader::read_chat_entries(&self.base_dir, path, offset)
        } else {
            reader::read_log_entries(&self.base_dir, path, offset)
        }
    }

    fn source_label(&self) -> &'static str {
        "gemini"
    }

    fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    fn matches_project_filter(&self, path: &Path, base_dir: &Path, filter: &str) -> bool {
        let filter = filter.to_ascii_lowercase();
        if let Ok(Some(root)) = project::project_root_for_path(base_dir, path, None) {
            if root.to_ascii_lowercase().contains(&filter) {
                return true;
            }
        }

        project::gemini_project_key(base_dir, path)
            .unwrap_or_else(|| path.to_string_lossy().to_string())
            .to_ascii_lowercase()
            .contains(&filter)
    }
}
