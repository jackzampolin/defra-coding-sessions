use crate::parser::{TokenUsage, ToolAction};

#[derive(Debug, Clone)]
pub struct SessionMutationInput {
    pub session_id: String,
    pub project_doc_id: String,
    pub git_branch: Option<String>,
    pub source: String,
    pub source_host: Option<String>,
    pub model_primary: Option<String>,
    pub claude_version: Option<String>,
    pub title: Option<String>,
    pub archived: Option<bool>,
    pub git_sha: Option<String>,
    pub git_origin_url: Option<String>,
    pub agent_role: Option<String>,
    pub reasoning_effort: Option<String>,
    pub created_at: Option<String>,
    pub finished_at: Option<String>,
    pub message_count: Option<u64>,
    pub user_message_count: Option<u64>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
    pub cache_read_input_tokens: Option<u64>,
    pub reasoning_output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub tools_used: Vec<String>,
    pub first_prompt: Option<String>,
    pub summary: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MessageMutationInput {
    pub message_id: String,
    pub session_doc_id: String,
    pub sequence: u64,
    pub role: String,
    pub source_host: Option<String>,
    pub model: Option<String>,
    pub content: Option<String>,
    pub thinking: Option<String>,
    pub tool_uses: Vec<String>,
    pub files_referenced: Vec<String>,
    pub usage: TokenUsage,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TokenUsageMutationInput {
    pub usage_id: String,
    pub session_id: String,
    pub source: String,
    pub source_host: String,
    pub model: Option<String>,
    pub usage: TokenUsage,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct ActionMutationInput {
    pub message_doc_id: String,
    pub session_doc_id: String,
    pub action: ToolAction,
    pub tags: Vec<String>,
    pub created_at: Option<String>,
}
