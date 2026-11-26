use std::fs;

#[path = "architecture_source_rules/walker.rs"]
mod walker;
use walker::{contains_ident, production_rs_files, repo_root, strip_test_code};

fn read_file(relative: &str) -> String {
    fs::read_to_string(repo_root().join(relative))
        .unwrap_or_else(|err| panic!("failed to read {relative}: {err}"))
}

fn read_production(relative: &str) -> String {
    strip_test_code(&read_file(relative))
}

fn assert_absent(source: &str, needle: &str, file: &str, rule: &str) {
    assert!(
        !source.contains(needle),
        "{file}: forbidden pattern {needle:?} — rule: {rule}"
    );
}

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
                "CONVENTION.md §3: policy engine must be a pure function — no I/O",
            );
        }
    }
}

#[test]
fn ui_does_not_call_audit_or_snapshot_business_logic() {
    for (rel, src) in production_rs_files("crates/aegis-tui/src") {
        assert_absent(
            &src,
            "use crate::audit",
            &rel,
            "CONVENTION.md §3: UI must not depend on audit",
        );
        assert_absent(
            &src,
            "use aegis_audit",
            &rel,
            "CONVENTION.md §3: UI must not depend on audit",
        );
        assert_absent(
            &src,
            "AuditLogger",
            &rel,
            "CONVENTION.md §3: UI must not reference AuditLogger",
        );

        assert_absent(
            &src,
            "use crate::runtime",
            &rel,
            "CONVENTION.md §3: UI must not depend on runtime",
        );
        assert_absent(
            &src,
            "use crate::planning",
            &rel,
            "CONVENTION.md §3: UI must not depend on planning",
        );

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
                "CONVENTION.md §3: UI may import SnapshotRecord but must not invoke snapshot business logic",
            );
        }
    }
}

const EVALUATE_POLICY_ALLOWED: &[(&str, &str)] = &[(
    "src/planning/",
    "the sanctioned consumer: planning is the one caller of the policy engine",
)];

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
            "{rel}: CONVENTION.md §3: only src/planning may call evaluate_policy — route through planning::*"
        );
    }
}

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
