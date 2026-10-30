use super::super::*;
use serde_json::json;

#[test]
fn parse_session_meta_with_structured_source() {
    let line = json!({
        "timestamp": "2026-03-13T18:28:32.988Z",
        "type": "session_meta",
        "payload": {
            "id": "session-1",
            "timestamp": "2026-03-13T18:28:32.967Z",
            "cwd": "/home/user/src/example",
            "cli_version": "0.114.0",
            "source": {
                "subagent": {
                    "thread_spawn": {
                        "parent_thread_id": "parent-1",
                        "agent_role": "worker"
                    }
                }
            },
            "git": {
                "commit_hash": "abc123",
                "branch": "main",
                "repository_url": "git@example.com:example/repo.git"
            }
        }
    })
    .to_string();

    let ParsedEntry::SessionMeta(meta) = parse_line(&line).unwrap() else {
        panic!("expected session meta");
    };

    assert_eq!(meta.session_id, "session-1");
    assert_eq!(meta.cwd.as_deref(), Some("/home/user/src/example"));
    assert!(meta.source.unwrap().contains("subagent"));
    assert_eq!(meta.git_sha.as_deref(), Some("abc123"));
    assert_eq!(meta.git_branch.as_deref(), Some("main"));
    assert_eq!(
        meta.git_origin_url.as_deref(),
        Some("git@example.com:example/repo.git")
    );
    assert_eq!(meta.agent_role.as_deref(), Some("worker"));
    assert_eq!(meta.timestamp.as_deref(), Some("2026-03-13T18:28:32Z"));
}
