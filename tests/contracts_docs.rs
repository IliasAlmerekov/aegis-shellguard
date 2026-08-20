use std::fs;
use std::path::PathBuf;

fn repo_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

#[test]
fn config_schema_contract_covers_exit_code_compatibility() {
    let path = repo_path("docs/config-schema.md");
    let contents = fs::read_to_string(&path).expect("docs/config-schema.md must exist");

    assert!(
        contents.contains("## Exit-code compatibility contract"),
        "config schema doc must document exit-code compatibility contract"
    );

    for needle in [
        "`0` — command approved/executed successfully",
        "`2` — user denied in a prompt path (`prompt` decision)",
        "`3` — hard block (`block` decision)",
        "`4` — internal/config error",
        "`exit_code` in `--output json` always matches",
        "1..=255",
    ] {
        assert!(
            contents.contains(needle),
            "config schema doc must mention `{needle}`; missing compatibility contract detail"
        );
    }
}

#[test]
fn threat_model_is_current_and_documents_non_goals_honestly() {
    let path = repo_path("docs/threat-model.md");
    let contents = fs::read_to_string(&path).expect("docs/threat-model.md must exist");

    for needle in [
        "Aegis is a **heuristic command guardrail**",
        "Aegis is **not**:",
        "Aegis does not aim to provide:",
        "Residual risk",
        "Known examples:",
        "Security invariants",
        "Explicit non-goals",
        "Verification maturity note",
        "Current fuzzing coverage includes parser and scanner harnesses",
    ] {
        assert!(
            contents.contains(needle),
            "threat-model doc must keep current scope-and-limit language: {needle}"
        );
    }
}

#[test]
fn h9_public_docs_distinguish_required_recovery_from_best_effort_snapshots() {
    let threat_model = fs::read_to_string(repo_path("docs/threat-model.md")).unwrap();
    let config_schema = fs::read_to_string(repo_path("docs/config-schema.md")).unwrap();
    let readme = fs::read_to_string(repo_path("README.md")).unwrap();

    for needle in [
        "Effect-opaque execution",
        "Required recovery",
        "one-time Recovery override",
        "Mode::Audit",
        "SnapshotPolicy::None",
    ] {
        assert!(
            threat_model.contains(needle),
            "threat model must document H9 term `{needle}`"
        );
    }
    for needle in [
        "Effect-opaque execution",
        "Required recovery",
        "no applicable Snapshot plugin",
        "Run once without recovery",
    ] {
        assert!(
            config_schema.contains(needle),
            "config schema must document H9 term `{needle}`"
        );
    }
    for needle in [
        "Effect-opaque execution",
        "Run once without recovery",
        "does not inspect the referenced script",
    ] {
        assert!(readme.contains(needle), "README must mention `{needle}`");
    }
    for stale in [
        "when snapshots are requested, that matters only for `Danger`",
        "Snapshot requests matter only for `Danger` flows.",
        "if there are no applicable snapshot plugins, no snapshots are requested even for `Danger`",
    ] {
        assert!(
            !config_schema.contains(stale),
            "config schema must remove stale snapshot claim `{stale}`"
        );
    }
}

/// The six confidentiality overclaims ADR-029 keeps banned. They survive the
/// move from an optional add-on to a mandatory layer unchanged: the layer
/// confines writes and network access, and no document may promise more.
const BANNED_CONFIDENTIALITY_OVERCLAIMS: [&str; 6] = [
    "provides a confidentiality boundary",
    "guarantees confidentiality",
    "provides complete read isolation",
    "all file reads are blocked",
    "hides all readable files",
    "hides all secrets",
];

fn assert_no_confidentiality_overclaims(contents: &str, doc: &str) {
    let lower = contents.to_ascii_lowercase();
    for forbidden in BANNED_CONFIDENTIALITY_OVERCLAIMS {
        assert!(
            !lower.contains(forbidden),
            "{doc} must not claim `{forbidden}`"
        );
    }
}

/// `PRD.md` is the normative source of the Sandbox contract, so it is pinned to
/// the mandatory-layer wording of ADR-029 and the derived-profile wording of
/// ADR-030.
#[test]
fn prd_defines_the_mandatory_sandbox_contract() {
    let prd = fs::read_to_string(repo_path("PRD.md")).unwrap();

    for needle in [
        // ADR-029: the layer is mandatory, and unavailability is one event with
        // the block rather than a warning followed by an unconfined run.
        "mandatory Sandbox",
        "sandbox_status = \"unavailable\" accompanies Decision::Blocked",
        // ADR-029 decision 7: the surviving honesty claim, both halves.
        "not a confidentiality boundary",
        "not a privilege boundary",
        "write/network guardrail",
        // ADR-030: the ceiling bounds, derivation only subtracts.
        "Trusted ceiling",
        "Confinement degradation",
        // ADR-029 amendment (2026-08-20): the `[sandbox]` migration contract.
        // A released config keeps loading, both runtime flags are inert, and a
        // malformed ceiling entry narrows instead of failing the load.
        "deprecated_sandbox_field",
        "trusted_ceiling_path_omitted",
        "zero configured writable roots",
        "never rewrites a config file",
    ] {
        assert!(
            prd.contains(needle),
            "PRD.md must state the mandatory Sandbox contract term `{needle}`"
        );
    }

    for stale in [
        // ADR-029 decision 2 removed the flag; a mandatory layer has no
        // bypass switch to document.
        "sandbox.required = true",
        // A mandatory layer never falls back to running unconfined.
        "optional unconfined fallback",
        // ADR-030: the PRD must not fix an index or a per-session profile.
        "MultiMap",
        // The §5.5 pending marker is spent: the `[sandbox]` migration contract
        // is decided
        // (<https://github.com/IliasAlmerekov/aegis-shellguard/issues/240>) and
        // stated, so a normative section may not go provisional again.
        "This section is not final",
    ] {
        assert!(
            !prd.contains(stale),
            "PRD.md must not retain the pre-ADR-029 claim `{stale}`"
        );
    }

    assert!(!prd.contains("`WARN` is emitted on the\n  `aegis::sandbox` target"));
    assert_no_confidentiality_overclaims(&prd, "PRD.md");
}

/// Pins the Sandbox wording of the documents that have **not** been rewritten
/// yet, so it cannot drift by accident before its own ticket lands.
///
/// The literals below are the *pre-ADR-029* wording. They are a deliberate,
/// named record of known debt — not normative truth. `PRD.md` is the normative
/// source (see `prd_defines_the_mandatory_sandbox_contract`), and these three
/// documents contradict it today.
///
/// Rewriting them is
/// <https://github.com/IliasAlmerekov/aegis-shellguard/issues/205>, now
/// unblocked: the `[sandbox]` migration contract it waited on is decided
/// (<https://github.com/IliasAlmerekov/aegis-shellguard/issues/240>) and stated
/// in `PRD.md` §5.5, so `docs/config-schema.md` — which enumerates the config
/// fields line by line and cannot stay silent on `enabled` the way a prose
/// section can — has a contract to copy from. When #205 lands, this test is
/// rewritten into the derived-doc contract rather than deleted.
#[test]
fn derived_sandbox_docs_remain_pinned_until_issue_205_rewrites_them() {
    let readme = fs::read_to_string(repo_path("README.md")).unwrap();
    let config_schema = fs::read_to_string(repo_path("docs/config-schema.md")).unwrap();
    let threat_model = fs::read_to_string(repo_path("docs/threat-model.md")).unwrap();
    let roadmap = fs::read_to_string(repo_path("ROADMAP.md")).unwrap();
    let architecture = fs::read_to_string(repo_path("ARCHITECTURE.md")).unwrap();

    for contents in [&readme, &config_schema, &threat_model] {
        assert!(contents.contains("write/network guardrail"));
        assert!(contents.contains("not a confidentiality boundary"));
    }
    for contents in [&config_schema, &roadmap, &architecture] {
        assert!(contents.contains("sandbox_status = \"unavailable\""));
        assert!(contents.contains("sandbox.required = true"));
    }
    assert!(config_schema.contains("macOS permits `file-read*`"));
    assert!(config_schema.contains("read-only system mounts"));
    assert!(architecture.contains("prepare_for_spawn"));

    for (contents, doc) in [
        (&readme, "README.md"),
        (&config_schema, "docs/config-schema.md"),
        (&threat_model, "docs/threat-model.md"),
        (&roadmap, "ROADMAP.md"),
        (&architecture, "ARCHITECTURE.md"),
    ] {
        assert_no_confidentiality_overclaims(contents, doc);
    }
}

#[test]
fn m3a_docs_keep_disabled_passthrough_and_hook_refresh_explicit() {
    // Prose wraps at the author's discretion, so match against a
    // whitespace-normalized copy: reflowing a paragraph must not fail this
    // contract, and deleting the promise must.
    fn unwrapped(path: &str) -> String {
        fs::read_to_string(repo_path(path))
            .unwrap()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    let readme = unwrapped("README.md");
    let troubleshooting = unwrapped("docs/troubleshooting.md");

    // The Toggle is an operator escape hatch (ADR-005), so the docs must say
    // plainly what an operator gets while it is off, and that a session says so.
    for needle in [
        "SessionStart effective-state notices",
        "unguarded passthrough",
        "`aegis on` re-enables enforcement",
    ] {
        assert!(
            readme.contains(needle),
            "README must document the disabled-Toggle session contract `{needle}`"
        );
    }

    // The CI override is the one case where the local Toggle does not decide
    // the effective state; a doc that omits it teaches operators to trust a
    // stale `aegis off`.
    assert!(
        readme.contains("CI keeps enforcement active even if the local disabled flag exists"),
        "README must document that CI overrides a local disabled Toggle"
    );

    // `aegis status` is the authoritative effective-state surface; the notice
    // only reports it.
    for needle in [
        "Run `aegis status` to inspect the configured and effective state.",
        "Managed hooks are updated explicitly; they never self-update while a session starts.",
    ] {
        assert!(
            troubleshooting.contains(needle),
            "troubleshooting must document `{needle}`"
        );
    }

    // Visibility is informational. Promising an audit trail for session starts
    // would exceed the contract — only Toggle transitions are auditable.
    for forbidden in [
        "session start is audited",
        "audits every session start",
        "session-start notices are recorded in the audit log",
    ] {
        let lower = readme.to_ascii_lowercase();
        assert!(
            !lower.contains(forbidden),
            "docs must not claim session-start notices are audited: `{forbidden}`"
        );
    }
}

#[test]
fn m1_sandbox_api_docs_define_failure_and_non_return_contracts() {
    let source = fs::read_to_string(repo_path("crates/aegis-sandbox/src/lib.rs")).unwrap();
    let plan = fs::read_to_string(repo_path(
        "docs/plans/2026-07-14-m1-sandbox-degradation-contract.md",
    ))
    .unwrap();

    let exec_docs = source
        .split("pub fn exec")
        .next()
        .and_then(|prefix| prefix.rsplit("/// Replace the current process").next())
        .expect("PreparedSandboxCommand::exec docs must exist");
    assert!(exec_docs.contains("does not return when process replacement succeeds"));
    assert!(exec_docs.contains("SandboxError::Io"));
    // Issue #211: Landlock is applied in the innermost re-exec'd wrapper inside
    // bwrap's mount namespace, so exec() no longer returns
    // SandboxError::Execution; a Landlock failure fails closed inside the
    // wrapper with a non-zero exit code.
    assert!(exec_docs.contains("innermost re-exec'd"));

    let prepare_exec_docs = source
        .split("pub fn prepare_for_exec")
        .next()
        .and_then(|prefix| prefix.rsplit("/// Prepare a").next())
        .expect("prepare_for_exec docs must exist");
    for error in [
        "SandboxError::Required",
        "SandboxError::Execution",
        "SandboxError::SetupFailed",
    ] {
        assert!(prepare_exec_docs.contains(error));
    }

    let prepare_spawn_docs = source
        .split("pub fn prepare_for_spawn")
        .next()
        .and_then(|prefix| prefix.rsplit("/// Prepare a child").next())
        .expect("prepare_for_spawn docs must exist");
    for error in [
        "SandboxError::Required",
        "SandboxError::Execution",
        "SandboxError::SetupFailed",
    ] {
        assert!(prepare_spawn_docs.contains(error));
    }

    assert!(
        !plan.contains("Sandbox state"),
        "M1 plan must use the canonical term `Sandbox status`"
    );
}

#[test]
fn readme_links_to_contract_docs() {
    let readme = fs::read_to_string(repo_path("README.md")).expect("README.md must exist");
    assert!(
        readme.contains("[Architecture decisions](docs/adr/README.md)"),
        "README must link to ADR index document"
    );
    assert!(
        readme.contains("[Config schema](docs/config-schema.md)"),
        "README must link to config schema contract document"
    );
    assert!(
        readme.contains("[Threat model](docs/threat-model.md)"),
        "README must link to threat model contract document"
    );
    assert!(
        readme.contains("[Release readiness](docs/release-readiness.md)"),
        "README must link to release-readiness contract document"
    );
    for needle in [
        "command -v aegis",
        "aegis --version",
        "SHELL",
        "AEGIS_REAL_SHELL",
        "find the `shell` field",
        "curl -fsSL",
        "install.sh",
        "Global",
        "Local",
        "Binary",
        "Claude Code",
        "Aegis is working",
        "Uninstall",
        "uninstall.sh",
    ] {
        assert!(readme.contains(needle), "README must mention `{needle}`");
    }
}

#[test]
fn adr_index_split_is_present_and_active_docs_reference_it() {
    let adr_index =
        fs::read_to_string(repo_path("docs/adr/README.md")).expect("docs/adr/README.md must exist");

    for needle in [
        "## Current architecture snapshot",
        "## ADR index",
        "## Verification guidance",
        "ADR-001",
        "ADR-010",
        "adr-010-full-shell-evaluation-and-deferred-execution-remain-non-goals.md",
    ] {
        assert!(
            adr_index.contains(needle),
            "ADR index must include `{needle}`"
        );
    }

    let architecture =
        fs::read_to_string(repo_path("ARCHITECTURE.md")).expect("ARCHITECTURE.md must exist");
    assert!(
        architecture.contains("docs/adr/README.md"),
        "ARCHITECTURE.md must point readers at the ADR index"
    );

    let contributing =
        fs::read_to_string(repo_path("CONTRIBUTING.md")).expect("CONTRIBUTING.md must exist");
    assert!(
        contributing.contains("docs/adr/README.md"),
        "CONTRIBUTING.md must point contributors at the ADR index"
    );

    let threat_model = fs::read_to_string(repo_path("docs/threat-model.md"))
        .expect("docs/threat-model.md must exist");
    assert!(
        threat_model.contains(
            "docs/adr/adr-010-full-shell-evaluation-and-deferred-execution-remain-non-goals.md"
        ),
        "threat-model doc must link to the ADR-010 non-goals record"
    );
}

#[test]
fn release_readiness_doc_separates_launch_and_security_checklists() {
    let path = repo_path("docs/release-readiness.md");
    let contents = fs::read_to_string(&path).expect("docs/release-readiness.md must exist");

    for needle in [
        "## Minimum Launch Checklist",
        "## Security-Grade Checklist",
        "## Verification-first manual install path",
        "sha256sum -c <asset-name>.sha256",
        "shasum -a 256 -c <asset-name>.sha256",
        "This verifies the downloaded binary against the checksum sidecar published",
        "It proves integrity of the file you downloaded",
        "does **not** authenticate the publisher",
        "signature /",
        "make the binary available on your `PATH`",
        "asset=aegis-linux-x86_64",
        "chmod +x \"./$asset\"",
        "mv \"./$asset\" \"$HOME/.local/bin/aegis\"",
        "Replace `aegis-linux-x86_64` with your platform asset name",
        "export PATH=\"$HOME/.local/bin:$PATH\"",
        "Claude Code: run `command -v aegis`, then paste the absolute path it",
        "shell-based launchers that honor `$SHELL`",
        "SHELL=/absolute/path/to/aegis",
        "AEGIS_REAL_SHELL=/absolute/path/to/your-real-shell",
        "integrity_mode = \"ChainSha256\"",
        "aegis audit --verify-integrity",
    ] {
        assert!(
            contents.contains(needle),
            "release-readiness doc must include `{needle}`"
        );
    }
}

#[test]
fn config_schema_recommends_chain_sha256_for_security_conscious_deployments() {
    let path = repo_path("docs/config-schema.md");
    let contents = fs::read_to_string(&path).expect("docs/config-schema.md must exist");

    for needle in [
        "## Audit integrity mode",
        "integrity_mode = \"Off\"",
        "integrity_mode = \"ChainSha256\"",
        "aegis audit --verify-integrity",
    ] {
        assert!(
            contents.contains(needle),
            "config schema doc must include `{needle}`"
        );
    }
}

#[test]
fn audit_integrity_docs_match_the_chain_sha256_runtime_default() {
    for path in ["docs/config-schema.md", "docs/release-readiness.md"] {
        let contents = fs::read_to_string(repo_path(path)).expect("audit integrity doc must exist");
        assert!(
            contents.contains("The runtime default is `ChainSha256`"),
            "{path} must state the ChainSha256 runtime default"
        );
    }
}

#[test]
fn troubleshooting_covers_manual_checksum_and_integrity_verification() {
    let path = repo_path("docs/troubleshooting.md");
    let contents = fs::read_to_string(&path).expect("docs/troubleshooting.md must exist");

    for needle in [
        "Manual checksum verification fails",
        "sha256sum -c <asset-name>.sha256",
        "shasum -a 256 -c <asset-name>.sha256",
        "Audit integrity verification",
        "aegis audit --verify-integrity",
    ] {
        assert!(
            contents.contains(needle),
            "troubleshooting doc must include `{needle}`"
        );
    }
}

#[test]
fn docs_should_document_explicit_shell_proxy_setup() {
    let readme = fs::read_to_string(repo_path("README.md")).expect("README.md must exist");

    assert!(
        readme.contains("aegis setup-shell"),
        "README must document the explicit `aegis setup-shell` opt-in command"
    );
    assert!(
        readme.contains("aegis setup-shell --remove"),
        "README must document how to undo shell-proxy setup with `aegis setup-shell --remove`"
    );
}

#[test]
fn docs_grammar_manifest_records_l1_foundation_provenance() {
    // ADR-022 §8: the release ships a grammar manifest of versions, provenance,
    // and licenses. This locks the human-readable manifest against silent drift
    // from the L1 foundation set (ADR-022 §9) and the four required release
    // targets. The machine-readable form is validated by `aegis-language`.
    let path = repo_path("docs/language-grammar-manifest.md");
    let contents = fs::read_to_string(&path).expect("docs/language-grammar-manifest.md must exist");

    for needle in [
        "tree-sitter-python",
        "tree-sitter-javascript",
        "tree-sitter-typescript",
        "tree-sitter-bash",
        "0.25.0",
        "0.23.2",
        "0.25.1",
        "MIT",
        "x86_64-unknown-linux-musl",
        "aarch64-unknown-linux-musl",
        "x86_64-apple-darwin",
        "aarch64-apple-darwin",
    ] {
        assert!(
            contents.contains(needle),
            "language grammar manifest must record `{needle}`"
        );
    }
}

#[test]
fn adr022_records_the_iteration10_direct_exec_degradation_closure() {
    let path =
        repo_path("docs/adr/adr-022-language-aware-analysis-is-an-additive-isolated-stage.md");
    let contents = fs::read_to_string(&path).expect("ADR-022 must exist");

    assert!(
        contents
            .contains("relative `Direct exec` target under a dynamic cwd records `Dynamic source`"),
        "ADR-022 must record the Iteration 10 P7 degradation behavior"
    );
    assert!(
        !contents.contains("is dropped\nduring routing instead of degrading"),
        "ADR-022 must not retain the closed P7 waiver as a current gap"
    );
}

#[test]
fn m4_docs_keep_the_fail_closed_hook_panic_guarantee_and_its_non_goals_explicit() {
    // Prose wraps at the author's discretion, so match against a
    // whitespace-normalized copy: reflowing a paragraph must not fail this
    // contract, and deleting the promise must.
    fn unwrapped(path: &str) -> String {
        fs::read_to_string(repo_path(path))
            .unwrap()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    let adr = unwrapped("docs/adr/adr-023-hook-panic-fails-closed-in-two-layers.md");
    let threat_model = unwrapped("docs/threat-model.md");
    let troubleshooting = unwrapped("docs/troubleshooting.md");
    let readme = unwrapped("README.md");
    let context = fs::read_to_string(repo_path("CONTEXT.md")).unwrap();
    let plan = fs::read_to_string(repo_path(
        "docs/plans/2026-07-14-m4-hook-panic-fail-closed.md",
    ))
    .unwrap();

    // The ADR must record the two-layer decision and the honest non-goals, so a
    // future contributor does not "simplify" the script back to `exec`.
    for needle in [
        "catch_unwind",
        "aegis hook failed internally; refusing to run command unscanned",
        "aegis hook terminated abnormally; refusing to run command unscanned",
        "non-zero exit status only",
        "external SIGKILL",
        "OOM-kill of the agent process itself",
        "corrupted `Hook` script",
    ] {
        assert!(
            adr.contains(needle),
            "ADR-023 must document the fail-closed guarantee `{needle}`"
        );
    }

    // The threat model must state plainly which failure modes are not covered.
    for needle in [
        "Hook panic or abnormal termination",
        "external SIGKILL",
        "OOM-kill of the agent process itself",
        "corrupted `Hook` script",
    ] {
        assert!(
            threat_model.contains(needle),
            "threat model must document the M4 guarantee and non-goals `{needle}`"
        );
    }

    // Existing installations must be told that refreshing the Hook scripts is
    // required to gain the protection.
    for needle in [
        "fail-closed `Hook` layer",
        "re-run `aegis install-hooks --all`",
        "never self-update while a session starts",
    ] {
        assert!(
            troubleshooting.contains(needle),
            "troubleshooting must document the Hook-refresh step `{needle}`"
        );
    }
    assert!(
        readme.contains("fail-closed panic layer"),
        "README must tell users that refreshing the hooks gains the fail-closed panic layer"
    );

    // The glossary must define the concept once, cross-referenced from Hook.
    assert!(
        context.contains("**Contained Hook Panic**"),
        "CONTEXT.md must define the canonical term `Contained Hook Panic`"
    );
    assert!(
        context.contains("Contained Hook Panic"),
        "CONTEXT.md Hook entry must cross-reference `Contained Hook Panic`"
    );

    // The plan must have left Draft and gained the script-level layer.
    assert!(
        plan.contains("Accepted"),
        "the M4 plan must have left Draft status"
    );
    assert!(
        plan.contains("Script-level layer"),
        "the M4 plan scope must mention the script-level layer"
    );
}
