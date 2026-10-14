//! Contract tests for the `smoke-install` job in the release workflow: it
//! installs the published Homebrew formula and npm package after a release.

use std::path::Path;

fn release_workflow() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/release.yml");
    std::fs::read_to_string(&path).expect("release workflow should be readable")
}

/// The text of one top-level job, from its `  <name>:` line to the next job
/// or the comment block that introduces it.
fn job(workflow: &str, name: &str) -> String {
    let header = format!("  {name}:");
    let mut lines = workflow.lines().skip_while(|line| *line != header);
    let first = lines
        .next()
        .unwrap_or_else(|| panic!("release workflow should contain the {name} job"));
    let mut body = vec![first];
    for line in lines {
        let top_level = line.starts_with("  ") && !line.starts_with("   ");
        if top_level && !line.trim().is_empty() {
            break;
        }
        body.push(line);
    }
    body.join("\n")
}

#[test]
fn install_smoke_job_should_follow_both_publish_jobs_on_every_runner() {
    let wf = release_workflow();
    let smoke = job(&wf, "smoke-install");

    assert!(
        smoke.contains("needs: [config, publish-homebrew-tap, publish-npm]"),
        "smoke-install must wait for both publish jobs"
    );
    assert!(smoke.contains("runs-on: ${{ matrix.os }}"));
    for output in ["linux_runner", "macos_runner", "macos_intel_runner"] {
        assert!(
            smoke.contains(&format!("needs.config.outputs.{output}")),
            "smoke-install must run on the {output} runner"
        );
        assert!(
            job(&wf, "config").contains(&format!(
                "{output}: ${{{{ steps.versions.outputs.{output} }}}}"
            )),
            "config must export {output}"
        );
    }
}

#[test]
fn install_smoke_job_should_skip_prereleases_and_run_npm_only_on_recovery() {
    let wf = release_workflow();
    let smoke = job(&wf, "smoke-install");

    for suffix in ["-rc", "-beta", "-alpha"] {
        assert!(
            smoke.contains(&format!("!contains(github.ref_name, '{suffix}')")),
            "smoke-install must skip {suffix} tags"
        );
    }
    assert!(
        smoke.contains("needs.publish-homebrew-tap.result == 'success'"),
        "a tag push needs the tap published"
    );
    assert!(
        smoke.contains("github.event_name == 'workflow_dispatch'"),
        "smoke-install must run on the npm recovery dispatch"
    );

    let brew = smoke
        .split("- name: ")
        .find(|step| step.starts_with("Homebrew smoke"))
        .expect("smoke-install should have a Homebrew smoke step");
    assert!(
        brew.contains("if: github.event_name == 'push'"),
        "the Homebrew smoke must not run on the recovery dispatch"
    );
    let npm = smoke
        .split("- name: ")
        .find(|step| step.starts_with("npm smoke"))
        .expect("smoke-install should have an npm smoke step");
    assert!(!npm.contains("if: github.event_name == 'push'"));
}

#[test]
fn install_smoke_job_should_run_the_gated_live_tests_with_the_tag_version() {
    let wf = release_workflow();
    let smoke = job(&wf, "smoke-install");

    assert!(smoke.contains("AEGIS_TEST_LIVE_HOMEBREW"));
    assert!(smoke.contains("AEGIS_TEST_LIVE_NPM"));
    assert!(smoke.contains("AEGIS_TEST_EXPECTED_VERSION"));
    assert!(
        !smoke.contains("substr("),
        "Actions expressions have no substr function; strip the v in the shell"
    );
    assert!(smoke.contains("cargo test --test homebrew_live"));
    assert!(smoke.contains("cargo test --test npm_live"));
    assert!(smoke.contains("/home/linuxbrew/.linuxbrew/bin"));
}
