use super::*;
use crate::tests::corpus_hash::corpus_hash;

fn observe(line: &str) -> String {
    format!("{:?}", find_unquoted_double_lt(line))
}

#[test]
fn find_unquoted_double_lt_matches_the_recorded_position_for_every_short_line() {
    const UNITS: &[&str] = &[
        "a", " ", "'", "\"", "`", "$(", "(", ")", "<", "\\", "$", "é", "<<",
    ];
    assert_eq!(corpus_hash(UNITS, 6, observe), 488_006_467_254_109_167);
}

#[test]
fn find_unquoted_double_lt_matches_the_recorded_position_for_every_deeply_nested_quote_line() {
    const UNITS: &[&str] = &["'", "\"", "`", "$(", "(", ")", "<<", " "];
    assert_eq!(corpus_hash(UNITS, 7, observe), 5_053_353_693_718_631_563);
}
