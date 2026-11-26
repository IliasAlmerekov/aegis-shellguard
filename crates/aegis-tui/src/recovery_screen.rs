use std::fs::OpenOptions;
use std::io::{self, BufRead, Write};

use aegis_types::RecoveryDegradation;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryPromptDecision {
    RunOnceWithoutRecovery,
    Deny,
}

pub fn show_recovery_override_decision(degradation: RecoveryDegradation) -> RecoveryPromptDecision {
    use std::io::IsTerminal;
    show_recovery_override_with_input(
        degradation,
        io::stdin().is_terminal(),
        &mut io::stdin().lock(),
        &mut io::stderr(),
    )
}

fn degradation_banner(degradation: RecoveryDegradation) -> &'static str {
    match degradation {
        RecoveryDegradation::PartialSnapshotCoverage => {
            "\n  AEGIS REQUIRED RECOVERY IS INCOMPLETE\n\n  This run requires Recovery. At least one applicable snapshot provider failed.\n  The Snapshots that were created do not cover everything this run requires."
        }
        RecoveryDegradation::UnavailableCwd => {
            "\n  AEGIS REQUIRED RECOVERY IS UNAVAILABLE\n\n  The working directory is unavailable, so no Snapshot could be created.\n  Proceeding would run without Recovery."
        }
        RecoveryDegradation::UnresolvedGitTarget => {
            "\n  AEGIS REQUIRED RECOVERY IS UNAVAILABLE\n\n  A git command's working tree could not be resolved (a variable, glob, GIT_DIR or\n  missing path), so no git Snapshot covers what it changes. Aegis does not\n  substitute a Snapshot of the current directory. Proceeding would run without recovery."
        }
        RecoveryDegradation::SnapshotArtifactUnavailable => {
            "\n  AEGIS REQUIRED RECOVERY IS UNAVAILABLE\n\n  A Snapshot was recorded, but its artifact is missing or unreadable. It cannot\n  be restored. Proceeding would run without a restorable Snapshot."
        }
        RecoveryDegradation::SnapshotArtifactInvalid => {
            "\n  AEGIS REQUIRED RECOVERY IS INVALID\n\n  A Snapshot was recorded, but its artifact failed validation. It cannot be\n  trusted for a restore. Proceeding would run without a restorable Snapshot."
        }
        _ => {
            "\n  AEGIS REQUIRED RECOVERY IS UNAVAILABLE\n\n  This run requires Recovery. No required Snapshot was created.\n  Proceeding would run without Recovery."
        }
    }
}

pub fn show_recovery_override_with_input<R: BufRead, W: Write>(
    degradation: RecoveryDegradation,
    is_interactive: bool,
    input: &mut R,
    output: &mut W,
) -> RecoveryPromptDecision {
    if writeln!(output, "{}", degradation_banner(degradation)).is_err() {
        return RecoveryPromptDecision::Deny;
    }
    if !is_interactive {
        return RecoveryPromptDecision::Deny;
    }

    if write!(output, "\n  Type \"yes\" to run once without Recovery: ")
        .and_then(|()| output.flush())
        .is_err()
    {
        return RecoveryPromptDecision::Deny;
    }

    let mut line = String::new();
    if input.read_line(&mut line).is_err() {
        return RecoveryPromptDecision::Deny;
    }

    if line.trim().eq_ignore_ascii_case("yes") {
        RecoveryPromptDecision::RunOnceWithoutRecovery
    } else {
        RecoveryPromptDecision::Deny
    }
}

pub fn show_recovery_override_via_tty(degradation: RecoveryDegradation) -> RecoveryPromptDecision {
    if std::env::var_os("AEGIS_FORCE_NO_TTY").is_some_and(|value| value == "1") {
        return RecoveryPromptDecision::Deny;
    }

    let tty = match OpenOptions::new().read(true).write(true).open("/dev/tty") {
        Ok(file) => file,
        Err(_) => return RecoveryPromptDecision::Deny,
    };
    let mut output = match tty.try_clone() {
        Ok(file) => file,
        Err(_) => return RecoveryPromptDecision::Deny,
    };

    show_recovery_override_with_input(degradation, true, &mut io::BufReader::new(tty), &mut output)
}
