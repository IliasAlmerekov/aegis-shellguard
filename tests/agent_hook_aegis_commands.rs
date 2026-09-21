//! Integration tests for how the agent hook treats commands that invoke
//! `aegis` itself (#333).

mod support;

use serde_json::Value;
use tempfile::TempDir;

use support::agent_hooks::run_claude_code_hook;

#[test]
fn claude_code_hook_wraps_read_only_aegis_and_denies_self_management() {
    let home = TempDir::new().unwrap();

    let help = run_claude_code_hook(home.path(), "aegis --help");
    assert!(help.status.success());
    let json: Value = serde_json::from_slice(&help.stdout).unwrap();
    assert_eq!(json["hookSpecificOutput"]["permissionDecision"], "allow");
    assert_eq!(
        json["hookSpecificOutput"]["updatedInput"]["command"], "aegis --command 'aegis --help'",
        "read-only aegis commands must run through the wrapper"
    );

    // #379: read-only commands with a redirect, a filter pipe, or a chain.
    for command in [
        "aegis --help 2>&1 | head -40",
        "aegis --version && aegis status",
    ] {
        let output = run_claude_code_hook(home.path(), command);
        assert!(output.status.success());
        let json: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(json["hookSpecificOutput"]["permissionDecision"], "allow");
        assert_eq!(
            json["hookSpecificOutput"]["updatedInput"]["command"],
            format!("aegis --command '{command}'"),
            "{command:?} must run through the wrapper"
        );
    }

    let off = run_claude_code_hook(home.path(), "aegis off");
    assert!(off.status.success());
    let json: Value = serde_json::from_slice(&off.stdout).unwrap();
    assert_eq!(json["hookSpecificOutput"]["permissionDecision"], "deny");
    assert!(
        json["reason"]
            .as_str()
            .unwrap()
            .contains("reserved for the human operator"),
        "aegis off must stay denied with a self-management reason: {json}"
    );
}
