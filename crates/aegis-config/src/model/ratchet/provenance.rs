use crate::allowlist::ConfigSourceLayer;

pub(crate) struct Provenance;

impl Provenance {
    pub(crate) fn appended(
        base: Vec<ConfigSourceLayer>,
        layer: ConfigSourceLayer,
        appended: usize,
    ) -> Vec<ConfigSourceLayer> {
        let mut recorded = base;
        recorded.extend(std::iter::repeat_n(layer, appended));
        recorded
    }

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
