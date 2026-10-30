use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

pub fn discover_jsonl_files(codex_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    collect_jsonl_files(&codex_dir.join("sessions"), &mut files)?;
    collect_archived_files(&codex_dir.join("archived_sessions"), &mut files)?;

    files.sort();
    Ok(files)
}

fn collect_jsonl_files(root: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if !root.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(root)? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                tracing::warn!(error = %error, path = %root.display(), "failed to read session entry");
                continue;
            }
        };

        let path = entry.path();
        if path.is_dir() {
            collect_jsonl_files(&path, files)?;
            continue;
        }

        if is_jsonl_file(&path) {
            files.push(path);
        }
    }

    Ok(())
}

fn collect_archived_files(root: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if !root.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(root)? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                tracing::warn!(error = %error, path = %root.display(), "failed to read archived session entry");
                continue;
            }
        };

        let path = entry.path();
        if is_jsonl_file(&path) {
            files.push(path);
        }
    }

    Ok(())
}

fn is_jsonl_file(path: &Path) -> bool {
    path.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("jsonl")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn fixture_codex_home() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join("codex-home")
    }

    #[test]
    fn discover_jsonl_files_walks_sessions_and_archived_sessions() {
        let tempdir = TempDir::new().unwrap();
        let codex_dir = tempdir.path();

        let nested = codex_dir.join("sessions/2026/03/23");
        let archived = codex_dir.join("archived_sessions");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::create_dir_all(&archived).unwrap();

        let session_a = nested.join("rollout-b.jsonl");
        let session_b = nested.join("rollout-a.jsonl");
        let archived_file = archived.join("archived.jsonl");
        let ignored = nested.join("notes.txt");

        std::fs::write(&session_a, "{}\n").unwrap();
        std::fs::write(&session_b, "{}\n").unwrap();
        std::fs::write(&archived_file, "{}\n").unwrap();
        std::fs::write(&ignored, "ignore\n").unwrap();

        let files = discover_jsonl_files(codex_dir).unwrap();

        assert_eq!(files, vec![archived_file, session_b, session_a]);
    }

    #[test]
    fn non_existent_codex_dir_returns_empty() {
        let missing = TempDir::new().unwrap().path().join("missing-codex-home");
        let files = discover_jsonl_files(&missing).unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn fixture_discovery_includes_archived_and_multifile_rollouts() {
        let files = discover_jsonl_files(&fixture_codex_home()).unwrap();

        assert_eq!(files.len(), 5);
        assert!(files.iter().any(|path| {
            path.components()
                .any(|component| component.as_os_str() == "archived_sessions")
        }));
        assert!(
            files
                .iter()
                .filter(|path| path.to_string_lossy().contains("2026/03/13"))
                .count()
                >= 2
        );
    }
}
