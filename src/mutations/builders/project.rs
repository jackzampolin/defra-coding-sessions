use super::super::formatting::escape_graphql;

pub fn upsert_project_mutation(
    path: &str,
    repo_name: Option<&str>,
    repo_owner: Option<&str>,
) -> String {
    let owner_field = repo_owner
        .filter(|value| !value.is_empty())
        .map(|value| format!(r#", repo_owner: "{}""#, escape_graphql(value)))
        .unwrap_or_default();
    let name_field = repo_name
        .filter(|value| !value.is_empty())
        .map(|value| format!(r#", repo_name: "{}""#, escape_graphql(value)))
        .unwrap_or_default();
    let escaped_path = escape_graphql(path);

    format!(
        r#"mutation {{
  upsert_CodingProject(
    filter: {{ path: {{ _eq: "{escaped_path}" }} }},
    add: {{ path: "{escaped_path}"{name_field}{owner_field} }},
    update: {{ path: "{escaped_path}"{name_field}{owner_field} }}
  ) {{ _docID }}
}}"#
    )
}
