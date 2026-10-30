use super::super::graphql::{ensure_success, extract_doc_id};
use super::super::{SharedContext, INITIAL_RETRY_BACKOFF_MS, MAX_MUTATION_RETRIES};
use crate::mutations::escape_graphql;
use anyhow::{bail, Context, Result};
use std::time::Duration;

impl SharedContext {
    pub(super) async fn lookup_project_doc_id(&self, cwd: &str) -> Result<String> {
        let query = format!(
            r#"query {{
  CodingProject(
    filter: {{ path: {{ _eq: "{}" }} }},
    limit: 1
  ) {{ _docID }}
}}"#,
            escape_graphql(cwd)
        );
        self.lookup_doc_id(&query, "CodingProject")
            .await
            .with_context(|| format!("looking up project doc_id for {}", cwd))
    }

    pub(super) async fn lookup_message_doc_id(&self, message_id: &str) -> Result<String> {
        let query = format!(
            r#"query {{
  CodingMessage(
    filter: {{ message_id: {{ _eq: "{}" }} }},
    limit: 1
  ) {{ _docID }}
}}"#,
            escape_graphql(message_id)
        );
        self.lookup_doc_id(&query, "CodingMessage")
            .await
            .with_context(|| format!("looking up message doc_id for {}", message_id))
    }

    pub(super) async fn lookup_action_doc_id(&self, action_id_tag: &str) -> Result<String> {
        let query = format!(
            r#"query {{
  CodingAction(
    filter: {{ tags: {{ _any: {{ _in: ["{}"] }} }} }},
    limit: 1
  ) {{ _docID }}
}}"#,
            escape_graphql(action_id_tag)
        );
        self.lookup_doc_id(&query, "CodingAction")
            .await
            .with_context(|| format!("looking up action doc_id for {}", action_id_tag))
    }

    async fn lookup_doc_id(&self, query: &str, field: &str) -> Result<String> {
        for attempt in 0..=MAX_MUTATION_RETRIES {
            if attempt > 0 {
                let backoff_ms = INITIAL_RETRY_BACKOFF_MS * (1_u64 << (attempt - 1));
                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
            }

            let response = self.client.execute(query).await;
            ensure_success(&response, "lookup document")?;
            if let Some(doc_id) = extract_doc_id(&response.data, field) {
                return Ok(doc_id);
            }
        }

        bail!("no _docID returned for {}", field)
    }
}
