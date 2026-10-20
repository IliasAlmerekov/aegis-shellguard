//! Rule C (issue #384/#430, GHSA-xj54): an unclaimed stage that used ANSI-C
//! quoting (`$'...'`) degrades rather than trusting the operands read from
//! it, whether or not the piece decodes exactly. The router reads the
//! decoded words with the quote edges bash uses, so an escaped quote inside
//! a piece no longer hides later segments. Split out of
//! `unclaimed_interpreter_net.rs` to keep that file under this project's
//! line budget. `use super::*` reaches the same `router` test imports
//! (`RoutedTarget`, `route`, ...) its sibling files use.

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
fn ansi_c_quoting_on_read_only_programs_is_not_routed() {
    for command in [r#"echo $'x'"#, r#"grep $'\t' file"#] {
        assert_eq!(route(command, &[]), Vec::new(), "{command}");
    }
    assert_eq!(route(r#"printf $'a\n'"#, &[]), vec![unresolved_dynamic()]);
    assert_eq!(route(r#"IFS=$'\n'"#, &[]), vec![unresolved_dynamic()]);
}

#[test]
fn escaped_quote_in_ansi_c_word_keeps_later_segments_routed() {
    // `\'` inside `$'...'` does not end the piece, so the `;` after it
    // separates segments and the interpreter launch after it is routed.
    assert_eq!(
        route(r"echo $'\'' ; python3 ./x.py ; echo $'\''", &[]),
        route("python3 ./x.py", &[])
    );
}

#[test]
fn exactly_decodable_ansi_c_quoting_still_degrades_under_rule_c() {
    for command in [r#"sed $'s/\t/ /' file"#, r#"strace -o $'x' ./pyx"#] {
        assert_eq!(route(command, &[]), vec![unresolved_dynamic()], "{command}");
    }
}

#[test]
fn inexact_ansi_c_quoting_with_an_operand_degrades() {
    for command in [r#"strace -o $'\'$(echo x ./pyx"#, r#"sed $'\xff' file"#] {
        assert_eq!(route(command, &[]), vec![unresolved_dynamic()], "{command}");
    }
}
