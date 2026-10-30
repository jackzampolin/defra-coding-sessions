use super::parser::{ParsedEntry, Role};
use serde_json::Value;

#[derive(Debug, Clone, Default)]
pub(super) struct AdjacentUserDuplicateFilter {
    previous: Option<UserRecord>,
}

impl AdjacentUserDuplicateFilter {
    pub(super) fn should_skip(&mut self, entry: &ParsedEntry) -> bool {
        let current = UserRecord::from_entry(entry);
        let skip = current
            .as_ref()
            .is_some_and(|record| self.is_duplicate_event(record));

        self.previous = if skip { None } else { current };
        skip
    }

    pub(super) fn reset(&mut self) {
        self.previous = None;
    }

    fn is_duplicate_event(&self, current: &UserRecord) -> bool {
        current.source == UserRecordSource::EventMsg
            && current.files_referenced.is_empty()
            && !current.content.trim().is_empty()
            && self.previous.as_ref().is_some_and(|previous| {
                previous.source == UserRecordSource::ResponseItem
                    && previous.timestamp.is_some()
                    && previous.timestamp == current.timestamp
                    && previous.content == current.content
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UserRecord {
    source: UserRecordSource,
    timestamp: Option<String>,
    content: String,
    files_referenced: Vec<String>,
}

impl UserRecord {
    fn from_entry(entry: &ParsedEntry) -> Option<Self> {
        let ParsedEntry::Message(message) = entry else {
            return None;
        };
        if message.role != Role::User {
            return None;
        }

        Some(Self {
            source: user_record_source(&message.raw_line)?,
            timestamp: message.timestamp.clone(),
            content: message.content.clone(),
            files_referenced: message.files_referenced.clone(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UserRecordSource {
    ResponseItem,
    EventMsg,
}

fn user_record_source(raw_line: &str) -> Option<UserRecordSource> {
    let value: Value = serde_json::from_str(raw_line).ok()?;
    let payload = value.get("payload").and_then(Value::as_object)?;

    match (
        value.get("type").and_then(Value::as_str),
        payload.get("type").and_then(Value::as_str),
        payload.get("role").and_then(Value::as_str),
    ) {
        (Some("response_item"), Some("message"), Some("user")) => {
            Some(UserRecordSource::ResponseItem)
        }
        (Some("event_msg"), Some("user_message"), _) => Some(UserRecordSource::EventMsg),
        _ => None,
    }
}
