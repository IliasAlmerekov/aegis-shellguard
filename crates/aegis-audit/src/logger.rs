use std::path::PathBuf;
use std::sync::atomic::AtomicU64;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use aegis_config::AuditIntegrityMode;
use aegis_explanation::CommandExplanation;
use aegis_types::{
    Category, Decision, MatchResult, Mode, PatternSource, RecoveryDegradation, RiskLevel,
    SandboxStatus, SnapshotAttempt, SnapshotRecord,
};

use crate::error::AuditError;

mod integrity;
mod query;
mod rotation;
mod writer;

type Result<T> = std::result::Result<T, AuditError>;
static AUDIT_SEQUENCE: AtomicU64 = AtomicU64::new(1);
const CHAIN_ALG_SHA256: &str = "sha256";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct AuditTimestamp(OffsetDateTime);

impl AuditTimestamp {
    fn now() -> Self {
        Self(OffsetDateTime::now_utc())
    }

    pub fn from_unix_seconds(seconds: i64) -> std::result::Result<Self, String> {
        OffsetDateTime::from_unix_timestamp(seconds)
            .map(Self)
            .map_err(|err| format!("invalid unix timestamp {seconds}: {err}"))
    }

    pub fn parse_rfc3339(value: &str) -> std::result::Result<Self, String> {
        OffsetDateTime::parse(value, &Rfc3339)
            .map(Self)
            .map_err(|err| format!("invalid RFC 3339 timestamp {value:?}: {err}"))
    }

    fn format_rfc3339(&self) -> String {
        self.0
            .format(&Rfc3339)
            .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
    }
}

impl From<AuditTimestamp> for std::time::SystemTime {
    fn from(timestamp: AuditTimestamp) -> Self {
        timestamp.0.into()
    }
}

impl std::fmt::Display for AuditTimestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.format_rfc3339())
    }
}

impl Serialize for AuditTimestamp {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.format_rfc3339())
    }
}

impl<'de> Deserialize<'de> for AuditTimestamp {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum TimestampRepr {
            Rfc3339(String),
            UnixSecondsI64(i64),
            UnixSecondsU64(u64),
        }

        match TimestampRepr::deserialize(deserializer)? {
            TimestampRepr::Rfc3339(value) => OffsetDateTime::parse(&value, &Rfc3339)
                .map(Self)
                .map_err(|err| {
                    D::Error::custom(format!("invalid RFC 3339 timestamp {value:?}: {err}"))
                }),
            TimestampRepr::UnixSecondsI64(value) => {
                Self::from_unix_seconds(value).map_err(D::Error::custom)
            }
            TimestampRepr::UnixSecondsU64(value) => {
                let seconds = i64::try_from(value).map_err(|_| {
                    D::Error::custom(format!("timestamp {value} exceeds i64 range"))
                })?;
                Self::from_unix_seconds(seconds).map_err(D::Error::custom)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionEntry {
    pub timestamp: AuditTimestamp,
    pub sequence: u64,
    pub command: String,
    pub risk: RiskLevel,
    pub matched_patterns: Vec<MatchedPattern>,
    pub pattern_ids: Vec<String>,
    pub decision: Decision,
    pub snapshots: Vec<AuditSnapshot>,
    pub explanation: Option<CommandExplanation>,
    pub mode: Option<Mode>,
    pub ci_detected: Option<bool>,
    pub allowlist_matched: Option<bool>,
    pub allowlist_effective: Option<bool>,
    pub chain_alg: Option<String>,
    pub prev_hash: Option<String>,
    pub entry_hash: Option<String>,
    pub allowlist_pattern: Option<String>,
    pub allowlist_reason: Option<String>,
    pub sandbox_status: SandboxStatus,
    pub effect_opaque: Option<bool>,
    pub snapshots_required: Option<bool>,
    pub confinement_required: Option<bool>,
    pub recovery_degradation: Option<RecoveryDegradation>,
    pub basis: Option<aegis_types::AssessmentBasis>,
    pub analysis: Option<aegis_types::AnalysisSummary>,
    pub snapshot_attempts: Option<Vec<SnapshotAttempt>>,
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchEntry {
    pub base: DecisionEntry,
    pub source: Option<String>,
    pub id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditEntry {
    Decision(DecisionEntry),
    Watch(WatchEntry),
}

impl AuditEntry {
    pub fn as_base(&self) -> &DecisionEntry {
        match self {
            AuditEntry::Decision(e) => e,
            AuditEntry::Watch(w) => &w.base,
        }
    }

    pub(super) fn as_base_mut(&mut self) -> &mut DecisionEntry {
        match self {
            AuditEntry::Decision(e) => e,
            AuditEntry::Watch(w) => &mut w.base,
        }
    }

    pub fn watch_context(&self) -> Option<&WatchEntry> {
        match self {
            AuditEntry::Watch(w) => Some(w),
            _ => None,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct AuditEntryFlat {
    timestamp: AuditTimestamp,
    #[serde(default)]
    sequence: u64,
    command: String,
    risk: RiskLevel,
    matched_patterns: Vec<MatchedPattern>,
    #[serde(default)]
    pattern_ids: Vec<String>,
    decision: Decision,
    snapshots: Vec<AuditSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    explanation: Option<CommandExplanation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mode: Option<Mode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ci_detected: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    allowlist_matched: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    allowlist_effective: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    chain_alg: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    prev_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    entry_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    allowlist_pattern: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    allowlist_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sandbox_status: Option<SandboxStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sandbox_active: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    effect_opaque: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    snapshots_required: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    confinement_required: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    recovery_degradation: Option<RecoveryDegradation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    basis: Option<aegis_types::AssessmentBasis>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    analysis: Option<aegis_types::AnalysisSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    snapshot_attempts: Option<Vec<SnapshotAttempt>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    transport: Option<String>,
}

impl From<AuditEntryFlat> for AuditEntry {
    fn from(flat: AuditEntryFlat) -> Self {
        let is_watch = flat.transport.as_deref() == Some("watch")
            || flat.source.is_some()
            || flat.id.is_some();
        let base = DecisionEntry {
            timestamp: flat.timestamp,
            sequence: flat.sequence,
            command: flat.command,
            risk: flat.risk,
            matched_patterns: flat.matched_patterns,
            pattern_ids: flat.pattern_ids,
            decision: flat.decision,
            snapshots: flat.snapshots,
            explanation: flat.explanation,
            mode: flat.mode,
            ci_detected: flat.ci_detected,
            allowlist_matched: flat.allowlist_matched,
            allowlist_effective: flat.allowlist_effective,
            chain_alg: flat.chain_alg,
            prev_hash: flat.prev_hash,
            entry_hash: flat.entry_hash,
            allowlist_pattern: flat.allowlist_pattern,
            allowlist_reason: flat.allowlist_reason,
            sandbox_status: flat
                .sandbox_status
                .unwrap_or_else(|| SandboxStatus::from(flat.sandbox_active)),
            effect_opaque: flat.effect_opaque,
            snapshots_required: flat.snapshots_required,
            confinement_required: flat.confinement_required,
            recovery_degradation: flat.recovery_degradation,
            basis: flat.basis,
            analysis: flat.analysis,
            snapshot_attempts: flat.snapshot_attempts,
            cwd: flat.cwd,
        };
        if is_watch {
            AuditEntry::Watch(WatchEntry {
                base,
                source: flat.source,
                id: flat.id,
            })
        } else {
            AuditEntry::Decision(base)
        }
    }
}

impl From<&AuditEntry> for AuditEntryFlat {
    fn from(entry: &AuditEntry) -> Self {
        let base = entry.as_base();
        let (source, id, transport) = match entry {
            AuditEntry::Watch(w) => (w.source.clone(), w.id.clone(), Some("watch".to_string())),
            AuditEntry::Decision(_) => (None, None, None),
        };
        Self {
            timestamp: base.timestamp,
            sequence: base.sequence,
            command: base.command.clone(),
            risk: base.risk,
            matched_patterns: base.matched_patterns.clone(),
            pattern_ids: base.pattern_ids.clone(),
            decision: base.decision,
            snapshots: base.snapshots.clone(),
            explanation: base.explanation.clone(),
            mode: base.mode,
            ci_detected: base.ci_detected,
            allowlist_matched: base.allowlist_matched,
            allowlist_effective: base.allowlist_effective,
            chain_alg: base.chain_alg.clone(),
            prev_hash: base.prev_hash.clone(),
            entry_hash: base.entry_hash.clone(),
            allowlist_pattern: base.allowlist_pattern.clone(),
            allowlist_reason: base.allowlist_reason.clone(),
            sandbox_status: Some(base.sandbox_status),
            sandbox_active: base.sandbox_status.as_legacy_active(),
            effect_opaque: base.effect_opaque,
            snapshots_required: base.snapshots_required,
            confinement_required: base.confinement_required,
            recovery_degradation: base.recovery_degradation,
            basis: base.basis.clone(),
            analysis: base.analysis.clone(),
            snapshot_attempts: base.snapshot_attempts.clone(),
            source,
            cwd: base.cwd.clone(),
            id,
            transport,
        }
    }
}

impl Serialize for AuditEntry {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        AuditEntryFlat::from(self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for AuditEntry {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        AuditEntryFlat::deserialize(deserializer).map(AuditEntry::from)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuditQuery {
    pub last: Option<usize>,
    pub risk: Option<RiskLevel>,
    pub decision: Option<Decision>,
    pub since: Option<AuditTimestamp>,
    pub until: Option<AuditTimestamp>,
    pub command_contains: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuditSummary {
    pub total_entries: usize,
    pub decision_counts: DecisionCounts,
    pub risk_counts: RiskCounts,
    pub top_patterns: Vec<PatternCount>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct DecisionCounts {
    pub approved: usize,
    pub denied: usize,
    pub auto_approved: usize,
    pub blocked: usize,
    pub pruned: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct RiskCounts {
    pub safe: usize,
    pub warn: usize,
    pub danger: usize,
    pub block: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PatternCount {
    pub id: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchedPattern {
    pub id: String,
    pub risk: RiskLevel,
    pub description: String,
    pub safe_alt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<Category>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matched_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<PatternSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<aegis_types::MatchEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detection_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditSnapshot {
    pub plugin: String,
    pub snapshot_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_target: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditRotationPolicy {
    max_file_size_bytes: u64,
    retention_files: usize,
    compress_rotated: bool,
}

#[derive(Debug, Clone)]
struct ArchiveSegment {
    path: std::path::PathBuf,
    compressed: bool,
    index: usize,
}

struct AuditLock {
    file: std::fs::File,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditIntegrityReport {
    pub status: AuditIntegrityStatus,
    pub checked_entries: usize,
    pub chained_entries: usize,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditIntegrityStatus {
    Verified,
    NoIntegrityData,
    Corrupt,
}

pub struct AuditLogger {
    pub(super) path: PathBuf,
    rotation: Option<AuditRotationPolicy>,
    integrity_mode: AuditIntegrityMode,
}

impl From<&MatchResult> for MatchedPattern {
    fn from(m: &MatchResult) -> Self {
        Self {
            id: m.pattern.id.to_string(),
            risk: m.pattern.risk,
            description: m.pattern.description.to_string(),
            safe_alt: m.pattern.safe_alt.as_ref().map(ToString::to_string),
            category: Some(m.pattern.category),
            matched_text: Some(m.public_matched_text().to_string()),
            source: Some(m.pattern.source),
            evidence: Some(m.evidence.clone()),
            detection_id: Some(detection_id_for(m)),
        }
    }
}

fn detection_id_for(m: &MatchResult) -> String {
    match &m.evidence {
        aegis_types::MatchEvidence::LanguageRule { provenance, .. } => provenance
            .rule_id
            .clone()
            .unwrap_or_else(|| m.pattern.id.to_string()),
        _ => m.pattern.id.to_string(),
    }
}

impl From<&SnapshotRecord> for AuditSnapshot {
    fn from(snapshot: &SnapshotRecord) -> Self {
        Self {
            plugin: snapshot.plugin.to_string(),
            snapshot_id: snapshot.snapshot_id.clone(),
            git_target: snapshot.git_target.clone(),
        }
    }
}

#[cfg(test)]
mod tests;
