use super::*;
use rusqlite::Connection;
use std::path::PathBuf;
use tempfile::TempDir;

fn fixture_sqlite_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("codex-home")
        .join("state_5.sqlite")
}

#[test]
fn read_threads_and_classify_subagents() {
    let tempdir = TempDir::new().unwrap();
    let db_path = tempdir.path().join("state_5.sqlite");
    let connection = Connection::open(&db_path).unwrap();
    connection
        .execute_batch(
            r#"
            CREATE TABLE threads (
                id TEXT PRIMARY KEY,
                rollout_path TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                source TEXT NOT NULL,
                model_provider TEXT NOT NULL,
                cwd TEXT NOT NULL,
                title TEXT NOT NULL,
                sandbox_policy TEXT NOT NULL,
                approval_mode TEXT NOT NULL,
                tokens_used INTEGER NOT NULL DEFAULT 0,
                has_user_event INTEGER NOT NULL DEFAULT 0,
                archived INTEGER NOT NULL DEFAULT 0,
                archived_at INTEGER,
                git_sha TEXT,
                git_branch TEXT,
                git_origin_url TEXT,
                cli_version TEXT NOT NULL DEFAULT '',
                first_user_message TEXT NOT NULL DEFAULT '',
                agent_nickname TEXT,
                agent_role TEXT,
                memory_mode TEXT NOT NULL DEFAULT 'enabled',
                model TEXT,
                reasoning_effort TEXT
            );
            "#,
        )
        .unwrap();

    connection
        .execute(
            r#"
            INSERT INTO threads (
                id, rollout_path, created_at, updated_at, source, model_provider, cwd,
                title, sandbox_policy, approval_mode, archived, git_sha, git_branch,
                git_origin_url, cli_version, first_user_message, agent_nickname,
                agent_role, model, reasoning_effort
            ) VALUES (
                ?1, ?2, ?3, ?4, 'cli', 'openai', ?5, ?6, 'danger-full-access',
                'never', ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16
            )
            "#,
            params![
                "session-root",
                "/home/user/.codex/sessions/root.jsonl",
                1774290086i64,
                1774290470i64,
                "/home/user/src/example",
                "Root session",
                0i64,
                "abc123",
                "main",
                "git@example.com:example/repo.git",
                "0.116.0",
                "Implement parser",
                Option::<String>::None,
                Option::<String>::None,
                "gpt-5.4",
                "xhigh",
            ],
        )
        .unwrap();

    connection
        .execute(
            r#"
            INSERT INTO threads (
                id, rollout_path, created_at, updated_at, source, model_provider, cwd,
                title, sandbox_policy, approval_mode, archived, git_sha, git_branch,
                git_origin_url, cli_version, first_user_message, agent_nickname,
                agent_role, model, reasoning_effort
            ) VALUES (
                ?1, ?2, ?3, ?4, '{"subagent":{"thread_spawn":{"parent_thread_id":"session-root"}}}',
                'openai', ?5, ?6, 'danger-full-access', 'never', ?7, NULL, NULL, NULL,
                ?8, ?9, ?10, ?11, ?12, ?13
            )
            "#,
            params![
                "session-subagent",
                "/home/user/.codex/sessions/subagent.jsonl",
                1773426512i64,
                1773427153i64,
                "/home/user/src/example",
                "Subagent session",
                1i64,
                "0.114.0",
                "Investigate issue",
                "Scout",
                "worker",
                Option::<String>::None,
                Option::<String>::None,
            ],
        )
        .unwrap();
    drop(connection);

    let rows = read_threads(&db_path).unwrap();
    assert_eq!(rows.len(), 2);

    let root = rows.iter().find(|row| row.id == "session-root").unwrap();
    assert!(!root.archived);
    assert_eq!(root.git_branch.as_deref(), Some("main"));
    assert_eq!(
        root.created_at.as_deref(),
        Some("2026-03-23T18:21:26+00:00")
    );
    assert!(!root.is_subagent());

    let subagent = rows
        .iter()
        .find(|row| row.id == "session-subagent")
        .unwrap();
    assert!(subagent.archived);
    assert_eq!(subagent.agent_nickname.as_deref(), Some("Scout"));
    assert_eq!(subagent.agent_role.as_deref(), Some("worker"));
    assert!(subagent.is_subagent());

    let fetched = read_thread(&db_path, "session-subagent").unwrap().unwrap();
    assert_eq!(fetched.id, "session-subagent");
    assert!(read_thread(&db_path, "missing").unwrap().is_none());
}

#[test]
fn fixture_catalog_contains_archived_and_subagent_threads() {
    let rows = read_threads(&fixture_sqlite_path()).unwrap();

    assert_eq!(rows.len(), 5);
    assert!(rows.iter().any(|row| row.archived));
    assert!(rows.iter().any(|row| row.is_subagent()));
    assert!(rows
        .iter()
        .any(|row| row.agent_role.as_deref() == Some("worker")));
}
