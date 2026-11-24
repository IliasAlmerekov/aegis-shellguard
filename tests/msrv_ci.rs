mod support;

use std::fs;

use support::repo_path;

fn ci_workflow() -> String {
    fs::read_to_string(repo_path(".github/workflows/ci.yml"))
        .expect("CI workflow should be readable")
}

fn release_workflow() -> String {
    fs::read_to_string(repo_path(".github/workflows/release.yml"))
        .expect("release workflow should be readable")
}

fn msrv_workflow() -> String {
    fs::read_to_string(repo_path(".github/workflows/msrv.yml"))
        .expect("MSRV workflow should be readable")
}

fn workspace_msrv() -> String {
    let manifest =
        fs::read_to_string(repo_path("Cargo.toml")).expect("Cargo.toml should be readable");

    manifest
        .lines()
        .map(str::trim)
        .skip_while(|line| *line != "[workspace.package]")
        .skip(1)
        .take_while(|line| !line.starts_with('['))
        .find_map(|line| line.strip_prefix("rust-version = "))
        .map(|value| value.trim_matches('"').to_string())
        .expect("Cargo.toml should set rust-version in [workspace.package]")
}

#[test]
fn msrv_workflow_reads_msrv_from_cargo_toml_and_never_hardcodes_it() {
    let msrv = workspace_msrv();
    for (name, workflow) in [
        ("ci.yml", ci_workflow()),
        ("release.yml", release_workflow()),
        ("msrv.yml", msrv_workflow()),
    ] {
        assert!(
            !workflow.contains(&msrv),
            "{name} must not hardcode the workspace MSRV {msrv}; read it from Cargo.toml"
        );
    }

    let workflow = msrv_workflow();
    assert!(
        workflow.contains("Cargo.toml")
            && workflow.contains("workspace")
            && workflow.contains("rust-version"),
        "msrv.yml must read rust-version from [workspace.package] in Cargo.toml"
    );
    assert!(
        workflow.contains("cargo \"+$MSRV\" check --workspace --all-targets --locked"),
        "msrv.yml must run cargo check on the extracted MSRV with --workspace --all-targets --locked"
    );
}

#[test]
fn msrv_workflow_uses_shared_actions_triggers_like_ci_and_read_only_permissions() {
    let msrv = msrv_workflow().replace("\r\n", "\n");
    let ci = ci_workflow().replace("\r\n", "\n");

    assert!(
        msrv.contains("permissions:\n  contents: read\n"),
        "msrv.yml must set permissions: contents: read"
    );
    assert!(
        msrv.contains("./.github/actions/setup-rust"),
        "msrv.yml must install the toolchain through ./.github/actions/setup-rust"
    );
    assert!(
        msrv.contains("./.github/actions/install-libcap"),
        "msrv.yml must install libcap headers through ./.github/actions/install-libcap"
    );

    let on_block = |workflow: &str, name: &str| {
        workflow
            .split("\non:\n")
            .nth(1)
            .unwrap_or_else(|| panic!("{name} should have an on: section"))
            .split("\nconcurrency:\n")
            .next()
            .unwrap_or_else(|| panic!("{name} should have a concurrency: section after on:"))
            .trim()
            .to_string()
    };
    assert_eq!(
        on_block(&msrv, "msrv.yml"),
        on_block(&ci, "ci.yml"),
        "msrv.yml must trigger on the same events as ci.yml"
    );
}

#[test]
fn docs_ci_and_versions_env_document_msrv_workflow() {
    let msrv = workspace_msrv();
    let docs = fs::read_to_string(repo_path("docs/ci.md")).expect("docs/ci.md should be readable");
    assert!(
        !docs.contains(&msrv),
        "docs/ci.md must not hardcode the workspace MSRV {msrv}"
    );
    let section = docs
        .split("## MSRV workflow")
        .nth(1)
        .expect("docs/ci.md should have an `## MSRV workflow` section")
        .split("\n## ")
        .next()
        .unwrap_or("");
    assert!(
        section.contains(".github/workflows/msrv.yml")
            && section.contains("Cargo.toml")
            && section.contains("Merge admission"),
        "docs/ci.md `## MSRV workflow` section must document msrv.yml, Cargo.toml, and Merge admission exclusion"
    );

    let versions_header: String = fs::read_to_string(repo_path(".github/versions.env"))
        .expect("versions.env should be readable")
        .lines()
        .take_while(|line| line.is_empty() || line.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        versions_header.contains("MSRV") && versions_header.contains("Cargo.toml"),
        ".github/versions.env header must record that MSRV lives in Cargo.toml"
    );
}
