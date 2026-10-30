use super::normalize::{string_field_from_map, value_to_plain_string};
use super::types::ToolAction;
use serde_json::{Map, Value};
use std::collections::BTreeSet;

pub(super) fn extract_function_call_action(payload: &Map<String, Value>) -> ToolAction {
    let action_type =
        string_field_from_map(payload, "name").unwrap_or_else(|| "unknown".to_string());
    let arguments = payload
        .get("arguments")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let normalized = serde_json::from_str::<Value>(arguments)
        .unwrap_or_else(|_| Value::String(arguments.to_string()));
    let target = extract_tool_target(&action_type, &normalized).unwrap_or_default();
    let command = serde_json::to_string(&normalized).unwrap_or_else(|_| "\"\"".to_string());

    ToolAction {
        call_id: string_field_from_map(payload, "call_id"),
        action_type,
        target,
        command,
    }
}

pub(super) fn extract_custom_tool_action(payload: &Map<String, Value>) -> ToolAction {
    let input = payload
        .get("input")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let target = extract_patch_file_paths(input)
        .into_iter()
        .next()
        .unwrap_or_default();

    ToolAction {
        call_id: string_field_from_map(payload, "call_id"),
        action_type: string_field_from_map(payload, "name")
            .unwrap_or_else(|| "custom_tool".to_string()),
        target,
        command: serde_json::to_string(input).unwrap_or_else(|_| "\"\"".to_string()),
    }
}

pub(super) fn extract_patch_file_paths(input: &str) -> Vec<String> {
    let mut files = BTreeSet::new();
    for line in input.lines() {
        for prefix in [
            "*** Update File: ",
            "*** Add File: ",
            "*** Delete File: ",
            "*** Move to: ",
        ] {
            if let Some(rest) = line.strip_prefix(prefix) {
                maybe_push_file(&mut files, rest.trim());
            }
        }
    }
    files.into_iter().collect()
}

pub(super) fn maybe_push_action_target(files: &mut BTreeSet<String>, target: &str) {
    maybe_push_file(files, target);
}

pub(super) fn maybe_push_file(files: &mut BTreeSet<String>, candidate: &str) {
    let candidate = candidate.trim();
    if candidate.is_empty() {
        return;
    }

    if candidate.contains('/') || candidate.starts_with("./") || candidate.starts_with("../") {
        files.insert(candidate.to_string());
    }
}

fn extract_tool_target(tool_name: &str, arguments: &Value) -> Option<String> {
    let keys = tool_target_keys(tool_name)?;
    match arguments {
        Value::Object(map) => {
            for key in keys {
                if let Some(value) = map.get(*key) {
                    return Some(value_to_plain_string(value));
                }
            }

            for key in keys {
                if let Some(value) = first_query_value(map, key) {
                    return Some(value);
                }
            }
        }
        Value::String(value) if keys.iter().any(|key| *key == "command" || *key == "cmd") => {
            return Some(value.clone());
        }
        _ => {}
    }

    None
}

fn first_query_value(map: &Map<String, Value>, key: &str) -> Option<String> {
    for collection in [
        "search_query",
        "image_query",
        "open",
        "click",
        "find",
        "screenshot",
    ] {
        let Some(field) = map
            .get(collection)
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(Value::as_object)
            .and_then(|object| object.get(key))
        else {
            continue;
        };
        return Some(value_to_plain_string(field));
    }

    None
}

fn tool_target_keys(tool_name: &str) -> Option<&'static [&'static str]> {
    match tool_name {
        "Read" | "Write" | "Edit" => Some(&["file_path", "path"]),
        "Glob" | "Grep" | "find" => Some(&["pattern"]),
        "Bash" => Some(&["command"]),
        "exec_command" => Some(&["cmd", "command"]),
        "shell" => Some(&["command", "cmd"]),
        "Agent" | "spawn_agent" => Some(&["message", "description"]),
        "send_input" => Some(&["id", "message"]),
        "close_agent" => Some(&["id"]),
        "wait" | "wait_agent" => Some(&["ids", "id"]),
        "WebSearch" | "search_query" | "image_query" => Some(&["q"]),
        "WebFetch" => Some(&["url"]),
        "NotebookEdit" => Some(&["notebook_path"]),
        "Skill" => Some(&["skill"]),
        "view_image" => Some(&["path"]),
        "open" | "click" | "screenshot" => Some(&["ref_id"]),
        "write_stdin" => Some(&["session_id"]),
        "update_plan" => Some(&["explanation"]),
        _ => None,
    }
}
