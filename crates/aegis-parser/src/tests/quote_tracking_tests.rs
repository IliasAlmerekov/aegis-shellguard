use super::super::*;
use crate::tests::corpus_hash::corpus_hash;

const SPLITTER_UNITS: &[&str] = &[
    "a", " ", "'", "\"", "`", "$(", "(", ")", "{", "}", ";", "&", "|", "\n", "\\", ">", "$",
];

const SPLITTER_DEEP_UNITS: &[&str] = &["a", " ", "'", "\"", "`", "$(", "(", ")", ";", "|"];

fn observe_list_segments(cmd: &str) -> String {
    format!("{:?}", list_segments(cmd))
}

#[test]
fn list_segments_matches_the_recorded_split_of_every_short_quote_and_separator_string() {
    assert_eq!(
        corpus_hash(SPLITTER_UNITS, 4, observe_list_segments),
        4_613_771_063_956_658_001
    );
}

#[test]
fn list_segments_matches_the_recorded_split_of_every_deeply_nested_quote_and_separator_string() {
    assert_eq!(
        corpus_hash(SPLITTER_DEEP_UNITS, 6, observe_list_segments),
        9_993_701_982_935_390_167
    );
}

const SPLITTER_BRACE_UNITS: &[&str] = &["a", "'", "\"", "`", "$(", ")", "{ ", " }", ";"];

#[test]
fn list_segments_matches_the_recorded_split_of_every_brace_group_nested_in_quotes_and_substitutions()
 {
    assert_eq!(
        corpus_hash(SPLITTER_BRACE_UNITS, 6, observe_list_segments),
        6_288_762_345_588_282_407
    );
}

const MARKER_PREFIX_UNITS: &[&str] = &[
    "cat ",
    "ls ",
    "git commit -m ",
    "'",
    "\"",
    "`",
    "$(",
    "(",
    ")",
    ";",
    "&&",
    "||",
    "\n",
    "\\",
    "&",
    "|",
];

fn observe_heredoc_bodies(prefix: &str) -> String {
    format!(
        "{:?}",
        extract_heredoc_bodies(&format!("{prefix}<<X\nb\nX"))
    )
}

#[test]
fn heredoc_bodies_match_the_recorded_consumer_flag_for_every_short_marker_prefix() {
    assert_eq!(
        corpus_hash(MARKER_PREFIX_UNITS, 4, observe_heredoc_bodies),
        5_748_035_997_838_307_531
    );
}
