use std::env;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use super::*;
use crate::error::AuditError;
use crate::secure_fs::{
    create_parent_directories, open_append, open_lock, parent_exists_and_is_safe,
};
use aegis_config::AuditConfig;

impl AuditEntry {
    pub fn new(
        command: impl Into<String>,
        risk: RiskLevel,
        matched_patterns: Vec<MatchedPattern>,
        decision: Decision,
        snapshots: Vec<AuditSnapshot>,
        allowlist_pattern: Option<String>,
        allowlist_reason: Option<String>,
    ) -> Self {
        let command = command.into();
        let pattern_ids = matched_patterns.iter().map(|p| p.id.clone()).collect();
        Self::Decision(DecisionEntry {
            timestamp: current_timestamp(),
            sequence: next_sequence(),
            command,
            risk,
            pattern_ids,
            matched_patterns,
            decision,
            snapshots,
            explanation: None,
            mode: None,
            ci_detected: None,
            allowlist_matched: Some(false),
            allowlist_effective: Some(false),
            chain_alg: None,
            prev_hash: None,
            entry_hash: None,
            allowlist_pattern,
            allowlist_reason,
            sandbox_status: SandboxStatus::NotConfigured,
            effect_opaque: Some(false),
            snapshots_required: Some(false),
            confinement_required: Some(false),
            recovery_degradation: None,
            basis: None,
            analysis: None,
            snapshot_attempts: None,
            cwd: None,
        })
    }

    pub fn with_explanation(mut self, explanation: CommandExplanation) -> Self {
        self.as_base_mut().explanation = Some(explanation);
        self
    }

    pub fn with_watch_context(
        self,
        source: Option<String>,
        cwd: Option<String>,
        id: Option<String>,
    ) -> Self {
        match self.with_cwd(cwd) {
            AuditEntry::Decision(base) => AuditEntry::Watch(WatchEntry { base, source, id }),
            AuditEntry::Watch(mut w) => {
                w.source = source;
                w.id = id;
                AuditEntry::Watch(w)
            }
        }
    }

    pub fn with_cwd(mut self, cwd: Option<String>) -> Self {
        self.as_base_mut().cwd = cwd;
        self
    }

    pub fn with_policy_context(
        mut self,
        mode: Mode,
        ci_detected: bool,
        allowlist_matched: bool,
        allowlist_effective: bool,
    ) -> Self {
        let base = self.as_base_mut();
        base.mode = Some(mode);
        base.ci_detected = Some(ci_detected);
        base.allowlist_matched = Some(allowlist_matched);
        base.allowlist_effective = Some(allowlist_effective);
        self
    }

    pub fn with_sandbox_status(mut self, status: SandboxStatus) -> Self {
        self.as_base_mut().sandbox_status = status;
        self
    }

    pub fn with_effect_opaque(mut self, effect_opaque: bool) -> Self {
        self.as_base_mut().effect_opaque = Some(effect_opaque);
        self
    }

    pub fn with_required_backstops(
        mut self,
        snapshots_required: bool,
        confinement_required: bool,
    ) -> Self {
        let base = self.as_base_mut();
        base.snapshots_required = Some(snapshots_required);
        base.confinement_required = Some(confinement_required);
        self
    }

    pub fn with_recovery_degradation(mut self, degradation: RecoveryDegradation) -> Self {
        self.as_base_mut().recovery_degradation = Some(degradation);
        self
    }

    pub fn with_basis(mut self, basis: aegis_types::AssessmentBasis) -> Self {
        self.as_base_mut().basis = Some(basis);
        self
    }

    pub fn with_snapshot_attempts(mut self, attempts: Vec<aegis_types::SnapshotAttempt>) -> Self {
        self.as_base_mut().snapshot_attempts = Some(attempts);
        self
    }

    pub fn with_analysis(mut self, analysis: Option<aegis_types::AnalysisSummary>) -> Self {
        self.as_base_mut().analysis = analysis;
        self
    }

    pub(super) fn normalize_legacy_fields(mut self) -> Self {
        let base = self.as_base_mut();
        if base.pattern_ids.is_empty() {
            base.pattern_ids = base.matched_patterns.iter().map(|p| p.id.clone()).collect();
        }
        let allowlist_present = base.allowlist_pattern.is_some();
        if base.allowlist_matched.is_none() {
            base.allowlist_matched = Some(allowlist_present);
        }
        if base.allowlist_effective.is_none() {
            base.allowlist_effective = Some(allowlist_present);
        }
        self
    }

    pub(super) fn with_integrity_chain(
        mut self,
        prev_hash: Option<String>,
        entry_hash: String,
    ) -> Self {
        let base = self.as_base_mut();
        base.chain_alg = Some(CHAIN_ALG_SHA256.to_string());
        base.prev_hash = prev_hash;
        base.entry_hash = Some(entry_hash);
        self
    }
}

impl Default for AuditLogger {
    fn default() -> Self {
        Self::new(default_audit_path())
    }
}

impl AuditLock {
    fn exclusive(path: &Path) -> Result<Self> {
        let file = open_lock_file(path, true)?;
        file.lock()?;
        Ok(Self { file })
    }

    fn shared(path: &Path) -> Result<Self> {
        let file = open_lock_file(path, false)?;
        file.lock_shared()?;
        Ok(Self { file })
    }
}

impl Drop for AuditLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

impl AuditLogger {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            rotation: None,
            integrity_mode: AuditIntegrityMode::ChainSha256,
        }
    }

    #[cfg(any(test, feature = "testing"))]
    pub fn with_path(self, path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            ..self
        }
    }

    pub fn with_rotation(mut self, policy: AuditRotationPolicy) -> Self {
        self.rotation = Some(policy);
        self
    }

    pub fn with_integrity_mode(mut self, mode: AuditIntegrityMode) -> Self {
        self.integrity_mode = mode;
        self
    }

    pub fn from_audit_config(config: &AuditConfig) -> Self {
        let logger = Self::default().with_integrity_mode(config.integrity_mode);
        if let Some(policy) = AuditRotationPolicy::from_config(config) {
            logger.with_rotation(policy)
        } else {
            logger
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use = "audit write failures must be handled — ignoring them silently defeats integrity checking"]
    pub fn append(&self, entry: AuditEntry) -> Result<()> {
        self.append_inner(entry)
            .map_err(|err| self.attach_write_context(err))
    }

    fn attach_write_context(&self, err: AuditError) -> AuditError {
        match err {
            AuditError::Io(source) => AuditError::WriteFailed {
                path: self.path.display().to_string(),
                source,
            },
            other => other,
        }
    }

    fn append_inner(&self, entry: AuditEntry) -> Result<()> {
        create_parent_directories(&self.path)?;
        let _lock = AuditLock::exclusive(&self.lock_path())?;

        let prev_hash = self.latest_chained_hash()?;
        let entry = self.apply_integrity(entry.normalize_legacy_fields(), prev_hash)?;
        let mut serialized =
            serde_json::to_vec(&entry).map_err(|e| AuditError::Io(std::io::Error::other(e)))?;
        serialized.push(b'\n');

        if let Some(policy) = &self.rotation {
            self.rotate_if_needed(policy, serialized.len() as u64)?;
        }

        let mut file = open_append(&self.path)?;

        file.write_all(&serialized)?;
        file.flush()?;
        Ok(())
    }

    pub(super) fn lock_path(&self) -> PathBuf {
        let mut file_name = self
            .path
            .file_name()
            .map(|name| name.to_os_string())
            .unwrap_or_else(|| "audit.jsonl".into());
        file_name.push(".lock");

        match self.path.parent() {
            Some(parent) => parent.join(file_name),
            None => PathBuf::from(file_name),
        }
    }

    pub(super) fn acquire_shared_lock(&self) -> Result<Option<AuditLock>> {
        let lock_path = self.lock_path();
        if !parent_exists_and_is_safe(&lock_path)? {
            return Ok(None);
        }

        AuditLock::shared(&lock_path).map(Some)
    }
}

fn default_audit_path() -> PathBuf {
    if let Some(path) = env::var_os("AEGIS_AUDIT_PATH").filter(|value| !value.is_empty()) {
        return PathBuf::from(path);
    }

    let home = env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .unwrap_or_else(|| ".".into());
    PathBuf::from(home).join(".aegis").join("audit.jsonl")
}

fn open_lock_file(path: &Path, create_parent: bool) -> Result<File> {
    if create_parent {
        create_parent_directories(path)?;
    }

    open_lock(path)
}

fn current_timestamp() -> AuditTimestamp {
    AuditTimestamp::now()
}

fn next_sequence() -> u64 {
    AUDIT_SEQUENCE.fetch_add(1, Ordering::Relaxed)
}
