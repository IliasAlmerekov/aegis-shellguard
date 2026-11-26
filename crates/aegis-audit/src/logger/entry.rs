use std::sync::atomic::Ordering;

use super::*;

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

fn current_timestamp() -> AuditTimestamp {
    AuditTimestamp::now()
}

fn next_sequence() -> u64 {
    AUDIT_SEQUENCE.fetch_add(1, Ordering::Relaxed)
}
