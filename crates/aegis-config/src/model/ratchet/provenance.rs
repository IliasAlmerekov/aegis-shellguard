use crate::allowlist::ConfigSourceLayer;

/// Marker for the `#[serde(skip)]` bookkeeping fields of
/// [`super::super::AegisConfig`]: `custom_pattern_layers`,
/// `allowlist_layers`, `blocklist_layers`, `audit_max_file_size_bytes_source`
/// and `audit_retention_files_source`.
///
/// These are not user config and carry no Ratchet direction: a project file
/// cannot write them, so there is nothing for the ratchet to keep or refuse.
/// That is why they are deliberately absent from ADR-013's direction table
/// and why this marker sits outside `direction`, next to the five closed
/// directions rather than among them. It also claims no field path in the
/// [`super::RatchetSink`], so the schema-coverage test keeps matching
/// declared paths one-for-one against the config's JSON schema, which these
/// fields are skipped from.
///
/// The marker earns its keep by naming what these fields are. They still go
/// through the exhaustive destructure in `model::merge_layer`, and every one
/// of them routes through a `Provenance` call there, so adding a new
/// bookkeeping field leaves a compile error rather than a silently
/// unmaintained record of where a value came from.
pub(crate) struct Provenance;

impl Provenance {
    /// Stamp `layer` onto the `appended` entries an `Append` direction just
    /// added, keeping the record the earlier layers built.
    pub(crate) fn appended(
        base: Vec<ConfigSourceLayer>,
        layer: ConfigSourceLayer,
        appended: usize,
    ) -> Vec<ConfigSourceLayer> {
        let mut recorded = base;
        recorded.extend(std::iter::repeat_n(layer, appended));
        recorded
    }

    /// Record `layer` as the source of a scalar when the merge kept exactly
    /// the value this layer requested. A layer that stayed silent, or whose
    /// request the ratchet refused, does not get to claim authorship of the
    /// value that survived.
    pub(crate) fn scalar_source<T: PartialEq>(
        base: Option<ConfigSourceLayer>,
        requested: Option<T>,
        kept: T,
        layer: ConfigSourceLayer,
    ) -> Option<ConfigSourceLayer> {
        if requested == Some(kept) {
            Some(layer)
        } else {
            base
        }
    }
}
