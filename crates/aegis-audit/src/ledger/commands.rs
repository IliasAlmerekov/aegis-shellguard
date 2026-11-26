use std::collections::HashMap;

use aegis_types::Decision;

use super::is_rollback_entry;
use crate::logger::{AuditEntry, AuditTimestamp};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSnapshot {
    pub plugin: String,
    pub snapshot_id: String,
    pub git_target: Option<String>,
    pub restored_at: Option<AuditTimestamp>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedCommand {
    pub command: String,
    pub decision: Decision,
    pub recorded_at: AuditTimestamp,
    pub source: Option<String>,
    pub cwd: Option<String>,
    pub snapshots: Vec<CommandSnapshot>,
}

pub(super) fn build(
    entries: &[AuditEntry],
    is_live: impl Fn(&str, &str) -> bool,
) -> Vec<RecordedCommand> {
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
                restored.remove(&key);
            }
        }
    }

    entries
        .iter()
        .enumerate()
        .rev()
        .filter_map(|(index, entry)| {
            let base = entry.as_base();
            let ran = matches!(base.decision, Decision::Approved | Decision::AutoApproved);
            if !ran || is_rollback_entry(base) {
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
