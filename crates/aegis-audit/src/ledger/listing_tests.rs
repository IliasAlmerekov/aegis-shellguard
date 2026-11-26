use super::*;
use crate::logger::AuditSnapshot;

fn snapshot_entry(
    command: &str,
    decision: Decision,
    plugin: &str,
    snapshot_id: &str,
) -> AuditEntry {
    AuditEntry::new(
        command,
        RiskLevel::Danger,
        Vec::new(),
        decision,
        vec![AuditSnapshot {
            plugin: plugin.to_string(),
            snapshot_id: snapshot_id.to_string(),
            git_target: None,
        }],
        None,
        None,
    )
}

fn entry_with_snapshot(command: &str, plugin: &str, snapshot_id: &str) -> AuditEntry {
    snapshot_entry(command, Decision::Approved, plugin, snapshot_id)
}

fn pruned(plugin: &str, snapshot_id: &str) -> AuditEntry {
    snapshot_entry(
        &format!("aegis prune {snapshot_id}"),
        Decision::Pruned,
        plugin,
        snapshot_id,
    )
}

fn format_snapshot_listing(entries: &[AuditEntry]) -> String {
    SnapshotLedger::from_entries(entries).format_listing()
}

#[test]
fn lists_id_and_provider_for_each_snapshot() {
    let entries = vec![
        entry_with_snapshot("rm -rf src", "git", "snap-git-001"),
        entry_with_snapshot("docker rm db", "docker", "snap-docker-001"),
    ];

    let output = format_snapshot_listing(&entries);

    assert!(output.contains("snap-git-001"), "missing git id: {output}");
    assert!(output.contains("git"), "missing git provider: {output}");
    assert!(
        output.contains("snap-docker-001"),
        "missing docker id: {output}"
    );
    assert!(
        output.contains("docker"),
        "missing docker provider: {output}"
    );
}

#[test]
fn deduplicates_repeated_snapshot_id() {
    let entries = vec![
        entry_with_snapshot("rm -rf src", "git", "snap-001"),
        entry_with_snapshot("aegis rollback snap-001", "git", "snap-001"),
    ];

    let output = format_snapshot_listing(&entries);

    assert_eq!(
        output.matches("snap-001").count(),
        1,
        "snap-001 must be listed exactly once: {output}"
    );
}

#[test]
fn same_id_under_two_providers_lists_one_row_per_pair_newest_first() {
    let entries = vec![
        entry_with_snapshot("first", "git", "dup-id"),
        entry_with_snapshot("second", "docker", "dup-id"),
    ];

    let output = format_snapshot_listing(&entries);

    assert_eq!(output.matches("dup-id").count(), 2, "{output}");
    let docker = output.find("provider: docker").expect("docker row");
    let git = output.find("provider: git").expect("git row");
    assert!(docker < git, "newest first: {output}");
}

#[test]
fn empty_log_yields_friendly_message() {
    let output = format_snapshot_listing(&[]);

    assert!(!output.is_empty());
    assert!(
        output.to_lowercase().contains("no snapshot"),
        "expected a friendly empty message: {output}"
    );
    assert_eq!(output, "No snapshots recorded.\n");
}

#[test]
fn preserves_tab_in_git_style_id() {
    let id = "/home/user/project\tabc123def456";
    let entries = vec![entry_with_snapshot("rm -rf .", "git", id)];

    let output = format_snapshot_listing(&entries);

    assert!(
        output.contains(id),
        "tab-bearing id not preserved: {output:?}"
    );
}

#[test]
fn all_pruned_names_the_hidden_count() {
    let output =
        format_snapshot_listing(&[entry_with_snapshot("rm a", "git", "A"), pruned("git", "A")]);

    assert_eq!(
        output,
        "No snapshots recorded. (1 pruned snapshot(s) hidden.)\n"
    );
}

#[test]
fn live_listing_has_a_pruned_footer_only_when_something_was_pruned() {
    let clean = format_snapshot_listing(&[entry_with_snapshot("rm a", "git", "A")]);
    let partly_pruned = format_snapshot_listing(&[
        entry_with_snapshot("rm a", "git", "A"),
        entry_with_snapshot("rm b", "git", "B"),
        pruned("git", "B"),
    ]);

    assert!(
        clean.starts_with("Recorded snapshots (newest first):\n"),
        "{clean}"
    );
    assert!(!clean.contains("hidden"), "{clean}");
    assert!(
        partly_pruned.ends_with("\n  (1 pruned snapshot(s) hidden.)\n"),
        "{partly_pruned}"
    );
}

#[test]
fn bound_commands_keeps_only_commands_with_a_bound_snapshot() {
    let ledger = SnapshotLedger::from_entries(&[
        entry_with_snapshot("unbound only", "docker", "D").with_cwd(Some("/r".to_string())),
        entry_with_snapshot("no cwd", "git", "N"),
        snapshot_entry("mixed", Decision::Approved, "git", "G").with_cwd(Some("/r".to_string())),
    ]);

    let found = ledger.bound_commands(|cwd, snapshot| cwd == "/r" && snapshot.plugin == "git");

    let names: Vec<&str> = found.iter().map(|c| c.command.command.as_str()).collect();
    assert_eq!(names, vec!["mixed"]);
    assert_eq!(found[0].bound.len(), 1);
    assert!(found[0].unbound.is_empty());
}
