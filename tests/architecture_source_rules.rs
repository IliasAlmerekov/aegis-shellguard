//! Architecture source-rule tests.
//!
//! Source-grep checks for the rules in `ARCHITECTURE.md` §2 and §4 that a
//! `Cargo.toml` dependency check cannot see: edges between modules of one
//! crate, and forbidden patterns inside a crate's production code. Crate-level
//! edges live in `tests/architecture_boundaries.rs`.
//!
//! Conventions:
//! - Every check walks production code only (see `walker`). It skips files
//!   that a parent module declares under `#[cfg(test)]`, and `strip_test_code`
//!   removes `#[cfg(test)]` items inside a file. Crate-level `tests/`,
//!   `benches/` and `examples/` directories are never walked.
//! - A walk over a missing or empty directory panics, so a moved directory
//!   cannot turn a check into a silent no-op.

use std::fs;

#[path = "architecture_source_rules/walker.rs"]
mod walker;
use walker::{contains_ident, production_rs_files, repo_root, strip_test_code};

fn read_file(relative: &str) -> String {
    fs::read_to_string(repo_root().join(relative))
        .unwrap_or_else(|err| panic!("failed to read {relative}: {err}"))
}

/// Read a source file and strip `#[cfg(test)]`-gated items so that boundary
/// checks only see production code.
fn read_production(relative: &str) -> String {
    strip_test_code(&read_file(relative))
}

fn assert_absent(source: &str, needle: &str, file: &str, rule: &str) {
    assert!(
        !source.contains(needle),
        "{file}: forbidden pattern {needle:?} — rule: {rule}"
    );
}

// ── §2 Diagnostic stream — no raw command or env value in a tracing field ────

/// CONVENTION.md §2: a `tracing` field must never carry a raw command string
/// or an environment variable value. Source-grep floor, not a proof
/// (ADR-033) — it cannot see a path that arrives through `Display` on an
/// error type such as `SnapshotError`.
#[test]
fn tracing_fields_never_carry_raw_command_or_env_value() {
    let files = production_rs_files("src")
        .into_iter()
        .chain(production_rs_files("crates"));

    for (rel, src) in files {
        for forbidden in ["%cmd", "%command", "%raw_command", "%env_value", "%env_var"] {
            assert_absent(
                &src,
                forbidden,
                &rel,
                "CONVENTION.md §2: tracing fields must never carry a raw command \
                 string or an environment variable value",
            );
        }
    }
}

// ── §4 Forbidden edges — Policy engine is pure ────────────────────────────────

/// I1 + §4: the policy engine is a pure function. No I/O, no process spawning,
/// no tokio, no filesystem, no logging. The engine lives in `aegis-policy`;
/// `src/decision` only re-exports it.
#[test]
fn decision_engine_is_pure_no_io() {
    for (rel, src) in production_rs_files("crates/aegis-policy/src") {
        for forbidden in [
            "std::fs",
            "std::process",
            "tokio::",
            "std::io",
            "std::env",
            "tracing::",
            "eprintln!",
            "println!",
        ] {
            assert_absent(
                &src,
                forbidden,
                &rel,
                "I1: policy engine must be a pure function — no I/O",
            );
        }
    }
}

// ── §4 Forbidden edges — Interceptor is a leaf ────────────────────────────────

/// §4: `interceptor/**` may not depend on `audit`, `snapshot`, `ui`, or
/// `runtime`. Scanner is transport-agnostic and has no recovery/logging/UI
/// concerns. `src/interceptor` holds real root-crate code (the scanner cache
/// and `assess()`), so a `Cargo.toml` check cannot see these module edges.
#[test]
fn interceptor_has_no_downstream_dependencies() {
    for (rel, src) in production_rs_files("src/interceptor") {
        for forbidden in [
            "use crate::audit",
            "use crate::snapshot",
            "use crate::ui",
            "use crate::runtime",
            "use crate::planning",
            "use crate::decision",
        ] {
            assert_absent(
                &src,
                forbidden,
                &rel,
                "§4: interceptor must not depend on audit/snapshot/ui/runtime/planning/decision",
            );
        }
    }
}

// ── §4 Forbidden edges — UI is rendering only ─────────────────────────────────

/// §4: `ui/**` may not write audit entries, run snapshot business logic, or
/// depend on runtime/planning. The documented allow-leak: importing the
/// `SnapshotRecord` display type from `snapshot`. Calling
/// `SnapshotRegistry::*`, `.snapshot_all(`, or `.rollback(` is forbidden.
#[test]
fn ui_does_not_call_audit_or_snapshot_business_logic() {
    // Check both the shim layer (src/ui) and the real implementation (crates/aegis-tui/src).
    let files = production_rs_files("src/ui")
        .into_iter()
        .chain(production_rs_files("crates/aegis-tui/src"));

    for (rel, src) in files {
        // No audit coupling at all (binary-crate or workspace-crate form).
        assert_absent(
            &src,
            "use crate::audit",
            &rel,
            "§4: UI must not depend on audit",
        );
        assert_absent(
            &src,
            "use aegis_audit",
            &rel,
            "§4: UI must not depend on audit",
        );
        assert_absent(
            &src,
            "AuditLogger",
            &rel,
            "§4: UI must not reference AuditLogger",
        );

        // No runtime/planning orchestration leaks.
        assert_absent(
            &src,
            "use crate::runtime",
            &rel,
            "§4: UI must not depend on runtime",
        );
        assert_absent(
            &src,
            "use crate::planning",
            &rel,
            "§4: UI must not depend on planning",
        );

        // Snapshot business logic is forbidden; only SnapshotRecord
        // (data type used for display) is allowed.
        for forbidden in [
            "SnapshotRegistry",
            ".snapshot_all(",
            ".rollback(",
            "snapshot_registry",
        ] {
            assert_absent(
                &src,
                forbidden,
                &rel,
                "§4: UI may import SnapshotRecord but must not invoke snapshot business logic",
            );
        }
    }
}

// ── §4 Forbidden edges — Transports go through planning ───────────────────────

/// Production paths allowed to name `evaluate_policy`, each with the reason.
const EVALUATE_POLICY_ALLOWED: &[(&str, &str)] = &[
    (
        "src/planning/",
        "the sanctioned consumer: planning is the one caller of the policy engine",
    ),
    (
        "src/decision/mod.rs",
        "re-exports the engine from aegis-policy for the rest of the crate",
    ),
];

/// I4 + §4: no module of the root crate outside `planning` may call
/// `evaluate_policy`; transports (`shell_flow`, `watch`, `install`) must go
/// through `planning::*`.
#[test]
fn transports_route_policy_through_planning_module() {
    for (rel, src) in production_rs_files("src") {
        if EVALUATE_POLICY_ALLOWED
            .iter()
            .any(|(allowed, _)| rel.starts_with(allowed))
        {
            continue;
        }
        assert!(
            !contains_ident(&src, "evaluate_policy"),
            "{rel}: I4: only src/planning may call evaluate_policy — route through planning::*"
        );
    }
}

/// Watch persists across commands, so synchronous platform preparation probes
/// must run off the Tokio worker that owns the control loop.
#[test]
fn watch_sandbox_preparation_does_not_block_the_async_control_loop() {
    let source = read_production("src/watch/sandbox.rs");

    assert!(
        source.contains("async fn prepare_watch_command"),
        "Watch Sandbox preparation must expose an async boundary"
    );
    assert!(
        source.contains("tokio::task::spawn_blocking"),
        "Watch Sandbox preparation must move synchronous platform probes to the blocking pool"
    );
}
