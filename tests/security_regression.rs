#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers fail loudly"
)]
mod support;

use std::fs;
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;
use support::base_command;
use tempfile::TempDir;

#[derive(Debug, Deserialize)]
struct SecurityRegressionCorpus {
    cases: Vec<SecurityRegressionCase>,
}

#[derive(Debug, Deserialize)]
struct SecurityRegressionCase {
    name: String,
    command: String,
    expected_risk: String,
    expected_decision: String,
    expected_exit_code: i32,
    expected_block_reason: Option<String>,
    #[cfg(not(feature = "language-analysis"))]
    #[serde(default)]
    default_expected_risk: Option<String>,
    #[cfg(not(feature = "language-analysis"))]
    #[serde(default)]
    default_expected_decision: Option<String>,
    #[cfg(not(feature = "language-analysis"))]
    #[serde(default)]
    default_expected_exit_code: Option<i32>,
    #[cfg(not(feature = "language-analysis"))]
    #[serde(default)]
    default_expected_block_reason: Option<String>,
}

impl SecurityRegressionCase {
    #[cfg(feature = "language-analysis")]
    fn expected(&self) -> (&str, &str, i32, Option<&str>) {
        (
            &self.expected_risk,
            &self.expected_decision,
            self.expected_exit_code,
            self.expected_block_reason.as_deref(),
        )
    }

    #[cfg(not(feature = "language-analysis"))]
    fn expected(&self) -> (&str, &str, i32, Option<&str>) {
        (
            self.default_expected_risk
                .as_deref()
                .unwrap_or(&self.expected_risk),
            self.default_expected_decision
                .as_deref()
                .unwrap_or(&self.expected_decision),
            self.default_expected_exit_code
                .unwrap_or(self.expected_exit_code),
            self.default_expected_block_reason
                .as_deref()
                .or(self.expected_block_reason.as_deref()),
        )
    }
}

fn load_corpus() -> SecurityRegressionCorpus {
    let path = Path::new("tests/fixtures/security_bypass_corpus.toml");
    let contents = fs::read_to_string(path).unwrap();
    toml::from_str(&contents).unwrap()
}

fn write_strict_security_config(workspace: &Path) {
    fs::write(
        workspace.join(".aegis.toml"),
        r#"
mode = "Strict"
auto_snapshot_git = false
auto_snapshot_docker = false
"#,
    )
    .unwrap();
}

fn run_case(home: &Path, workspace: &Path, case: &SecurityRegressionCase) -> (i32, Value, Vec<u8>) {
    let output = base_command(home)
        .current_dir(workspace)
        .args(["-c", &case.command, "--output", "json"])
        .output()
        .unwrap();

    let exit_code = output.status.code().unwrap_or_default();
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    (exit_code, json, output.stderr)
}

#[test]
fn security_regression_corpus_preserves_non_safe_decisions_for_bypass_attempts() {
    let corpus = load_corpus();
    let home = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    write_strict_security_config(workspace.path());

    for case in &corpus.cases {
        let (exit_code, json, stderr) = run_case(home.path(), workspace.path(), case);
        let (expected_risk, expected_decision, expected_exit_code, expected_block_reason) =
            case.expected();

        assert!(
            stderr.is_empty(),
            "case {} must keep stderr empty in json mode: {}",
            case.name,
            String::from_utf8_lossy(&stderr)
        );
        assert_eq!(
            exit_code, expected_exit_code,
            "case {} exit code mismatch",
            case.name
        );
        assert_eq!(
            json["risk"].as_str(),
            Some(expected_risk),
            "case {} risk mismatch",
            case.name
        );
        assert_eq!(
            json["decision"].as_str(),
            Some(expected_decision),
            "case {} decision mismatch",
            case.name
        );
        assert!(
            json["matched_patterns"]
                .as_array()
                .is_some_and(|patterns| !patterns.is_empty()),
            "case {} must report at least one matched pattern",
            case.name
        );

        match expected_block_reason {
            Some(reason) => assert_eq!(
                json["block_reason"].as_str(),
                Some(reason),
                "case {} block_reason mismatch",
                case.name
            ),
            None => assert!(
                json.get("block_reason").is_none() || json["block_reason"].is_null(),
                "case {} should not report block_reason",
                case.name
            ),
        }
    }
}
