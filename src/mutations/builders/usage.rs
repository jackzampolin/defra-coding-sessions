use super::super::formatting::{escape_graphql, non_empty, non_zero};
use super::super::inputs::TokenUsageMutationInput;

pub fn create_token_usage_mutation(event: &TokenUsageMutationInput) -> String {
    let mut fields = identity_fields(event);
    append_mutable_fields(&mut fields, event);
    format!(
        "mutation {{\n  create_CodingTokenUsage(input: {{ {} }}) {{ _docID }}\n}}",
        fields.join(", ")
    )
}

pub fn update_token_usage_mutation(event: &TokenUsageMutationInput) -> String {
    let mut fields = Vec::new();
    append_mutable_fields(&mut fields, event);
    format!(
        r#"mutation {{
  update_CodingTokenUsage(
    filter: {{ usage_id: {{ _eq: "{}" }} }},
    input: {{ {} }}
  ) {{ _docID }}
}}"#,
        escape_graphql(&event.usage_id),
        fields.join(", ")
    )
}

fn identity_fields(event: &TokenUsageMutationInput) -> Vec<String> {
    vec![
        format!(r#"usage_id: "{}""#, escape_graphql(&event.usage_id)),
        format!(r#"session_id: "{}""#, escape_graphql(&event.session_id)),
        format!(r#"source: "{}""#, escape_graphql(&event.source)),
        format!(r#"source_host: "{}""#, escape_graphql(&event.source_host)),
    ]
}

fn append_mutable_fields(fields: &mut Vec<String>, event: &TokenUsageMutationInput) {
    if let Some(model) = non_empty(&event.model) {
        fields.push(format!(r#"model: "{}""#, escape_graphql(model)));
    }
    if let Some(tokens) = non_zero(event.usage.input_tokens) {
        fields.push(format!("input_tokens: {tokens}"));
    }
    if let Some(tokens) = non_zero(event.usage.output_tokens) {
        fields.push(format!("output_tokens: {tokens}"));
    }
    if let Some(tokens) = non_zero(event.usage.cache_creation_input_tokens) {
        fields.push(format!("cache_creation_input_tokens: {tokens}"));
    }
    if let Some(tokens) = non_zero(event.usage.cache_read_input_tokens) {
        fields.push(format!("cache_read_input_tokens: {tokens}"));
    }
    if let Some(tokens) = non_zero(event.usage.reasoning_output_tokens) {
        fields.push(format!("reasoning_output_tokens: {tokens}"));
    }
    if let Some(tokens) = non_zero(event.usage.total_tokens) {
        fields.push(format!("total_tokens: {tokens}"));
    }
    fields.push(format!(
        r#"created_at: "{}""#,
        escape_graphql(&event.created_at)
    ));
}
