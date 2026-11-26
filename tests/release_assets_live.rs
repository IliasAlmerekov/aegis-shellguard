#![expect(
    clippy::print_stderr,
    clippy::panic,
    reason = "test helpers fail loudly"
)]
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

const EXPECTED_ASSETS: [&str; 4] = [
    "aegis-linux-x86_64",
    "aegis-linux-aarch64",
    "aegis-macos-x86_64",
    "aegis-macos-aarch64",
];

fn run(command: &str, args: &[&str]) -> std::process::Output {
    Command::new(command)
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("failed to run {command} {args:?}: {error}"))
}

fn command_succeeds(command: &str, args: &[&str]) -> bool {
    Command::new(command)
        .args(args)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn release_tag() -> String {
    std::env::var("AEGIS_TEST_RELEASE_TAG").unwrap_or_else(|_| "v0.5.7".to_string())
}

fn repository() -> String {
    std::env::var("AEGIS_TEST_RELEASE_REPO")
        .unwrap_or_else(|_| "IliasAlmerekov/aegis-shellguard".to_string())
}

fn download(url: &str, destination: &Path) {
    let destination = destination.to_str().unwrap_or_else(|| {
        panic!(
            "download destination should be UTF-8: {}",
            destination.display()
        )
    });
    let output = run("curl", &["-fL", "-sS", url, "-o", destination]);
    assert!(
        output.status.success(),
        "curl download failed for {url}\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn checksum_tool() -> (&'static str, Vec<&'static str>) {
    if command_succeeds("sha256sum", &["--version"]) {
        ("sha256sum", vec!["-c"])
    } else {
        ("shasum", vec!["-a", "256", "-c"])
    }
}

fn verify_sidecar(download_dir: &Path, sidecar: &Path) {
    let (tool, mut args) = checksum_tool();
    let sidecar_name = sidecar
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_else(|| {
            panic!(
                "sidecar path should have UTF-8 file name: {}",
                sidecar.display()
            )
        });
    args.push(sidecar_name);

    let output = Command::new(tool)
        .args(args)
        .current_dir(download_dir)
        .output()
        .unwrap_or_else(|error| panic!("failed to run checksum tool {tool}: {error}"));

    assert!(
        output.status.success(),
        "{tool} verification failed for {}\nstdout:\n{}\nstderr:\n{}",
        sidecar.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn live_github_release_has_all_binaries_and_matching_sha256_sidecars() {
    if std::env::var_os("AEGIS_TEST_LIVE_RELEASE").is_none() {
        eprintln!("skipping live release test; set AEGIS_TEST_LIVE_RELEASE=1");
        return;
    }

    let repo = repository();
    let tag = release_tag();
    let api_url = format!("https://api.github.com/repos/{repo}/releases/tags/{tag}");
    let temp = TempDir::new().expect("temporary directory should be created");
    let release_json = temp.path().join("release.json");

    download(&api_url, &release_json);
    let raw = fs::read_to_string(&release_json).expect("release JSON should be readable");
    let release: Value = serde_json::from_str(&raw).expect("release JSON should parse");
    let assets = release
        .get("assets")
        .and_then(Value::as_array)
        .expect("release JSON should contain assets array");

    for asset in EXPECTED_ASSETS {
        for name in [asset.to_string(), format!("{asset}.sha256")] {
            let browser_download_url = assets
                .iter()
                .find(|entry| entry.get("name").and_then(Value::as_str) == Some(name.as_str()))
                .and_then(|entry| entry.get("browser_download_url"))
                .and_then(Value::as_str)
                .unwrap_or_else(|| panic!("release {tag} must contain asset {name}"));

            let destination = temp.path().join(&name);
            download(browser_download_url, &destination);
        }

        verify_sidecar(
            temp.path(),
            &PathBuf::from(temp.path()).join(format!("{asset}.sha256")),
        );
    }
}
