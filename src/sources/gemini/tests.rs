use super::*;
use crate::parser::ParsedEntry;
use tempfile::TempDir;

#[test]
fn read_chat_entries_uses_project_hash_and_extracts_actions() {
    let tempdir = TempDir::new().unwrap();
    let gemini_dir = tempdir.path().join(".gemini");
    std::fs::create_dir_all(
        gemini_dir
            .join("tmp/f5d0461acf907393d9313d83c92f45827d3d3768f4e1428dd7edee6c1f6cf6aa/chats"),
    )
    .unwrap();
    std::fs::write(
        gemini_dir.join("projects.json"),
        serde_json::json!({
            "projects": {
                "/home/user/src/github.com/octo-org/example-app": "example-app"
            }
        })
        .to_string(),
    )
    .unwrap();
    let chat_path = gemini_dir.join(
        "tmp/f5d0461acf907393d9313d83c92f45827d3d3768f4e1428dd7edee6c1f6cf6aa/chats/session.json",
    );
    std::fs::write(
        &chat_path,
        serde_json::json!({
            "sessionId": "session-1",
            "projectHash": "f5d0461acf907393d9313d83c92f45827d3d3768f4e1428dd7edee6c1f6cf6aa",
            "startTime": "2026-03-17T14:32:14.555Z",
            "summary": "Improve iPhone app UI",
            "messages": [
                {
                    "id": "user-1",
                    "timestamp": "2026-03-17T14:32:14.555Z",
                    "type": "user",
                    "content": [{"text": "hello"}]
                },
                {
                    "id": "assistant-1",
                    "timestamp": "2026-03-17T14:32:22.252Z",
                    "type": "gemini",
                    "content": "",
                    "displayContent": "I will inspect the app.",
                    "model": "gemini-3-pro-preview",
                    "tokens": {"input": 100, "output": 20},
                    "toolCalls": [
                        {
                            "id": "replace-1",
                            "name": "replace",
                            "args": {
                                "file_path": "/home/user/src/github.com/octo-org/example-app/App.swift",
                                "old_string": "old",
                                "new_string": "new"
                            }
                        }
                    ]
                }
            ]
        })
        .to_string(),
    )
    .unwrap();

    let source = GeminiSource::new(gemini_dir);
    let chunk = source.read_entries_from_offset(&chat_path, 0).unwrap();

    assert_eq!(chunk.entries.len(), 3);
    let ParsedEntry::SessionMeta(meta) = chunk.entries[0].as_ref().unwrap() else {
        panic!("expected session meta");
    };
    assert_eq!(
        meta.cwd.as_deref(),
        Some("/home/user/src/github.com/octo-org/example-app")
    );
    assert_eq!(meta.title.as_deref(), Some("Improve iPhone app UI"));

    let ParsedEntry::Message(message) = chunk.entries[2].as_ref().unwrap() else {
        panic!("expected message");
    };
    assert_eq!(
        message.message_id.as_deref(),
        Some("gemini:session-1:assistant-1")
    );
    assert_eq!(message.model.as_deref(), Some("gemini-3-pro-preview"));
    assert_eq!(message.content, "I will inspect the app.");
    assert_eq!(message.tool_uses, vec!["replace"]);
    assert_eq!(
        message.files_referenced,
        vec!["/home/user/src/github.com/octo-org/example-app/App.swift"]
    );
    assert_eq!(message.usage.input_tokens, Some(100));
    assert_eq!(message.usage.output_tokens, Some(20));
    assert_eq!(message.actions.len(), 1);
    assert_eq!(message.actions[0].call_id.as_deref(), Some("replace-1"));
    assert_eq!(message.actions[0].action_type, "replace");
}

#[test]
fn read_log_entries_skip_sessions_that_have_chat_files() {
    let tempdir = TempDir::new().unwrap();
    let gemini_dir = tempdir.path().join(".gemini");
    let project_dir = gemini_dir.join("tmp/example-repo");
    let chats_dir = project_dir.join("chats");
    std::fs::create_dir_all(&chats_dir).unwrap();
    std::fs::write(
        project_dir.join(".project_root"),
        "/home/user/src/github.com/octo-org/example-repo\n",
    )
    .unwrap();
    std::fs::write(
        chats_dir.join("session-main.json"),
        serde_json::json!({
            "sessionId": "session-main",
            "messages": [{
                "id": "assistant-1",
                "timestamp": "2026-03-19T13:32:00Z",
                "type": "gemini",
                "content": "done"
            }]
        })
        .to_string(),
    )
    .unwrap();
    let logs_path = project_dir.join("logs.json");
    std::fs::write(
        &logs_path,
        serde_json::json!([
            {
                "sessionId": "session-main",
                "messageId": 0,
                "type": "user",
                "message": "duplicate"
            },
            {
                "sessionId": "session-log-only",
                "messageId": 1,
                "type": "user",
                "message": "keep me"
            }
        ])
        .to_string(),
    )
    .unwrap();

    let source = GeminiSource::new(gemini_dir);
    let chunk = source.read_entries_from_offset(&logs_path, 0).unwrap();

    assert_eq!(chunk.entries.len(), 2);
    assert!(matches!(
        chunk.entries[0].as_ref().unwrap(),
        ParsedEntry::Skip
    ));
    let ParsedEntry::Message(message) = chunk.entries[1].as_ref().unwrap() else {
        panic!("expected message");
    };
    assert_eq!(message.session_id.as_deref(), Some("session-log-only"));
    assert_eq!(message.content, "keep me");
    assert_eq!(
        message.cwd.as_deref(),
        Some("/home/user/src/github.com/octo-org/example-repo")
    );
}

#[test]
fn discover_files_includes_rootless_tmp_entries_for_checkpointing() {
    let tempdir = TempDir::new().unwrap();
    let gemini_dir = tempdir.path().join(".gemini");
    let orphan_dir = gemini_dir.join("tmp/orphan");
    let chats_dir = orphan_dir.join("chats");
    std::fs::create_dir_all(&chats_dir).unwrap();
    std::fs::write(
        chats_dir.join("session-orphan.json"),
        serde_json::json!({
            "sessionId": "session-orphan",
            "messages": [{
                "id": "assistant-1",
                "timestamp": "2026-03-19T13:32:00Z",
                "type": "gemini",
                "content": "done"
            }]
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        orphan_dir.join("logs.json"),
        serde_json::json!([
            {
                "sessionId": "session-orphan-log",
                "messageId": 1,
                "type": "user",
                "message": "keep me"
            }
        ])
        .to_string(),
    )
    .unwrap();

    let source = GeminiSource::new(gemini_dir.clone());
    let files = source.discover_files(&gemini_dir).unwrap();

    assert_eq!(files.len(), 2);
    assert!(files
        .iter()
        .any(|path| path.ends_with("session-orphan.json")));
    assert!(files.iter().any(|path| path.ends_with("logs.json")));
}
