use std::io::Write;

use crossterm::{
    queue,
    style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor},
};

use aegis_explanation::CommandExplanation;
use aegis_types::{Assessment, RiskLevel};

use super::shared::{
    block_reason_text, confirmation_reason_text, pattern_source_label, print_analysis_reasons,
    print_command_line,
};

pub(super) fn render_block<W: Write>(
    assessment: &Assessment,
    explanation: &CommandExplanation,
    out: &mut W,
) {
    let _ = queue!(
        out,
        SetForegroundColor(Color::Red),
        SetAttribute(Attribute::Bold),
        Print("\n  AEGIS BLOCKED THIS COMMAND\n"),
        ResetColor,
    );

    print_command_line(assessment, out);

    let _ = queue!(
        out,
        Print(format!("  Reason: {}\n", block_reason_text(explanation))),
        Print("  Hint: review the matched patterns below.\n"),
        Print("  Hint: rerun with --output json for machine-readable policy details.\n"),
    );

    if !assessment.matched.is_empty() {
        let _ = queue!(out, Print("\n  Matched patterns:\n"));
        for m in &assessment.matched {
            let source_label = pattern_source_label(m.pattern.source);
            let _ = queue!(
                out,
                SetForegroundColor(Color::Red),
                Print(format!(
                    "    [{}] {:?} | {} ({})\n",
                    m.pattern.id, m.pattern.category, m.pattern.description, source_label
                )),
                ResetColor,
            );
            if let Some(justification) = &m.pattern.justification {
                let _ = queue!(
                    out,
                    SetForegroundColor(Color::DarkGrey),
                    Print(format!("         {}\n", justification)),
                    ResetColor,
                );
            }
        }
    }

    let _ = out.flush();
}

pub(super) fn render_noninteractive_denial<W: Write>(
    assessment: &Assessment,
    explanation: &CommandExplanation,
    out: &mut W,
) {
    let risk_label = match assessment.risk {
        RiskLevel::Danger => "dangerous",
        RiskLevel::Warn => "suspicious",
        _ => "flagged",
    };

    let _ = queue!(
        out,
        SetForegroundColor(Color::Yellow),
        SetAttribute(Attribute::Bold),
        Print(format!(
            "\n  AEGIS: non-interactive mode — {risk_label} command denied\n"
        )),
        ResetColor,
    );

    print_command_line(assessment, out);

    let _ = queue!(
        out,
        Print(format!(
            "  Reason: {}\n",
            confirmation_reason_text(explanation)
        )),
        Print(
            "  Hint: rerun interactively to approve this occurrence once, without persisting a rule.\n"
        ),
        Print("  Hint: add the command to the allowlist for CI use.\n"),
        Print("  Hint: rerun with --output json for machine-readable policy details.\n"),
    );

    let _ = out.flush();
}

pub(super) fn render_noninteractive_analysis_denial<W: Write>(
    assessment: &Assessment,
    explanation: &CommandExplanation,
    out: &mut W,
) {
    let _ = queue!(
        out,
        SetForegroundColor(Color::Yellow),
        SetAttribute(Attribute::Bold),
        Print("\n  AEGIS: non-interactive mode — one-time approval required\n"),
        ResetColor,
    );

    print_command_line(assessment, out);
    print_analysis_reasons(assessment, explanation, out);

    let _ = queue!(
        out,
        Print("  Hint: rerun interactively to make an interactive one-time decision.\n"),
        Print("  Hint: this decision cannot be persisted or reused.\n"),
        Print("  Hint: rerun with --output json for machine-readable policy details.\n"),
    );

    let _ = out.flush();
}

pub(crate) fn render_policy_block<W: Write>(
    assessment: &Assessment,
    explanation: &CommandExplanation,
    out: &mut W,
) {
    let _ = queue!(
        out,
        SetForegroundColor(Color::Yellow),
        SetAttribute(Attribute::Bold),
        Print("\n  AEGIS POLICY BLOCKED THIS COMMAND\n\n"),
        ResetColor,
    );

    print_command_line(assessment, out);

    let _ = queue!(
        out,
        Print(format!("  Reason: {}\n", block_reason_text(explanation))),
        Print("  Hint: inspect the allowlist or run aegis config validate.\n"),
        Print("  Hint: rerun with --output json for machine-readable policy details.\n"),
    );
    let _ = out.flush();
}
