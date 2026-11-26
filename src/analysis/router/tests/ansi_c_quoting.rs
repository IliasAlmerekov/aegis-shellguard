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
