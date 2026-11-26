#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers fail loudly"
)]
mod support;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use support::{base_command, write_global_config};
use tempfile::TempDir;

#[cfg(feature = "language-analysis")]
const GOLDEN_PATH: &str = "tests/fixtures/evaluation_golden.jsonl";
#[cfg(not(feature = "language-analysis"))]
const GOLDEN_PATH: &str = "tests/fixtures/evaluation_golden_default.jsonl";
const UPDATE_VAR: &str = "AEGIS_UPDATE_EVALUATION_GOLDEN";
const MODES: [&str; 2] = ["protect", "strict"];

#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    id: String,
    command: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Case {
    entry: Entry,
    mode: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Row {
    id: String,
    mode: String,
    command: String,
    risk: String,
    decision: String,
    exit_code: i64,
    block_reason: Option<String>,
    decision_source: String,
    pattern_ids: Vec<String>,
}

impl Row {
    fn fields(&self) -> [(&'static str, String); 7] {
        [
            ("command", format!("{:?}", self.command)),
            ("risk", self.risk.clone()),
            ("decision", self.decision.clone()),
            ("exit_code", self.exit_code.to_string()),
            ("block_reason", format!("{:?}", self.block_reason)),
            ("decision_source", self.decision_source.clone()),
            ("pattern_ids", format!("{:?}", self.pattern_ids)),
        ]
    }
}

fn changed_fields(golden: &Row, actual: &Row) -> Vec<String> {
    golden
        .fields()
        .into_iter()
        .zip(actual.fields())
        .filter(|((_, old), (_, new))| old != new)
        .map(|((name, old), (_, new))| format!("{name}: {old} -> {new}"))
        .collect()
}

fn compare_with_retry(
    golden: &[Row],
    cases: &[Case],
    mut evaluate: impl FnMut(&Case) -> Row,
) -> Result<(), String> {
    let by_key: BTreeMap<(&str, &str), &Row> = golden
        .iter()
        .map(|row| ((row.id.as_str(), row.mode.as_str()), row))
        .collect();
    let mut report = String::new();
    for case in cases {
        let key = (case.entry.id.as_str(), case.mode);
        let Some(expected) = by_key.get(&key) else {
            let _ = writeln!(report, "{} [{}]: missing from the golden", key.0, key.1);
            continue;
        };
        let mut actual = evaluate(case);
        if actual != **expected {
            actual = evaluate(case);
        }
        let changes = changed_fields(expected, &actual);
        if !changes.is_empty() {
            let _ = writeln!(report, "{} [{}] {:?}", key.0, key.1, case.entry.command);
            for change in changes {
                let _ = writeln!(report, "    {change}");
            }
        }
    }
    let known: Vec<(&str, &str)> = cases
        .iter()
        .map(|case| (case.entry.id.as_str(), case.mode))
        .collect();
    for (id, mode) in by_key.keys().filter(|key| !known.contains(key)) {
        let _ = writeln!(report, "{id} [{mode}]: golden row not in the corpus");
    }
    if report.is_empty() {
        Ok(())
    } else {
        Err(report)
    }
}

#[derive(Deserialize)]
struct BypassCorpus {
    cases: Vec<BypassCase>,
}

#[derive(Deserialize)]
struct BypassCase {
    name: String,
    command: String,
}

#[derive(Deserialize)]
struct EvaluationCorpus {
    seed_files: Vec<String>,
    commands: Vec<NamedCommand>,
}

#[derive(Deserialize)]
struct NamedCommand {
    name: String,
    command: String,
}

fn read_fixture(relative: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn load_entries() -> Vec<Entry> {
    let bypass: BypassCorpus =
        toml::from_str(&read_fixture("tests/fixtures/security_bypass_corpus.toml")).unwrap();
    let corpus: EvaluationCorpus =
        toml::from_str(&read_fixture("tests/fixtures/evaluation_corpus.toml")).unwrap();

    let mut entries: Vec<Entry> = bypass
        .cases
        .into_iter()
        .map(|case| Entry {
            id: format!("bypass:{}", case.name),
            command: case.command,
        })
        .collect();
    entries.extend(corpus.seed_files.iter().map(|path| Entry {
        id: format!("seed:{path}"),
        command: read_fixture(path).replace("\r\n", "\n"),
    }));
    entries.extend(corpus.commands.into_iter().map(|command| Entry {
        id: format!("command:{}", command.name),
        command: command.command,
    }));
    entries
}

fn cases(entries: &[Entry]) -> Vec<Case> {
    entries
        .iter()
        .flat_map(|entry| {
            MODES.map(|mode| Case {
                entry: entry.clone(),
                mode,
            })
        })
        .collect()
}

fn evaluate(case: &Case) -> Row {
    let home = TempDir::new().unwrap();
    let cwd = TempDir::new().unwrap();
    write_global_config(
        home.path(),
        &format!("mode = \"{}\"\n", capitalize(case.mode)),
    );

    let output = base_command(home.path())
        .current_dir(cwd.path())
        .args(["-c", &case.entry.command, "--output", "json"])
        .output()
        .unwrap();
    let json: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "{}: stdout is not JSON ({error}); stderr: {}",
            case.entry.id,
            String::from_utf8_lossy(&output.stderr)
        )
    });
    assert_eq!(
        json["mode"], case.mode,
        "{}: mode not applied",
        case.entry.id
    );
    assert_eq!(
        json["execution"]["will_execute"], false,
        "{}: an Evaluation must not execute",
        case.entry.id
    );

    let text = |key: &str| json[key].as_str().unwrap_or_default().to_string();
    let exit_code = json["exit_code"].as_i64().unwrap();
    assert_eq!(output.status.code(), i32::try_from(exit_code).ok());
    let mut pattern_ids: Vec<String> = json["matched_patterns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|pattern| pattern["id"].as_str().unwrap().to_string())
        .collect();
    pattern_ids.sort();
    Row {
        id: case.entry.id.clone(),
        mode: case.mode.to_string(),
        command: case.entry.command.clone(),
        risk: text("risk"),
        decision: text("decision"),
        exit_code,
        block_reason: json["block_reason"].as_str().map(str::to_string),
        decision_source: text("decision_source"),
        pattern_ids,
    }
}

fn capitalize(mode: &str) -> String {
    let (first, rest) = mode.split_at(1);
    first.to_uppercase() + rest
}

fn render_golden(rows: &[Row]) -> String {
    rows.iter()
        .map(|row| serde_json::to_string(row).unwrap() + "\n")
        .collect()
}

fn parse_golden(contents: &str) -> Vec<Row> {
    contents
        .lines()
        .map(|line| serde_json::from_str(line).unwrap_or_else(|error| panic!("{line}: {error}")))
        .collect()
}

#[test]
fn corpus_ids_and_commands_are_unique() {
    let entries = load_entries();
    let mut ids: Vec<_> = entries.iter().map(|entry| &entry.id).collect();
    let mut commands: Vec<_> = entries.iter().map(|entry| &entry.command).collect();
    ids.sort();
    ids.dedup();
    commands.sort();
    commands.dedup();
    assert_eq!(ids.len(), entries.len(), "duplicate corpus id");
    assert_eq!(commands.len(), entries.len(), "duplicate corpus command");
}

#[test]
fn evaluation_matches_the_golden() {
    let cases = cases(&load_entries());
    if std::env::var_os(UPDATE_VAR).is_some() {
        let rows: Vec<Row> = cases.iter().map(evaluate).collect();
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(GOLDEN_PATH);
        fs::write(path, render_golden(&rows)).unwrap();
        return;
    }
    let golden = parse_golden(&read_fixture(GOLDEN_PATH));
    if let Err(report) = compare_with_retry(&golden, &cases, evaluate) {
        panic!(
            "the Evaluation differs from {GOLDEN_PATH}; if the change is intended, run \
             `{UPDATE_VAR}=1 cargo test --test evaluation_golden` and commit the golden:\n{report}"
        );
    }
}

#[cfg(test)]
mod compare_tests {
    use super::*;

    fn case(id: &str, mode: &'static str) -> Case {
        Case {
            entry: Entry {
                id: id.to_string(),
                command: format!("cmd-{id}"),
            },
            mode,
        }
    }

    fn row(id: &str, mode: &str, risk: &str) -> Row {
        Row {
            id: id.to_string(),
            mode: mode.to_string(),
            command: format!("cmd-{id}"),
            risk: risk.to_string(),
            decision: "auto_approve".to_string(),
            exit_code: 0,
            block_reason: None,
            decision_source: "fallback".to_string(),
            pattern_ids: Vec::new(),
        }
    }

    #[test]
    fn compare_passes_when_every_row_matches() {
        let golden = [row("a", "protect", "safe")];
        let result = compare_with_retry(&golden, &[case("a", "protect")], |_| {
            row("a", "protect", "safe")
        });
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn compare_passes_when_a_row_differs_only_on_the_first_run() {
        let golden = [row("a", "protect", "safe")];
        let mut calls = 0;
        let result = compare_with_retry(&golden, &[case("a", "protect")], |_| {
            calls += 1;
            row("a", "protect", if calls == 1 { "warn" } else { "safe" })
        });
        assert_eq!(result, Ok(()));
        assert_eq!(calls, 2);
    }

    #[test]
    fn compare_names_command_mode_and_changed_field_when_a_row_differs_twice() {
        let golden = [row("a", "strict", "safe")];
        let message = compare_with_retry(&golden, &[case("a", "strict")], |_| {
            row("a", "strict", "warn")
        })
        .unwrap_err();
        assert!(message.contains("cmd-a"), "{message}");
        assert!(message.contains("strict"), "{message}");
        assert!(message.contains("risk: safe -> warn"), "{message}");
    }

    #[test]
    fn compare_reports_a_case_missing_from_the_golden_and_an_extra_golden_row() {
        let golden = [row("old", "protect", "safe")];
        let message = compare_with_retry(&golden, &[case("new", "protect")], |_| {
            row("new", "protect", "safe")
        })
        .unwrap_err();
        assert!(message.contains("missing from the golden"), "{message}");
        assert!(message.contains("old"), "{message}");
        assert!(message.contains("not in the corpus"), "{message}");
    }
}
