use std::fs::OpenOptions;
use std::io::{self, BufRead, Write};

use aegis_types::RecoveryDegradation;

/// The user's one-time response to a Recovery degradation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryPromptDecision {
    /// Run this command once without the required recovery backstop.
    RunOnceWithoutRecovery,
    /// Deny this command.
    Deny,
}

/// Show the Shell Recovery override prompt on stderr and read stdin.
pub fn show_recovery_override_decision(degradation: RecoveryDegradation) -> RecoveryPromptDecision {
    use std::io::IsTerminal;
    show_recovery_override_with_input(
        degradation,
        io::stdin().is_terminal(),
        &mut io::stdin().lock(),
        &mut io::stderr(),
    )
}

/// The banner that names which Recovery degradation the human is deciding on.
///
/// Partial coverage and total failure read differently: "no Snapshot was
/// created" would be a false statement about a partial attempt, and a human
/// who is told it will misjudge what running anyway costs.
fn degradation_banner(degradation: RecoveryDegradation) -> &'static str {
    match degradation {
        RecoveryDegradation::PartialSnapshotCoverage => {
            "\n  AEGIS REQUIRED RECOVERY IS INCOMPLETE\n\n  Aegis could not determine the eventual effect from the assessed command text.\n  At least one applicable snapshot provider failed. The Snapshots that were\n  created do not cover everything the ADR-016 recovery backstop would cover."
        }
        _ => {
            "\n  AEGIS REQUIRED RECOVERY IS UNAVAILABLE\n\n  Aegis could not determine the eventual effect from the assessed command text.\n  No required Snapshot was created. Proceeding would run without the ADR-016 recovery backstop."
        }
    }
}

/// Testable Recovery override prompt using explicit input and output streams.
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

    if write!(output, "\n  Run once without recovery? [r/N]: ")
        .and_then(|()| output.flush())
        .is_err()
    {
        return RecoveryPromptDecision::Deny;
    }

    let mut line = String::new();
    if input.read_line(&mut line).is_err() {
        return RecoveryPromptDecision::Deny;
    }

    match line.trim().to_ascii_lowercase().as_str() {
        "r" | "run" | "run once" => RecoveryPromptDecision::RunOnceWithoutRecovery,
        _ => RecoveryPromptDecision::Deny,
    }
}

/// Show the Watch Recovery override prompt through `/dev/tty`.
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
