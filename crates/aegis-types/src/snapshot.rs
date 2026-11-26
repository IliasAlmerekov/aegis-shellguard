#[derive(Debug, Clone)]
pub struct SnapshotRecord {
    pub plugin: &'static str,
    pub snapshot_id: String,
    pub git_target: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedSnapshot {
    pub plugin: String,
    pub snapshot_id: String,
    pub recorded_at: std::time::SystemTime,
}
