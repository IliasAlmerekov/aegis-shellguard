//! Parity between the two readers of one bash word (ADR-022 §8, ADR-051).
//!
//! `aegis-language` is a leaf crate and cannot call the `aegis-parser`
//! tokenizer, so its bash adapter carries its own word reader
//! ([`aegis_language::languages::bash_word::shell_word`]). This test runs both
//! readers over a shared table and an exhaustive set of short inputs, so the
//! two cannot drift apart: a word must read the same way in the shell scanner
//! and in the language adapter.

use aegis_language::languages::bash_word::shell_word;
use aegis_parser::split_tokens;

/// What the parser tokenizer reads `text` as when it is exactly one word.
fn parser_word(text: &str) -> Option<String> {
    match split_tokens(text).as_slice() {
        [word] => Some(word.clone()),
        _ => None,
    }
}

fn assert_parity(text: &str) {
    assert_eq!(shell_word(text), parser_word(text), "input: {text:?}");
}

const TABLE: &[&str] = &[
    "rm",
    r"r\m",
    r#""rm""#,
    "r''m",
    r"$'\162m'",
    r"$'\x72m'",
    r"$'rm'",
    r"$'\U00000072m'",
    r"$'\U110000'",
    r"$'\ud800'",
    "r$'m'",
    "$'r''m'",
    r#"$'r'"m"''"#,
    r#"$'a\nb\tc\\d\'e\"f\?g'"#,
    r"$'\a\b\e\E\f\r\v'",
    r"$'\777'",
    r"$'\0rm'",
    r"$'rm\0x'",
    r"$'r\x00m'",
    r"$'\xff'",
    r"$'\xc3\xa9'",
    r"$'\ca'",
    r"$'\c?'",
    r"$'\c\\x'",
    r"$'\c\x'",
    r"$'\c'",
    r"$'\c",
    r"$'\q'",
    r"$'\x'",
    r"$'\u'",
    "$'unterminated",
    r#"$"rm""#,
    r#""r\m""#,
    r#""r\$m""#,
    r#""r\"m""#,
    r#""r\\m""#,
    "\"r\\`m\"",
    "\"r\\\nm\"",
    "r\\\nm",
    "r\\",
    "-r\\f",
    r#""-rf""#,
    "--recursive",
    r#""ba\sh""#,
    r"ba\sh",
    "'-c'",
    "$HOME",
    "$IFS",
    "a$IFS",
    "a${IFS}b",
    "${IFS}",
    "$IFSHOME",
    "${IFS:-x}",
    "a b",
    r"a\ b",
    "'a b'",
    "a;b",
    "a&&b",
    "a&b",
    "a|b",
    "a||b",
    "a>|b",
    r"a\>|b",
    "a>\\\\|b",
    "",
    "''",
    "\"\"",
    "$''",
    "é$'\\351'",
];

#[test]
fn table_inputs_read_the_same_in_both_readers() {
    for text in TABLE {
        assert_parity(text);
    }
}

#[test]
fn every_short_input_reads_the_same_in_both_readers() {
    const ALPHABET: &[char] = &[
        '\\', '\'', '"', '$', '`', ' ', '\n', ';', '&', '|', '>', '<', '{', '}', 'I', 'F', 'S',
        'c', 'x', 'u', '1', '7', '?', '\0', 'é',
    ];
    let mut text = String::new();
    enumerate(ALPHABET, 4, &mut text);
}

fn enumerate(alphabet: &[char], depth: usize, text: &mut String) {
    assert_parity(text);
    if depth > 0 {
        for &c in alphabet {
            text.push(c);
            enumerate(alphabet, depth - 1, text);
            text.pop();
        }
    }
}

#[test]
fn ansi_c_escape_sequences_read_the_same_in_both_readers() {
    // Every escape letter and digit run inside `$'...'`, with a trailing
    // character to catch over- or under-consumption.
    let mut text = String::new();
    for esc in (0x20u8..0x7f).map(char::from) {
        for tail in [
            "", "0", "7", "f", "F", "g", "'", "\\", "00", "0000072", "1F600",
        ] {
            text.clear();
            text.push_str("$'\\");
            text.push(esc);
            text.push_str(tail);
            text.push_str("'z");
            assert_parity(&text);
        }
    }
}
