#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TokenUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
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
    pub source: Option<String>,
    pub cli_version: Option<String>,
    pub model: Option<String>,
    pub git_sha: Option<String>,
    pub git_branch: Option<String>,
    pub git_origin_url: Option<String>,
    pub agent_role: Option<String>,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedTurnContext {
    pub session_id: Option<String>,
    pub turn_id: Option<String>,
    pub cwd: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedMessage {
    pub session_id: Option<String>,
    pub role: Role,
    pub content: String,
    pub thinking: Option<String>,
    pub tool_uses: Vec<String>,
    pub files_referenced: Vec<String>,
    pub usage: TokenUsage,
    pub timestamp: Option<String>,
    pub actions: Vec<ToolAction>,
    pub raw_line: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedEntry {
    SessionMeta(Box<ParsedSessionMeta>),
    TurnContext(Box<ParsedTurnContext>),
    Message(Box<ParsedMessage>),
    Skip,
}
