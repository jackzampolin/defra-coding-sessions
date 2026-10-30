use super::dedupe::AdjacentUserDuplicateFilter;
use super::map::map_entry;
use super::parser;
use crate::parser::ParsedEntry;
use anyhow::Result;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub(super) struct ResumeContext {
    pub(super) session_id: Option<String>,
    pub(super) turn_id: Option<String>,
    pub(super) cwd: Option<String>,
    pub(super) git_branch: Option<String>,
    pub(super) model: Option<String>,
    pub(super) cli_version: Option<String>,
    pub(super) git_sha: Option<String>,
    pub(super) git_origin_url: Option<String>,
    pub(super) agent_role: Option<String>,
    pub(super) reasoning_effort: Option<String>,
}

impl ResumeContext {
    pub(super) fn observe_source_entry(&mut self, entry: &parser::ParsedEntry) {
        match entry {
            parser::ParsedEntry::SessionMeta(_) => self.turn_id = None,
            parser::ParsedEntry::TurnContext(context) => {
                self.turn_id = context.turn_id.clone();
            }
            parser::ParsedEntry::Message(_) | parser::ParsedEntry::Skip => {}
        }
    }

    pub(super) fn observe_entry(&mut self, entry: &ParsedEntry) {
        match entry {
            ParsedEntry::Skip => {}
            ParsedEntry::Usage(_) => {}
            ParsedEntry::SessionMeta(meta) => {
                self.session_id = Some(meta.session_id.clone());
                if let Some(cwd) = meta.cwd.as_ref() {
                    self.cwd = Some(cwd.clone());
                }
                if let Some(git_branch) = meta.git_branch.as_ref() {
                    self.git_branch = Some(git_branch.clone());
                }
                if let Some(model) = meta.model.as_ref() {
                    self.model = Some(model.clone());
                }
                if let Some(cli_version) = meta.claude_version.as_ref() {
                    self.cli_version = Some(cli_version.clone());
                }
                if let Some(git_sha) = meta.git_sha.as_ref() {
                    self.git_sha = Some(git_sha.clone());
                }
                if let Some(git_origin_url) = meta.git_origin_url.as_ref() {
                    self.git_origin_url = Some(git_origin_url.clone());
                }
                if let Some(agent_role) = meta.agent_role.as_ref() {
                    self.agent_role = Some(agent_role.clone());
                }
            }
            ParsedEntry::TurnContext(context) => {
                if let Some(session_id) = context.session_id.as_ref() {
                    self.session_id = Some(session_id.clone());
                }
                if let Some(cwd) = context.cwd.as_ref() {
                    self.cwd = Some(cwd.clone());
                }
                if let Some(model) = context.model.as_ref() {
                    self.model = Some(model.clone());
                }
                if let Some(reasoning_effort) = context.reasoning_effort.as_ref() {
                    self.reasoning_effort = Some(reasoning_effort.clone());
                }
            }
            ParsedEntry::Message(message) => {
                if let Some(session_id) = message.session_id.as_ref() {
                    self.session_id = Some(session_id.clone());
                }
                if let Some(cwd) = message.cwd.as_ref() {
                    self.cwd = Some(cwd.clone());
                }
                if let Some(git_branch) = message.git_branch.as_ref() {
                    self.git_branch = Some(git_branch.clone());
                }
                if let Some(model) = message.model.as_ref() {
                    self.model = Some(model.clone());
                }
                if let Some(cli_version) = message.claude_version.as_ref() {
                    self.cli_version = Some(cli_version.clone());
                }
                if let Some(git_sha) = message.git_sha.as_ref() {
                    self.git_sha = Some(git_sha.clone());
                }
                if let Some(git_origin_url) = message.git_origin_url.as_ref() {
                    self.git_origin_url = Some(git_origin_url.clone());
                }
                if let Some(agent_role) = message.agent_role.as_ref() {
                    self.agent_role = Some(agent_role.clone());
                }
                if let Some(reasoning_effort) = message.reasoning_effort.as_ref() {
                    self.reasoning_effort = Some(reasoning_effort.clone());
                }
            }
        }
    }
}

pub(super) fn read_context_before_offset(
    path: &Path,
    offset: u64,
) -> Result<(ResumeContext, AdjacentUserDuplicateFilter)> {
    let mut duplicate_filter = AdjacentUserDuplicateFilter::default();

    if offset == 0 {
        return Ok((ResumeContext::default(), duplicate_filter));
    }

    let mut file = std::fs::File::open(path)?;
    let size = file.metadata()?.len();
    if size < offset {
        return Ok((ResumeContext::default(), duplicate_filter));
    }

    file.seek(SeekFrom::Start(0))?;
    let mut reader = BufReader::new(file);
    let mut buffer = String::new();
    let mut resume_context = ResumeContext::default();

    loop {
        let position = reader.stream_position()?;
        if position >= offset {
            break;
        }

        buffer.clear();
        if reader.read_line(&mut buffer)? == 0 {
            break;
        }

        let line = buffer.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            continue;
        }

        let entry = parser::parse_line(line)?;
        resume_context.observe_source_entry(&entry);
        if duplicate_filter.should_skip(&entry) {
            continue;
        }
        let mapped = map_entry(entry, &resume_context);
        resume_context.observe_entry(&mapped);
    }

    Ok((resume_context, duplicate_filter))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn_context(turn_id: Option<&str>) -> parser::ParsedEntry {
        parser::ParsedEntry::TurnContext(Box::new(parser::ParsedTurnContext {
            session_id: Some("session-1".to_owned()),
            turn_id: turn_id.map(str::to_owned),
            cwd: None,
            model: None,
            effort: None,
            timestamp: None,
        }))
    }

    #[test]
    fn turn_context_without_id_clears_stale_turn_identity() {
        let mut context = ResumeContext::default();
        context.observe_source_entry(&turn_context(Some("turn-1")));
        assert_eq!(context.turn_id.as_deref(), Some("turn-1"));

        context.observe_source_entry(&turn_context(None));
        assert_eq!(context.turn_id, None);
    }
}
