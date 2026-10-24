//! CLI implementation for `aegis snapshot verify` (ADR-052 §5).

use aegis::error::AegisError;
use aegis_audit::{AuditLogger, SnapshotLedger};
use aegis_config::AegisConfig;
use aegis_snapshot::{Liveness, SnapshotRegistry, SnapshotRegistryConfig};
use serde::Serialize;

use crate::{EXIT_ABSENT, EXIT_INCONCLUSIVE, EXIT_INTERNAL, rollback};

/// A `Liveness` tag, or `Error` when the check itself failed.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Outcome {
    Present,
    NothingCaptured,
    Absent,
    Inconclusive,
    Error,
}

impl Outcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Present => "present",
            Self::NothingCaptured => "nothing_captured",
            Self::Absent => "absent",
            Self::Inconclusive => "inconclusive",
            Self::Error => "error",
        }
    }
}

/// One checked Snapshot.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub(crate) struct VerifyRow {
    pub(crate) plugin: String,
    pub(crate) snapshot_id: String,
    pub(crate) outcome: Outcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reason: Option<String>,
}

impl VerifyRow {
    fn new(
        plugin: &str,
        snapshot_id: &str,
        result: Result<Liveness, aegis_snapshot::SnapshotError>,
    ) -> Self {
        let (outcome, reason) = match result {
            Ok(Liveness::Present) => (Outcome::Present, None),
            Ok(Liveness::NothingCaptured) => (Outcome::NothingCaptured, None),
            Ok(Liveness::Absent { reason }) => (Outcome::Absent, Some(reason)),
            Ok(Liveness::Inconclusive { reason }) => (Outcome::Inconclusive, Some(reason)),
            Err(error) => (Outcome::Error, Some(error.to_string())),
        };
        Self {
            plugin: plugin.to_string(),
            snapshot_id: snapshot_id.to_string(),
            outcome,
            reason,
        }
    }
}

/// Check one Snapshot, or every live record of the Snapshot ledger one after another.
/// Nothing is cached: each call asks the providers again.
pub(crate) async fn execute(id: Option<String>) -> Result<Vec<VerifyRow>, AegisError> {
    let config = AegisConfig::load()?;
    let logger = AuditLogger::from_audit_config(&config.audit);
    let targets: Vec<(String, String)> = match id {
        Some(id) => {
            let target = rollback::find_snapshot_target(&logger, &id)?;
            vec![(target.plugin, target.snapshot_id)]
        }
        None => SnapshotLedger::load(&logger)?
            .live_records()
            .iter()
            .map(|record| (record.plugin.clone(), record.snapshot_id.clone()))
            .collect(),
    };
    let registry = SnapshotRegistry::from_runtime_config(
        &SnapshotRegistryConfig::for_rollback_from_config(&config)?,
    );

    let mut rows = Vec::with_capacity(targets.len());
    for (plugin, snapshot_id) in targets {
        let result = registry.liveness(&plugin, &snapshot_id).await;
        rows.push(VerifyRow::new(&plugin, &snapshot_id, result));
    }
    Ok(rows)
}

/// 4 for any error, else 1 for any `absent`, else 5 for any `inconclusive`, else 0.
pub(crate) fn exit_code(rows: &[VerifyRow]) -> i32 {
    let has = |outcome| rows.iter().any(|row| row.outcome == outcome);
    if has(Outcome::Error) {
        EXIT_INTERNAL
    } else if has(Outcome::Absent) {
        EXIT_ABSENT
    } else if has(Outcome::Inconclusive) {
        EXIT_INCONCLUSIVE
    } else {
        0
    }
}

pub(crate) fn render_text(rows: &[VerifyRow]) -> String {
    if rows.is_empty() {
        return "No snapshots recorded.\n".to_string();
    }
    let mut out = String::new();
    for row in rows {
        out.push_str(&format!(
            "{}\t{}\t{}",
            row.plugin,
            row.snapshot_id,
            row.outcome.as_str()
        ));
        if let Some(reason) = &row.reason {
            out.push_str(&format!("\t{reason}"));
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(outcome: Outcome) -> VerifyRow {
        VerifyRow {
            plugin: "git".to_string(),
            snapshot_id: "id".to_string(),
            outcome,
            reason: None,
        }
    }

    fn code(outcomes: &[Outcome]) -> i32 {
        exit_code(&outcomes.iter().map(|o| row(*o)).collect::<Vec<_>>())
    }

    #[test]
    fn exit_code_is_0_for_present_and_nothing_captured() {
        assert_eq!(code(&[Outcome::Present, Outcome::NothingCaptured]), 0);
        assert_eq!(code(&[]), 0);
    }

    #[test]
    fn exit_code_is_1_for_absent_even_beside_inconclusive() {
        assert_eq!(
            code(&[Outcome::Present, Outcome::Inconclusive, Outcome::Absent]),
            1
        );
    }

    #[test]
    fn exit_code_is_5_for_inconclusive_without_absent() {
        assert_eq!(code(&[Outcome::Present, Outcome::Inconclusive]), 5);
    }

    #[test]
    fn exit_code_is_4_when_any_row_is_an_error() {
        assert_eq!(code(&[Outcome::Absent, Outcome::Error]), 4);
    }

    #[test]
    fn render_text_puts_plugin_id_outcome_and_reason_on_one_row() {
        let rows = [VerifyRow {
            reason: Some("gone".to_string()),
            ..row(Outcome::Absent)
        }];
        assert_eq!(render_text(&rows), "git\tid\tabsent\tgone\n");
    }
}
