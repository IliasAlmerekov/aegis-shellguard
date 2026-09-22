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

// ── §4 Forbidden edges — UI is rendering only ─────────────────────────────────

/// §4: `ui/**` may not write audit entries, run snapshot business logic, or
/// depend on runtime/planning. The documented allow-leak: importing the
/// `SnapshotRecord` display type from `snapshot`. Calling
/// `SnapshotRegistry::*`, `.snapshot_all(`, or `.rollback(` is forbidden.
#[test]
fn ui_does_not_call_audit_or_snapshot_business_logic() {
    for (rel, src) in production_rs_files("crates/aegis-tui/src") {
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
const EVALUATE_POLICY_ALLOWED: &[(&str, &str)] = &[(
    "src/planning/",
    "the sanctioned consumer: planning is the one caller of the policy engine",
)];

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

// ── #281 Narrowed library surface — no re-forwarding of aegis-types ───────────

/// #281: only `aegis-types` may hold a `pub use aegis_types` line. Every other
/// crate imports the type it needs directly from `aegis-types`; forwarding it
/// back out under a second name defeats the point of the narrowed surface.
#[test]
fn only_aegis_types_re_exports_aegis_types() {
    for (rel, src) in production_rs_files("crates") {
        if rel.starts_with("crates/aegis-types/") {
            continue;
        }
        assert_absent(
            &src,
            "pub use aegis_types",
            &rel,
            "#281: only aegis-types may re-export its own items — import aegis_types::X directly",
        );
    }
}

/// #281: the shim modules (`src/audit`, `src/config`, `src/decision`,
/// `src/snapshot`, `src/ui`, `src/interceptor`) are gone. Nothing in `src/` or
/// `tests/` may name them again, either as `crate::<shim>::` (inside the root
/// crate) or `aegis::<shim>::` (from an integration test).
#[test]
fn former_shim_paths_do_not_reappear() {
    let files = production_rs_files("src")
        .into_iter()
        .chain(production_rs_files("tests"));

    for (rel, src) in files {
        for shim in [
            "audit",
            "config",
            "decision",
            "snapshot",
            "ui",
            "interceptor",
        ] {
            for prefix in ["crate", "aegis"] {
                let needle = format!("{prefix}::{shim}::");
                assert_absent(
                    &src,
                    &needle,
                    &rel,
                    "#281: the src/ re-export shims are deleted — import from the crate that defines the type",
                );
            }
        }
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
