//! The command view of the Snapshot ledger (#499, ADR-053): which commands
//! still have a live Snapshot, with the Command location of each.

use std::collections::HashMap;

use aegis_types::Decision;

use super::is_rollback_entry;
use crate::logger::{AuditEntry, AuditTimestamp};

/// One live Snapshot of a [`RecordedCommand`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSnapshot {
    /// Name of the plugin that created the Snapshot.
    pub plugin: String,
    /// Opaque identifier used to restore the Snapshot.
    pub snapshot_id: String,
    /// The git working tree a git Snapshot covers, when recorded.
    pub git_target: Option<String>,
    /// When `aegis rollback` last restored it, if ever.
    pub restored_at: Option<AuditTimestamp>,
}

/// A command that still has at least one live Snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedCommand {
    /// The command as intercepted.
    pub command: String,
    /// What was decided for it.
    pub decision: Decision,
    /// When the entry was recorded.
    pub recorded_at: AuditTimestamp,
    /// Agent identity of a watch-mode entry.
    pub source: Option<String>,
    /// The directory it ran in; `None` on lines that predate the field.
    pub cwd: Option<String>,
    /// Its live Snapshots, in the order they were taken.
    pub snapshots: Vec<CommandSnapshot>,
}

/// Build the command view, latest first. `is_live` says whether the pair
/// (plugin, `snapshot_id`) escaped every prune.
pub(super) fn build(
    entries: &[AuditEntry],
    is_live: impl Fn(&str, &str) -> bool,
) -> Vec<RecordedCommand> {
    // A pair recorded twice belongs to its latest entry only, like the ledger's
    // own records.
    let mut owner: HashMap<(&str, &str), usize> = HashMap::new();
    let mut restored: HashMap<(&str, &str), AuditTimestamp> = HashMap::new();
    for (index, entry) in entries.iter().enumerate() {
        let base = entry.as_base();
        if base.decision == Decision::Pruned {
            continue;
        }
        for snapshot in &base.snapshots {
            let key = (snapshot.plugin.as_str(), snapshot.snapshot_id.as_str());
            if is_rollback_entry(base) {
                restored.insert(key, base.timestamp);
            } else {
                owner.insert(key, index);
            }
        }
    }

    entries
        .iter()
        .enumerate()
        .rev()
        .filter_map(|(index, entry)| {
            let base = entry.as_base();
            if base.decision == Decision::Pruned || is_rollback_entry(base) {
                return None;
            }
            let snapshots: Vec<CommandSnapshot> = base
                .snapshots
                .iter()
                .filter(|s| {
                    let key = (s.plugin.as_str(), s.snapshot_id.as_str());
                    owner.get(&key) == Some(&index) && is_live(key.0, key.1)
                })
                .map(|s| CommandSnapshot {
                    plugin: s.plugin.clone(),
                    snapshot_id: s.snapshot_id.clone(),
                    git_target: s.git_target.clone(),
                    restored_at: restored
                        .get(&(s.plugin.as_str(), s.snapshot_id.as_str()))
                        .copied(),
                })
                .collect();
            (!snapshots.is_empty()).then(|| RecordedCommand {
                command: base.command.clone(),
                decision: base.decision,
                recorded_at: base.timestamp,
                source: entry.watch_context().and_then(|w| w.source.clone()),
                cwd: base.cwd.clone(),
                snapshots,
            })
        })
        .collect()
}
