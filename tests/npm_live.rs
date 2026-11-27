#![expect(
    clippy::print_stderr,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers fail loudly"
)]
mod support;

use std::process::Command;
use std::time::Duration;

use support::version_matches;

const PACKAGE: &str = "@iliasalmerekov/aegis";
const INSTALL_ATTEMPTS: u32 = 10;
const RETRY_DELAY: Duration = Duration::from_secs(30);

fn run(command: &str, args: &[&str]) -> std::process::Output {
    Command::new(command)
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("failed to run {command} {args:?}: {error}"))
}

fn npm_install_args(spec: &str) -> [&str; 4] {
    ["install", "-g", "--prefer-online", spec]
}

#[test]
fn npm_install_args_should_revalidate_cached_metadata() {
    let spec = "@iliasalmerekov/aegis@1.2.3";
    assert_eq!(
        npm_install_args(spec),
        ["install", "-g", "--prefer-online", spec]
    );
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
    let args = npm_install_args(&spec);
    let shown = args.join(" ");

    let mut last = None;
    for attempt in 1..=INSTALL_ATTEMPTS {
        let install = run("npm", &args);
        if install.status.success() {
            last = None;
            break;
        }
        eprintln!("npm {shown} failed (attempt {attempt}/{INSTALL_ATTEMPTS})");
        last = Some(install);
        if attempt < INSTALL_ATTEMPTS {
            std::thread::sleep(RETRY_DELAY);
        }
    }
    if let Some(install) = last {
        panic!(
            "npm {shown} failed after {INSTALL_ATTEMPTS} attempts\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&install.stdout),
            String::from_utf8_lossy(&install.stderr)
        );
    }

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
