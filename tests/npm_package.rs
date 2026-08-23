use std::path::Path;

fn repo_file(path: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} should be readable: {error}", path.display()))
}

#[test]
fn npm_package_should_define_global_aegis_binary_and_postinstall() {
    let package = repo_file("packaging/npm/package.json");

    assert!(
        package.contains("\"name\": \"@iliasalmerekov/aegis\""),
        "npm package should use the confirmed scoped package name"
    );
    assert!(
        package.contains("\"bin\"") && package.contains("\"aegis\": \"bin/aegis.js\""),
        "npm package must expose a global aegis command"
    );
    assert!(
        package.contains("\"postinstall\": \"node scripts/install.js\""),
        "npm package must download and verify the native binary during postinstall"
    );
    assert!(
        package.contains("\"os\": [")
            && package.contains("\"darwin\"")
            && package.contains("\"linux\""),
        "npm package must declare supported operating systems"
    );
    assert!(
        package.contains("\"cpu\": [")
            && package.contains("\"x64\"")
            && package.contains("\"arm64\""),
        "npm package must declare supported CPU architectures"
    );
}

#[test]
fn npm_installer_should_map_all_release_assets_and_verify_sha256() {
    let installer = repo_file("packaging/npm/scripts/install.js");

    for asset in [
        "aegis-linux-x86_64",
        "aegis-linux-aarch64",
        "aegis-macos-x86_64",
        "aegis-macos-aarch64",
    ] {
        assert!(
            installer.contains(asset),
            "installer must know how to install release asset {asset}"
        );
    }

    assert!(
        installer.contains("createHash(\"sha256\")"),
        "installer must verify SHA256 before accepting the downloaded binary"
    );
    assert!(
        installer.contains("0o755"),
        "installer must make the native binary executable"
    );
    assert!(
        !installer.contains("curl | sh")
            && !installer.contains("curl -fsSL")
            && !installer.contains(".bashrc")
            && !installer.contains(".zshrc"),
        "npm installer must not shell out to curl installer or mutate shell rc files"
    );
}

#[test]
fn npm_checksums_should_pin_each_supported_asset() {
    let checksums = repo_file("packaging/npm/checksums.json");

    for asset in [
        "aegis-linux-x86_64",
        "aegis-linux-aarch64",
        "aegis-macos-x86_64",
        "aegis-macos-aarch64",
    ] {
        assert!(checksums.contains(asset), "checksums.json must pin {asset}");
    }

    let sha_count = checksums
        .split('"')
        .filter(|part| part.len() == 64 && part.chars().all(|ch| ch.is_ascii_hexdigit()))
        .count();

    assert_eq!(
        sha_count, 4,
        "checksums.json must contain exactly four 64-hex SHA256 values"
    );
}

#[test]
fn npm_updater_should_fetch_all_sidecars_and_fail_closed() {
    let script = repo_file("scripts/update-npm-package.sh");

    assert!(
        script.contains("set -eu"),
        "npm updater must fail closed on unset variables and command failures"
    );
    assert!(
        script.contains("aegis-linux-x86_64.sha256")
            && script.contains("aegis-linux-aarch64.sha256")
            && script.contains("aegis-macos-x86_64.sha256")
            && script.contains("aegis-macos-aarch64.sha256"),
        "npm updater must fetch all four checksum sidecars"
    );
    assert!(
        script.contains("grep -Eq '^[[:xdigit:]]{64}$'"),
        "npm updater must validate checksum format"
    );
    assert!(
        script.contains("checksums.json"),
        "npm updater must write packaging/npm/checksums.json"
    );
    // The updater is the only producer of the staged notice, and the failure
    // would surface in `publish-npm` — after the GitHub Release is already
    // public. Guard the link at PR time.
    assert!(
        script.contains("stage-npm-notices.sh"),
        "npm updater must invoke the third-party notice staging script"
    );
}

/// The npm channel distributes the same statically linked Tree-sitter binary as
/// the GitHub Release assets, so it must carry the same MIT attribution. npm
/// cannot pack files above the package root, so the notice is staged as a
/// generated copy. This exercises the staging script for real — it is split out
/// of `update-npm-package.sh` precisely so it runs without network access.
#[test]
fn npm_notice_staging_should_copy_the_root_notice_verbatim() {
    let staged_dir = tempdir("stage-npm-notices-ok");
    // Nested, to prove the script creates the destination directory itself.
    let staged = staged_dir.join("nested").join("THIRD_PARTY_NOTICES.md");
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("THIRD_PARTY_NOTICES.md");

    let ok = run_staging_script(&source, &staged);
    assert!(
        ok.status.success(),
        "staging must succeed with a present notice: {}",
        String::from_utf8_lossy(&ok.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(&staged).expect("notice must be staged"),
        std::fs::read_to_string(&source).expect("root notice must be readable"),
        "the staged notice must be a byte-identical copy of the root notice"
    );

    let _ = std::fs::remove_dir_all(&staged_dir);
}

/// The two cases above override both paths, so they never exercise the CWD-relative
/// defaults that release automation actually uses. This runs the script bare from
/// the repo root and checks it lands on `packaging/npm/`.
///
/// It must leave the staged file exactly as it found it. `release.yml` runs
/// `update-npm-package.sh` (which stages the notice) *before* `cargo test --test
/// npm_package`, and the `npm pack` verification after it — a test that deleted
/// the staged copy would fail every release.
#[test]
fn npm_notice_staging_should_default_to_the_package_directory() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let default_destination = repo_root.join("packaging/npm/THIRD_PARTY_NOTICES.md");
    let was_staged = default_destination.exists();

    let output = std::process::Command::new("sh")
        .arg(repo_root.join("scripts/stage-npm-notices.sh"))
        .current_dir(repo_root)
        .output()
        .expect("staging script should be runnable");

    assert!(
        output.status.success(),
        "staging must succeed with default paths from the repo root: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(&default_destination).expect("default destination must exist"),
        std::fs::read_to_string(repo_root.join("THIRD_PARTY_NOTICES.md"))
            .expect("root notice must be readable"),
        "the default staged path must hold a copy of the root notice"
    );

    if !was_staged {
        // Only clean up what this test created; a copy staged by release
        // automation must survive, or the pack verification after us breaks.
        let _ = std::fs::remove_file(&default_destination);
    }
}

/// Fail closed: a missing root notice must abort before publish, not warn. A
/// fail-open staging step would publish the npm channel unattributed.
#[test]
fn npm_notice_staging_should_fail_closed_when_the_root_notice_is_absent() {
    let missing_dir = tempdir("stage-npm-notices-missing");
    let absent = missing_dir.join("THIRD_PARTY_NOTICES.md");
    let destination = missing_dir.join("staged.md");

    let failed = run_staging_script(&absent, &destination);
    assert!(
        !failed.status.success(),
        "staging must fail when the root notice is absent"
    );
    assert!(
        !destination.exists(),
        "staging must not produce an output file when the source is absent"
    );

    let _ = std::fs::remove_dir_all(&missing_dir);
}

/// `files` in package.json is an allowlist, so packing the staged notice needs an
/// explicit entry; CI additionally greps `npm pack --dry-run` output to prove it
/// reached the tarball. Both halves must stay in place.
#[test]
fn npm_package_and_release_workflow_should_pack_and_verify_the_notice() {
    let package_json = repo_file("packaging/npm/package.json");
    let workflow = repo_file(".github/workflows/release.yml");

    assert!(
        package_json.contains("\"THIRD_PARTY_NOTICES.md\""),
        "npm package `files` must pack the third-party notices"
    );
    assert!(
        workflow.contains("scripts/update-npm-package.sh"),
        "npm publish must run the updater that stages the notice"
    );
    assert!(
        workflow.contains("grep -q 'THIRD_PARTY_NOTICES.md' pack.log"),
        "npm publish must verify the notice actually landed in the packed tarball"
    );
}

fn tempdir(label: &str) -> std::path::PathBuf {
    // Each label is used by exactly one test; the pid keeps concurrent
    // `cargo test` runs on one host from colliding. Removed on success.
    let dir = std::env::temp_dir().join(format!("aegis-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir should be creatable");
    dir
}

fn run_staging_script(source: &Path, destination: &Path) -> std::process::Output {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    std::process::Command::new("sh")
        .arg(repo_root.join("scripts/stage-npm-notices.sh"))
        .current_dir(repo_root)
        .env("AEGIS_NPM_NOTICES_SOURCE", source)
        .env("AEGIS_NPM_NOTICES", destination)
        .output()
        .expect("staging script should be runnable")
}

#[test]
fn release_workflow_should_publish_npm_after_github_release_assets_exist() {
    let workflow = repo_file(".github/workflows/release.yml");

    assert!(
        workflow.contains("publish-npm:"),
        "release workflow must include an npm publish job"
    );
    assert!(
        workflow.contains("needs: release"),
        "npm publish must wait for the GitHub Release job so checksum sidecars exist"
    );
    assert!(
        workflow.contains("scripts/update-npm-package.sh \"${{ github.ref_name }}\""),
        "npm publish must generate checksums.json from the pushed release tag"
    );
    assert!(
        workflow.contains("secrets.NPM_TOKEN"),
        "npm publish must use the repository NPM_TOKEN secret"
    );
    assert!(
        workflow.contains("npm publish --access public"),
        "release workflow must publish the scoped public npm package"
    );
}

#[test]
fn npm_installer_should_fail_closed_and_support_test_overrides() {
    let installer = repo_file("packaging/npm/scripts/install.js");

    assert!(
        installer.contains("process.env.AEGIS_NPM_PLATFORM")
            && installer.contains("process.env.AEGIS_NPM_ARCH"),
        "installer should support deterministic platform/arch overrides for tests"
    );
    assert!(
        installer.contains("Unsupported platform or architecture"),
        "installer must fail closed on unsupported hosts"
    );
    assert!(
        installer.contains("SHA256 mismatch"),
        "installer must fail closed on checksum mismatch"
    );
    assert!(
        installer.contains("AEGIS_NPM_SKIP_DOWNLOAD"),
        "installer should support a no-network test path"
    );
}

#[test]
fn npm_installer_should_follow_github_release_redirects() {
    let installer = repo_file("packaging/npm/scripts/install.js");

    // GitHub release asset URLs respond with 302 -> release-assets.githubusercontent.com;
    // Node's https.get does not follow redirects automatically, so the installer
    // must follow the Location header itself to be functional on real installs.
    assert!(
        installer.contains("headers.location"),
        "installer must follow the Location header on GitHub release redirects"
    );
    assert!(
        installer.contains("301")
            && installer.contains("302")
            && installer.contains("307")
            && installer.contains("308"),
        "installer must handle the standard permanent/temporary redirect status codes"
    );
    assert!(
        installer.contains("MAX_REDIRECTS"),
        "installer must cap redirect following to avoid redirect loops"
    );
}

#[test]
fn npm_installer_should_best_effort_install_agent_hooks_without_failing_install() {
    let installer = repo_file("packaging/npm/scripts/install.js");

    assert!(
        installer.contains("install-hooks") && installer.contains("--all"),
        "npm postinstall should attempt best-effort agent hook installation"
    );
    assert!(
        installer.contains(".claude") && installer.contains(".codex"),
        "npm postinstall must gate hook setup on existing agent directories"
    );
    assert!(
        installer.contains("AEGIS_NPM_SKIP_HOOKS"),
        "npm postinstall must support a deterministic opt-out for tests"
    );
    assert!(
        installer.contains("If you install Claude Code or Codex later, run:"),
        "npm postinstall must print actionable next steps when no agent dir exists"
    );
    // Best-effort: never create agent dirs and never crash the install.
    assert!(
        !installer.contains("mkdirSync(claudeDir") && !installer.contains("mkdirSync(codexDir"),
        "npm postinstall must not create agent directories just to install hooks"
    );
}

#[test]
fn npm_installer_should_print_shell_setup_next_steps() {
    let installer = repo_file("packaging/npm/scripts/install.js");

    assert!(
        installer.contains("aegis setup-shell"),
        "npm postinstall should point users at the explicit shell-proxy setup command"
    );
    assert!(
        installer.contains("aegis -c 'echo hello'"),
        "npm postinstall should show a quick smoke-test command"
    );
}

#[test]
fn readme_should_document_npm_and_cargo_without_overclaiming_shell_setup() {
    let readme = repo_file("README.md");

    assert!(
        readme.contains("npm i -g @iliasalmerekov/aegis"),
        "README must document npm global install"
    );
    assert!(
        readme.contains(
            "cargo install --git https://github.com/IliasAlmerekov/aegis-shellguard --tag v0.6.5 aegis"
        ),
        "README must document cargo install from a release tag"
    );
    assert!(
        readme.contains("npm and Cargo install the binary only")
            || readme.contains("npm installs the binary only"),
        "README must explain npm/Cargo do not run global shell setup"
    );
}

#[test]
fn release_readiness_should_track_npm_and_cargo_evidence() {
    let docs = repo_file("docs/release-readiness.md");

    assert!(
        docs.contains("npm i -g @iliasalmerekov/aegis"),
        "release readiness must require npm global install evidence"
    );
    assert!(
        docs.contains("npm publish --dry-run"),
        "release readiness must include npm dry-run packaging"
    );
    assert!(
        docs.contains("cargo install --git"),
        "release readiness must document Cargo source-build validation"
    );
}
