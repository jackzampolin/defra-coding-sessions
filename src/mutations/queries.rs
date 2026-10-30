use super::formatting::escape_graphql;

pub fn query_session_checkpoint(session_id: &str) -> String {
    format!(
        r#"query {{
  CodingSession(
    filter: {{ session_id: {{ _eq: "{}" }} }},
    limit: 1
  ) {{
    _docID
    git_branch
    source_host
    model_primary
    claude_version
    title
    archived
    git_sha
    git_origin_url
    agent_role
    reasoning_effort
    message_count
    user_message_count
    input_tokens
    output_tokens
    cache_creation_input_tokens
    cache_read_input_tokens
    reasoning_output_tokens
    total_tokens
    tools_used
    first_prompt
  }}
}}"#,
        escape_graphql(session_id)
    )
}

pub fn query_message_max_sequence(session_doc_id: &str) -> String {
    format!(
        r#"query {{
  CodingMessage(
    filter: {{ _sessionID: {{ _eq: "{}" }} }},
    order: {{ sequence: DESC }},
    limit: 1
  ) {{ sequence }}
}}"#,
        escape_graphql(session_doc_id)
    )
}

pub fn query_session_message_gaps(limit: usize) -> String {
    format!(
        r#"query {{
  CodingSession(limit: {limit}, order: {{ created_at: ASC }}) {{
    _docID
    session_id
    message_count
    source
    created_at
    messages {{ _docID }}
  }}
}}"#
    )
}
