use std::fs;
use std::path::Path;

use super::*;
use crate::config::{CiPolicy, UserPattern};
use crate::decision::{ExecutionTransport, PolicyAction, PolicyRationale};
use crate::explanation::formatter::allowlist_explanation_from;
use crate::explanation::{
    CommandExplanation, ExecutionContextExplanation, ExplainedPatternMatch, PolicyExplanation,
    ScanExplanation,
};
use crate::interceptor::RiskLevel;
use crate::interceptor::patterns::{Category, PatternSource};
use tempfile::TempDir;
use time::OffsetDateTime;

fn test_handle() -> Handle {
    // Leak a runtime so the Handle outlives each test.
    // This is fine for unit tests — the OS reclaims it on process exit.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let handle = rt.handle().clone();
    std::mem::forget(rt);
    handle
}

#[test]
fn custom_patterns_are_built_once_into_runtime_scanner() {
    let _guard = SCANNER_BUILD_COUNT_MUTEX.lock().unwrap();

    let mut config = AegisConfig::default();
    config.custom_patterns = vec![UserPattern {
        id: "USR-CTX-001".to_string(),
        category: Category::Cloud,
        risk: RiskLevel::Warn,
        pattern: "internal-teardown".to_string(),
        description: "custom warning".to_string(),
        safe_alt: None,
        justification: None,
    }];

    let context = RuntimeContext::new(config, test_handle()).unwrap();
    let assessment = context.assess("internal-teardown && rm -rf /tmp/demo");

    assert_eq!(assessment.risk, RiskLevel::Danger);
    assert!(
        assessment
            .matched
            .iter()
            .any(|matched| matched.pattern.id.as_ref() == "USR-CTX-001"
                && matched.pattern.source == PatternSource::Custom),
        "custom pattern must fire alongside the built-in one"
    );
    assert!(
        assessment
            .matched
            .iter()
            .any(|matched| matched.pattern.source == PatternSource::Builtin),
        "a built-in pattern must fire too, proving the merged set — not just the custom pattern — reached the runtime scanner"
    );
}

#[test]
fn invalid_custom_scanner_aborts_runtime_context_construction() {
    let _guard = SCANNER_BUILD_COUNT_MUTEX.lock().unwrap();

    let mut config = AegisConfig::default();
    config.custom_patterns = vec![UserPattern {
        id: "FS-001".to_string(),
        category: Category::Filesystem,
        risk: RiskLevel::Warn,
        pattern: "echo hello".to_string(),
        description: "duplicate id".to_string(),
        safe_alt: None,
        justification: None,
    }];

    let err = match RuntimeContext::new(config, test_handle()) {
        Ok(_) => panic!("invalid custom patterns must abort runtime context construction"),
        Err(err) => err,
    };
    assert!(err.to_string().contains("duplicate pattern id"));
    assert!(
        !err.to_string().contains("invalid config"),
        "a directly-constructed config carries no file provenance for its custom \
         patterns, so the error must not fabricate a config file path: {err}"
    );
}

/// Serializes tests that measure `aegis_scanner::try_new_call_count_for_tests`
/// deltas — the counter is process-global, so concurrent scanner builds from
/// other custom-pattern tests in this file would make the delta flaky.
static SCANNER_BUILD_COUNT_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A config with custom patterns contributed by both the global and project
/// layers must build the scanner exactly once on the success path — the
/// layered file-load path used to build it once per layer plus once more in
/// `RuntimeContext` (issue #399).
#[test]
fn layered_custom_patterns_build_the_scanner_exactly_once() {
    let _guard = SCANNER_BUILD_COUNT_MUTEX.lock().unwrap();

    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();

    let global_dir = home.path().join(".config").join("aegis");
    fs::create_dir_all(&global_dir).unwrap();
    fs::write(
        global_dir.join("config.toml"),
        r#"
[[custom_patterns]]
id = "USR-GLOBAL"
category = "Filesystem"
risk = "Warn"
pattern = "custom-global-only-pattern"
description = "global entry"
"#,
    )
    .unwrap();

    fs::write(
        workspace.path().join(".aegis.toml"),
        r#"
[[custom_patterns]]
id = "USR-PROJECT"
category = "Filesystem"
risk = "Warn"
pattern = "custom-project-only-pattern"
description = "project entry"
"#,
    )
    .unwrap();

    // Warm the cached built-in scanner first: its one-time build (shared by
    // every `assess()` call in this process) happens on first use from
    // *any* concurrently-running test, not just this one. Forcing it here
    // keeps that one-off build out of the measurement window below, so the
    // delta only reflects the layered-config path this test exercises.
    let _ = aegis_scanner::scanner_for(&[]);

    let before = aegis_scanner::try_new_call_count_for_tests();
    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    assert_eq!(config.custom_patterns.len(), 2);

    let _context = RuntimeContext::new(config, test_handle()).unwrap();
    let after = aegis_scanner::try_new_call_count_for_tests();

    assert_eq!(
        after - before,
        1,
        "exactly one Scanner::try_new call is expected on the success path"
    );
}

#[cfg(not(windows))]
#[test]
fn config_is_shared_across_runtime_dependencies() {
    use crate::config::AllowlistRule;

    let mut config = AegisConfig::default();
    config.allowlist_override_level = AllowlistOverrideLevel::Danger;
    config.allowlist = vec![AllowlistRule {
        pattern: "echo trusted".to_string(),
        cwd: Some(".".to_string()),
        user: None,
        expires_at: None,
        reason: "runtime test".to_string(),
    }];
    config.auto_snapshot_git = false;
    config.auto_snapshot_docker = false;
    config.ci_policy = CiPolicy::Allow;

    let context = RuntimeContext::new(config.clone(), test_handle()).unwrap();

    assert_eq!(context.config().mode, config.mode);
    assert_eq!(context.config().ci_policy, config.ci_policy);
    assert_eq!(
        context.config().strict_allowlist_override,
        AllowlistOverrideLevel::Danger
    );
    assert_eq!(context.config().snapshot_policy, config.snapshot_policy);
    let Some(current_user) = context.current_user() else {
        panic!("test requires a resolvable user");
    };
    let allowlist_ctx =
        AllowlistContext::new("echo trusted", Path::new("."), current_user, now_utc());
    assert_eq!(
        context.allowlist_match(&allowlist_ctx).map(|m| m.pattern),
        Some("echo trusted".to_string())
    );
    assert!(
        context
            .create_snapshots(Path::new("."), "rm -rf /tmp/runtime-context-test", false)
            .records
            .is_empty()
    );
    assert_eq!(context.config().ci_policy, CiPolicy::Allow);
    assert_eq!(
        context.config().strict_allowlist_override,
        AllowlistOverrideLevel::Danger
    );
    assert_eq!(context.config().snapshot_policy, config.snapshot_policy);
}

#[test]
fn runtime_context_rejects_expired_allowlist_rules() {
    use crate::config::AllowlistRule;
    use time::{OffsetDateTime, format_description::well_known::Rfc3339};

    let mut config = AegisConfig::default();
    config.allowlist = vec![AllowlistRule {
        pattern: "terraform destroy -target=module.test.*".to_string(),
        cwd: None,
        user: None,
        expires_at: Some(OffsetDateTime::parse("2020-01-01T00:00:00Z", &Rfc3339).unwrap()),
        reason: "expired teardown".to_string(),
    }];

    let err = match RuntimeContext::new(config, test_handle()) {
        Ok(_) => panic!("expired allowlist rules must be rejected before runtime setup"),
        Err(err) => err,
    };

    assert!(err.to_string().contains("expired"));
}

#[test]
fn runtime_context_rejects_unscoped_allowlist_rules() {
    use crate::config::AllowlistRule;

    let mut config = AegisConfig::default();
    config.allowlist = vec![AllowlistRule {
        pattern: "terraform destroy *".to_string(),
        cwd: None,
        user: None,
        expires_at: None,
        reason: "too broad".to_string(),
    }];

    let err = match RuntimeContext::new(config, test_handle()) {
        Ok(_) => panic!("runtime context must reject unscoped allowlist rules"),
        Err(err) => err,
    };
    assert!(err.to_string().contains("must declare cwd or user scope"));
}

#[test]
fn runtime_context_accepts_scoped_allowlist_rules() {
    use crate::config::AllowlistRule;

    let mut config = AegisConfig::default();
    config.allowlist = vec![AllowlistRule {
        pattern: "terraform destroy -target=module.test.*".to_string(),
        cwd: Some("/srv/infra".to_string()),
        user: None,
        expires_at: None,
        reason: "scoped teardown".to_string(),
    }];

    let context = RuntimeContext::new(config, test_handle()).unwrap();
    let allowlist_ctx = AllowlistContext::with_optional_scope(
        "terraform destroy -target=module.test.api",
        Some(Path::new("/srv/infra")),
        context.current_user(),
        now_utc(),
    );

    assert!(context.allowlist_match(&allowlist_ctx).is_some());
}

#[test]
fn runtime_context_accepts_user_scoped_allowlist_rules() {
    use crate::config::AllowlistRule;

    let Some(current_user) = detect_effective_user() else {
        return;
    };
    let mut config = AegisConfig::default();
    config.allowlist = vec![AllowlistRule {
        pattern: "terraform destroy -target=module.test.*".to_string(),
        cwd: None,
        user: Some(current_user.clone()),
        expires_at: None,
        reason: "scoped teardown".to_string(),
    }];

    let context = RuntimeContext::new(config, test_handle()).unwrap();
    let Some(current_user) = context.current_user() else {
        panic!("test requires a resolvable user");
    };
    let allowlist_ctx = AllowlistContext::new(
        "terraform destroy -target=module.test.api",
        Path::new("/srv/infra"),
        current_user,
        now_utc(),
    );

    assert!(context.allowlist_match(&allowlist_ctx).is_some());
}

#[test]
fn unknown_user_does_not_match_user_scoped_allowlist_rule() {
    use crate::config::AllowlistRule;

    let mut config = AegisConfig::default();
    config.allowlist = vec![AllowlistRule {
        pattern: "terraform destroy -target=module.test.*".to_string(),
        cwd: None,
        user: Some("ci".to_string()),
        expires_at: None,
        reason: "user scoped teardown".to_string(),
    }];

    let mut context = RuntimeContext::new(config, test_handle()).unwrap();
    context.current_user = None;

    assert!(
        context
            .allowlist_match_for_command(
                "terraform destroy -target=module.test.api",
                Some(Path::new("/srv/infra")),
            )
            .is_none()
    );
}

#[test]
fn unknown_cwd_does_not_match_cwd_scoped_allowlist_rule() {
    use crate::config::AllowlistRule;

    let mut config = AegisConfig::default();
    config.allowlist = vec![AllowlistRule {
        pattern: "terraform destroy -target=module.test.*".to_string(),
        cwd: Some("/srv/infra".to_string()),
        user: None,
        expires_at: None,
        reason: "scoped teardown".to_string(),
    }];

    let context = RuntimeContext::new(config, test_handle()).unwrap();

    assert!(
        context
            .allowlist_match_for_command("terraform destroy -target=module.test.api", None,)
            .is_none()
    );
}

#[cfg(not(windows))]
#[test]
fn load_for_preserves_project_allowlist_precedence_into_runtime_matching() {
    let workspace = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let global_dir = home.path().join(".config/aegis");
    fs::create_dir_all(&global_dir).unwrap();

    let workspace_cwd = workspace.path().to_string_lossy();
    fs::write(
        global_dir.join("config.toml"),
        format!(
            r#"
[[allow]]
pattern = "terraform destroy -target=module.test.*"
cwd = "{workspace_cwd}"
reason = "global teardown"
expires_at = "2030-01-01T00:00:00Z"
"#
        ),
    )
    .unwrap();
    fs::write(
        workspace.path().join(".aegis.toml"),
        format!(
            r#"
[[allow]]
pattern = "terraform destroy -target=module.test.*"
cwd = "{workspace_cwd}"
reason = "project teardown"
expires_at = "2030-01-01T00:00:00Z"
"#
        ),
    )
    .unwrap();

    let config = AegisConfig::load_for(workspace.path(), Some(home.path())).unwrap();
    let context = RuntimeContext::new(config, test_handle()).unwrap();
    let matched = context
        .allowlist_match_for_command(
            "terraform destroy -target=module.test.api",
            Some(workspace.path()),
        )
        .unwrap();

    assert_eq!(matched.reason, "project teardown");
    assert_eq!(
        matched.source_layer,
        crate::config::ConfigSourceLayer::Project
    );
}

#[test]
fn runtime_context_uses_external_handle_for_snapshots() {
    // Persistent runtime: RuntimeContext must accept an external Handle
    // instead of owning its own Runtime. This proves:
    // 1. RuntimeContext::new accepts a Handle parameter
    // 2. create_snapshots works through the external handle
    // 3. No internal SnapshotRuntime::Ready(Runtime) exists
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let handle = rt.handle().clone();

    let mut config = AegisConfig::default();
    config.auto_snapshot_git = false;
    config.auto_snapshot_docker = false;

    let context = RuntimeContext::new(config, handle).unwrap();
    let snapshots = context.create_snapshots(Path::new("."), "echo test", false);

    // With both snapshot plugins disabled, result is empty — but the call
    // must succeed without panicking (proving the external handle works).
    assert!(snapshots.records.is_empty());
}

#[test]
fn runtime_context_new_does_not_build_snapshot_registry_eagerly() {
    crate::snapshot::reset_snapshot_registry_build_count_for_tests();

    let mut config = AegisConfig::default();
    config.snapshot_policy = SnapshotPolicy::Selective;
    config.auto_snapshot_git = true;
    config.auto_snapshot_docker = false;

    let _context = RuntimeContext::new(config, test_handle()).unwrap();

    assert_eq!(
        crate::snapshot::snapshot_registry_build_count_for_tests(),
        0
    );
}

#[test]
fn runtime_context_new_requires_handle_parameter() {
    // Verify the two-argument signature is the only way to construct.
    // This test will fail to compile if RuntimeContext::new still accepts
    // only Config (one argument).
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let handle = rt.handle().clone();
    let config = AegisConfig::default();

    // Must compile with two arguments.
    let _context = RuntimeContext::new(config, handle).unwrap();
}

#[test]
fn append_audit_entry_enriches_explanation_with_runtime_outcome() {
    let context = RuntimeContext::new(AegisConfig::default(), test_handle()).unwrap();
    let assessment = context.assess("rm -rf target");
    let snapshots = vec![SnapshotRecord {
        plugin: "git",
        snapshot_id: "snap-1".to_string(),
    }];
    let explanation = CommandExplanation {
        scan: ScanExplanation {
            highest_risk: assessment.risk,
            decision_source: assessment.decision_source(),
            basis: assessment.basis(),
            matched_patterns: vec![ExplainedPatternMatch {
                id: "FS-001".to_string(),
                risk: RiskLevel::Danger,
                description: "recursive delete".to_string(),
                matched_text: "rm -rf".to_string(),
                justification: None,
            }],
        },
        policy: PolicyExplanation {
            action: PolicyAction::Prompt,
            rationale: PolicyRationale::RequiresConfirmation,
            requires_confirmation: true,
            snapshots_required: true,
            allowlist_effective: false,
            block_reason: None,
        },
        context: ExecutionContextExplanation {
            mode: context.config().mode,
            transport: ExecutionTransport::Shell,
            ci_detected: false,
            allowlist_match: None,
            applicable_snapshot_plugins: vec!["git".to_string()],
        },
        outcome: None,
    };

    let entry = context.build_audit_entry(
        &assessment,
        Decision::Approved,
        &snapshots,
        &explanation,
        AuditWriteOptions {
            allowlist_match: None,
            allowlist_effective: false,
            ci_detected: false,
            sandbox_status: SandboxStatus::NotConfigured,
        },
        None,
    );

    let outcome = entry
        .as_base()
        .explanation
        .as_ref()
        .and_then(|value| value.outcome.as_ref());
    assert_eq!(
        outcome.map(|value| value.decision),
        Some(crate::explanation::ExecutionDecisionExplanation::Approved)
    );
    assert_eq!(
        outcome
            .and_then(|value| value.snapshots.first())
            .map(|value| value.plugin.as_str()),
        Some("git")
    );
}

#[test]
fn append_audit_entry_preserves_allowlist_context_fields() {
    let mut config = AegisConfig::default();
    config.allowlist = vec![crate::config::AllowlistRule {
        pattern: "rm -rf target".to_string(),
        cwd: Some(".".to_string()),
        user: None,
        expires_at: None,
        reason: "approved cleanup".to_string(),
    }];
    let context = RuntimeContext::new(config, test_handle()).unwrap();
    let assessment = context.assess("rm -rf target");
    let allowlist_match =
        context.allowlist_match_for_command("rm -rf target", Some(Path::new(".")));
    let explanation = CommandExplanation {
        scan: ScanExplanation {
            highest_risk: assessment.risk,
            decision_source: assessment.decision_source(),
            basis: assessment.basis(),
            matched_patterns: vec![ExplainedPatternMatch {
                id: "FS-001".to_string(),
                risk: RiskLevel::Danger,
                description: "recursive delete".to_string(),
                matched_text: "rm -rf".to_string(),
                justification: None,
            }],
        },
        policy: PolicyExplanation {
            action: PolicyAction::AutoApprove,
            rationale: PolicyRationale::AllowlistOverride,
            requires_confirmation: false,
            snapshots_required: false,
            allowlist_effective: true,
            block_reason: None,
        },
        context: ExecutionContextExplanation {
            mode: context.config().mode,
            transport: ExecutionTransport::Shell,
            ci_detected: false,
            allowlist_match: allowlist_match.as_ref().map(allowlist_explanation_from),
            applicable_snapshot_plugins: Vec::new(),
        },
        outcome: None,
    };

    let entry = context.build_audit_entry(
        &assessment,
        Decision::AutoApproved,
        &[],
        &explanation,
        AuditWriteOptions {
            allowlist_match: allowlist_match.as_ref(),
            allowlist_effective: true,
            ci_detected: false,
            sandbox_status: SandboxStatus::NotConfigured,
        },
        None,
    );

    let base = entry.as_base();
    assert_eq!(base.allowlist_pattern.as_deref(), Some("rm -rf target"));
    assert_eq!(base.allowlist_reason.as_deref(), Some("approved cleanup"));
    assert_eq!(base.allowlist_matched, Some(true));
    assert_eq!(base.allowlist_effective, Some(true));
}

#[test]
fn build_audit_entry_records_effect_opaque_and_backstop_state_from_runtime_facts() {
    // ADR-016 / Standards #1: the audit entry must reflect the assessment's
    // `effect_opaque` flag and the policy's `snapshots_required` decision —
    // not the `Some(false)` defaults emitted by `AuditEntry::new`. Otherwise a
    // real `sh ./cleanup.sh` execution that policy required recovery for would
    // be logged as if no backstop was ever needed.
    let context = RuntimeContext::new(AegisConfig::default(), test_handle()).unwrap();
    // `sh ./cleanup.sh` is Safe to the quick scan yet effect-opaque.
    let assessment = context.assess("sh ./cleanup.sh");
    assert!(
        assessment.effect_opaque,
        "fixture premise: command must be effect-opaque"
    );
    let explanation = CommandExplanation {
        scan: ScanExplanation {
            highest_risk: assessment.risk,
            decision_source: assessment.decision_source(),
            basis: assessment.basis(),
            matched_patterns: Vec::new(),
        },
        policy: PolicyExplanation {
            action: PolicyAction::AutoApprove,
            rationale: PolicyRationale::SafeCommand,
            requires_confirmation: false,
            // Policy resolved a pre-exec recovery backstop for this command.
            snapshots_required: true,
            allowlist_effective: false,
            block_reason: None,
        },
        context: ExecutionContextExplanation {
            mode: context.config().mode,
            transport: ExecutionTransport::Shell,
            ci_detected: false,
            allowlist_match: None,
            applicable_snapshot_plugins: vec!["git".to_string()],
        },
        outcome: None,
    };

    let entry = context.build_audit_entry(
        &assessment,
        Decision::AutoApproved,
        &[],
        &explanation,
        AuditWriteOptions {
            allowlist_match: None,
            allowlist_effective: false,
            ci_detected: false,
            sandbox_status: SandboxStatus::NotConfigured,
        },
        Some(aegis_types::RecoveryDegradation::NoSnapshotAvailable),
    );

    let base = entry.as_base();
    assert_eq!(
        base.effect_opaque,
        Some(true),
        "audit must record the assessment's effect-opaque flag, not the default"
    );
    assert_eq!(
        base.snapshots_required,
        Some(true),
        "audit must record the policy's recovery requirement, not the default"
    );
    assert_eq!(
        base.confinement_required,
        Some(false),
        "audit must record the v1 confinement state (optional strict tier not engaged)"
    );
    assert_eq!(
        base.recovery_degradation,
        Some(aegis_types::RecoveryDegradation::NoSnapshotAvailable)
    );
}

#[test]
fn build_audit_entry_records_plain_safe_command_without_backstops() {
    // Orthogonal direction: a non-effect-opaque safe command whose policy did
    // not require recovery must record `Some(false)` for both axes — the
    // honest decision, not an unset default.
    let context = RuntimeContext::new(AegisConfig::default(), test_handle()).unwrap();
    let assessment = context.assess("echo ok");
    assert!(
        !assessment.effect_opaque,
        "fixture premise: command must not be effect-opaque"
    );
    let explanation = CommandExplanation {
        scan: ScanExplanation {
            highest_risk: assessment.risk,
            decision_source: assessment.decision_source(),
            basis: assessment.basis(),
            matched_patterns: Vec::new(),
        },
        policy: PolicyExplanation {
            action: PolicyAction::AutoApprove,
            rationale: PolicyRationale::SafeCommand,
            requires_confirmation: false,
            snapshots_required: false,
            allowlist_effective: false,
            block_reason: None,
        },
        context: ExecutionContextExplanation {
            mode: context.config().mode,
            transport: ExecutionTransport::Shell,
            ci_detected: false,
            allowlist_match: None,
            applicable_snapshot_plugins: Vec::new(),
        },
        outcome: None,
    };

    let entry = context.build_audit_entry(
        &assessment,
        Decision::AutoApproved,
        &[],
        &explanation,
        AuditWriteOptions {
            allowlist_match: None,
            allowlist_effective: false,
            ci_detected: false,
            sandbox_status: SandboxStatus::NotConfigured,
        },
        None,
    );

    let base = entry.as_base();
    assert_eq!(base.effect_opaque, Some(false));
    assert_eq!(base.snapshots_required, Some(false));
    assert_eq!(base.confinement_required, Some(false));
}

fn now_utc() -> OffsetDateTime {
    OffsetDateTime::now_utc()
}

// This build lacks the `starlark-policy` feature (the default for
// `cargo test --workspace`), so an explicit `policy.star` path takes the
// "compiled without the feature" branch below rather than a real load.
#[cfg(not(feature = "starlark-policy"))]
#[test]
fn policy_star_without_the_starlark_feature_is_an_internal_error_not_a_config_error() {
    let workspace = TempDir::new().unwrap();
    let policy_path = workspace.path().join("policy.star");
    fs::write(&policy_path, "").unwrap();

    let err = match RuntimeContext::new_with_policy_path(
        AegisConfig::default(),
        test_handle(),
        Some(&policy_path),
    ) {
        Ok(_) => panic!("a policy.star path must fail closed without the starlark-policy feature"),
        Err(err) => err,
    };

    assert_eq!(
        err.to_string(),
        format!(
            "internal error: policy.star exists at {} but this Aegis build was compiled without the starlark-policy feature",
            policy_path.display()
        )
    );
    assert!(
        !err.is_config_fault(),
        "compiling without an optional feature is Aegis's own fault, not a config fault"
    );
}
