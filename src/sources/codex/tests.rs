use super::*;
use std::io::Write;
use tempfile::TempDir;

#[test]
fn read_entries_restores_session_hint_from_prefix() {
    let tempdir = TempDir::new().unwrap();
    let codex_dir = tempdir.path().join(".codex");
    let sessions_dir = codex_dir.join("sessions/2026/03/25");
    std::fs::create_dir_all(&sessions_dir).unwrap();
    let file = sessions_dir.join("rollout.jsonl");
    let prefix = [
        serde_json::json!({
            "timestamp": "2026-03-25T15:40:49.737Z",
            "type": "session_meta",
            "payload": {
                "id": "session-1",
                "cwd": "/tmp/repo",
                "cli_version": "0.117.0",
                "model": "gpt-5.4",
                "git_branch": "main"
            }
        })
        .to_string(),
        serde_json::json!({
            "timestamp": "2026-03-25T15:40:50.000Z",
            "type": "response_item",
            "payload": {
                "type": "message",
                "role": "assistant",
                "content": [{ "type": "output_text", "text": "first" }]
            }
        })
        .to_string(),
    ]
    .join("\n");
    std::fs::write(&file, format!("{prefix}\n")).unwrap();
    let offset = std::fs::metadata(&file).unwrap().len();
    std::fs::OpenOptions::new()
        .append(true)
        .open(&file)
        .unwrap()
        .write_all(
            format!(
                "{}\n",
                serde_json::json!({
                    "timestamp": "2026-03-25T15:40:51.000Z",
                    "type": "response_item",
                    "payload": {
                        "type": "message",
                        "role": "assistant",
                        "content": [{ "type": "output_text", "text": "second" }]
                    }
                })
            )
            .as_bytes(),
        )
        .unwrap();

    let source = CodexSource::new(codex_dir);
    let chunk = source.read_entries_from_offset(&file, offset).unwrap();

    let ParsedEntry::Message(message) = chunk.entries[0].as_ref().unwrap() else {
        panic!("expected message");
    };
    assert_eq!(message.session_id.as_deref(), Some("session-1"));
    assert_eq!(message.cwd.as_deref(), Some("/tmp/repo"));
    assert_eq!(message.claude_version.as_deref(), Some("0.117.0"));
    assert_eq!(message.model.as_deref(), Some("gpt-5.4"));
    assert_eq!(message.git_branch.as_deref(), Some("main"));
}

#[test]
fn read_entries_skips_paired_user_event_duplicates() {
    let tempdir = TempDir::new().unwrap();
    let codex_dir = tempdir.path().join(".codex");
    let sessions_dir = codex_dir.join("sessions/2026/03/25");
    std::fs::create_dir_all(&sessions_dir).unwrap();
    let file = sessions_dir.join("rollout.jsonl");

    let lines = [
        serde_json::json!({
            "timestamp": "2026-03-25T15:40:49.737Z",
            "type": "session_meta",
            "payload": { "id": "session-1", "cwd": "/tmp/repo" }
        })
        .to_string(),
        serde_json::json!({
            "timestamp": "2026-03-25T15:40:50.100Z",
            "type": "response_item",
            "payload": {
                "type": "message",
                "role": "user",
                "content": [{ "type": "input_text", "text": "same prompt" }]
            }
        })
        .to_string(),
        serde_json::json!({
            "timestamp": "2026-03-25T15:40:50.114Z",
            "type": "event_msg",
            "payload": {
                "type": "user_message",
                "message": "same prompt",
                "images": [],
                "local_images": []
            }
        })
        .to_string(),
    ];
    std::fs::write(&file, format!("{}\n", lines.join("\n"))).unwrap();

    let source = CodexSource::new(codex_dir);
    let chunk = source.read_entries_from_offset(&file, 0).unwrap();

    assert!(matches!(
        chunk.entries[0].as_ref().unwrap(),
        ParsedEntry::SessionMeta(_)
    ));
    let ParsedEntry::Message(message) = chunk.entries[1].as_ref().unwrap() else {
        panic!("expected response_item user message");
    };
    assert_eq!(message.content, "same prompt");
    assert_eq!(message.session_id.as_deref(), Some("session-1"));
    assert!(matches!(
        chunk.entries[2].as_ref().unwrap(),
        ParsedEntry::Skip
    ));
}

#[test]
fn read_entries_skips_user_event_duplicate_across_offset_boundary() {
    let tempdir = TempDir::new().unwrap();
    let codex_dir = tempdir.path().join(".codex");
    let sessions_dir = codex_dir.join("sessions/2026/03/25");
    std::fs::create_dir_all(&sessions_dir).unwrap();
    let file = sessions_dir.join("rollout.jsonl");

    let prefix = [
        serde_json::json!({
            "timestamp": "2026-03-25T15:40:49.737Z",
            "type": "session_meta",
            "payload": { "id": "session-1", "cwd": "/tmp/repo" }
        })
        .to_string(),
        serde_json::json!({
            "timestamp": "2026-03-25T15:40:50.100Z",
            "type": "response_item",
            "payload": {
                "type": "message",
                "role": "user",
                "content": [{ "type": "input_text", "text": "same prompt" }]
            }
        })
        .to_string(),
    ];
    std::fs::write(&file, format!("{}\n", prefix.join("\n"))).unwrap();
    let offset = std::fs::metadata(&file).unwrap().len();
    std::fs::OpenOptions::new()
        .append(true)
        .open(&file)
        .unwrap()
        .write_all(
            format!(
                "{}\n",
                serde_json::json!({
                    "timestamp": "2026-03-25T15:40:50.114Z",
                    "type": "event_msg",
                    "payload": {
                        "type": "user_message",
                        "message": "same prompt",
                        "images": [],
                        "local_images": []
                    }
                })
            )
            .as_bytes(),
        )
        .unwrap();

    let source = CodexSource::new(codex_dir);
    let chunk = source.read_entries_from_offset(&file, offset).unwrap();

    assert_eq!(chunk.entries.len(), 1);
    assert!(matches!(
        chunk.entries[0].as_ref().unwrap(),
        ParsedEntry::Skip
    ));
}

#[test]
fn read_entries_keeps_user_event_with_image_reference() {
    let tempdir = TempDir::new().unwrap();
    let codex_dir = tempdir.path().join(".codex");
    let sessions_dir = codex_dir.join("sessions/2026/03/25");
    std::fs::create_dir_all(&sessions_dir).unwrap();
    let file = sessions_dir.join("rollout.jsonl");

    let lines = [
        serde_json::json!({
            "timestamp": "2026-03-25T15:40:49.737Z",
            "type": "session_meta",
            "payload": { "id": "session-1", "cwd": "/tmp/repo" }
        })
        .to_string(),
        serde_json::json!({
            "timestamp": "2026-03-25T15:40:50.100Z",
            "type": "response_item",
            "payload": {
                "type": "message",
                "role": "user",
                "content": [{ "type": "input_text", "text": "same prompt" }]
            }
        })
        .to_string(),
        serde_json::json!({
            "timestamp": "2026-03-25T15:40:50.114Z",
            "type": "event_msg",
            "payload": {
                "type": "user_message",
                "message": "same prompt",
                "images": [],
                "local_images": ["/tmp/screenshot.png"]
            }
        })
        .to_string(),
    ];
    std::fs::write(&file, format!("{}\n", lines.join("\n"))).unwrap();

    let source = CodexSource::new(codex_dir);
    let chunk = source.read_entries_from_offset(&file, 0).unwrap();

    let ParsedEntry::Message(message) = chunk.entries[2].as_ref().unwrap() else {
        panic!("expected image-bearing user event");
    };
    assert_eq!(message.content, "same prompt");
    assert_eq!(
        message.files_referenced,
        vec!["/tmp/screenshot.png".to_string()]
    );
}

#[test]
fn read_entries_converges_agent_event_and_response_item_ids() {
    let tempdir = TempDir::new().unwrap();
    let codex_dir = tempdir.path().join(".codex");
    let sessions_dir = codex_dir.join("sessions/2026/03/25");
    std::fs::create_dir_all(&sessions_dir).unwrap();
    let file = sessions_dir.join("rollout.jsonl");

    let lines = [
        serde_json::json!({
            "timestamp": "2026-03-25T15:40:49.737Z",
            "type": "session_meta",
            "payload": { "id": "session-1", "cwd": "/tmp/repo" }
        })
        .to_string(),
        serde_json::json!({
            "timestamp": "2026-03-25T15:40:50.000Z",
            "type": "turn_context",
            "payload": { "turn_id": "turn-1", "cwd": "/tmp/repo" }
        })
        .to_string(),
        serde_json::json!({
            "timestamp": "2026-03-25T15:40:51.100Z",
            "type": "event_msg",
            "payload": {
                "type": "agent_message",
                "message": "same assistant update"
            }
        })
        .to_string(),
        serde_json::json!({
            "timestamp": "2026-03-25T15:40:51.114Z",
            "type": "response_item",
            "payload": {
                "type": "message",
                "role": "assistant",
                "content": [{ "type": "output_text", "text": "same assistant update" }],
                "internal_chat_message_metadata_passthrough": {
                    "turn_id": "turn-1"
                }
            }
        })
        .to_string(),
    ];
    std::fs::write(&file, format!("{}\n", lines.join("\n"))).unwrap();

    let source = CodexSource::new(codex_dir);
    let chunk = source.read_entries_from_offset(&file, 0).unwrap();
    let ParsedEntry::Message(event_message) = chunk.entries[2].as_ref().unwrap() else {
        panic!("expected event message");
    };
    let ParsedEntry::Message(response_message) = chunk.entries[3].as_ref().unwrap() else {
        panic!("expected response message");
    };

    assert_eq!(event_message.content, response_message.content);
    assert_ne!(event_message.message_id, response_message.message_id);
    assert_eq!(
        event_message.transient_dedupe_id,
        response_message.transient_dedupe_id
    );
}
