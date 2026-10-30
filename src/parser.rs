#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TokenUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
    pub cache_read_input_tokens: Option<u64>,
    pub reasoning_output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolAction {
    pub call_id: Option<String>,
    pub action_type: String,
    pub target: String,
    pub command: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSessionMeta {
    pub session_id: String,
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    pub model: Option<String>,
    pub claude_version: Option<String>,
    pub title: Option<String>,
    pub archived: Option<bool>,
    pub git_sha: Option<String>,
    pub git_origin_url: Option<String>,
    pub agent_role: Option<String>,
    pub reasoning_effort: Option<String>,
    pub timestamp: Option<String>,
    /// Session-level total tokens when exposed outside per-message usage
    /// (e.g. Grok `signals.json` contextTokensUsed).
    pub total_tokens: Option<u64>,
    /// Optional tools observed at session scope (e.g. Grok signals.toolsUsed).
    pub tools_used: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedTurnContext {
    pub session_id: Option<String>,
    pub cwd: Option<String>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedUsageEvent {
    pub usage_id: String,
    pub session_id: String,
    pub model: Option<String>,
    pub usage: TokenUsage,
    pub timestamp: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedMessage {
    pub session_id: Option<String>,
    /// Durable source identity written to CodingMessage. Existing source key
    /// versions must remain stable across re-ingest.
    pub message_id: Option<String>,
    /// In-memory identity used to collapse replayed source records without
    /// changing the durable message key.
    pub transient_dedupe_id: Option<String>,
    pub role: Role,
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    pub model: Option<String>,
    pub claude_version: Option<String>,
    pub title: Option<String>,
    pub archived: Option<bool>,
    pub git_sha: Option<String>,
    pub git_origin_url: Option<String>,
    pub agent_role: Option<String>,
    pub reasoning_effort: Option<String>,
    pub content: String,
    /// Model reasoning / chain-of-thought, stored separately from user-visible content.
    pub thinking: Option<String>,
    pub tool_uses: Vec<String>,
    pub files_referenced: Vec<String>,
    pub usage: TokenUsage,
    pub timestamp: Option<String>,
    pub actions: Vec<ToolAction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedEntry {
    SessionMeta(Box<ParsedSessionMeta>),
    TurnContext(Box<ParsedTurnContext>),
    Message(Box<ParsedMessage>),
    Usage(Box<ParsedUsageEvent>),
    Skip,
}

pub fn extract_repo_info(cwd: &str) -> (Option<String>, Option<String>) {
    for marker in ["github.com/", "github-com/"] {
        if let Some(index) = cwd.find(marker) {
            let after = &cwd[index + marker.len()..];
            let mut parts = after.trim_matches('/').split('/');
            let owner = parts.next().filter(|part| !part.is_empty());
            let repo = parts.next().filter(|part| !part.is_empty());
            if let Some(repo_name) = repo {
                return (owner.map(ToOwned::to_owned), Some(repo_name.to_string()));
            }
        }
    }

    let repo_name = cwd
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned);
    (None, repo_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_repo_info_matches_github_paths() {
        let (owner, repo) = extract_repo_info("/home/user/src/github.com/octo-org/example-repo");
        assert_eq!(owner.as_deref(), Some("octo-org"));
        assert_eq!(repo.as_deref(), Some("example-repo"));
    }
}
