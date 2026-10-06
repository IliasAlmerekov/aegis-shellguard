use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

fn repo_file(path: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} should be readable: {error}", path.display()))
}

const SIDECAR_ASSETS: [&str; 4] = [
    "aegis-linux-x86_64",
    "aegis-linux-aarch64",
    "aegis-macos-x86_64",
    "aegis-macos-aarch64",
];

const FIXTURE_TAG: &str = "v9.8.7";

fn fixture_checksum(index: usize) -> String {
    format!("{:x}", index + 1).repeat(64)
}

fn tempdir(label: &str) -> PathBuf {
    // The pid keeps concurrent `cargo test` runs on one host apart and the
    // counter keeps tests in this binary apart, since many call `formula()`.
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("aegis-{label}-{}-{id}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir should be creatable");
    dir
}

/// Writes the release fixtures the updater downloads: four `.sha256`
/// sidecars and `THIRD_PARTY_NOTICES.md`. `skip` leaves one file out and
/// `override_sidecar` replaces one sidecar body.
fn write_release_fixtures(dir: &Path, skip: Option<&str>, override_sidecar: Option<(&str, &str)>) {
    for (index, asset) in SIDECAR_ASSETS.iter().enumerate() {
        let name = format!("{asset}.sha256");
        if skip == Some(name.as_str()) {
            continue;
        }
        let body = match override_sidecar {
            Some((target, body)) if target == name => body.to_string(),
            _ => format!("{}  {asset}\n", fixture_checksum(index)),
        };
        std::fs::write(dir.join(&name), body).expect("sidecar fixture should be writable");
    }
    if skip != Some("THIRD_PARTY_NOTICES.md") {
        std::fs::write(dir.join("THIRD_PARTY_NOTICES.md"), "fixture notices\n")
            .expect("notices fixture should be writable");
    }
}

struct UpdaterRun {
    output: Output,
    formula_path: PathBuf,
}

impl UpdaterRun {
    fn formula(&self) -> String {
        std::fs::read_to_string(&self.formula_path).unwrap_or_else(|error| {
            panic!(
                "updater should have written {}: {error}\nstderr: {}",
                self.formula_path.display(),
                String::from_utf8_lossy(&self.output.stderr)
            )
        })
    }
}

/// Runs the updater against `file://` fixtures, never the network.
fn run_updater(
    label: &str,
    tag: &str,
    skip: Option<&str>,
    override_sidecar: Option<(&str, &str)>,
) -> UpdaterRun {
    let dir = tempdir(label);
    let release_dir = dir.join("release");
    std::fs::create_dir_all(&release_dir).expect("release fixture dir should be creatable");
    write_release_fixtures(&release_dir, skip, override_sidecar);
    let formula_path = dir.join("out").join("aegis.rb");

    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = Command::new("sh")
        .arg(repo_root.join("scripts/update-homebrew-formula.sh"))
        .arg(tag)
        .current_dir(repo_root)
        .env(
            "AEGIS_RELEASE_BASE_URL",
            format!("file://{}", release_dir.display()),
        )
        .env("AEGIS_HOMEBREW_FORMULA", &formula_path)
        .output()
        .expect("updater should be runnable");
    UpdaterRun {
        output,
        formula_path,
    }
}

fn formula() -> String {
    run_updater("formula-default", FIXTURE_TAG, None, None).formula()
}

#[test]
fn homebrew_updater_should_fail_when_a_sidecar_is_missing() {
    let run = run_updater(
        "formula-missing-sidecar",
        FIXTURE_TAG,
        Some("aegis-macos-aarch64.sha256"),
        None,
    );

    assert!(!run.output.status.success());
    assert!(!run.formula_path.exists(), "no formula on a missing sidecar");
}

#[test]
fn homebrew_updater_should_fail_when_the_notices_asset_is_missing() {
    let run = run_updater(
        "formula-missing-notices",
        FIXTURE_TAG,
        Some("THIRD_PARTY_NOTICES.md"),
        None,
    );

    assert!(!run.output.status.success());
    assert!(!run.formula_path.exists(), "no formula without the notices");
}

#[test]
fn homebrew_updater_should_fail_on_a_malformed_checksum() {
    let run = run_updater(
        "formula-bad-checksum",
        FIXTURE_TAG,
        None,
        Some(("aegis-linux-x86_64.sha256", "not-a-checksum  aegis-linux-x86_64\n")),
    );

    assert!(!run.output.status.success());
    assert!(
        String::from_utf8_lossy(&run.output.stderr).contains("invalid sha256"),
        "stderr should name the checksum failure"
    );
    assert!(!run.formula_path.exists(), "no formula on a bad checksum");
}

#[test]
fn homebrew_updater_should_fail_on_a_short_checksum() {
    let run = run_updater(
        "formula-short-checksum",
        FIXTURE_TAG,
        None,
        Some(("aegis-linux-aarch64.sha256", "abc123  aegis-linux-aarch64\n")),
    );

    assert!(!run.output.status.success());
}

#[test]
fn homebrew_formula_version_and_urls_should_follow_the_tag() {
    let formula = run_updater("formula-tag", "v1.2.3", None, None).formula();

    assert!(formula.contains("version \"1.2.3\""));
    let url_lines: Vec<&str> = formula
        .lines()
        .filter(|line| line.trim_start().starts_with("url \""))
        .collect();
    assert_eq!(url_lines.len(), 5, "four binaries plus the notices resource");
    for line in url_lines {
        assert!(
            line.contains(
                "\"https://github.com/IliasAlmerekov/aegis-shellguard/releases/download/v1.2.3/"
            ),
            "url must point at the tagged GitHub Release, not the fixture base: {line}"
        );
    }
}

#[test]
fn homebrew_formula_should_pin_the_fixture_checksums_to_their_platforms() {
    let formula = formula();

    for (index, asset) in SIDECAR_ASSETS.iter().enumerate() {
        let url_pos = formula
            .find(&format!("/{asset}\", using"))
            .unwrap_or_else(|| panic!("formula must reference {asset}"));
        let after = &formula[url_pos..];
        let sha_line = after.lines().nth(1).expect("sha line follows url");
        assert!(
            sha_line.contains(&fixture_checksum(index)),
            "{asset} must carry its own sidecar checksum: {sha_line}"
        );
    }
}

#[test]
fn repo_should_not_keep_a_second_copy_of_the_formula() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("packaging/homebrew/Formula/aegis.rb");

    assert!(
        !path.exists(),
        "the tap holds the only formula; the release workflow generates it"
    );
}

#[test]
fn homebrew_formula_should_install_release_binary_assets_for_all_supported_platforms() {
    let formula = formula();

    for asset in [
        "aegis-linux-x86_64",
        "aegis-linux-aarch64",
        "aegis-macos-x86_64",
        "aegis-macos-aarch64",
    ] {
        assert!(
            formula.contains(asset),
            "Homebrew formula must reference release asset {asset}"
        );
    }

    assert!(
        formula.contains("on_macos do"),
        "formula must branch for macOS assets"
    );
    assert!(
        formula.contains("on_linux do"),
        "formula must branch for Linux assets"
    );
}

#[test]
fn homebrew_formula_should_pin_each_download_with_a_sha256() {
    let formula = formula();
    let sha_count = formula
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            trimmed.starts_with("sha256 \"")
                && trimmed.ends_with('"')
                && trimmed
                    .trim_start_matches("sha256 \"")
                    .trim_end_matches('"')
                    .chars()
                    .all(|ch| ch.is_ascii_hexdigit())
                && trimmed
                    .trim_start_matches("sha256 \"")
                    .trim_end_matches('"')
                    .len()
                    == 64
        })
        .count();

    assert_eq!(
        sha_count, 5,
        "formula must pin the four binary downloads plus the third-party notices resource with 64-hex SHA256 values"
    );
}

#[test]
fn homebrew_formula_should_ship_third_party_notices_as_a_pinned_resource() {
    let formula = formula();

    assert!(
        formula.contains("resource \"third_party_notices\""),
        "formula must declare a resource for THIRD_PARTY_NOTICES.md"
    );
    assert!(
        formula.contains("/THIRD_PARTY_NOTICES.md\""),
        "third_party_notices resource must download the checked-in root notice asset"
    );
}

#[test]
fn homebrew_formula_should_install_third_party_notices_into_share_doc_aegis() {
    let formula = formula();

    assert!(
        formula.contains("resource(\"third_party_notices\").stage do"),
        "install must stage the third_party_notices resource"
    );
    assert!(
        formula.contains("(share/\"doc/aegis\").install \"THIRD_PARTY_NOTICES.md\""),
        "install must place the staged notice under share/doc/aegis, matching scripts/install.sh"
    );
}

#[test]
fn homebrew_formula_should_install_the_binary_without_shell_installer_side_effects() {
    let formula = formula();

    assert!(
        formula.contains("bin.install"),
        "formula must install the downloaded binary into Homebrew's bin directory"
    );
    assert!(
        formula.contains("=> \"aegis\""),
        "formula must rename the platform asset to the executable name aegis"
    );
    assert!(
        !formula.contains("curl -fsSL"),
        "Homebrew formula must not shell out to the convenience curl installer"
    );
    assert!(
        !formula.contains(".bashrc") && !formula.contains(".zshrc"),
        "Homebrew formula must not mutate user shell rc files during install"
    );
}

#[test]
fn homebrew_formula_should_have_a_non_interactive_runtime_test() {
    let formula = formula();

    assert!(
        formula.contains("test do"),
        "formula must include a Homebrew test block"
    );
    assert!(
        formula.contains("#{bin}/aegis -c"),
        "formula test should exercise Aegis command execution, not only --version"
    );
    assert!(
        formula.contains("brew-test"),
        "formula test should assert a deterministic safe command output"
    );
}

#[test]
fn homebrew_formula_updater_should_exist_and_fail_closed_on_missing_release_input() {
    let script = repo_file("scripts/update-homebrew-formula.sh");

    assert!(
        script.contains("set -eu"),
        "updater script must fail closed on unset variables and command failures"
    );
    assert!(
        script.contains("usage()"),
        "updater script must provide a usage path"
    );
    assert!(
        script.contains("aegis-linux-x86_64.sha256")
            && script.contains("aegis-linux-aarch64.sha256")
            && script.contains("aegis-macos-x86_64.sha256")
            && script.contains("aegis-macos-aarch64.sha256"),
        "updater must fetch all four release checksum sidecars"
    );
    assert!(
        script.contains("grep -Eq '^[[:xdigit:]]{64}$'"),
        "updater must validate checksum format before writing the formula"
    );
}

#[test]
fn homebrew_formula_updater_should_pin_third_party_notices_with_a_locally_computed_checksum() {
    let script = repo_file("scripts/update-homebrew-formula.sh");

    assert!(
        script.contains("THIRD_PARTY_NOTICES.md"),
        "updater must fetch the checked-in root notice asset from the release"
    );
    assert!(
        script.contains("sha256sum") && script.contains("shasum"),
        "updater must fall back to shasum when sha256sum is unavailable, matching scripts/install.sh"
    );
    assert!(
        script.contains("resource \"third_party_notices\" do"),
        "updater must emit a third_party_notices resource block into the generated formula"
    );
    assert!(
        script.contains("resource(\"third_party_notices\").stage do"),
        "updater must emit an install step that stages the third_party_notices resource"
    );
}

#[test]
fn homebrew_formula_should_explain_post_install_setup_caveats() {
    let formula = formula();

    assert!(
        formula.contains("def caveats"),
        "formula must explain Homebrew-specific post-install setup"
    );
    assert!(
        formula.contains("aegis install-hooks --all"),
        "caveats should tell users how to install supported agent hooks"
    );
    assert!(
        formula.contains("aegis setup-shell"),
        "caveats should recommend the explicit aegis setup-shell command for shell-proxy mode"
    );
}

#[test]
fn readme_should_document_homebrew_install_without_overclaiming_shell_setup() {
    let readme = repo_file("README.md");

    assert!(
        readme.contains("brew tap IliasAlmerekov/aegis"),
        "README must document the tap command"
    );
    assert!(
        readme.contains("brew install aegis"),
        "README must document brew install"
    );
    assert!(
        readme.contains("Homebrew installs the binary only"),
        "README must explain that Homebrew does not run the global shell installer"
    );
}

#[test]
fn release_readiness_should_track_homebrew_evidence() {
    let docs = repo_file("docs/release-readiness.md");

    assert!(
        docs.contains("Homebrew"),
        "release readiness docs must mention Homebrew"
    );
    assert!(
        docs.contains("brew install"),
        "release readiness docs must require brew install evidence"
    );
    assert!(
        docs.contains("macOS") && docs.contains("Linux"),
        "release readiness docs must require macOS and Linux smoke-test evidence"
    );
}

#[test]
fn homebrew_formula_should_download_raw_binaries_without_decompression() {
    let formula = formula();
    let url_lines: Vec<&str> = formula
        .lines()
        .filter(|line| {
            line.trim_start().starts_with("url \"") && !line.contains("THIRD_PARTY_NOTICES.md")
        })
        .collect();

    assert_eq!(
        url_lines.len(),
        4,
        "formula must have exactly four binary download urls"
    );
    assert!(
        url_lines
            .iter()
            .all(|line| line.contains("using: :nounzip")),
        "every raw-binary url must opt out of archive decompression with using: :nounzip"
    );
}

#[test]
fn release_readiness_should_publish_a_concrete_tap_runbook() {
    let docs = repo_file("docs/release-readiness.md");

    assert!(
        docs.contains("gh repo create IliasAlmerekov/homebrew-aegis"),
        "runbook must show how to create the tap repository"
    );
    assert!(
        docs.contains("brew audit --strict --online --formula aegis"),
        "runbook must audit the formula inside the tap"
    );
    assert!(
        docs.contains("brew test aegis"),
        "runbook must smoke-test the published formula with brew test"
    );
    assert!(
        docs.contains("scripts/update-homebrew-formula.sh"),
        "runbook must regenerate the formula from the deterministic updater"
    );
}
