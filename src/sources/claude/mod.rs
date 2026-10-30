mod discovery;
mod parse;

#[cfg(test)]
mod tests;

use crate::parser::ParsedEntry;
use crate::source::SourceParser;
use anyhow::Result;
use serde_json::Value;
use std::path::{Path, PathBuf};

pub struct ClaudeSource {
    base_dir: PathBuf,
}

impl ClaudeSource {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }
}

impl SourceParser for ClaudeSource {
    fn parse_line(&self, line: &str) -> Result<ParsedEntry> {
        let value: Value = serde_json::from_str(line)?;
        parse::parse_value(&value)
    }

    fn discover_files(&self, base_dir: &Path) -> Result<Vec<PathBuf>> {
        discovery::discover_files(base_dir)
    }

    fn source_label(&self) -> &'static str {
        "claude"
    }

    fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    fn matches_project_filter(&self, path: &Path, base_dir: &Path, filter: &str) -> bool {
        let filter = filter.to_ascii_lowercase();
        let projects_dir = base_dir.join("projects");
        let project_name = path
            .strip_prefix(&projects_dir)
            .ok()
            .and_then(|relative| relative.components().next())
            .map(|component| component.as_os_str().to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());

        project_name.to_ascii_lowercase().contains(&filter)
    }
}
