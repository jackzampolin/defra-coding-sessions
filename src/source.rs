use crate::parser::ParsedEntry;
use crate::sources::{ClaudeSource, CodexSource, GeminiSource, GrokSource};
use anyhow::Result;
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    #[default]
    Claude,
    Codex,
    Gemini,
    Grok,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Gemini => "gemini",
            Self::Grok => "grok",
        }
    }

    pub fn default_source_dir(self) -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        match self {
            Self::Claude => home.join(".claude"),
            Self::Codex => home.join(".codex"),
            Self::Gemini => home.join(".gemini"),
            Self::Grok => home.join(".grok"),
        }
    }

    pub fn default_sqlite_path(self) -> Option<PathBuf> {
        match self {
            Self::Codex => Some(self.default_source_dir().join("state_5.sqlite")),
            Self::Claude | Self::Gemini | Self::Grok => None,
        }
    }

    pub fn default_reconcile_interval_secs(self) -> Option<u64> {
        match self {
            Self::Codex => Some(60),
            Self::Claude | Self::Gemini | Self::Grok => None,
        }
    }

    pub fn ingest_label(self) -> &'static str {
        match self {
            Self::Claude => "claude-code",
            Self::Codex => "codex",
            Self::Gemini => "gemini",
            Self::Grok => "grok",
        }
    }

    pub fn build_parser(self, base_dir: PathBuf) -> Arc<dyn SourceParser> {
        match self {
            Self::Claude => Arc::new(ClaudeSource::new(base_dir)),
            Self::Codex => Arc::new(CodexSource::new(base_dir)),
            Self::Gemini => Arc::new(GeminiSource::new(base_dir)),
            Self::Grok => Arc::new(GrokSource::new(base_dir)),
        }
    }
}

pub trait SourceParser: Send + Sync {
    fn parse_line(&self, line: &str) -> Result<ParsedEntry>;
    fn discover_files(&self, base_dir: &Path) -> Result<Vec<PathBuf>>;
    fn source_label(&self) -> &'static str;
    fn base_dir(&self) -> &Path;
    fn read_entries_from_offset(&self, path: &Path, offset: u64) -> Result<ReadEntriesResult> {
        let (lines, new_offset) = crate::backfill::read_lines_from_offset(path, offset)?;
        let entries = lines
            .into_iter()
            .map(|line| self.parse_line(&line))
            .collect::<Vec<_>>();
        Ok(ReadEntriesResult {
            entries,
            new_offset,
        })
    }

    fn matches_project_filter(&self, path: &Path, _base_dir: &Path, filter: &str) -> bool {
        path.to_string_lossy()
            .to_ascii_lowercase()
            .contains(&filter.to_ascii_lowercase())
    }
}

pub struct ReadEntriesResult {
    pub entries: Vec<Result<ParsedEntry>>,
    pub new_offset: u64,
}
