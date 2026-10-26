//! `aegis rollback --last` (#499, ADR-053): pick the latest command whose
//! Snapshots are bound to the current location, confirm, restore each one
//! through the same liveness preflight as `aegis rollback <id>`.

use std::io::{BufRead, IsTerminal};
use std::path::{Path, PathBuf};

use aegis::error::AegisError;
use aegis_audit::{AuditLogger, CommandSnapshot, RecordedCommand, SnapshotLedger};
use aegis_config::AegisConfig;
use aegis_snapshot::{SnapshotRegistry, SnapshotRegistryConfig, SqlitePlugin, git_toplevel};

use crate::rollback::{Preflight, RollbackTarget, restore_checked};
use crate::{EXIT_DENIED, EXIT_INTERNAL};

type Result<T> = std::result::Result<T, AegisError>;

/// How many candidates "list" and the non-interactive listing show.
const LIST_LIMIT: usize = 10;

/// The directory `--last` runs in, and the git working tree around it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Location {
    cwd: PathBuf,
    repo_top: Option<PathBuf>,
}

/// A command with at least one live Snapshot bound to the [`Location`].
#[derive(Debug)]
pub(crate) struct Candidate<'a> {
    command: &'a RecordedCommand,
    bound: Vec<&'a CommandSnapshot>,
    unbound: Vec<&'a CommandSnapshot>,
}

/// What `--last` did.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LastOutcome {
    /// Nothing was restored: no candidate, or the operator declined.
    Refused { code: i32, message: String },
    /// A chosen command's bound Snapshots were attempted.
    Attempted(LastReport),
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct LastReport {
    pub(crate) restored: Vec<RollbackTarget>,
    /// Snapshot, exit code, reason.
    pub(crate) failed: Vec<(RollbackTarget, i32, String)>,
    /// Restored, but the Rollback audit entry was not written: Snapshot, reason.
    pub(crate) unaudited: Vec<(RollbackTarget, String)>,
    /// Unbound Snapshots of the chosen command; never restored.
    pub(crate) skipped: Vec<RollbackTarget>,
}

enum Choice {
    Restore(usize),
    Refuse { code: i32, message: String },
}

/// Canonical form of a recorded path. A path that no longer exists keeps its
/// recorded spelling, so a Snapshot of a since-deleted directory still binds
/// by the same string it was recorded with.
fn canon(path: &str) -> PathBuf {
    Path::new(path)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(path))
}

/// Whether `snapshot`, taken by a command that ran in `command_cwd`, is bound
/// to `location` (ADR-053).
fn is_bound(location: &Location, command_cwd: &str, snapshot: &CommandSnapshot) -> bool {
    match snapshot.plugin.as_str() {
        "git" => match (&location.repo_top, &snapshot.git_target) {
            (Some(top), Some(target)) => canon(target) == *top,
            (Some(_), None) => false,
            (None, _) => canon(command_cwd).starts_with(&location.cwd),
        },
        "sqlite" => SqlitePlugin::database_path(&snapshot.snapshot_id).is_some_and(|db| {
            let base = location.repo_top.as_ref().unwrap_or(&location.cwd);
            canon(&db.to_string_lossy()).starts_with(base)
        }),
        // Docker scoping ignores cwd (ADR-039) and database targets have no directory.
        _ => false,
    }
}

/// Candidates for `location`, latest first. An entry without a recorded `cwd`
/// is never a candidate.
fn candidates<'a>(ledger: &'a SnapshotLedger, location: &Location) -> Vec<Candidate<'a>> {
    ledger
        .recent_commands()
        .iter()
        .filter_map(|command| {
            let cwd = command.cwd.as_deref()?;
            let (bound, unbound): (Vec<_>, Vec<_>) = command
                .snapshots
                .iter()
                .partition(|snapshot| is_bound(location, cwd, snapshot));
            (!bound.is_empty()).then_some(Candidate {
                command,
                bound,
                unbound,
            })
        })
        .collect()
}

fn target_of(snapshot: &CommandSnapshot) -> RollbackTarget {
    RollbackTarget {
        plugin: snapshot.plugin.clone(),
        snapshot_id: snapshot.snapshot_id.clone(),
    }
}

fn describe(candidate: &Candidate<'_>) -> String {
    let command = candidate.command;
    let source = command
        .source
        .as_deref()
        .map(|source| format!(", source {source}"))
        .unwrap_or_default();
    format!(
        "{} ({:?}{source}): {}",
        command.recorded_at, command.decision, command.command
    )
}

fn summary(candidate: &Candidate<'_>, location: &Location) -> String {
    let mut text = format!(
        "Latest command with Snapshots for {}:\n  {}\n",
        location_label(location),
        describe(candidate)
    );
    text.push_str("Restores:\n");
    for snapshot in &candidate.bound {
        text.push_str(&format!("  {} {}\n", snapshot.plugin, snapshot.snapshot_id));
    }
    if !candidate.unbound.is_empty() {
        text.push_str("Skips (not bound to this location):\n");
        for snapshot in &candidate.unbound {
            text.push_str(&format!(
                "  {} {}  (aegis rollback {})\n",
                snapshot.plugin, snapshot.snapshot_id, snapshot.snapshot_id
            ));
        }
    }
    let restored = candidate
        .bound
        .iter()
        .filter_map(|snapshot| snapshot.restored_at)
        .max();
    if let Some(at) = restored {
        text.push_str(&format!("Already restored at {at}.\n"));
    }
    text
}

fn listing(candidates: &[Candidate<'_>]) -> String {
    let mut text = String::from("Recent commands with Snapshots for this location:\n");
    for (index, candidate) in candidates.iter().take(LIST_LIMIT).enumerate() {
        text.push_str(&format!("  {}. {}\n", index + 1, describe(candidate)));
    }
    text
}

fn location_label(location: &Location) -> String {
    match &location.repo_top {
        Some(top) => format!("the git repository {}", top.display()),
        None => format!("the directory {}", location.cwd.display()),
    }
}

fn no_candidate_message(location: &Location) -> String {
    format!(
        "no command with a Snapshot is recorded for {}. Use `aegis rollback <snapshot_id>` with an id from `aegis audit`",
        location_label(location)
    )
}

/// Pick which candidate to restore. Only a question needs a terminal.
fn choose(
    candidates: &[Candidate<'_>],
    location: &Location,
    yes: bool,
    is_tty: bool,
    input: &mut impl BufRead,
) -> Choice {
    eprint!("{}", summary(&candidates[0], location));
    if yes {
        return Choice::Restore(0);
    }
    if !is_tty {
        eprint!("{}", listing(candidates));
        return Choice::Refuse {
            code: EXIT_DENIED,
            message: "there is no terminal to confirm; pass --yes to restore the latest command, or use `aegis rollback <snapshot_id>`".to_string(),
        };
    }
    let cancelled = || Choice::Refuse {
        code: EXIT_DENIED,
        message: "rollback cancelled".to_string(),
    };
    match ask(input, "Restore these Snapshots? [y/N/list] ").as_str() {
        "y" | "yes" => Choice::Restore(0),
        "l" | "list" => {
            eprint!("{}", listing(candidates));
            let shown = candidates.len().min(LIST_LIMIT);
            match ask(input, "Pick a number (anything else cancels): ").parse::<usize>() {
                Ok(n) if (1..=shown).contains(&n) => {
                    eprint!("{}", summary(&candidates[n - 1], location));
                    Choice::Restore(n - 1)
                }
                _ => cancelled(),
            }
        }
        _ => cancelled(),
    }
}

fn ask(input: &mut impl BufRead, prompt: &str) -> String {
    eprint!("{prompt}");
    let mut line = String::new();
    // A read error counts as "no", like EOF.
    let _ = input.read_line(&mut line);
    line.trim().to_ascii_lowercase()
}

pub(crate) async fn execute(yes: bool) -> Result<LastOutcome> {
    let config = AegisConfig::load()?;
    let audit_logger = AuditLogger::from_audit_config(&config.audit);
    let ledger = SnapshotLedger::load(&audit_logger)?;
    let cwd = std::env::current_dir()
        .and_then(|dir| dir.canonicalize())
        .map_err(|error| AegisError::Internal {
            detail: format!("cannot resolve the current directory: {error}"),
        })?;
    let location = Location {
        repo_top: git_toplevel(&cwd).await,
        cwd,
    };

    let candidates = candidates(&ledger, &location);
    if candidates.is_empty() {
        return Ok(LastOutcome::Refused {
            code: EXIT_INTERNAL,
            message: no_candidate_message(&location),
        });
    }

    let stdin = std::io::stdin();
    let is_tty = stdin.is_terminal();
    let mut input = stdin.lock();
    let chosen = match choose(&candidates, &location, yes, is_tty, &mut input) {
        Choice::Restore(index) => &candidates[index],
        Choice::Refuse { code, message } => return Ok(LastOutcome::Refused { code, message }),
    };

    let registry = SnapshotRegistry::from_runtime_config(
        &SnapshotRegistryConfig::for_rollback_from_config(&config)?,
    );
    let mut report = LastReport {
        skipped: chosen.unbound.iter().map(|s| target_of(s)).collect(),
        ..LastReport::default()
    };
    for snapshot in &chosen.bound {
        let target = target_of(snapshot);
        match restore_one(&registry, &audit_logger, &target, yes, is_tty, &mut input).await {
            Ok(None) => report.restored.push(target),
            Ok(Some(reason)) => report.unaudited.push((target, reason)),
            Err((code, reason)) => report.failed.push((target, code, reason)),
        }
    }
    Ok(LastOutcome::Attempted(report))
}

/// Run one Snapshot through the `aegis rollback <id>` preflight and restore.
/// A failure is returned, not raised, so the other providers still run.
/// `Ok(Some(reason))` means restored with no audit entry.
async fn restore_one(
    registry: &SnapshotRegistry,
    logger: &AuditLogger,
    target: &RollbackTarget,
    yes: bool,
    is_tty: bool,
    input: &mut impl BufRead,
) -> std::result::Result<Option<String>, (i32, String)> {
    match restore_checked(registry, logger, target, yes, is_tty, input).await {
        Ok(Preflight::Restore) => Ok(None),
        Ok(Preflight::RestoredUnaudited { reason }) => Ok(Some(reason)),
        Ok(Preflight::Refused { code, message }) => Err((code, message)),
        Err(error) => Err((EXIT_INTERNAL, error.to_string())),
    }
}

#[cfg(test)]
mod tests;
