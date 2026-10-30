#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ScanStats {
    pub files_scanned: usize,
    pub lines_read: u64,
    pub lines_skipped: u64,
    pub messages_processed: u64,
    pub usage_events_processed: u64,
    pub actions_created: u64,
    pub sessions_finalized: u64,
    pub errors: u64,
}
