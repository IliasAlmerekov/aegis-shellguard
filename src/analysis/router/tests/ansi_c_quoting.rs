//! Rule C (issue #384/#430, GHSA-xj54): the tokenizer does not understand
//! ANSI-C quoting (`$'...'`), so it can glue a real operand into the garbage
//! token that results. An unclaimed stage with at least one operand whose
//! raw text carries unquoted `$'` degrades rather than trusting whatever the
//! tokenizer made of it. Split out of `unclaimed_interpreter_net.rs` to keep
//! that file under this project's line budget. `use super::*` reaches the
//! same `router` test imports (`RoutedTarget`, `route`, ...) its sibling
//! files use.

use super::*;

#[test]
fn unquoted_ansi_c_quoting_with_an_operand_degrades() {
    for command in [
        r#"strace -o $'\'$(echo x' ./pyx ')'"#,
        r#"taskset -c 0 $'\'$(echo x' ./pyx ')'"#,
        r#"sed $'s/\t/ /' file"#,
    ] {
        assert_eq!(route(command, &[]), vec![unresolved_dynamic()], "{command}");
    }
}

#[test]
fn unquoted_ansi_c_quoting_that_mistokenizes_away_the_operand_still_degrades() {
    // The tokenizer's `$'...'` blindness can eat the operand entirely, not
    // just glue extra garbage onto it: `-o$'\'' ./pyx #'` collapses to a
    // single dash-prefixed token with nothing `operands` (the dash-stripped
    // filter) counts as an operand at all, so the old `operands.clone()
    // .next().is_some()` gate never reached the `$'` check below it. The
    // stage still carries unquoted `$'`, and that alone is reason enough to
    // stop trusting whatever the tokenizer made of the rest of the line.
    assert_eq!(
        route(r#"strace -o$'\'' ./pyx #'"#, &[]),
        vec![unresolved_dynamic()]
    );
}

#[test]
fn ansi_c_quoting_on_a_name_only_program_or_with_no_operand_is_not_routed() {
    for command in [
        r#"printf $'a\n'"#,
        r#"echo $'x'"#,
        r#"grep $'\t' file"#,
        r#"IFS=$'\n'"#,
    ] {
        assert_eq!(route(command, &[]), Vec::new(), "{command}");
    }
}
