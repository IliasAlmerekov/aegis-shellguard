// Gated live npm smoke test. Default `cargo test` is network-free: this test
// is a no-op unless `AEGIS_TEST_LIVE_NPM=1` is set, in which case it installs
// the published `@iliasalmerekov/aegis@AEGIS_TEST_EXPECTED_VERSION` from the
// npm registry, retrying while the registry catches up with a fresh publish,
// and checks that `aegis --version` prints that version. The release
// workflow's `smoke-install` job runs it; run it by hand where npm is
// available:
//   AEGIS_TEST_LIVE_NPM=1 AEGIS_TEST_EXPECTED_VERSION=0.7.0 \
//     cargo test --test npm_live -- --nocapture

use std::process::Command;
use std::time::Duration;

const PACKAGE: &str = "@iliasalmerekov/aegis";
const INSTALL_ATTEMPTS: u32 = 10;
const RETRY_DELAY: Duration = Duration::from_secs(30);

fn run(command: &str, args: &[&str]) -> std::process::Output {
    Command::new(command)
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("failed to run {command} {args:?}: {error}"))
}

/// True when `aegis --version` output names exactly `expected` as a token.
fn version_matches(output: &str, expected: &str) -> bool {
    output.split_whitespace().any(|token| token == expected)
}

#[test]
fn version_matches_should_compare_whole_tokens() {
    assert!(version_matches("aegis 0.7.0\n", "0.7.0"));
    assert!(!version_matches("aegis 0.7.01\n", "0.7.0"));
    assert!(!version_matches("aegis 0.6.3\n", "0.7.0"));
}

#[test]
fn live_npm_package_installs_and_runs_aegis() {
    if std::env::var_os("AEGIS_TEST_LIVE_NPM").is_none() {
        eprintln!("skipping live npm test; set AEGIS_TEST_LIVE_NPM=1");
        return;
    }
    let expected = std::env::var("AEGIS_TEST_EXPECTED_VERSION")
        .expect("AEGIS_TEST_EXPECTED_VERSION must name the published version");
    let spec = format!("{PACKAGE}@{expected}");

    let mut last = None;
    for attempt in 1..=INSTALL_ATTEMPTS {
        let install = run("npm", &["install", "-g", &spec]);
        if install.status.success() {
            last = None;
            break;
        }
        eprintln!("npm install -g {spec} failed (attempt {attempt}/{INSTALL_ATTEMPTS})");
        last = Some(install);
        if attempt < INSTALL_ATTEMPTS {
            std::thread::sleep(RETRY_DELAY);
        }
    }
    if let Some(install) = last {
        panic!(
            "npm install -g {spec} failed after {INSTALL_ATTEMPTS} attempts\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&install.stdout),
            String::from_utf8_lossy(&install.stderr)
        );
    }

    // Run the binary npm installed, not whatever else is on PATH.
    let prefix = run("npm", &["prefix", "-g"]);
    assert!(prefix.status.success(), "npm prefix -g failed");
    let prefix = String::from_utf8_lossy(&prefix.stdout).trim().to_string();
    let version = Command::new(format!("{prefix}/bin/aegis"))
        .arg("--version")
        .output()
        .expect("installed aegis should run");
    assert!(
        version.status.success(),
        "aegis --version failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&version.stdout),
        String::from_utf8_lossy(&version.stderr)
    );
    let printed = String::from_utf8_lossy(&version.stdout);
    assert!(
        version_matches(&printed, &expected),
        "aegis --version printed {printed:?}, expected version {expected}"
    );
}
