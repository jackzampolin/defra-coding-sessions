use super::normalize::{
    normalize_timestamp, optional_session_id, string_field_from_map, string_or_json,
};
use super::types::{ParsedEntry, ParsedSessionMeta, ParsedTurnContext};
use anyhow::Result;
use serde_json::Value;

pub(super) fn parse_session_meta(value: &Value) -> Result<ParsedEntry> {
    let Some(payload) = value.get("payload").and_then(Value::as_object) else {
        return Ok(ParsedEntry::Skip);
    };

    let Some(session_id) = string_field_from_map(payload, "id") else {
        return Ok(ParsedEntry::Skip);
    };

    let git = payload.get("git").and_then(Value::as_object);
    let git_sha = git
        .and_then(|git| string_field_from_map(git, "commit_hash"))
        .or_else(|| string_field_from_map(payload, "git_sha"));
    let git_branch = git
        .and_then(|git| string_field_from_map(git, "branch"))
        .or_else(|| string_field_from_map(payload, "git_branch"));
    let git_origin_url = git
        .and_then(|git| string_field_from_map(git, "repository_url"))
        .or_else(|| string_field_from_map(payload, "git_origin_url"));

    Ok(ParsedEntry::SessionMeta(Box::new(ParsedSessionMeta {
        session_id,
        cwd: string_field_from_map(payload, "cwd"),
        source: payload.get("source").and_then(string_or_json),
        cli_version: string_field_from_map(payload, "cli_version"),
        model: string_field_from_map(payload, "model"),
        git_sha,
        git_branch,
        git_origin_url,
        agent_role: string_field_from_map(payload, "agent_role")
            .or_else(|| extract_source_agent_role(payload.get("source"))),
        timestamp: normalize_timestamp(
            payload
                .get("timestamp")
                .and_then(Value::as_str)
                .or_else(|| value.get("timestamp").and_then(Value::as_str)),
        ),
    })))
}

pub(super) fn parse_turn_context(value: &Value) -> Result<ParsedEntry> {
    let Some(payload) = value.get("payload").and_then(Value::as_object) else {
        return Ok(ParsedEntry::Skip);
    };

    Ok(ParsedEntry::TurnContext(Box::new(ParsedTurnContext {
        session_id: optional_session_id(value, payload),
        turn_id: string_field_from_map(payload, "turn_id"),
        cwd: string_field_from_map(payload, "cwd"),
        model: string_field_from_map(payload, "model"),
        effort: string_field_from_map(payload, "effort"),
        timestamp: normalize_timestamp(
            value
                .get("timestamp")
                .and_then(Value::as_str)
                .or_else(|| payload.get("timestamp").and_then(Value::as_str)),
        ),
    })))
}

fn extract_source_agent_role(source: Option<&Value>) -> Option<String> {
    let source = source?.as_object()?;
    source
        .get("subagent")
        .and_then(Value::as_object)
        .and_then(|subagent| subagent.get("thread_spawn"))
        .and_then(Value::as_object)
        .and_then(|spawn| string_field_from_map(spawn, "agent_role"))
}
