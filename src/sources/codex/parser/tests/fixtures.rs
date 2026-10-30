use super::super::super::scanner::discover_jsonl_files;
use super::super::*;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

fn fixture_codex_home() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("codex-home")
}

#[test]
fn fixture_corpus_parses_and_preserves_multifile_session_ids() {
    let root = fixture_codex_home();
    let mut session_meta_counts = HashMap::<String, usize>::new();
    let mut saw_exec_command_target = false;
    let mut saw_old_reasoning = false;
    let mut saw_new_reasoning = false;

    for path in discover_jsonl_files(&root).unwrap() {
        let contents = fs::read_to_string(&path).unwrap();
        for line in contents.lines() {
            if line.contains(r#""type":"agent_reasoning""#) {
                saw_old_reasoning = true;
            }
            if line.contains(r#""type":"reasoning""#) {
                saw_new_reasoning = true;
            }
            let parsed = parse_line(line).unwrap();
            match parsed {
                ParsedEntry::SessionMeta(meta) => {
                    *session_meta_counts
                        .entry(meta.session_id.clone())
                        .or_default() += 1;
                }
                ParsedEntry::Message(message) => {
                    for action in &message.actions {
                        if action.action_type == "exec_command" || action.action_type == "shell" {
                            assert!(!action.target.is_empty());
                            saw_exec_command_target = true;
                        }
                    }
                }
                ParsedEntry::TurnContext(_) | ParsedEntry::Skip => {}
            }
        }
    }

    assert!(session_meta_counts.values().any(|count| *count > 1));
    assert!(saw_exec_command_target);
    assert!(saw_old_reasoning);
    assert!(saw_new_reasoning);
}
