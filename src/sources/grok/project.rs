use std::path::Path;

/// Resolve workspace cwd from a chat_history path.
///
/// Path shape: `.../sessions/<url-encoded-cwd>/<session-id>/chat_history.jsonl`
pub(super) fn cwd_from_chat_history_path(base_dir: &Path, path: &Path) -> Option<String> {
    let sessions_dir = base_dir.join("sessions");
    let relative = path.strip_prefix(&sessions_dir).ok()?;
    let encoded = relative.components().next()?.as_os_str().to_string_lossy();
    let decoded = percent_decode(&encoded)?;
    (!decoded.is_empty()).then_some(decoded)
}

pub(super) fn session_id_from_chat_history_path(path: &Path) -> Option<String> {
    path.parent()
        .and_then(|session_dir| session_dir.file_name())
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
}

fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok()?;
                let value = u8::from_str_radix(hex, 16).ok()?;
                out.push(value);
                index += 3;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn decodes_percent_encoded_cwd() {
        let base = PathBuf::from("/home/user/.grok");
        let path = base.join(
            "sessions/%2Fhome%2Fuser%2Fsrc%2Fexample%2Fexample-repo/019f47b1-e872-7272-85c2-c5b47afd4854/chat_history.jsonl",
        );
        assert_eq!(
            cwd_from_chat_history_path(&base, &path).as_deref(),
            Some("/home/user/src/example/example-repo")
        );
        assert_eq!(
            session_id_from_chat_history_path(&path).as_deref(),
            Some("019f47b1-e872-7272-85c2-c5b47afd4854")
        );
    }
}
