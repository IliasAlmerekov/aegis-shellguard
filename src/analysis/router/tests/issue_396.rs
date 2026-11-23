//! `Data consumer` nowdoc masking in the unclaimed-interpreter net (#396, #432).

use super::*;

#[test]
fn jq_nowdoc_body_with_interpreter_looking_json_is_not_routed() {
    let cmd = "jq -c . <<'JSON'\n{\"cmd\": \"python3 ./x\"}\nJSON";
    assert_eq!(route(cmd, &[]), Vec::new());
}

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

#[test]
fn unquoted_jq_heredoc_body_naming_an_interpreter_is_routed() {
    let cmd = "jq -c . <<JSON\npython3 ./x\nJSON";
    assert_eq!(route(cmd, &[]), vec![unresolved_dynamic()]);
}

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
