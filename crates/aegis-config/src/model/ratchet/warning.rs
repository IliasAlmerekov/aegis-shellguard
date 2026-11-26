use crate::allowlist::ConfigSourceLayer;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SecurityRatchetWarning {
    pub(crate) field: &'static str,
    pub(crate) requested: String,
    pub(crate) kept: String,
    pub(crate) location: String,
}

#[derive(Debug, Default)]
pub(crate) struct RatchetSink {
    warnings: Vec<SecurityRatchetWarning>,
    #[cfg(test)]
    pub(crate) touched: Vec<&'static str>,
}

impl RatchetSink {
    #[cfg(test)]
    pub(crate) fn touch(&mut self, path: &'static str) {
        self.touched.push(path);
    }

    #[cfg(not(test))]
    pub(crate) fn touch(&mut self, _path: &'static str) {}

    pub(crate) fn warn(
        &mut self,
        field: &'static str,
        requested: String,
        kept: String,
        location: &str,
    ) {
        if requested != kept {
            self.warnings.push(SecurityRatchetWarning {
                field,
                requested,
                kept,
                location: location.to_string(),
            });
        }
    }

    pub(crate) fn into_warnings(self) -> Vec<SecurityRatchetWarning> {
        self.warnings
    }
}

pub(crate) fn is_project(layer: ConfigSourceLayer) -> bool {
    layer == ConfigSourceLayer::Project
}
