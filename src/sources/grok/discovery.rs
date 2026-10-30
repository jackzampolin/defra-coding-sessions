use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

/// Discover Grok chat histories and their authoritative usage journals.
///
/// Layout:
/// ```text
/// sessions/<url-encoded-cwd>/<session-uuid>/chat_history.jsonl
/// ```
/// Subagent sessions are sibling dirs under the same cwd (full history of their own).
pub(super) fn discover_session_files(base_dir: &Path) -> Result<Vec<PathBuf>> {
    let sessions_dir = base_dir.join("sessions");
    if !sessions_dir.exists() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    for project_entry in fs::read_dir(&sessions_dir)? {
        let project_entry = match project_entry {
            Ok(entry) => entry,
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    path = %sessions_dir.display(),
                    "failed to read grok sessions project entry"
                );
                continue;
            }
        };

        let project_path = project_entry.path();
        if !project_path.is_dir() {
            continue;
        }

        for session_entry in fs::read_dir(&project_path)? {
            let session_entry = match session_entry {
                Ok(entry) => entry,
                Err(error) => {
                    tracing::warn!(
                        error = %error,
                        path = %project_path.display(),
                        "failed to read grok session entry"
                    );
                    continue;
                }
            };

            let session_path = session_entry.path();
            if !session_path.is_dir() {
                continue;
            }

            let chat_history = session_path.join("chat_history.jsonl");
            if chat_history.is_file() {
                files.push(chat_history);
            }
            let updates = session_path.join("updates.jsonl");
            if updates.is_file() {
                files.push(updates);
            }
        }
    }

    files.sort();
    Ok(files)
}

pub(super) fn is_chat_history_file(path: &Path) -> bool {
    path.file_name().and_then(|name| name.to_str()) == Some("chat_history.jsonl")
}

pub(super) fn is_updates_file(path: &Path) -> bool {
    path.file_name().and_then(|name| name.to_str()) == Some("updates.jsonl")
}

pub(super) fn session_dir_for_chat_history(path: &Path) -> Option<&Path> {
    if !is_chat_history_file(path) {
        return None;
    }
    path.parent()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn discovers_primary_and_subagent_sessions() {
        let temp = TempDir::new().unwrap();
        let project = temp.path().join("sessions").join("%2Ftmp%2Frepo");
        let primary = project.join("session-primary");
        let subagent = project.join("session-subagent");
        fs::create_dir_all(primary.join("subagents/session-subagent")).unwrap();
        fs::create_dir_all(&subagent).unwrap();
        fs::write(primary.join("chat_history.jsonl"), "{}\n").unwrap();
        fs::write(subagent.join("chat_history.jsonl"), "{}\n").unwrap();
        // noise
        fs::write(project.join("prompt_history.jsonl"), "{}\n").unwrap();
        fs::write(primary.join("summary.json"), "{}").unwrap();

        fs::write(primary.join("updates.jsonl"), "{}\n").unwrap();

        let files = discover_session_files(temp.path()).unwrap();
        assert_eq!(files.len(), 3);
        assert_eq!(
            files
                .iter()
                .filter(|path| is_chat_history_file(path))
                .count(),
            2
        );
        assert_eq!(files.iter().filter(|path| is_updates_file(path)).count(), 1);
    }
}
