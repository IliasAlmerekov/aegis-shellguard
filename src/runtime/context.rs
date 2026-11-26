use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use time::OffsetDateTime;
use tokio::runtime::{Handle, RuntimeFlavor};

use crate::error::AegisError;
use crate::explanation::CommandExplanation;
use crate::explanation::formatter::{CommandExplanationExt, build_outcome_explanation};
use crate::planning::InterceptionPlan;
use aegis_audit::{AuditEntry, AuditError, AuditLogger, AuditSnapshot, SnapshotLedger};
use aegis_config::{AegisConfig, Allowlist, AllowlistContext, AllowlistMatch, Blocklist};
use aegis_scanner::Scanner;
use aegis_snapshot::{
    Clock, RetentionPolicy, SnapshotCoverage, SnapshotRegistry, SnapshotRegistryConfig, SystemClock,
};
use aegis_types::{
    AllowlistOverrideLevel, Assessment, Decision, RecoveryDegradation, SandboxStatus,
    SnapshotAttempt, SnapshotPolicy, SnapshotRecord,
};

use super::user::detect_effective_user;

#[derive(Clone, Debug)]
pub struct RuntimeConfig {
    pub mode: aegis_types::Mode,
    pub ci_policy: aegis_types::CiPolicy,
    pub strict_allowlist_override: AllowlistOverrideLevel,
    pub snapshot_policy: SnapshotPolicy,
    pub sandbox: Option<aegis_sandbox::SandboxConfig>,
    pub language_analysis_aliases: Vec<(String, String)>,
    pub language_analysis_budget: crate::analysis::OrchestrationBudget,
}

impl From<&AegisConfig> for RuntimeConfig {
    fn from(config: &AegisConfig) -> Self {
        let cwd = config
            .source_current_dir()
            .map(Path::to_path_buf)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();
        let sandbox = super::sandbox_ceiling(config, &cwd).map(|ceiling| ceiling.config);
        Self {
            mode: config.mode,
            ci_policy: config.ci_policy,
            strict_allowlist_override: config.allowlist_override_level,
            snapshot_policy: config.snapshot_policy,
            sandbox,
            language_analysis_aliases: config
                .language_analysis
                .trusted_aliases
                .iter()
                .map(|alias| (alias.alias.clone(), alias.canonical.clone()))
                .collect(),
            language_analysis_budget: crate::analysis::OrchestrationBudget {
                inline_source_limit_bytes: config.language_analysis.inline_source_limit_bytes
                    as usize,
                script_file_limit_bytes: config.language_analysis.script_file_limit_bytes,
                max_script_files: config.language_analysis.max_script_files as usize,
                max_depth: config.language_analysis.max_depth as u32,
                max_targets: config.language_analysis.max_targets as usize,
                max_aggregate_bytes: config.language_analysis.max_aggregate_bytes as usize,
                total_timeout: Duration::from_millis(config.language_analysis.timeout_ms),
            },
        }
    }
}

pub struct RuntimeContext {
    auto_prune_config: AegisConfig,
    runtime_config: RuntimeConfig,
    allowlist: Allowlist,
    blocklist: Blocklist,
    current_user: Option<String>,
    scanner: Arc<Scanner>,
    snapshot_registry_config: SnapshotRegistryConfig,
    snapshot_registry: OnceLock<SnapshotRegistry>,
    async_handle: Handle,
    audit_logger: AuditLogger,
    policy_rules: Vec<aegis_config::PolicyRule>,
}

pub struct AuditWriteOptions<'a> {
    pub allowlist_match: Option<&'a AllowlistMatch>,
    pub allowlist_effective: bool,
    pub ci_detected: bool,
    pub sandbox_status: SandboxStatus,
    pub snapshot_attempts: &'a [SnapshotAttempt],
    pub recovery_degradation: Option<RecoveryDegradation>,
    pub watch: Option<WatchAuditContext>,
}

impl<'a> AuditWriteOptions<'a> {
    pub fn for_plan(plan: &'a InterceptionPlan, sandbox_status: SandboxStatus) -> Self {
        Self {
            allowlist_match: plan.decision_context().allowlist_match(),
            allowlist_effective: plan.policy_decision().allowlist_effective,
            ci_detected: plan.decision_context().ci_detected(),
            sandbox_status,
            snapshot_attempts: &[],
            recovery_degradation: None,
            watch: None,
        }
    }
}

pub struct WatchAuditContext {
    pub source: Option<String>,
    pub cwd: Option<String>,
    pub id: Option<String>,
}

impl RuntimeContext {
    pub fn load(_verbose: bool, handle: Handle) -> Result<Self, AegisError> {
        let config = AegisConfig::load()?;
        Self::new(config, handle)
    }

    pub fn new(config: AegisConfig, handle: Handle) -> Result<Self, AegisError> {
        let policy_path = policy_star_path();
        Self::new_with_policy_path(config, handle, policy_path.as_deref())
    }

    #[cfg(test)]
    pub(crate) fn new_with_audit_path(
        config: AegisConfig,
        handle: Handle,
        audit_path: std::path::PathBuf,
    ) -> Result<Self, AegisError> {
        let mut context = Self::new_with_policy_path(config, handle, None)?;
        context.audit_logger = AuditLogger::new(audit_path);
        Ok(context)
    }

    #[cfg(test)]
    pub(crate) fn set_snapshot_registry_for_tests(
        &self,
        registry: aegis_snapshot::SnapshotRegistry,
    ) {
        self.snapshot_registry
            .set(registry)
            .map_err(|_| ())
            .expect("snapshot registry already initialized");
    }

    #[doc(hidden)]
    pub fn new_with_policy_path(
        config: AegisConfig,
        handle: Handle,
        explicit_policy_path: Option<&std::path::Path>,
    ) -> Result<Self, AegisError> {
        config.validate_runtime_requirements()?;
        let custom_patterns: Vec<aegis_types::Pattern> = config
            .custom_patterns
            .iter()
            .cloned()
            .map(Into::into)
            .collect();
        let scanner = aegis_scanner::scanner_for(&custom_patterns).map_err(|err| {
            if !config.has_full_custom_pattern_provenance() {
                return AegisError::from(err);
            }
            let current_dir = config
                .source_current_dir()
                .map(std::path::Path::to_path_buf)
                .or_else(|| std::env::current_dir().ok());
            let home_dir = config
                .source_home_dir()
                .map(std::path::Path::to_path_buf)
                .or_else(aegis_config::home_dir);
            aegis_config::locate_invalid_custom_pattern(
                &config,
                current_dir.as_deref(),
                home_dir.as_deref(),
            )
            .into()
        })?;
        let current_user = detect_effective_user();

        if let Some(star_path) = explicit_policy_path {
            aegis_config::reject_policy_star_at(star_path)?;
        }
        let policy_rules = config.rules.clone();

        Ok(Self {
            auto_prune_config: config.clone(),
            allowlist: Allowlist::from_layered_rules(&config.layered_allowlist_rules())?,
            blocklist: Blocklist::from_layered_rules(&config.layered_blocklist_rules())?,
            snapshot_registry_config: SnapshotRegistryConfig::try_new(&config)?,
            snapshot_registry: OnceLock::new(),
            async_handle: handle,
            audit_logger: build_audit_logger(&config),
            current_user,
            runtime_config: RuntimeConfig::from(&config),
            policy_rules,
            scanner,
        })
    }

    pub fn config(&self) -> &RuntimeConfig {
        &self.runtime_config
    }

    pub(crate) fn effective_config(&self) -> &AegisConfig {
        &self.auto_prune_config
    }

    pub fn policy_rules(&self) -> &[aegis_config::PolicyRule] {
        &self.policy_rules
    }

    pub fn assess(&self, cmd: &str) -> Assessment {
        self.scanner.assess(cmd)
    }

    pub fn assess_with_language_analysis(&self, cmd: &str) -> Assessment {
        self.assess_with_language_analysis_in_cwd(
            cmd,
            crate::analysis::AnalysisCwd::Resolved(Path::new(".")),
        )
    }

    pub fn assess_with_language_analysis_in_cwd(
        &self,
        cmd: &str,
        cwd: crate::analysis::AnalysisCwd<'_>,
    ) -> Assessment {
        self.async_handle
            .block_on(self.assess_with_language_analysis_async_in_cwd(cmd, cwd))
    }

    pub async fn assess_with_language_analysis_async(&self, cmd: &str) -> Assessment {
        self.assess_with_language_analysis_async_in_cwd(
            cmd,
            crate::analysis::AnalysisCwd::Resolved(Path::new(".")),
        )
        .await
    }

    pub async fn assess_with_language_analysis_async_in_cwd(
        &self,
        cmd: &str,
        cwd: crate::analysis::AnalysisCwd<'_>,
    ) -> Assessment {
        let baseline = self.assess(cmd);
        let aliases: Vec<(&str, &str)> = self
            .runtime_config
            .language_analysis_aliases
            .iter()
            .map(|(alias, canonical)| (alias.as_str(), canonical.as_str()))
            .collect();
        match crate::analysis::run_with_budget_in_cwd(
            cmd,
            cwd,
            &baseline,
            None,
            &aliases,
            self.runtime_config.language_analysis_budget,
            Some(&self.scanner),
        )
        .await
        {
            crate::analysis::Outcome::NotStarted { baseline }
            | crate::analysis::Outcome::Analyzed {
                assessment: baseline,
                ..
            } => baseline,
        }
    }

    pub fn current_user(&self) -> Option<&str> {
        self.current_user.as_deref()
    }

    fn snapshot_registry(&self) -> &SnapshotRegistry {
        self.snapshot_registry
            .get_or_init(|| SnapshotRegistry::from_runtime_config(&self.snapshot_registry_config))
    }

    pub fn allowlist_match(&self, context: &AllowlistContext<'_>) -> Option<AllowlistMatch> {
        self.allowlist.match_reason(context)
    }

    pub fn allowlist_match_for_command(
        &self,
        command: &str,
        cwd: Option<&Path>,
    ) -> Option<AllowlistMatch> {
        let now = OffsetDateTime::now_utc();
        let context = AllowlistContext::with_optional_scope(command, cwd, self.current_user(), now);

        self.allowlist_match(&context)
    }

    pub fn is_blocked(&self, context: &AllowlistContext<'_>) -> bool {
        self.blocklist.is_blocked(context)
    }

    pub fn is_blocked_for_command(&self, command: &str, cwd: Option<&Path>) -> bool {
        let now = OffsetDateTime::now_utc();
        let context = AllowlistContext::with_optional_scope(command, cwd, self.current_user(), now);

        self.is_blocked(&context)
    }

    pub fn create_snapshots(&self, cwd: &Path, cmd: &str, _verbose: bool) -> SnapshotCoverage {
        self.async_handle
            .block_on(self.create_snapshots_async(cwd, cmd))
    }

    pub fn applicable_snapshot_plugins(&self, cwd: &Path) -> Vec<&'static str> {
        self.async_handle
            .block_on(self.snapshot_registry().applicable_plugins(cwd))
    }

    pub async fn applicable_snapshot_plugins_async(&self, cwd: &Path) -> Vec<&'static str> {
        self.snapshot_registry().applicable_plugins(cwd).await
    }

    pub fn async_handle(&self) -> &Handle {
        &self.async_handle
    }

    pub async fn create_snapshots_async(
        &self,
        cwd: &std::path::Path,
        cmd: &str,
    ) -> SnapshotCoverage {
        let registry = self.snapshot_registry();
        let mut coverage = registry.snapshot_all(cwd, cmd).await;
        registry.check_readiness(&mut coverage).await;
        coverage
    }

    pub async fn create_snapshots_for_plan_async(
        &self,
        cwd: &Path,
        cmd: &str,
        git_targets: &crate::planning::GitTargetPlan,
    ) -> SnapshotCoverage {
        let registry = self.snapshot_registry();
        let mut coverage = if git_targets.process_cwd {
            registry.snapshot_all(cwd, cmd).await
        } else {
            registry.snapshot_all_except("git", cwd, cmd).await
        };
        for path in git_targets.resolved_paths() {
            coverage.merge(registry.snapshot_plugin_at("git", path, cmd).await);
        }
        registry.check_readiness(&mut coverage).await;
        coverage
    }

    pub fn create_snapshots_for_plan(
        &self,
        cwd: &Path,
        cmd: &str,
        git_targets: &crate::planning::GitTargetPlan,
    ) -> SnapshotCoverage {
        self.async_handle
            .block_on(self.create_snapshots_for_plan_async(cwd, cmd, git_targets))
    }

    pub fn git_snapshot_enabled(&self) -> bool {
        self.snapshot_registry()
            .configured_provider_names()
            .contains(&"git")
    }

    pub async fn git_snapshot_applicable_at_async(&self, dir: &Path) -> bool {
        self.snapshot_registry()
            .plugin_applicable_at("git", dir)
            .await
    }

    pub fn git_stage_report(
        &self,
        cmd: &str,
        assessment: &Assessment,
    ) -> aegis_scanner::GitStageReport {
        self.scanner.git_stage_report(cmd, assessment)
    }

    pub fn append_audit_entry(
        &self,
        assessment: &Assessment,
        decision: Decision,
        snapshots: &[SnapshotRecord],
        explanation: &CommandExplanation,
        options: AuditWriteOptions<'_>,
    ) -> Result<(), AegisError> {
        let entry = self.build_audit_entry(assessment, decision, snapshots, explanation, &options);
        self.audit_logger.append(entry)?;
        self.complete_automatic_prune(snapshots)?;
        Ok(())
    }

    fn complete_automatic_prune(&self, snapshots: &[SnapshotRecord]) -> Result<(), AegisError> {
        if !self.auto_prune_config.prune.enabled || snapshots.is_empty() {
            return Ok(());
        }
        let config = self.auto_prune_config.clone();

        match Handle::try_current() {
            Ok(handle) if handle.runtime_flavor() == RuntimeFlavor::MultiThread => {
                tokio::task::block_in_place(|| self.async_handle.block_on(automatic_prune(config)))
            }
            Ok(_) => std::thread::scope(|scope| {
                scope
                    .spawn(move || {
                        tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()?
                            .block_on(automatic_prune(config))
                    })
                    .join()
                    .map_err(|_| AegisError::Internal {
                        detail: "automatic snapshot prune worker panicked".to_string(),
                    })?
            }),
            Err(_) => self.async_handle.block_on(automatic_prune(config)),
        }
    }

    fn build_audit_entry(
        &self,
        assessment: &Assessment,
        decision: Decision,
        snapshots: &[SnapshotRecord],
        explanation: &CommandExplanation,
        options: &AuditWriteOptions<'_>,
    ) -> AuditEntry {
        let allowlist_pattern = (options.allowlist_effective)
            .then(|| options.allowlist_match.map(|m| m.pattern.clone()))
            .flatten();
        let allowlist_reason = (options.allowlist_effective)
            .then(|| options.allowlist_match.map(|m| m.reason.clone()))
            .flatten();

        let mut entry = AuditEntry::new(
            assessment.command.raw.clone(),
            assessment.risk,
            assessment.matched.iter().map(Into::into).collect(),
            decision,
            snapshots.iter().map(Into::into).collect(),
            allowlist_pattern,
            allowlist_reason,
        )
        .with_explanation(
            explanation
                .clone()
                .with_runtime_outcome(build_outcome_explanation(decision, snapshots)),
        )
        .with_policy_context(
            self.runtime_config.mode,
            options.ci_detected,
            options.allowlist_match.is_some(),
            options.allowlist_effective,
        )
        .with_sandbox_status(options.sandbox_status)
        .with_effect_opaque(assessment.effect_opaque)
        .with_required_backstops(explanation.policy.snapshots_required, false)
        .with_basis(assessment.basis())
        .with_analysis(assessment.analysis.clone())
        .with_cwd(
            std::env::current_dir()
                .ok()
                .map(|dir| dir.to_string_lossy().into_owned()),
        );

        if !options.snapshot_attempts.is_empty() {
            entry = entry.with_snapshot_attempts(options.snapshot_attempts.to_vec());
        }
        if let Some(degradation) = options.recovery_degradation {
            entry = entry.with_recovery_degradation(degradation);
        }
        match &options.watch {
            Some(watch) => {
                entry.with_watch_context(watch.source.clone(), watch.cwd.clone(), watch.id.clone())
            }
            None => entry,
        }
    }
}

fn audit_error_category(error: &AuditError) -> &'static str {
    match error {
        AuditError::InsecureAuditArtifact { .. } => "insecure audit artifact",
        AuditError::Io(_) | AuditError::WriteFailed { .. } => "audit log i/o failure",
        AuditError::Parse { .. } => "unreadable audit log line",
        _ => "audit log error",
    }
}

async fn automatic_prune(config: AegisConfig) -> Result<(), AegisError> {
    let registry_config = SnapshotRegistryConfig::for_rollback_from_config(&config)?;
    let registry = SnapshotRegistry::from_runtime_config(&registry_config);
    let logger = AuditLogger::from_audit_config(&config.audit);
    let ledger = match SnapshotLedger::load(&logger) {
        Ok(ledger) => ledger,
        Err(error) => {
            tracing::warn!(
                reason = audit_error_category(&error),
                "automatic snapshot prune skipped: the audit log could not be read"
            );
            return Ok(());
        }
    };
    let records = registry.resolve_prunable_records(&ledger.into_live_snapshots());
    let candidates = RetentionPolicy::from_config(&config.prune).apply(&records, SystemClock.now());
    for record in candidates {
        if let Err(error) = registry.delete(&record.plugin, &record.snapshot_id).await {
            tracing::warn!(plugin = %record.plugin, id = %record.snapshot_id, %error, "automatic snapshot prune failed");
            continue;
        }
        let entry = AuditEntry::new(
            format!("aegis prune {}", record.snapshot_id),
            aegis_types::RiskLevel::Safe,
            Vec::new(),
            Decision::Pruned,
            vec![AuditSnapshot {
                plugin: record.plugin,
                snapshot_id: record.snapshot_id,
                git_target: None,
            }],
            None,
            None,
        );
        logger.append(entry)?;
    }
    Ok(())
}

fn build_audit_logger(config: &AegisConfig) -> AuditLogger {
    AuditLogger::from_audit_config(&config.audit)
}

fn policy_star_path() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(|h| {
        std::path::PathBuf::from(h)
            .join(".aegis")
            .join("policy.star")
    })
}

#[cfg(test)]
mod tests;
