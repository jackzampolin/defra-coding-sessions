use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
#[cfg(test)]
use rusqlite::{params, OptionalExtension};
use rusqlite::{Connection, OpenFlags};
use std::path::Path;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadRow {
    pub id: String,
    pub rollout_path: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub cwd: Option<String>,
    pub title: Option<String>,
    pub archived: bool,
    pub git_sha: Option<String>,
    pub git_branch: Option<String>,
    pub git_origin_url: Option<String>,
    pub cli_version: Option<String>,
    pub first_user_message: Option<String>,
    pub agent_nickname: Option<String>,
    pub agent_role: Option<String>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
}

#[cfg(test)]
impl ThreadRow {
    pub fn is_subagent(&self) -> bool {
        self.agent_role.is_some() || self.agent_nickname.is_some()
    }
}

pub fn read_threads(path: &Path) -> Result<Vec<ThreadRow>> {
    let connection = open_read_only(path)?;
    let mut statement = connection.prepare(
        r#"
        SELECT
            id,
            rollout_path,
            created_at,
            updated_at,
            cwd,
            title,
            archived,
            git_sha,
            git_branch,
            git_origin_url,
            cli_version,
            first_user_message,
            agent_nickname,
            agent_role,
            model,
            reasoning_effort
        FROM threads
        ORDER BY id ASC
        "#,
    )?;

    let rows = statement
        .query_map([], |row| {
            Ok(ThreadRow {
                id: row.get::<_, String>(0)?,
                rollout_path: normalize_optional(row.get::<_, Option<String>>(1)?),
                created_at: normalize_unix_seconds(row.get::<_, Option<i64>>(2)?),
                updated_at: normalize_unix_seconds(row.get::<_, Option<i64>>(3)?),
                cwd: normalize_optional(row.get::<_, Option<String>>(4)?),
                title: normalize_optional(row.get::<_, Option<String>>(5)?),
                archived: row.get::<_, i64>(6)? != 0,
                git_sha: normalize_optional(row.get::<_, Option<String>>(7)?),
                git_branch: normalize_optional(row.get::<_, Option<String>>(8)?),
                git_origin_url: normalize_optional(row.get::<_, Option<String>>(9)?),
                cli_version: normalize_optional(row.get::<_, Option<String>>(10)?),
                first_user_message: normalize_optional(row.get::<_, Option<String>>(11)?),
                agent_nickname: normalize_optional(row.get::<_, Option<String>>(12)?),
                agent_role: normalize_optional(row.get::<_, Option<String>>(13)?),
                model: normalize_optional(row.get::<_, Option<String>>(14)?),
                reasoning_effort: normalize_optional(row.get::<_, Option<String>>(15)?),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(rows)
}

#[cfg(test)]
pub fn read_thread(path: &Path, session_id: &str) -> Result<Option<ThreadRow>> {
    let connection = open_read_only(path)?;
    let mut statement = connection.prepare(
        r#"
        SELECT
            id,
            rollout_path,
            created_at,
            updated_at,
            cwd,
            title,
            archived,
            git_sha,
            git_branch,
            git_origin_url,
            cli_version,
            first_user_message,
            agent_nickname,
            agent_role,
            model,
            reasoning_effort
        FROM threads
        WHERE id = ?1
        LIMIT 1
        "#,
    )?;

    let row = statement
        .query_row(params![session_id], |row| {
            Ok(ThreadRow {
                id: row.get::<_, String>(0)?,
                rollout_path: normalize_optional(row.get::<_, Option<String>>(1)?),
                created_at: normalize_unix_seconds(row.get::<_, Option<i64>>(2)?),
                updated_at: normalize_unix_seconds(row.get::<_, Option<i64>>(3)?),
                cwd: normalize_optional(row.get::<_, Option<String>>(4)?),
                title: normalize_optional(row.get::<_, Option<String>>(5)?),
                archived: row.get::<_, i64>(6)? != 0,
                git_sha: normalize_optional(row.get::<_, Option<String>>(7)?),
                git_branch: normalize_optional(row.get::<_, Option<String>>(8)?),
                git_origin_url: normalize_optional(row.get::<_, Option<String>>(9)?),
                cli_version: normalize_optional(row.get::<_, Option<String>>(10)?),
                first_user_message: normalize_optional(row.get::<_, Option<String>>(11)?),
                agent_nickname: normalize_optional(row.get::<_, Option<String>>(12)?),
                agent_role: normalize_optional(row.get::<_, Option<String>>(13)?),
                model: normalize_optional(row.get::<_, Option<String>>(14)?),
                reasoning_effort: normalize_optional(row.get::<_, Option<String>>(15)?),
            })
        })
        .optional()
        .context("querying threads table")?;

    Ok(row)
}

fn open_read_only(path: &Path) -> Result<Connection> {
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )
    .with_context(|| format!("opening sqlite catalog {}", path.display()))
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

fn normalize_unix_seconds(value: Option<i64>) -> Option<String> {
    let seconds = value?;
    let timestamp = DateTime::<Utc>::from_timestamp(seconds, 0)?;
    Some(timestamp.to_rfc3339())
}
