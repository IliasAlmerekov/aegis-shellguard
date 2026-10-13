// Gated live Homebrew smoke test. Default `cargo test` is network-free: this
// test is a no-op unless `AEGIS_TEST_LIVE_HOMEBREW=1` is set, in which case it
// taps the published tap, audits the formula, installs it, runs `brew test`,
// and checks that `aegis --version` prints `AEGIS_TEST_EXPECTED_VERSION`. The
// release workflow's `smoke-install` job runs it; run it by hand where
// Homebrew is available:
//   AEGIS_TEST_LIVE_HOMEBREW=1 AEGIS_TEST_EXPECTED_VERSION=0.7.0 \
//     cargo test --test homebrew_live -- --nocapture

use std::process::Command;

const FORMULA: &str = "IliasAlmerekov/aegis/aegis";

fn run_brew(args: &[&str]) -> std::process::Output {
    Command::new("brew")
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("failed to run brew {args:?}: {error}"))
}

fn assert_brew_ok(label: &str, output: &std::process::Output) {
    assert!(
        output.status.success(),
        "{label} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
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
fn live_homebrew_tap_installs_and_tests_aegis() {
    if std::env::var_os("AEGIS_TEST_LIVE_HOMEBREW").is_none() {
        eprintln!("skipping live Homebrew test; set AEGIS_TEST_LIVE_HOMEBREW=1");
        return;
    }
    let expected = std::env::var("AEGIS_TEST_EXPECTED_VERSION")
        .expect("AEGIS_TEST_EXPECTED_VERSION must name the version the tap should install");

    assert_brew_ok("brew tap", &run_brew(&["tap", "IliasAlmerekov/aegis"]));
    assert_brew_ok(
        "brew audit",
        &run_brew(&["audit", "--strict", "--online", FORMULA]),
    );
    assert_brew_ok("brew install", &run_brew(&["install", FORMULA]));
    assert_brew_ok("brew test", &run_brew(&["test", FORMULA]));

    // Run the binary the formula installed, not whatever else is on PATH.
    let prefix = run_brew(&["--prefix"]);
    assert_brew_ok("brew --prefix", &prefix);
    let prefix = String::from_utf8_lossy(&prefix.stdout).trim().to_string();
    let version = Command::new(format!("{prefix}/bin/aegis"))
        .arg("--version")
        .output()
        .expect("installed aegis should run");
    assert_brew_ok("aegis --version", &version);
    let printed = String::from_utf8_lossy(&version.stdout);
    assert!(
        version_matches(&printed, &expected),
        "aegis --version printed {printed:?}, expected version {expected}"
    );
}
