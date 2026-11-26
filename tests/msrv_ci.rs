#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers fail loudly"
)]
mod support;

use std::fs;
use std::process::Command;

use support::{repo_path, workspace_msrv};

fn ci_workflow() -> String {
    fs::read_to_string(repo_path(".github/workflows/ci.yml"))
        .expect("CI workflow should be readable")
}

fn msrv_workflow() -> String {
    fs::read_to_string(repo_path(".github/workflows/msrv.yml"))
        .expect("MSRV workflow should be readable")
}

fn extract_msrv_step_script(workflow: &str) -> String {
    let normalized = workflow.replace("\r\n", "\n");
    let step = normalized
        .split("- name: Read MSRV from Cargo.toml\n")
        .nth(1)
        .expect("msrv.yml should define the `Read MSRV from Cargo.toml` step");
    let after_run = step
        .split("run: |\n")
        .nth(1)
        .expect("Read MSRV step should have a multiline `run: |` block");

    let mut lines = Vec::new();
    let mut base_indent: Option<usize> = None;
    for line in after_run.lines() {
        if line.trim().is_empty() {
            lines.push("");
            continue;
        }
        let indent = line.len() - line.trim_start_matches(' ').len();
        let min_indent = *base_indent.get_or_insert(indent);
        if indent < min_indent {
            break;
        }
        lines.push(&line[min_indent..]);
    }
    let script = lines.join("\n");
    assert!(
        !script.trim().is_empty(),
        "extracted Read MSRV script should not be empty"
    );
    script
}

fn run_extraction(script: &str, cargo_toml: &str) -> (std::process::Output, String) {
    let dir = tempfile::tempdir().expect("tempdir should be created");
    fs::write(dir.path().join("Cargo.toml"), cargo_toml)
        .expect("fixture Cargo.toml should be writable");
    let github_output = dir.path().join("github_output");
    fs::write(&github_output, "").expect("GITHUB_OUTPUT file should be writable");

    let output = Command::new("bash")
        .arg("-c")
        .arg(script)
        .current_dir(dir.path())
        .env("GITHUB_OUTPUT", &github_output)
        .output()
        .expect("bash should execute the MSRV extraction script");
    let recorded = fs::read_to_string(&github_output).expect("GITHUB_OUTPUT should be readable");
    (output, recorded)
}

#[test]
fn no_workflow_file_hardcodes_the_workspace_msrv() {
    let msrv = workspace_msrv();
    let workflows_dir = repo_path(".github/workflows");
    let mut checked = Vec::new();

    for entry in fs::read_dir(&workflows_dir).expect(".github/workflows should be readable") {
        let path = entry.expect("workflow dir entry should be readable").path();
        let is_yaml = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext == "yml" || ext == "yaml");
        if !is_yaml {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .expect("workflow filename should be valid UTF-8")
            .to_string();
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("{name} should be readable: {err}"));
        assert!(
            !text.contains(&msrv),
            "{name} must not hardcode the workspace MSRV {msrv}; read it from Cargo.toml"
        );
        checked.push(name);
    }

    for expected in ["ci.yml", "release.yml", "msrv.yml"] {
        assert!(
            checked.iter().any(|name| name == expected),
            "expected to scan {expected} under .github/workflows, scanned {checked:?}"
        );
    }
}

#[test]
fn msrv_extraction_step_reads_workspace_package_and_fails_when_missing() {
    let workflow = msrv_workflow();
    let script = extract_msrv_step_script(&workflow);

    let (pos, recorded) = run_extraction(
        &script,
        "[package]\nname = \"fixture\"\nrust-version.workspace = true\n\n[workspace.package]\nrust-version = \"1.89\"\n",
    );
    assert!(
        pos.status.success(),
        "MSRV extraction script failed on valid fixture:\n{}",
        String::from_utf8_lossy(&pos.stderr)
    );
    assert_eq!(
        recorded.trim(),
        "msrv=1.89",
        "extraction script must write msrv=1.89 from [workspace.package]"
    );

    let (neg, _) = run_extraction(
        &script,
        "[package]\nname = \"fixture\"\nrust-version = \"1.70\"\n\n[workspace.package]\nversion = \"0.1.0\"\n",
    );
    assert!(
        !neg.status.success(),
        "MSRV extraction script must fail when [workspace.package].rust-version is missing"
    );

    assert!(
        workflow.contains("toolchain: ${{ steps.msrv.outputs.msrv }}"),
        "msrv.yml must pass the extracted MSRV output to setup-rust"
    );
    assert!(
        workflow.contains("MSRV: ${{ steps.msrv.outputs.msrv }}"),
        "msrv.yml must pass the extracted MSRV output to the cargo check step"
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
