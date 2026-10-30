use super::types::GeminiProjectsFile;
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

pub(super) fn gemini_project_key(base_dir: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(base_dir.join("tmp"))
        .ok()
        .and_then(|relative| relative.components().next())
        .map(|component| component.as_os_str().to_string_lossy().to_string())
}

pub(super) fn project_root_for_path(
    base_dir: &Path,
    path: &Path,
    project_hash_hint: Option<&str>,
) -> Result<Option<String>> {
    let key = project_hash_hint
        .map(ToOwned::to_owned)
        .or_else(|| gemini_project_key(base_dir, path));
    let Some(key) = key else {
        return Ok(None);
    };

    if let Some(root) = read_project_root(&base_dir.join("tmp").join(&key).join(".project_root"))? {
        return Ok(Some(root));
    }

    if let Some(root) =
        read_project_root(&base_dir.join("history").join(&key).join(".project_root"))?
    {
        return Ok(Some(root));
    }

    project_root_from_projects_file(base_dir, &key)
}

fn project_root_from_projects_file(base_dir: &Path, key: &str) -> Result<Option<String>> {
    let projects_path = base_dir.join("projects.json");
    if !projects_path.is_file() {
        return Ok(None);
    }

    let projects = serde_json::from_str::<GeminiProjectsFile>(&fs::read_to_string(projects_path)?)?;
    for (root, name) in projects.projects {
        if name == key || project_hash(&root) == key {
            return Ok(Some(root));
        }
    }

    Ok(None)
}

fn read_project_root(path: &Path) -> Result<Option<String>> {
    if !path.is_file() {
        return Ok(None);
    }

    let root = fs::read_to_string(path)?;
    let root = root.trim();
    if root.is_empty() {
        return Ok(None);
    }

    Ok(Some(root.to_string()))
}

fn project_hash(path: &str) -> String {
    let digest = Sha256::digest(path.as_bytes());
    format!("{digest:x}")
}
