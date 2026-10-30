use super::super::formatting::{escape_graphql, non_empty, truncate_chars};
use super::super::inputs::ActionMutationInput;

pub fn create_action_mutation(action: &ActionMutationInput) -> String {
    let mut fields = vec![format!(r#"_messageID: "{}""#, action.message_doc_id)];
    fields.push(format!(r#"_sessionID: "{}""#, action.session_doc_id));
    fields.push(format!(
        r#"action_type: "{}""#,
        escape_graphql(&action.action.action_type)
    ));

    if !action.tags.is_empty() {
        fields.push(format!(
            "tags: {}",
            serde_json::to_string(&action.tags).unwrap_or_else(|_| "[]".to_string())
        ));
    }
    if !action.action.target.is_empty() {
        fields.push(format!(
            r#"target: "{}""#,
            escape_graphql(&truncate_chars(&action.action.target, 500))
        ));
    }
    if !action.action.command.is_empty() {
        fields.push(format!(
            r#"command: "{}""#,
            escape_graphql(&truncate_chars(&action.action.command, 2_000))
        ));
    }
    if let Some(created_at) = non_empty(&action.created_at) {
        fields.push(format!(r#"created_at: "{created_at}""#));
    }

    format!(
        "mutation {{\n  create_CodingAction(input: {{ {} }}) {{ _docID }}\n}}",
        fields.join(", ")
    )
}
