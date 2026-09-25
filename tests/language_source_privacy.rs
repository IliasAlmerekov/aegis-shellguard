//! End-to-end privacy regressions for Language-aware source analysis.

mod support;

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::Duration;

use aegis::analysis::{AnalysisCwd, OrchestrationBudget, Outcome, run_with_budget_in_cwd};
use aegis_scanner::PatternSet;
use aegis_scanner::Scanner;
use tempfile::TempDir;

const SOURCE_SENTINEL: &str = "LANGUAGE_SOURCE_PRIVACY_SENTINEL_7f1c";

fn write_analyzed_script(cwd: &Path) -> String {
    fs::write(
        cwd.join("checked.py"),
        format!("import os\nos.remove({SOURCE_SENTINEL:?})\n"),
    )
    .expect("privacy fixture source must be written");
    "python3 ./checked.py".to_string()
}

fn shows_match(bytes: &[u8]) -> bool {
    String::from_utf8_lossy(bytes).contains("LANG-FS-DEL")
}

fn read_audit(home: &Path) -> Vec<u8> {
    fs::read(home.join(".aegis/audit.jsonl")).unwrap_or_default()
}

/// Runs one surface in a fresh home until `ran` sees the fixture's Match, so
/// each privacy check runs against real analysis output. The CLI clamps the
/// analysis deadline to 100 ms, and a loaded test run can miss it (#458).
/// Every fixture here denies, so a repeat executes nothing.
fn until_match(
    label: &str,
    mut run: impl FnMut(&Path) -> Output,
    ran: impl Fn(&TempDir, &Output) -> bool,
) -> (TempDir, Output) {
    support::until_analysis_completes(
        label,
        || {
            let home = TempDir::new().expect("temporary fixture home");
            let output = run(home.path());
            (home, output)
        },
        |(home, output)| ran(home, output),
    )
}

fn run_aegis(home: &Path, cwd: &Path, args: &[&str], input: Option<&[u8]>, ci: bool) -> Output {
    let mut process = Command::new(env!("CARGO_BIN_EXE_aegis"));
    process
        .args(args)
        .env("HOME", home)
        .env("AEGIS_REAL_SHELL", "/bin/sh")
        .env("AEGIS_FORCE_NO_TTY", "1")
        .env("AEGIS_CI", if ci { "1" } else { "0" })
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = process.spawn().expect("Aegis privacy fixture must spawn");
    if let Some(input) = input {
        child
            .stdin
            .as_mut()
            .expect("stdin must be piped")
            .write_all(input)
            .expect("fixture input must be written");
    }
    child.stdin.take();
    child
        .wait_with_output()
        .expect("Aegis privacy fixture must finish")
}

/// Positive counterpart to `assert_does_not_disclose_source`: proves language
/// analysis actually ran and produced the fixture's `LANG-FS-DEL` Match on this
/// surface, so the accompanying privacy assertion cannot pass vacuously because
/// analysis silently degraded (e.g. missed the timeout) and produced no Match
/// at all.
fn assert_ran_language_analysis(label: &str, bytes: &[u8]) {
    let rendered = String::from_utf8_lossy(bytes);
    assert!(
        rendered.contains("LANG-FS-DEL"),
        "{label} must show the fixture's language-aware Match ran: {rendered}"
    );
}

fn assert_does_not_disclose_source(label: &str, bytes: &[u8]) {
    let rendered = String::from_utf8_lossy(bytes);
    assert!(
        !rendered.contains(SOURCE_SENTINEL),
        "{label} must not disclose analyzed source: {rendered}"
    );
}

#[tokio::test]
async fn analyzed_script_source_is_absent_from_every_public_output_surface() {
    // The command deliberately contains only a path. The sentinel exists solely
    // in the inspected script, so this detects source disclosure rather than the
    // supported raw-command echo in Shell output and audit records.
    let cwd = TempDir::new().expect("temporary command cwd");
    let command = write_analyzed_script(cwd.path());

    let scanner = Scanner::try_new(PatternSet::load().expect("built-ins load"))
        .expect("built-in scanner compiles");
    let baseline = scanner.assess(&command);
    let assessment = match run_with_budget_in_cwd(
        &command,
        AnalysisCwd::Resolved(cwd.path()),
        &baseline,
        Some(env!("CARGO_BIN_EXE_aegis")),
        &[],
        OrchestrationBudget {
            total_timeout: Duration::from_secs(2),
            ..OrchestrationBudget::L1_DEFAULT
        },
        Some(&scanner),
    )
    .await
    {
        Outcome::Analyzed { assessment, .. } => assessment,
        outcome => panic!("script-file fixture must reach language analysis: {outcome:?}"),
    };
    assert!(
        assessment
            .matched
            .iter()
            .any(|matched| matched.pattern.id.as_ref() == "LANG-FS-DEL"),
        "fixture must produce a source-derived Match: {assessment:?}"
    );
    assert_does_not_disclose_source(
        "Assessment",
        assessment
            .matched
            .iter()
            .map(|matched| matched.matched_text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
            .as_bytes(),
    );

    let (shell_home, shell) = until_match(
        "Shell",
        |home| run_aegis(home, cwd.path(), &["--command", &command], None, false),
        |home, _| shows_match(&read_audit(home.path())),
    );
    // Shell's non-interactive text output only renders match detail with
    // `--verbose`; the audit log is the surface that proves language analysis
    // ran without disclosing source for the default (non-verbose) invocation.
    assert_does_not_disclose_source("Shell stdout", &shell.stdout);
    assert_does_not_disclose_source("Shell stderr", &shell.stderr);
    let shell_audit = fs::read(shell_home.path().join(".aegis/audit.jsonl"))
        .expect("Shell assessment must be audited");
    assert_ran_language_analysis("audit JSONL", &shell_audit);
    assert_does_not_disclose_source("audit JSONL", &shell_audit);

    let (tui_home, tui) = until_match(
        "interactive TUI",
        |home| run_interactive_aegis(home, cwd.path(), &command),
        |home, output| shows_match(&output.stderr) && shows_match(&read_audit(home.path())),
    );
    assert_eq!(
        tui.status.code(),
        Some(2),
        "declining TUI confirmation denies"
    );
    assert!(
        String::from_utf8_lossy(&tui.stderr).contains("AEGIS INTERCEPTED"),
        "fixture must render the interactive TUI confirmation"
    );
    assert_ran_language_analysis("interactive TUI stderr", &tui.stderr);
    assert_does_not_disclose_source("interactive TUI stdout", &tui.stdout);
    assert_does_not_disclose_source("interactive TUI stderr", &tui.stderr);
    let tui_audit = fs::read(tui_home.path().join(".aegis/audit.jsonl"))
        .expect("interactive assessment must be audited");
    assert_ran_language_analysis("interactive TUI audit JSONL", &tui_audit);
    assert_does_not_disclose_source("interactive TUI audit JSONL", &tui_audit);

    let watch_input = format!(r#"{{"cmd":{command:?},"id":"privacy"}}"#) + "\n";
    let (watch_home, watch) = until_match(
        "Watch",
        |home| {
            run_aegis(
                home,
                cwd.path(),
                &["watch"],
                Some(watch_input.as_bytes()),
                false,
            )
        },
        |home, _| shows_match(&read_audit(home.path())),
    );
    // Watch's NDJSON result frame carries only `decision`/`exit_code` — it
    // structurally cannot echo match text — so the audit log is the surface
    // that actually proves Watch ran language analysis without disclosing source.
    assert_does_not_disclose_source("Watch NDJSON", &watch.stdout);
    assert_does_not_disclose_source("Watch diagnostics", &watch.stderr);
    let watch_audit = fs::read(watch_home.path().join(".aegis/audit.jsonl"))
        .expect("Watch assessment must be audited");
    assert_ran_language_analysis("Watch audit JSONL", &watch_audit);
    assert_does_not_disclose_source("Watch audit JSONL", &watch_audit);

    let (_ci_home, ci) = until_match(
        "non-interactive CI",
        |home| {
            run_aegis(
                home,
                cwd.path(),
                &["--output", "json", "--command", &command],
                None,
                true,
            )
        },
        |_, output| shows_match(&output.stdout),
    );
    assert_ran_language_analysis("non-interactive CI JSON", &ci.stdout);
    assert_does_not_disclose_source("non-interactive CI JSON", &ci.stdout);
    assert_does_not_disclose_source("non-interactive CI diagnostics", &ci.stderr);
}

fn run_interactive_aegis(home: &Path, cwd: &Path, command: &str) -> Output {
    let mut process = Command::new(env!("CARGO_BIN_EXE_aegis"));
    process
        .args(["--command", command])
        .env("HOME", home)
        .env("AEGIS_REAL_SHELL", "/bin/sh")
        .env("AEGIS_FORCE_INTERACTIVE", "1")
        .env("AEGIS_CI", "0")
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = process.spawn().expect("interactive TUI fixture must spawn");
    child
        .stdin
        .as_mut()
        .expect("interactive stdin must be piped")
        .write_all(b"n\n")
        .expect("interactive decline must be written");
    child.stdin.take();
    child
        .wait_with_output()
        .expect("interactive TUI fixture must finish")
}
