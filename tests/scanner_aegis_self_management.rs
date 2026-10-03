//! Issue #334 — the scanner, not the agent hook, decides on Aegis' own
//! state-changing commands.
//!
//! The hook only denies a command whose first word is literally `aegis`, so
//! `echo hi && aegis off`, `/usr/bin/aegis off`, `env aegis off` and
//! `command aegis off` all reached the scanner and were auto-approved as Safe.
//! These tests pin the scanner classification for every spelling.

use aegis_scanner::assess;
use aegis_types::RiskLevel;

fn risk_of(command: &str) -> RiskLevel {
    assess(command)
        .unwrap_or_else(|e| panic!("assess({command:?}) failed: {e}"))
        .risk
}

fn matched_ids(command: &str) -> Vec<String> {
    assess(command)
        .unwrap_or_else(|e| panic!("assess({command:?}) failed: {e}"))
        .matched
        .iter()
        .map(|m| m.pattern.id.to_string())
        .collect()
}

#[test]
fn disabling_aegis_is_danger_in_every_spelling() {
    for command in [
        "aegis off",
        "echo hi && aegis off",
        "/usr/bin/aegis off",
        "env aegis off",
        "command aegis off",
        "sudo aegis off",
        "echo hi; /usr/local/bin/aegis off",
    ] {
        assert_eq!(
            risk_of(command),
            RiskLevel::Danger,
            "{command:?} must reach Danger"
        );
        assert!(
            matched_ids(command).iter().any(|id| id == "AEG-001"),
            "{command:?} must match AEG-001, matched {:?}",
            matched_ids(command)
        );
    }
}

#[test]
fn uninstalling_aegis_is_danger_in_every_spelling() {
    for command in [
        "aegis uninstall",
        "aegis uninstall --purge-data",
        "env aegis uninstall",
        "sudo /usr/local/bin/aegis uninstall",
        "echo hi && aegis uninstall",
    ] {
        assert_eq!(risk_of(command), RiskLevel::Danger, "{command}");
        assert!(matched_ids(command).iter().any(|id| id == "AEG-007"));
    }
}

/// Aegis' own global options before the subcommand used to hide it from
/// every `AEG-*` prefix rule, so `aegis --quiet uninstall --purge-data` was
/// auto-approved as Safe.
#[test]
fn aegis_global_options_do_not_hide_a_self_management_subcommand() {
    for (command, id) in [
        ("aegis --quiet uninstall --purge-data", "AEG-007"),
        ("aegis -v uninstall", "AEG-007"),
        ("aegis --verbose off", "AEG-001"),
        ("aegis --output text off", "AEG-001"),
        ("aegis --output=json uninstall", "AEG-007"),
        ("aegis --verbosity quiet off", "AEG-001"),
        ("aegis --verbosity=quiet off", "AEG-001"),
        ("aegis -c true off", "AEG-001"),
        ("aegis -ctrue off", "AEG-001"),
        ("aegis -vc true uninstall", "AEG-007"),
        ("aegis --command=true --quiet uninstall", "AEG-007"),
        (
            "echo hi && sudo /usr/local/bin/aegis --quiet off",
            "AEG-001",
        ),
    ] {
        assert_eq!(
            risk_of(command),
            RiskLevel::Danger,
            "{command:?} must reach Danger"
        );
        assert!(
            matched_ids(command).iter().any(|matched| matched == id),
            "{command:?} must match {id}, matched {:?}",
            matched_ids(command)
        );
    }
}

#[test]
fn aegis_global_options_before_a_read_only_subcommand_stay_safe() {
    for command in ["aegis --quiet status", "aegis --output json audit"] {
        assert_eq!(risk_of(command), RiskLevel::Safe, "{command:?}");
    }
}

#[test]
fn rollback_and_prune_are_danger() {
    assert_eq!(risk_of("aegis rollback snap-123"), RiskLevel::Danger);
    assert!(
        matched_ids("aegis rollback snap-123")
            .iter()
            .any(|id| id == "AEG-002")
    );

    assert_eq!(risk_of("aegis snapshot prune --yes"), RiskLevel::Danger);
    assert!(
        matched_ids("aegis snapshot prune --yes")
            .iter()
            .any(|id| id == "AEG-003")
    );
}

#[test]
fn reconfiguring_aegis_is_warn() {
    for (command, id) in [
        ("aegis config init", "AEG-004"),
        ("aegis install-hooks --all", "AEG-005"),
        ("aegis install --all", "AEG-005"),
        ("aegis setup-shell --remove", "AEG-006"),
    ] {
        assert_eq!(
            risk_of(command),
            RiskLevel::Warn,
            "{command:?} must reach Warn"
        );
        assert!(
            matched_ids(command).iter().any(|matched| matched == id),
            "{command:?} must match {id}, matched {:?}",
            matched_ids(command)
        );
    }
}

#[test]
fn read_only_and_tightening_aegis_commands_stay_safe() {
    for command in [
        "aegis on",
        "aegis status",
        "aegis --version",
        "aegis audit --last 5",
        "aegis snapshot list",
        "aegis snapshot prune --dry-run",
        "aegis snapshot prune",
        "aegis config show",
        "aegis config validate",
    ] {
        assert_eq!(
            risk_of(command),
            RiskLevel::Safe,
            "{command:?} must stay Safe"
        );
    }
}

#[test]
fn unrelated_programs_named_like_aegis_stay_safe() {
    for command in ["aegisctl off", "echo aegis off", "grep -r aegis off src"] {
        assert_eq!(
            risk_of(command),
            RiskLevel::Safe,
            "{command:?} must stay Safe"
        );
    }
}
