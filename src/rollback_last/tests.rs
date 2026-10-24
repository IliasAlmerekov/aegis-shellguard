use super::*;
use aegis_audit::{AuditEntry, AuditSnapshot};
use aegis_types::{Decision, RiskLevel};

fn location(cwd: &str, repo_top: Option<&str>) -> Location {
    Location {
        cwd: PathBuf::from(cwd),
        repo_top: repo_top.map(PathBuf::from),
    }
}

fn snap(plugin: &str, id: &str, git_target: Option<&str>) -> AuditSnapshot {
    AuditSnapshot {
        plugin: plugin.to_string(),
        snapshot_id: id.to_string(),
        git_target: git_target.map(str::to_string),
    }
}

fn entry(command: &str, cwd: Option<&str>, snapshots: Vec<AuditSnapshot>) -> AuditEntry {
    AuditEntry::new(
        command,
        RiskLevel::Danger,
        Vec::new(),
        Decision::Approved,
        snapshots,
        None,
        None,
    )
    .with_cwd(cwd.map(str::to_string))
}

fn names(candidates: &[Candidate<'_>]) -> Vec<String> {
    candidates
        .iter()
        .map(|c| c.command.command.clone())
        .collect()
}

fn command_snapshot(plugin: &str, id: &str, git_target: Option<&str>) -> CommandSnapshot {
    CommandSnapshot {
        plugin: plugin.to_string(),
        snapshot_id: id.to_string(),
        git_target: git_target.map(str::to_string),
        restored_at: None,
    }
}

#[test]
fn git_snapshot_is_bound_when_its_target_is_the_repository_top_level() {
    let here = location("/r/sub", Some("/r"));

    assert!(is_bound(
        &here,
        "/r/sub",
        &command_snapshot("git", "i", Some("/r"))
    ));
}

#[test]
fn git_snapshot_of_another_repository_is_not_bound() {
    let here = location("/r", Some("/r"));

    assert!(!is_bound(
        &here,
        "/r",
        &command_snapshot("git", "i", Some("/other"))
    ));
}

#[test]
fn git_snapshot_without_a_target_is_not_bound_inside_a_repository() {
    let here = location("/r", Some("/r"));

    assert!(!is_bound(&here, "/r", &command_snapshot("git", "i", None)));
}

#[test]
fn outside_git_a_git_snapshot_is_bound_by_the_command_cwd() {
    let here = location("/work", None);
    let snapshot = command_snapshot("git", "i", Some("/anything"));

    assert!(is_bound(&here, "/work", &snapshot));
    assert!(is_bound(&here, "/work/deeper", &snapshot));
    assert!(!is_bound(&here, "/elsewhere", &snapshot));
}

#[test]
fn sqlite_snapshot_is_bound_by_its_database_file() {
    let id = "/r/data/app.db\t/store/dump.db";
    let snapshot = command_snapshot("sqlite", id, None);

    assert!(is_bound(&location("/r", Some("/r")), "/r", &snapshot));
    assert!(!is_bound(&location("/x", Some("/x")), "/x", &snapshot));
    assert!(is_bound(&location("/r/data", None), "/r/data", &snapshot));
    assert!(!is_bound(
        &location("/r/other", None),
        "/r/other",
        &snapshot
    ));
}

#[test]
fn docker_and_database_snapshots_are_never_bound() {
    let here = location("/r", Some("/r"));

    for plugin in ["docker", "postgres", "mysql", "supabase"] {
        assert!(
            !is_bound(&here, "/r", &command_snapshot(plugin, "i", Some("/r"))),
            "{plugin}"
        );
    }
}

#[test]
fn candidates_are_latest_first_and_skip_entries_without_a_cwd() {
    let ledger = SnapshotLedger::from_entries(&[
        entry("old", Some("/r"), vec![snap("git", "A", Some("/r"))]),
        entry("legacy", None, vec![snap("git", "B", Some("/r"))]),
        entry("new", Some("/r"), vec![snap("git", "C", Some("/r"))]),
        entry("elsewhere", Some("/o"), vec![snap("git", "D", Some("/o"))]),
    ]);

    let found = candidates(&ledger, &location("/r", Some("/r")));

    assert_eq!(names(&found), vec!["new", "old"]);
}

#[test]
fn candidate_splits_a_multi_provider_command_into_bound_and_unbound() {
    let db = "/r/app.db\t/store/d.db";
    let ledger = SnapshotLedger::from_entries(&[entry(
        "danger",
        Some("/r"),
        vec![
            snap("git", "G", Some("/r")),
            snap("sqlite", db, None),
            snap("docker", "D", None),
        ],
    )]);

    let found = candidates(&ledger, &location("/r", Some("/r")));

    let ids = |list: &[&CommandSnapshot]| list.iter().map(|s| s.plugin.clone()).collect::<Vec<_>>();
    assert_eq!(ids(&found[0].bound), vec!["git", "sqlite"]);
    assert_eq!(ids(&found[0].unbound), vec!["docker"]);
}

#[test]
fn a_command_with_only_unbound_snapshots_is_not_a_candidate() {
    let ledger = SnapshotLedger::from_entries(&[entry(
        "docker rm c",
        Some("/r"),
        vec![snap("docker", "D", None)],
    )]);

    assert!(candidates(&ledger, &location("/r", Some("/r"))).is_empty());
}

fn two_candidates() -> SnapshotLedger {
    SnapshotLedger::from_entries(&[
        entry("older", Some("/r"), vec![snap("git", "A", Some("/r"))]),
        entry("newer", Some("/r"), vec![snap("git", "B", Some("/r"))]),
    ])
}

fn pick(yes: bool, is_tty: bool, typed: &str) -> Choice {
    let ledger = two_candidates();
    let here = location("/r", Some("/r"));
    choose(
        &candidates(&ledger, &here),
        &here,
        yes,
        is_tty,
        &mut typed.as_bytes(),
    )
}

fn restored_index(choice: &Choice) -> Option<usize> {
    match choice {
        Choice::Restore(index) => Some(*index),
        Choice::Refuse { .. } => None,
    }
}

#[test]
fn yes_restores_the_latest_without_asking() {
    assert_eq!(restored_index(&pick(true, false, "")), Some(0));
}

#[test]
fn without_a_terminal_and_without_yes_the_choice_is_refused_naming_yes() {
    match pick(false, false, "y\n") {
        Choice::Refuse { code, message } => {
            assert_eq!(code, EXIT_DENIED);
            assert!(message.contains("--yes"), "{message}");
        }
        Choice::Restore(_) => panic!("must fail closed without a terminal"),
    }
}

#[test]
fn a_terminal_y_restores_the_latest() {
    for typed in ["y\n", "YES\n"] {
        assert_eq!(
            restored_index(&pick(false, true, typed)),
            Some(0),
            "{typed:?}"
        );
    }
}

#[test]
fn a_terminal_defaults_to_no() {
    for typed in ["\n", "n\n", "", "what\n"] {
        assert_eq!(restored_index(&pick(false, true, typed)), None, "{typed:?}");
    }
}

#[test]
fn list_lets_the_operator_pick_the_older_command() {
    assert_eq!(restored_index(&pick(false, true, "list\n2\n")), Some(1));
}

#[test]
fn list_cancels_on_an_out_of_range_or_blank_pick() {
    for typed in ["list\n3\n", "list\n0\n", "list\n\n", "list\nx\n", "list\n"] {
        assert_eq!(restored_index(&pick(false, true, typed)), None, "{typed:?}");
    }
}

#[test]
fn summary_names_restored_and_skipped_snapshots_with_their_rollback_line() {
    let ledger = SnapshotLedger::from_entries(&[entry(
        "danger",
        Some("/r"),
        vec![snap("git", "G", Some("/r")), snap("docker", "D1", None)],
    )]);
    let here = location("/r", Some("/r"));
    let found = candidates(&ledger, &here);

    let text = summary(&found[0], &here);

    assert!(text.contains("danger"), "{text}");
    assert!(text.contains("git G"), "{text}");
    assert!(text.contains("aegis rollback D1"), "{text}");
    assert!(!text.contains("Already restored"), "{text}");
}

#[test]
fn summary_says_already_restored_when_every_bound_snapshot_was_restored() {
    let ledger = SnapshotLedger::from_entries(&[
        entry("danger", Some("/r"), vec![snap("git", "G", Some("/r"))]),
        AuditEntry::new(
            "aegis rollback G",
            RiskLevel::Safe,
            Vec::new(),
            Decision::Approved,
            vec![snap("git", "G", None)],
            None,
            None,
        ),
    ]);
    let here = location("/r", Some("/r"));
    let found = candidates(&ledger, &here);

    assert!(summary(&found[0], &here).contains("Already restored at "));
}

#[test]
fn no_candidate_message_names_the_location_and_the_alternatives() {
    let message = no_candidate_message(&location("/r", Some("/r")));

    assert!(message.contains("/r"), "{message}");
    assert!(
        message.contains("aegis rollback <snapshot_id>"),
        "{message}"
    );
    assert!(message.contains("aegis audit"), "{message}");
}
