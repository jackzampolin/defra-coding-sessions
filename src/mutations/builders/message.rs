use super::super::formatting::{escape_graphql, non_empty, non_zero, truncate_chars};
use super::super::inputs::MessageMutationInput;
use crate::parser::TokenUsage;

pub fn create_message_mutation(message: &MessageMutationInput) -> String {
    let mut fields = vec![format!(
        r#"message_id: "{}""#,
        escape_graphql(&message.message_id)
    )];
    fields.push(format!(r#"_sessionID: "{}""#, message.session_doc_id));
    fields.push(format!("sequence: {}", message.sequence));
    fields.push(format!(r#"role: "{}""#, escape_graphql(&message.role)));
    if let Some(source_host) = non_empty(&message.source_host) {
        fields.push(format!(r#"source_host: "{}""#, escape_graphql(source_host)));
    }

    if let Some(model) = non_empty(&message.model) {
        fields.push(format!(r#"model: "{}""#, escape_graphql(model)));
    }
    if let Some(content) = non_empty(&message.content) {
        fields.push(format!(
            r#"content: "{}""#,
            escape_graphql(&truncate_chars(content, 50_000))
        ));
    }
    if let Some(thinking) = non_empty(&message.thinking) {
        fields.push(format!(
            r#"thinking: "{}""#,
            escape_graphql(&truncate_chars(thinking, 50_000))
        ));
    }
    if !message.tool_uses.is_empty() {
        fields.push(format!(
            "tool_uses: {}",
            serde_json::to_string(&message.tool_uses).unwrap_or_else(|_| "[]".to_string())
        ));
    }
    if !message.files_referenced.is_empty() {
        let files: Vec<String> = message.files_referenced.iter().take(50).cloned().collect();
        fields.push(format!(
            "files_referenced: {}",
            serde_json::to_string(&files).unwrap_or_else(|_| "[]".to_string())
        ));
    }
    if let Some(input_tokens) = non_zero(message.usage.input_tokens) {
        fields.push(format!("input_tokens: {input_tokens}"));
    }
    if let Some(output_tokens) = non_zero(message.usage.output_tokens) {
        fields.push(format!("output_tokens: {output_tokens}"));
    }
    append_usage_token_fields(&mut fields, &message.usage);
    if let Some(created_at) = non_empty(&message.created_at) {
        fields.push(format!(r#"created_at: "{created_at}""#));
    }

    format!(
        "mutation {{\n  create_CodingMessage(input: {{ {} }}) {{ _docID }}\n}}",
        fields.join(", ")
    )
}

fn append_usage_token_fields(fields: &mut Vec<String>, usage: &TokenUsage) {
    if let Some(tokens) = non_zero(usage.cache_creation_input_tokens) {
        fields.push(format!("cache_creation_input_tokens: {tokens}"));
    }
    if let Some(tokens) = non_zero(usage.cache_read_input_tokens) {
        fields.push(format!("cache_read_input_tokens: {tokens}"));
    }
    if let Some(tokens) = non_zero(usage.reasoning_output_tokens) {
        fields.push(format!("reasoning_output_tokens: {tokens}"));
    }
    if let Some(tokens) = non_zero(usage.total_tokens) {
        fields.push(format!("total_tokens: {tokens}"));
    }
}
