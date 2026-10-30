use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub(super) struct GrokSummary {
    #[serde(default)]
    pub(super) info: Option<GrokSummaryInfo>,
    #[serde(default)]
    pub(super) session_summary: Option<String>,
    #[serde(default)]
    pub(super) created_at: Option<String>,
    #[serde(default)]
    pub(super) updated_at: Option<String>,
    #[serde(default)]
    pub(super) current_model_id: Option<String>,
    #[serde(default)]
    pub(super) git_remotes: Vec<String>,
    #[serde(default)]
    pub(super) head_commit: Option<String>,
    #[serde(default)]
    pub(super) head_branch: Option<String>,
    #[serde(default)]
    pub(super) generated_title: Option<String>,
    #[serde(default)]
    pub(super) agent_name: Option<String>,
    #[serde(default)]
    pub(super) reasoning_effort: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct GrokSummaryInfo {
    #[serde(default)]
    pub(super) cwd: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct GrokSignals {
    #[serde(default, rename = "contextTokensUsed")]
    pub(super) context_tokens_used: Option<u64>,
    #[serde(default, rename = "toolsUsed")]
    pub(super) tools_used: Vec<String>,
    #[serde(default, rename = "primaryModelId")]
    pub(super) primary_model_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct GrokSubagentMeta {
    #[serde(default)]
    pub(super) subagent_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct GrokChatMessage {
    #[serde(rename = "type")]
    pub(super) message_type: String,
    #[serde(default)]
    pub(super) content: Value,
    #[serde(default)]
    pub(super) synthetic_reason: Option<String>,
    #[serde(default)]
    pub(super) tool_calls: Vec<GrokToolCall>,
    #[serde(default)]
    pub(super) model_id: Option<String>,
    #[serde(default)]
    pub(super) reasoning_effort: Option<String>,
    #[serde(default)]
    pub(super) summary: Value,
    /// Present on `backend_tool_call` rows (built-in web_search etc.).
    #[serde(default)]
    pub(super) kind: Value,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct GrokToolCall {
    #[serde(default)]
    pub(super) id: Option<String>,
    pub(super) name: String,
    #[serde(default)]
    pub(super) arguments: Value,
}
