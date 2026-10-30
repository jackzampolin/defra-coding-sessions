use std::collections::BTreeSet;

use crate::parser::ToolAction;
use serde_json::Value;

use super::super::types::GrokToolCall;

pub(super) fn extract_tool_uses(tool_calls: &[GrokToolCall]) -> Vec<String> {
    let mut tool_uses = Vec::new();
    for tool_call in tool_calls {
        if !tool_uses.contains(&tool_call.name) {
            tool_uses.push(tool_call.name.clone());
        }
    }
    tool_uses
}

pub(super) fn extract_files_referenced(tool_calls: &[GrokToolCall]) -> Vec<String> {
    let mut files = BTreeSet::new();
    for tool_call in tool_calls {
        let args = parse_tool_args(&tool_call.arguments);
        if let Value::Object(map) = &args {
            for key in [
                "target_file",
                "file_path",
                "path",
                "target_directory",
                "cwd",
            ] {
                if let Some(path) = map.get(key).and_then(Value::as_str) {
                    files.insert(path.to_string());
                }
            }
        }
    }
    files.into_iter().collect()
}

pub(super) fn extract_actions(tool_calls: &[GrokToolCall]) -> Vec<ToolAction> {
    tool_calls
        .iter()
        .map(|tool_call| {
            let args = parse_tool_args(&tool_call.arguments);
            ToolAction {
                call_id: tool_call.id.clone(),
                action_type: tool_call.name.clone(),
                target: extract_tool_target(&tool_call.name, &args).unwrap_or_default(),
                command: serde_json::to_string(&args).unwrap_or_else(|_| "null".to_string()),
            }
        })
        .collect()
}

fn parse_tool_args(arguments: &Value) -> Value {
    match arguments {
        Value::String(raw) => serde_json::from_str(raw).unwrap_or(Value::String(raw.clone())),
        other => other.clone(),
    }
}

fn extract_tool_target(tool_name: &str, args: &Value) -> Option<String> {
    let Value::Object(map) = args else {
        return (!args.is_null()).then(|| value_to_plain_string(args));
    };

    let keys = match tool_name {
        "run_terminal_command" => &["command", "description"][..],
        "read_file" | "write" | "search_replace" | "image_edit" => {
            &["target_file", "file_path", "path"][..]
        }
        "list_dir" => &["target_directory"][..],
        "grep" => &["pattern", "path"][..],
        "web_search" | "web_fetch" | "open_page" | "x_semantic_search" | "x_keyword_search" => {
            &["query", "url"][..]
        }
        "todo_write" => &["todos"][..],
        "spawn_subagent" => &["description", "prompt"][..],
        _ => &[][..],
    };

    for key in keys {
        if let Some(value) = map.get(*key) {
            return Some(value_to_plain_string(value));
        }
    }

    (!map.is_empty()).then(|| Value::Object(map.clone()).to_string())
}

fn value_to_plain_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}
