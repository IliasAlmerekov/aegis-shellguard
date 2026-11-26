#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers fail loudly"
)]
mod support;

use std::fs;

use support::repo_path;

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

#[test]
fn prd_defines_the_mandatory_sandbox_contract() {
    let prd = fs::read_to_string(repo_path("PRD.md")).unwrap();

    for needle in [
        "mandatory Sandbox",
        "sandbox_status = \"unavailable\" accompanies Decision::Blocked",
        "not a confidentiality boundary",
        "not a privilege boundary",
        "write/network guardrail",
        "Trusted ceiling",
        "Confinement degradation",
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
        "sandbox.required = true",
        "optional unconfined fallback",
        "MultiMap",
        "This section is not final",
    ] {
        assert!(
            !prd.contains(stale),
            "PRD.md must not retain the pre-1.0 claim `{stale}`"
        );
    }

    assert!(!prd.contains("`WARN` is emitted on the\n  `aegis::sandbox` target"));
    assert_no_confidentiality_overclaims(&prd, "PRD.md");
}

const DERIVED_SANDBOX_DOCS: [&str; 3] =
    ["README.md", "docs/config-schema.md", "docs/threat-model.md"];

const PRE_1_0_SANDBOX_MARKER: &str = "current pre-1.0 implementation";

const OPTIONAL_SANDBOX_PHRASES: [&str; 4] = [
    "sandbox.required = true",
    "optional unconfined fallback",
    "off by default",
    "optional Sandbox",
];

#[test]
fn derived_docs_state_the_mandatory_sandbox_contract() {
    for doc in DERIVED_SANDBOX_DOCS {
        let contents = fs::read_to_string(repo_path(doc)).unwrap();
        assert!(
            contents.contains("mandatory"),
            "{doc} must state that the 1.0 Sandbox layer is mandatory"
        );
        assert!(
            contents.contains("not a privilege boundary"),
            "{doc} must keep the privilege half of the honesty claim"
        );
        assert_no_confidentiality_overclaims(&contents, doc);
    }

    for doc in ["README.md", "docs/config-schema.md", "docs/threat-model.md"] {
        let contents = fs::read_to_string(repo_path(doc)).unwrap();
        assert!(
            contents.contains("write/network guardrail"),
            "{doc} must say what the layer actually confines"
        );
        assert!(
            contents.contains("not a confidentiality boundary"),
            "{doc} must keep the confidentiality half of the honesty claim"
        );
    }

    let config_schema = fs::read_to_string(repo_path("docs/config-schema.md")).unwrap();
    for needle in [
        "trusted_ceiling_path_omitted",
        "zero configured writable roots",
        "never rewrites a config file",
        "Trusted ceiling",
    ] {
        assert!(
            config_schema.contains(needle),
            "docs/config-schema.md must state the `[sandbox]` migration term `{needle}`"
        );
    }
}

#[test]
fn derived_docs_label_the_optional_sandbox_as_current_behaviour() {
    for doc in DERIVED_SANDBOX_DOCS {
        let contents = fs::read_to_string(repo_path(doc)).unwrap();
        let marked = contents
            .to_ascii_lowercase()
            .contains(PRE_1_0_SANDBOX_MARKER);

        for phrase in OPTIONAL_SANDBOX_PHRASES {
            if contents.contains(phrase) {
                assert!(
                    marked,
                    "{doc} uses `{phrase}`, which only holds under the pre-1.0 \
                     optional model, without labelling it `{PRE_1_0_SANDBOX_MARKER}`"
                );
            }
        }

        assert!(
            marked,
            "{doc} must disclose that the shipped 0.x code has not caught up with \
             the mandatory-layer contract yet (#229, #230)"
        );
    }
}

#[test]
fn sandbox_platform_seams_stay_documented() {
    let config_schema = fs::read_to_string(repo_path("docs/config-schema.md")).unwrap();
    assert!(config_schema.contains("macOS permits `file-read*`"));
    assert!(config_schema.contains("read-only system mounts"));

    for doc in ["README.md", "docs/config-schema.md"] {
        let contents = fs::read_to_string(repo_path(doc)).unwrap();
        assert!(
            contents.contains("sandbox_status = \"unavailable\""),
            "{doc} must name the audited status for an unestablished layer"
        );
    }
}

#[test]
fn m3a_docs_keep_disabled_passthrough_and_hook_refresh_explicit() {
    fn unwrapped(path: &str) -> String {
        fs::read_to_string(repo_path(path))
            .unwrap()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    let readme = unwrapped("README.md");
    let troubleshooting = unwrapped("docs/troubleshooting.md");

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

    assert!(
        readme.contains("CI keeps enforcement active even if the local disabled flag exists"),
        "README must document that CI overrides a local disabled Toggle"
    );

    for needle in [
        "Run `aegis status` to inspect the configured and effective state.",
        "Managed hooks are updated explicitly; they never self-update while a session starts.",
    ] {
        assert!(
            troubleshooting.contains(needle),
            "troubleshooting must document `{needle}`"
        );
    }

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
fn readme_links_to_contract_docs() {
    let readme = fs::read_to_string(repo_path("README.md")).expect("README.md must exist");
    assert!(
        readme.contains("[Architecture](ARCHITECTURE.md)"),
        "README must link to the architecture document"
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
        "### Quick install",
        "### npm",
        "### Homebrew",
        "### Developer source install",
        "### Shell-proxy mode",
        "aegis setup-shell",
        "Claude Code",
        "Aegis is working",
        "Uninstall",
        "uninstall.sh",
    ] {
        assert!(readme.contains(needle), "README must mention `{needle}`");
    }
}

#[test]
fn architecture_doc_is_present_and_active_docs_reference_it() {
    let architecture =
        fs::read_to_string(repo_path("ARCHITECTURE.md")).expect("ARCHITECTURE.md must exist");
    for needle in [
        "## Workspace layout",
        "## Security invariants",
        "## Config schema",
    ] {
        assert!(
            architecture.contains(needle),
            "ARCHITECTURE.md must include `{needle}`"
        );
    }

    let contributing =
        fs::read_to_string(repo_path("CONTRIBUTING.md")).expect("CONTRIBUTING.md must exist");
    assert!(
        contributing.contains("ARCHITECTURE.md"),
        "CONTRIBUTING.md must point contributors at ARCHITECTURE.md"
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
fn m4_docs_keep_the_fail_closed_hook_panic_guarantee_and_its_non_goals_explicit() {
    fn unwrapped(path: &str) -> String {
        fs::read_to_string(repo_path(path))
            .unwrap()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    let threat_model = unwrapped("docs/threat-model.md");
    let troubleshooting = unwrapped("docs/troubleshooting.md");
    let readme = unwrapped("README.md");
    let context = fs::read_to_string(repo_path("CONTEXT.md")).unwrap();

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

    assert!(
        context.contains("**Contained Hook Panic**"),
        "CONTEXT.md must define the canonical term `Contained Hook Panic`"
    );
    assert!(
        context.contains("Contained Hook Panic"),
        "CONTEXT.md Hook entry must cross-reference `Contained Hook Panic`"
    );
}
