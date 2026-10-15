/// A record of a single successful snapshot created by one plugin.
#[derive(Debug, Clone)]
pub struct SnapshotRecord {
    /// Name of the plugin that created this snapshot.
    pub plugin: &'static str,
    /// Opaque identifier returned by the plugin (e.g. stash ref, image tag).
    pub snapshot_id: String,
}

/// One live `Snapshot` as the Snapshot ledger reads it from the Audit log.
///
/// The ledger keys a Snapshot by the pair (`plugin`, `snapshot_id`).
/// `recorded_at` is the timestamp of the entry that created the Snapshot; a
/// later rollback of the same Snapshot does not change it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedSnapshot {
    /// Name of the plugin that created this snapshot.
    pub plugin: String,
    /// Opaque identifier returned by the plugin.
    pub snapshot_id: String,
    /// Timestamp of the Audit log entry that created this snapshot.
    pub recorded_at: std::time::SystemTime,
}
