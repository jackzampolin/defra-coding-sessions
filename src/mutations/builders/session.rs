use super::super::formatting::{escape_graphql, non_empty, non_zero};
use super::super::inputs::SessionMutationInput;

pub fn create_session_mutation(session: &SessionMutationInput) -> String {
    let mut fields = vec![format!(
        r#"session_id: "{}""#,
        escape_graphql(&session.session_id)
    )];
    fields.push(format!(r#"_projectID: "{}""#, session.project_doc_id));
    fields.push(format!(r#"source: "{}""#, escape_graphql(&session.source)));
    if let Some(source_host) = non_empty(&session.source_host) {
        fields.push(format!(r#"source_host: "{}""#, escape_graphql(source_host)));
    }

    if let Some(git_branch) = non_empty(&session.git_branch) {
        fields.push(format!(r#"git_branch: "{}""#, escape_graphql(git_branch)));
    }
    if let Some(model_primary) = non_empty(&session.model_primary) {
        fields.push(format!(
            r#"model_primary: "{}""#,
            escape_graphql(model_primary)
        ));
    }
    if let Some(claude_version) = non_empty(&session.claude_version) {
        fields.push(format!(
            r#"claude_version: "{}""#,
            escape_graphql(claude_version)
        ));
    }
    if let Some(title) = non_empty(&session.title) {
        fields.push(format!(r#"title: "{}""#, escape_graphql(title)));
    }
    if let Some(archived) = session.archived {
        fields.push(format!(
            "archived: {}",
            if archived { "true" } else { "false" }
        ));
    }
    if let Some(git_sha) = non_empty(&session.git_sha) {
        fields.push(format!(r#"git_sha: "{}""#, escape_graphql(git_sha)));
    }
    if let Some(git_origin_url) = non_empty(&session.git_origin_url) {
        fields.push(format!(
            r#"git_origin_url: "{}""#,
            escape_graphql(git_origin_url)
        ));
    }
    if let Some(agent_role) = non_empty(&session.agent_role) {
        fields.push(format!(r#"agent_role: "{}""#, escape_graphql(agent_role)));
    }
    if let Some(reasoning_effort) = non_empty(&session.reasoning_effort) {
        fields.push(format!(
            r#"reasoning_effort: "{}""#,
            escape_graphql(reasoning_effort)
        ));
    }
    if let Some(created_at) = non_empty(&session.created_at) {
        fields.push(format!(r#"created_at: "{created_at}""#));
    }
    if let Some(finished_at) = non_empty(&session.finished_at) {
        fields.push(format!(r#"finished_at: "{finished_at}""#));
    }
    if let Some(message_count) = non_zero(session.message_count) {
        fields.push(format!("message_count: {message_count}"));
    }
    if let Some(user_message_count) = non_zero(session.user_message_count) {
        fields.push(format!("user_message_count: {user_message_count}"));
    }
    if let Some(input_tokens) = non_zero(session.input_tokens) {
        fields.push(format!("input_tokens: {input_tokens}"));
    }
    if let Some(output_tokens) = non_zero(session.output_tokens) {
        fields.push(format!("output_tokens: {output_tokens}"));
    }
    append_session_token_fields(&mut fields, session);
    if !session.tools_used.is_empty() {
        fields.push(format!(
            "tools_used: {}",
            serde_json::to_string(&session.tools_used).unwrap_or_else(|_| "[]".to_string())
        ));
    }
    if let Some(first_prompt) = non_empty(&session.first_prompt) {
        fields.push(format!(
            r#"first_prompt: "{}""#,
            escape_graphql(first_prompt)
        ));
    }
    if let Some(summary) = non_empty(&session.summary) {
        fields.push(format!(r#"summary: "{}""#, escape_graphql(summary)));
    }

    format!(
        "mutation {{\n  create_CodingSession(input: {{ {} }}) {{ _docID }}\n}}",
        fields.join(", ")
    )
}

pub(crate) fn update_session_mutation(session_id: &str, session: &SessionMutationInput) -> String {
    let mut input = Vec::new();

    if let Some(created_at) = non_empty(&session.created_at) {
        input.push(format!(r#"created_at: "{created_at}""#));
    }
    if let Some(model_primary) = non_empty(&session.model_primary) {
        input.push(format!(
            r#"model_primary: "{}""#,
            escape_graphql(model_primary)
        ));
    }
    if let Some(claude_version) = non_empty(&session.claude_version) {
        input.push(format!(
            r#"claude_version: "{}""#,
            escape_graphql(claude_version)
        ));
    }
    if let Some(title) = non_empty(&session.title) {
        input.push(format!(r#"title: "{}""#, escape_graphql(title)));
    }
    if let Some(archived) = session.archived {
        input.push(format!(
            "archived: {}",
            if archived { "true" } else { "false" }
        ));
    }
    if let Some(git_sha) = non_empty(&session.git_sha) {
        input.push(format!(r#"git_sha: "{}""#, escape_graphql(git_sha)));
    }
    if let Some(git_origin_url) = non_empty(&session.git_origin_url) {
        input.push(format!(
            r#"git_origin_url: "{}""#,
            escape_graphql(git_origin_url)
        ));
    }
    if let Some(agent_role) = non_empty(&session.agent_role) {
        input.push(format!(r#"agent_role: "{}""#, escape_graphql(agent_role)));
    }
    if let Some(reasoning_effort) = non_empty(&session.reasoning_effort) {
        input.push(format!(
            r#"reasoning_effort: "{}""#,
            escape_graphql(reasoning_effort)
        ));
    }
    if let Some(source_host) = non_empty(&session.source_host) {
        input.push(format!(r#"source_host: "{}""#, escape_graphql(source_host)));
    }
    if let Some(finished_at) = non_empty(&session.finished_at) {
        input.push(format!(r#"finished_at: "{finished_at}""#));
    }
    if let Some(message_count) = non_zero(session.message_count) {
        input.push(format!("message_count: {message_count}"));
    }
    if let Some(user_message_count) = non_zero(session.user_message_count) {
        input.push(format!("user_message_count: {user_message_count}"));
    }
    if let Some(input_tokens) = non_zero(session.input_tokens) {
        input.push(format!("input_tokens: {input_tokens}"));
    }
    if let Some(output_tokens) = non_zero(session.output_tokens) {
        input.push(format!("output_tokens: {output_tokens}"));
    }
    append_session_token_fields(&mut input, session);
    if !session.tools_used.is_empty() {
        input.push(format!(
            "tools_used: {}",
            serde_json::to_string(&session.tools_used).unwrap_or_else(|_| "[]".to_string())
        ));
    }
    if let Some(first_prompt) = non_empty(&session.first_prompt) {
        input.push(format!(
            r#"first_prompt: "{}""#,
            escape_graphql(first_prompt)
        ));
    }
    if let Some(summary) = non_empty(&session.summary) {
        input.push(format!(r#"summary: "{}""#, escape_graphql(summary)));
    }

    format!(
        r#"mutation {{
  update_CodingSession(
    filter: {{ session_id: {{ _eq: "{}" }} }},
    input: {{ {} }}
  ) {{ _docID }}
}}"#,
        escape_graphql(session_id),
        input.join(", ")
    )
}

fn append_session_token_fields(fields: &mut Vec<String>, session: &SessionMutationInput) {
    if let Some(tokens) = non_zero(session.cache_creation_input_tokens) {
        fields.push(format!("cache_creation_input_tokens: {tokens}"));
    }
    if let Some(tokens) = non_zero(session.cache_read_input_tokens) {
        fields.push(format!("cache_read_input_tokens: {tokens}"));
    }
    if let Some(tokens) = non_zero(session.reasoning_output_tokens) {
        fields.push(format!("reasoning_output_tokens: {tokens}"));
    }
    if let Some(tokens) = non_zero(session.total_tokens) {
        fields.push(format!("total_tokens: {tokens}"));
    }
}
