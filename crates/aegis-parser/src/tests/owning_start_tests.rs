use super::*;
use crate::tests::corpus_hash::corpus_hash;

const UNITS: &[&str] = &[
    "a", " ", "'", "\"", "`", "$(", "(", ")", ";", "&&", "||", "\n", "\\", "&", "|", "$", "é",
];

#[test]
fn owning_simple_command_start_matches_the_recorded_start_for_every_short_command_text() {
    assert_eq!(
        corpus_hash(UNITS, 5, |text| owning_simple_command_start(text)
            .to_string()),
        9_174_611_016_252_860_073
    );
}
