use std::fs::OpenOptions;
use std::io;

use aegis_explanation::CommandExplanation;
use aegis_types::{Assessment, RiskLevel, SnapshotRecord};

use super::block_screen::{render_block, render_policy_block};
use super::stdout_renderer::{PromptDecision, show_confirmation_with_decision};

pub fn tty_unavailable_decision(assessment: &Assessment) -> bool {
    matches!(assessment.risk, RiskLevel::Safe)
}

pub(crate) fn tty_unavailable_prompt_decision(
    assessment: &Assessment,
    explanation: &CommandExplanation,
) -> PromptDecision {
    if explanation.policy.requires_confirmation {
        PromptDecision::Deny
    } else if tty_unavailable_decision(assessment) {
        PromptDecision::Approve
    } else {
        PromptDecision::Deny
    }
}

#[expect(
    clippy::print_stderr,
    reason = "rendering module writes diagnostics to stderr"
)]
pub fn show_confirmation_via_tty_with_decision(
    assessment: &Assessment,
    explanation: &CommandExplanation,
    snapshots: &[SnapshotRecord],
) -> PromptDecision {
    if let Some(disclosure) = &explanation.context.no_rollback_disclosure {
        eprintln!("{disclosure}");
    }
    if std::env::var_os("AEGIS_FORCE_NO_TTY")
        .map(|value| value == "1")
        .unwrap_or(false)
    {
        return tty_unavailable_prompt_decision(assessment, explanation);
    }

    let tty = match OpenOptions::new().read(true).write(true).open("/dev/tty") {
        Ok(f) => f,
        Err(_) => {
            return tty_unavailable_prompt_decision(assessment, explanation);
        }
    };
    let tty_write = match tty.try_clone() {
        Ok(f) => f,
        Err(_) => {
            return tty_unavailable_prompt_decision(assessment, explanation);
        }
    };

    show_confirmation_with_decision(
        assessment,
        explanation,
        snapshots,
        true,
        &mut io::BufReader::new(tty),
        &mut { tty_write },
    )
}

pub fn show_policy_block_via_tty(assessment: &Assessment, explanation: &CommandExplanation) {
    if let Ok(mut tty) = OpenOptions::new().write(true).open("/dev/tty") {
        render_policy_block(assessment, explanation, &mut tty);
    }
}

pub fn show_block_via_tty(assessment: &Assessment, explanation: &CommandExplanation) {
    if let Ok(mut tty) = OpenOptions::new().write(true).open("/dev/tty") {
        render_block(assessment, explanation, &mut tty);
    }
}
