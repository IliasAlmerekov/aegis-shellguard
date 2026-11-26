#[test]
fn test_show_confirmation_and_prompt_decision_accessible() {
    use aegis_tui::{PromptDecision, show_confirmation, show_policy_block};
    let _: Option<fn(_, _, _) -> bool> = Some(show_confirmation);
    let _: Option<fn(_, _)> = Some(show_policy_block);
    let _: PromptDecision = PromptDecision::Approve;
}

#[test]
fn test_show_block_via_tty_accessible() {
    use aegis_tui::show_block_via_tty;
    let _f: fn(_, _) = show_block_via_tty;
}

#[test]
fn test_show_confirmation_via_tty_with_decision_accessible() {
    use aegis_tui::show_confirmation_via_tty_with_decision;
    let _f: fn(_, _, _) -> aegis_tui::PromptDecision = show_confirmation_via_tty_with_decision;
}
