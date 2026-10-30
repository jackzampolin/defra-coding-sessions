use super::super::graphql::{execute_with_retry, extract_doc_id, is_already_exists_error};
use super::super::{CachedProject, SharedContext, MAX_MUTATION_RETRIES};
use crate::mutations::upsert_project_mutation;
use crate::parser::extract_repo_info;
use anyhow::{bail, Result};
use std::time::SystemTime;

impl SharedContext {
    pub(in crate::shared) async fn ensure_project(&mut self, cwd: &str) -> Result<String> {
        if let Some(project) = self.project_cache.get_mut(cwd) {
            project.last_touched = SystemTime::now();
            return Ok(project.doc_id.clone());
        }

        if self.dry_run {
            let doc_id = format!("dry-run-project-{cwd}");
            self.project_cache.insert(
                cwd.to_string(),
                CachedProject {
                    doc_id: doc_id.clone(),
                    last_touched: SystemTime::now(),
                },
            );
            return Ok(doc_id);
        }

        let (repo_owner, repo_name) = extract_repo_info(cwd);
        let mutation = upsert_project_mutation(cwd, repo_name.as_deref(), repo_owner.as_deref());
        let response = execute_with_retry(
            &self.client,
            &mutation,
            MAX_MUTATION_RETRIES,
            self.metrics.as_ref(),
        )
        .await;

        let doc_id = if response.has_errors() {
            if is_already_exists_error(&response) {
                self.lookup_project_doc_id(cwd).await?
            } else {
                bail!("upsert_CodingProject failed: {:?}", response.errors);
            }
        } else {
            extract_doc_id(&response.data, "upsert_CodingProject")
                .or_else(|| extract_doc_id(&response.data, "create_CodingProject"))
                .or_else(|| extract_doc_id(&response.data, "update_CodingProject"))
                .or_else(|| extract_doc_id(&response.data, "CodingProject"))
                .unwrap_or_default()
        };

        let doc_id = if doc_id.is_empty() {
            self.lookup_project_doc_id(cwd).await?
        } else {
            doc_id
        };

        self.project_cache.insert(
            cwd.to_string(),
            CachedProject {
                doc_id: doc_id.clone(),
                last_touched: SystemTime::now(),
            },
        );
        Ok(doc_id)
    }
}
