use chrono::{DateTime, SecondsFormat, Utc};
use std::hash::{Hash, Hasher};

pub(super) fn grok_message_id(
    session_id: &str,
    role: &str,
    line_index: u64,
    stable_hint: Option<&str>,
    content: &str,
    thinking: Option<&str>,
) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    role.hash(&mut hasher);
    content.hash(&mut hasher);
    thinking.unwrap_or_default().hash(&mut hasher);
    let digest = hasher.finish();
    let stable_hint = stable_hint
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("line-{line_index}"));
    format!("grok:v2:{session_id}:{role}:{stable_hint}:{digest:016x}")
}

pub(super) fn normalize_timestamp(raw: Option<&str>) -> Option<String> {
    raw.map(|timestamp| {
        DateTime::parse_from_rfc3339(timestamp)
            .map(|parsed| {
                parsed
                    .with_timezone(&Utc)
                    .to_rfc3339_opts(SecondsFormat::Secs, true)
            })
            .unwrap_or_else(|_| timestamp.to_string())
    })
}

pub(super) fn non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}
