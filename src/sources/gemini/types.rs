use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GeminiLogEntry {
    pub(super) session_id: String,
    #[serde(default)]
    pub(super) message_id: Option<u64>,
    #[serde(rename = "type")]
    pub(super) entry_type: String,
    pub(super) message: String,
    #[serde(default)]
    pub(super) timestamp: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GeminiChatFile {
    pub(super) session_id: String,
    #[serde(default)]
    pub(super) project_hash: Option<String>,
    #[serde(default)]
    pub(super) start_time: Option<String>,
    #[serde(default)]
    pub(super) last_updated: Option<String>,
    #[serde(default)]
    pub(super) kind: Option<String>,
    #[serde(default)]
    pub(super) summary: Option<String>,
    #[serde(default)]
    pub(super) messages: Vec<GeminiChatMessage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GeminiChatMessage {
    #[serde(default)]
    pub(super) id: Option<String>,
    #[serde(default)]
    pub(super) timestamp: Option<String>,
    #[serde(rename = "type")]
    pub(super) message_type: String,
    #[serde(default)]
    pub(super) content: Value,
    #[serde(default)]
    pub(super) display_content: Option<Value>,
    #[serde(default)]
    pub(super) tool_calls: Vec<GeminiToolCall>,
    #[serde(default)]
    pub(super) model: Option<String>,
    #[serde(default)]
    pub(super) tokens: Option<GeminiTokens>,
}

#[derive(Debug, Deserialize)]
pub(super) struct GeminiToolCall {
    #[serde(default)]
    pub(super) id: Option<String>,
    pub(super) name: String,
    #[serde(default)]
    pub(super) args: Value,
}

#[derive(Debug, Deserialize)]
pub(super) struct GeminiTokens {
    #[serde(default)]
    pub(super) input: Option<u64>,
    #[serde(default)]
    pub(super) output: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub(super) struct GeminiProjectsFile {
    #[serde(default)]
    pub(super) projects: HashMap<String, String>,
}
