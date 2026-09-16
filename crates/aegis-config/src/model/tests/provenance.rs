//! The `Provenance` marker (#270, Q8).
//!
//! The `#[serde(skip)]` bookkeeping fields of `AegisConfig` (`*_layers`,
//! `audit_*_source`) are not user config: they record which layer a value
//! came from. They carry no Ratchet direction and stay out of ADR-013's
//! table, but they still pass through the exhaustive destructure in
//! `merge_layer` behind this marker, so a new bookkeeping field cannot be
//! left silently unhandled.

use super::partial::PartialConfig;
use super::*;
use crate::model::ratchet::Provenance;

#[test]
fn provenance_stamps_the_merging_layer_on_appended_entries() {
    let recorded = Provenance::appended(
        vec![ConfigSourceLayer::Global],
        ConfigSourceLayer::Project,
        2,
    );

    assert_eq!(
        recorded,
        vec![
            ConfigSourceLayer::Global,
            ConfigSourceLayer::Project,
            ConfigSourceLayer::Project,
        ],
        "entries the base contributed keep their layer; appended ones take the merging layer"
    );
}

#[test]
fn provenance_leaves_the_record_untouched_when_a_layer_appends_nothing() {
    let recorded = Provenance::appended(
        vec![ConfigSourceLayer::Global],
        ConfigSourceLayer::Project,
        0,
    );

    assert_eq!(recorded, vec![ConfigSourceLayer::Global]);
}

#[test]
fn provenance_records_the_layer_whose_scalar_the_merge_kept() {
    let recorded = Provenance::scalar_source(
        Some(ConfigSourceLayer::Global),
        Some(42_u64),
        42_u64,
        ConfigSourceLayer::Project,
    );

    assert_eq!(recorded, Some(ConfigSourceLayer::Project));
}

#[test]
fn provenance_keeps_the_earlier_source_when_the_ratchet_refused_the_request() {
    let recorded = Provenance::scalar_source(
        Some(ConfigSourceLayer::Global),
        Some(1_u64),
        42_u64,
        ConfigSourceLayer::Project,
    );

    assert_eq!(
        recorded,
        Some(ConfigSourceLayer::Global),
        "a rejected request must not claim authorship of the value that survived"
    );
}

#[test]
fn provenance_keeps_the_earlier_source_when_the_layer_is_silent() {
    let recorded = Provenance::scalar_source(
        Some(ConfigSourceLayer::Global),
        None::<u64>,
        42_u64,
        ConfigSourceLayer::Project,
    );

    assert_eq!(recorded, Some(ConfigSourceLayer::Global));
}

#[test]
fn provenance_fields_declare_no_ratchet_path() {
    let (_config, sink) = AegisConfig::merge_layer_for_coverage_test(
        AegisConfig::defaults(),
        PartialConfig::default(),
        ConfigSourceLayer::Project,
        "provenance_test.aegis.toml",
    );

    let bookkeeping: Vec<&&str> = sink
        .touched
        .iter()
        .filter(|path| path.ends_with("_layers") || path.ends_with("_source"))
        .collect();

    assert!(
        bookkeeping.is_empty(),
        "the Provenance marker is not a Ratchet direction and must claim no field path: {bookkeeping:?}"
    );
}
