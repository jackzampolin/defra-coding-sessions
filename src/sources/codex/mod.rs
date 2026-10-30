mod dedupe;
mod map;
mod parser;
pub mod reconciler;
mod resume;
mod scanner;
mod sqlite;

#[cfg(test)]
mod tests;

pub use reconciler::Reconciler;

use crate::backfill::read_lines_from_offset;
use crate::parser::ParsedEntry;
use crate::source::{ReadEntriesResult, SourceParser};
use anyhow::Result;
use map::map_entry;
use resume::{read_context_before_offset, ResumeContext};
use scanner::discover_jsonl_files;
use std::path::{Path, PathBuf};

pub struct CodexSource {
    base_dir: PathBuf,
}

impl CodexSource {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }
}

impl SourceParser for CodexSource {
    fn parse_line(&self, line: &str) -> Result<ParsedEntry> {
        let entry = parser::parse_line(line)?;
        Ok(map_entry(entry, &ResumeContext::default()))
    }

    fn discover_files(&self, base_dir: &Path) -> Result<Vec<PathBuf>> {
        discover_jsonl_files(base_dir)
    }

    fn read_entries_from_offset(&self, path: &Path, offset: u64) -> Result<ReadEntriesResult> {
        let (lines, new_offset) = read_lines_from_offset(path, offset)?;
        let (mut resume_context, mut duplicate_filter) = read_context_before_offset(path, offset)?;
        let mut entries = Vec::with_capacity(lines.len());

        for line in lines {
            let entry = match parser::parse_line(&line) {
                Ok(entry) => entry,
                Err(error) => {
                    duplicate_filter.reset();
                    entries.push(Err(error));
                    continue;
                }
            };

            resume_context.observe_source_entry(&entry);
            if duplicate_filter.should_skip(&entry) {
                entries.push(Ok(ParsedEntry::Skip));
                continue;
            }

            let mapped = map_entry(entry, &resume_context);
            resume_context.observe_entry(&mapped);
            entries.push(Ok(mapped));
        }

        Ok(ReadEntriesResult {
            entries,
            new_offset,
        })
    }

    fn source_label(&self) -> &'static str {
        "codex"
    }

    fn base_dir(&self) -> &Path {
        &self.base_dir
    }
}
