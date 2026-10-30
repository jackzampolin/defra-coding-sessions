use crate::parser::ToolAction;
use sha2::{Digest, Sha256};

pub fn deterministic_message_id(session_id: &str, sequence: u64) -> String {
    let digest = Sha256::digest(format!("{session_id}:{sequence}"));
    let hash = format!("{digest:x}");
    let prefix: String = session_id.chars().take(8).collect();
    format!("{prefix}-msg-{}", &hash[..16])
}

pub fn deterministic_action_id(session_id: &str, action: &ToolAction) -> String {
    let call_id = action.call_id.as_deref().unwrap_or_default();
    let digest = Sha256::digest(format!(
        "{session_id}:{call_id}:{}:{}",
        action.action_type, action.command
    ));
    format!("{digest:x}")[..24].to_string()
}
