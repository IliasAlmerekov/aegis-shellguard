//! Blocklist and allowlist matching against quoted spellings of a command.

use std::path::Path;

use time::OffsetDateTime;

use super::{Allowlist, AllowlistContext, Blocklist, LayeredAllowlistRule, LayeredBlocklistRule};
use crate::{AllowlistRule, BlockRule};

fn ctx(command: &str) -> AllowlistContext<'_> {
    AllowlistContext::new(
        command,
        Path::new("/srv/infra"),
        "ci",
        OffsetDateTime::now_utc(),
    )
}

fn blocklist(pattern: &str) -> Blocklist {
    Blocklist::from_layered_rules(&[LayeredBlocklistRule::project(BlockRule {
        pattern: pattern.to_string(),
        cwd: Some("/srv/infra".to_string()),
        user: None,
        expires_at: None,
        reason: "no recursive deletes".to_string(),
    })])
    .unwrap()
}

fn allowlist(pattern: &str) -> Allowlist {
    Allowlist::new(&[LayeredAllowlistRule::project(AllowlistRule {
        pattern: pattern.to_string(),
        cwd: Some("/srv/infra".to_string()),
        user: None,
        expires_at: None,
        reason: "routine".to_string(),
    })])
    .unwrap()
}

#[test]
fn blocklist_entry_blocks_quoted_spellings_of_the_command() {
    let blocklist = blocklist("rm -rf *");
    for command in [
        r"$'rm' -rf x",
        r"$'\162m' -rf x",
        r#""rm" -rf x"#,
        r"r''m -rf x",
        r"\rm -rf x",
        r#"rm "-rf" x"#,
        r#"$"rm" -rf x"#,
    ] {
        assert!(blocklist.is_blocked(&ctx(command)), "{command}");
    }
}

#[test]
fn blocklist_match_on_the_unquoted_form_reports_the_entry() {
    let found = blocklist("rm -rf *").match_reason(&ctx(r#""rm" -rf x"#));
    assert_eq!(found.map(|m| m.pattern), Some("rm -rf *".to_string()));
}

#[test]
fn blocklist_still_ignores_a_different_command_in_quotes() {
    let blocklist = blocklist("rm -rf *");
    for command in [r#"echo "rm -rf x""#, r"echo 'rm' -rf x", "rm -r x"] {
        assert!(!blocklist.is_blocked(&ctx(command)), "{command}");
    }
}

#[test]
fn blocklist_wildcard_still_stops_at_a_separator_in_the_unquoted_form() {
    assert!(!blocklist("echo *").is_blocked(&ctx(r#"echo "x" ; ls"#)));
}

#[test]
fn allowlist_matches_only_the_command_as_written() {
    let allowlist = allowlist("ls *");
    assert!(allowlist.is_allowed(&ctx("ls build")));
    for command in [r"$'ls' build", r#""ls" build"#, r"l''s build"] {
        assert!(!allowlist.is_allowed(&ctx(command)), "{command}");
    }
}
