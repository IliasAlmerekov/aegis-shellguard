//! Bash ANSI-C quoting (`$'...'`) and locale quoting (`$"..."`): the tokenizer
//! hands every consumer the word bash would run, not its quoted spelling.

use crate::*;

fn words(cmd: &str) -> Vec<String> {
    split_tokens(cmd)
}

#[test]
fn octal_escape_decodes_to_the_character() {
    assert_eq!(words(r"$'\162m' -rf x"), vec!["rm", "-rf", "x"]);
}

#[test]
fn hex_escape_decodes_to_the_character() {
    assert_eq!(words(r"$'\x72m' -rf x"), vec!["rm", "-rf", "x"]);
}

#[test]
fn unicode_escapes_decode_to_the_character() {
    assert_eq!(words(r"$'rm'"), vec!["rm"]);
    assert_eq!(words(r"$'\U00000072m'"), vec!["rm"]);
}

#[test]
fn ansi_c_piece_joins_the_surrounding_word() {
    assert_eq!(words("r$'m' -rf x"), vec!["rm", "-rf", "x"]);
    assert_eq!(words("$'r''m'"), vec!["rm"]);
    assert_eq!(words(r#"$'r'"m"'' x"#), vec!["rm", "x"]);
}

#[test]
fn named_escapes_decode_like_bash() {
    assert_eq!(
        words(r#"$'a\nb\tc\\d\'e\"f\?g'"#),
        vec!["a\nb\tc\\d'e\"f?g"]
    );
    assert_eq!(
        words(r"$'\a\b\e\E\f\r\v'"),
        vec!["\x07\x08\x1b\x1b\x0c\r\x0b"]
    );
}

#[test]
fn control_escape_decodes_like_bash() {
    assert_eq!(words(r"$'\ca\cZ\c?'"), vec!["\x01\x1a\x7f"]);
}

#[test]
fn unknown_escape_keeps_the_backslash() {
    assert_eq!(words(r"$'\q'"), vec![r"\q"]);
    assert_eq!(words(r"$'\x'"), vec![r"\x"]);
}

#[test]
fn escaped_quote_does_not_end_the_word() {
    assert_eq!(
        words(r"echo $'\'' ; git push --force ; echo $'\''"),
        vec!["echo", "'", ";", "git", "push", "--force", ";", "echo", "'"]
    );
}

#[test]
fn escaped_bytes_form_one_utf8_character() {
    assert_eq!(words(r"$'\xc3\xa9'"), vec!["é"]);
}

#[test]
fn nul_ends_the_ansi_c_piece_like_bash() {
    assert_eq!(words(r"$'r\0junk'm"), vec!["rm"]);
}

#[test]
fn ansi_c_inside_double_quotes_stays_literal() {
    assert_eq!(words(r#"echo "$'rm'""#), vec!["echo", "$'rm'"]);
}

#[test]
fn escaped_dollar_is_not_ansi_c() {
    assert_eq!(words(r"echo \$'\162'"), vec!["echo", r"$\162"]);
}

#[test]
fn locale_quoting_reads_like_double_quotes() {
    assert_eq!(words(r#"$"rm" -rf x"#), vec!["rm", "-rf", "x"]);
    assert_eq!(
        words(r#"git $"push" --force"#),
        vec!["git", "push", "--force"]
    );
}

#[test]
fn ansi_c_quoting_check_accepts_exact_words() {
    assert!(!ansi_c_quoting_is_inexact(r"$'\162m' -rf x"));
    assert!(!ansi_c_quoting_is_inexact("echo plain"));
}

#[test]
fn ansi_c_quoting_check_flags_unterminated_word() {
    assert!(ansi_c_quoting_is_inexact(r"$'\162m -rf x"));
}

#[test]
fn unterminated_word_keeps_its_raw_spelling() {
    assert_eq!(words(r"$'\162m -rf x"), vec![r"$'\162m -rf x"]);
}

#[test]
fn ansi_c_quoting_check_flags_nul_and_invalid_bytes() {
    assert!(ansi_c_quoting_is_inexact(r"$'r\0m'"));
    assert!(ansi_c_quoting_is_inexact(r"$'\xff'"));
    assert!(ansi_c_quoting_is_inexact(r"$'\U110000'"));
    assert!(ansi_c_quoting_is_inexact(r"$'\ud800'"));
}

#[test]
fn ansi_c_quoting_check_ignores_literal_spellings() {
    assert!(!ansi_c_quoting_is_inexact(r#"echo "$'\xff'" '$'\''\0'"#));
}

#[test]
fn nested_shell_body_in_ansi_c_quotes_is_decoded_once() {
    assert_eq!(
        extract_nested_commands(r"bash -c $'\162m -rf x\necho \\x41'"),
        vec![r"rm -rf x", r"echo x41"]
    );
}

#[test]
fn canonical_form_spells_ansi_c_pieces_as_single_quotes() {
    assert_eq!(canonical_ansi_c_quoting(r"$'\162m' -rf x"), "'rm' -rf x");
    assert_eq!(canonical_ansi_c_quoting(r#"$"rm" x"#), r#""rm" x"#);
}

#[test]
fn canonical_form_spells_an_escaped_quote_so_segments_split_like_bash() {
    let canonical = canonical_ansi_c_quoting(r"echo $'\'' ; git push --force ; echo $'\''");
    assert_eq!(canonical, r"echo ''\''' ; git push --force ; echo ''\'''");
    assert!(has_multiple_top_level_segments(&canonical));
}

#[test]
fn canonical_form_borrows_text_without_ansi_c_quoting() {
    assert!(matches!(
        canonical_ansi_c_quoting("rm -rf x"),
        std::borrow::Cow::Borrowed(_)
    ));
}

#[test]
fn canonical_form_leaves_quoted_and_escaped_spellings_alone() {
    let text = r#"echo '$'\''x' "$'y'" \$'z'"#;
    assert_eq!(canonical_ansi_c_quoting(text), text);
}

#[test]
fn canonical_form_keeps_an_inexact_piece_raw() {
    assert_eq!(canonical_ansi_c_quoting(r"$'r\0m' x"), r"$'r\0m' x");
    assert_eq!(canonical_ansi_c_quoting(r"$'rm x"), r"$'rm x");
}

#[test]
fn marked_canonical_form_keeps_an_empty_ansi_c_piece_bash_reads_the_same() {
    let marked = canonical_ansi_c_quoting_marked(r"echo $'\'' ; $'\162m' x");
    assert_eq!(marked, r"echo $''''\''' ; $'''rm' x");
    assert_eq!(
        split_tokens(&marked),
        split_tokens(r"echo $'\'' ; $'\162m' x")
    );
    assert!(has_multiple_top_level_segments(&marked));
}

#[test]
fn canonical_form_rewrites_ansi_c_inside_double_quoted_substitution() {
    let canonical =
        canonical_ansi_c_quoting(r#"echo "$(echo $'\'' ; git push --force ; echo $'\'')""#);
    assert_eq!(
        canonical,
        r#"echo "$(echo ''\''' ; git push --force ; echo ''\''')""#
    );
    assert_eq!(
        extract_command_substitution_bodies(&canonical),
        vec![r"echo ''\''' ; git push --force ; echo ''\'''"]
    );
}

#[test]
fn canonical_form_rewrites_ansi_c_inside_double_quoted_backticks() {
    assert_eq!(
        canonical_ansi_c_quoting(r#"echo "`$'\162m' x`""#),
        r#"echo "`'rm' x`""#
    );
}

#[test]
fn canonical_form_keeps_double_quotes_after_the_substitution_closes() {
    let text = r#"echo "$(echo $((1+(2))) ) $'y'""#;
    assert_eq!(canonical_ansi_c_quoting(text), text);
}
