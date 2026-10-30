use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::Path;

use anyhow::Result;

pub fn read_lines_from_offset(path: &Path, offset: u64) -> Result<(Vec<String>, u64)> {
    let mut file = std::fs::File::open(path)?;
    let size = file.metadata()?.len();
    let start = if size < offset { 0 } else { offset };
    file.seek(SeekFrom::Start(start))?;

    let mut reader = BufReader::new(file);
    let mut lines = Vec::new();
    let mut buffer = String::new();

    loop {
        let line_start = reader.stream_position()?;
        buffer.clear();
        if reader.read_line(&mut buffer)? == 0 {
            break;
        }

        if !buffer.ends_with('\n') {
            reader.seek(SeekFrom::Start(line_start))?;
            break;
        }

        let line = buffer.trim_end_matches(['\r', '\n']);
        if !line.is_empty() {
            lines.push(line.to_string());
        }
    }

    let new_offset = reader.stream_position()?;
    Ok((lines, new_offset))
}

pub(super) fn normalize_worker_count(worker_count: usize) -> usize {
    worker_count.max(1)
}

pub(super) fn worker_index(session_id: &str, worker_count: usize) -> usize {
    let mut hasher = DefaultHasher::new();
    session_id.hash(&mut hasher);
    (hasher.finish() as usize) % worker_count
}
