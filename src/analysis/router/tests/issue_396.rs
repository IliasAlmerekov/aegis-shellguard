//! Router unit tests for issue #396/#432: a nowdoc body fed to a `Data
//! consumer` (`cat`, `tee`, `jq`) must not be tokenized as argv by the
//! fail-closed unclaimed-interpreter net (`unclaimed.rs`), the same way the
//! scanner no longer scans it as live command text
//! (`crates/aegis-scanner/src/scanner/tests/issue_396.rs`). `use super::*`
//! reaches the same `router` test imports (`RoutedTarget`, `route`,
//! `unresolved_dynamic`, ...) `router::tests`'s other siblings use.

use super::*;

// ── The bug: a jq heredoc body must not be tokenized as argv ───────────────

#[test]
fn jq_nowdoc_body_with_interpreter_looking_json_is_not_routed() {
    let cmd = "jq -c . <<'JSON'\n{\"cmd\": \"python3 ./x\"}\nJSON";
    assert_eq!(route(cmd, &[]), Vec::new());
}

// ── The #432 fix: the owning command is found past the last `;`/`&&`,
// not the stage's first token — `cat` stays a `Data consumer` either way,
// and stays unrouted (`cat` is on `NAME_ONLY_PROGRAMS` regardless of its
// heredoc body, so this pins today's behavior against a future regression
// in that exclusion). ──────────────────────────────────────────────────────

#[test]
fn heredoc_owning_command_after_semicolon_is_not_routed() {
    let cmd = "true; cat > /tmp/aegis-396-x.sh <<'EOF'\npython3 ./x\nEOF";
    assert_eq!(route(cmd, &[]), Vec::new());
}

#[test]
fn heredoc_owning_command_after_and_and_is_not_routed() {
    let cmd = "true && cat > /tmp/aegis-396-x.sh <<'EOF'\npython3 ./x\nEOF";
    assert_eq!(route(cmd, &[]), Vec::new());
}

// ── Guard: `xargs` is not a `Data consumer` — its stdin becomes argv for
// real, so a dangerous-looking body must keep degrading. `aegis-parser`
// treats `xargs` as a launcher word in its own right (the program right
// after its flags is the one it runs), so these two route through that
// existing launcher-prefix mechanism rather than through the unclaimed net
// — either way, the body is never silently auto-approved. ─────────────────

#[test]
fn xargs_nowdoc_body_naming_an_interpreter_is_routed() {
    let cmd = "xargs <<'EOF'\npython3 ./x\nEOF";
    assert_eq!(
        route(cmd, &[]),
        vec![RoutedTarget::ScriptFile {
            language: SourceLanguage::Python,
            path: PathBuf::from("./x"),
        }]
    );
}

#[test]
fn xargs_dash_i_sh_dash_c_nowdoc_body_is_routed() {
    let cmd = "xargs -I{} sh -c {} <<'EOF'\npython3 ./x\nEOF";
    assert_eq!(
        route(cmd, &[]),
        vec![RoutedTarget::Inline {
            language: SourceLanguage::Bash,
            source: "{}".to_owned(),
        }]
    );
}

// ── Guard: an unquoted heredoc still expands at construction time, so its
// body is never masked regardless of consumer ──────────────────────────────

#[test]
fn unquoted_jq_heredoc_body_naming_an_interpreter_is_routed() {
    let cmd = "jq -c . <<JSON\npython3 ./x\nJSON";
    assert_eq!(route(cmd, &[]), vec![unresolved_dynamic()]);
}

// ── Guard: an interpreter target still executes its nowdoc body verbatim,
// unaffected by the `Data consumer` predicate ───────────────────────────────

#[test]
fn bash_nowdoc_body_naming_an_interpreter_is_routed() {
    let cmd = "bash <<'EOF'\npython3 ./x\nEOF";
    assert_eq!(
        route(cmd, &[]),
        vec![RoutedTarget::Inline {
            language: SourceLanguage::Bash,
            source: "python3 ./x".to_owned(),
        }]
    );
}
