use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn discover_files(base_dir: &Path) -> Result<Vec<PathBuf>> {
    let projects_dir = base_dir.join("projects");
    if !projects_dir.exists() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    for project_entry in fs::read_dir(&projects_dir)? {
        let project_entry = match project_entry {
            Ok(entry) => entry,
            Err(error) => {
                tracing::warn!(error = %error, "failed to read project entry");
                continue;
            }
        };

        let project_path = project_entry.path();
        if !project_path.is_dir() {
            continue;
        }

        for entry in fs::read_dir(&project_path)? {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    tracing::warn!(
                        error = %error,
                        path = %project_path.display(),
                        "failed to read project child"
                    );
                    continue;
                }
            };
            let path = entry.path();

            if is_jsonl_file(&path) {
                files.push(path);
                continue;
            }

            if !path.is_dir() {
                continue;
            }

            let subagents_dir = path.join("subagents");
            if !subagents_dir.is_dir() {
                continue;
            }

            for subagent_entry in fs::read_dir(&subagents_dir)? {
                let subagent_entry = match subagent_entry {
                    Ok(entry) => entry,
                    Err(error) => {
                        tracing::warn!(
                            error = %error,
                            path = %subagents_dir.display(),
                            "failed to read subagent entry"
                        );
                        continue;
                    }
                };
                let subagent_path = subagent_entry.path();
                if is_jsonl_file(&subagent_path) {
                    files.push(subagent_path);
                }
            }
        }
    }

    files.sort();
    Ok(files)
}

fn is_jsonl_file(path: &Path) -> bool {
    path.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("jsonl")
}
