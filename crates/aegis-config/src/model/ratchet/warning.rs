use crate::allowlist::ConfigSourceLayer;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SecurityRatchetWarning {
    pub(crate) field: &'static str,
    pub(crate) requested: String,
    pub(crate) kept: String,
    pub(crate) location: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeprecatedSandboxFieldWarning {
    pub(crate) field: &'static str,
    pub(crate) layer: ConfigSourceLayer,
    pub(crate) location: String,
}

#[derive(Debug, Default)]
pub(crate) struct RatchetSink {
    warnings: Vec<SecurityRatchetWarning>,
    deprecated_sandbox_fields: Vec<DeprecatedSandboxFieldWarning>,
    #[cfg(test)]
    pub(crate) touched: Vec<&'static str>,
}

impl RatchetSink {
    pub(crate) fn touch(
        &mut self,
        #[cfg_attr(not(test), allow(unused_variables))] path: &'static str,
    ) {
        #[cfg(test)]
        self.touched.push(path);
    }

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

    pub(crate) fn warn_deprecated_sandbox(
        &mut self,
        field: &'static str,
        layer: ConfigSourceLayer,
        location: &str,
    ) {
        self.deprecated_sandbox_fields
            .push(DeprecatedSandboxFieldWarning {
                field,
                layer,
                location: location.to_string(),
            });
    }

    pub(crate) fn into_warnings(self) -> Vec<SecurityRatchetWarning> {
        self.warnings
    }

    pub(crate) fn into_all_warnings(
        self,
    ) -> (
        Vec<SecurityRatchetWarning>,
        Vec<DeprecatedSandboxFieldWarning>,
    ) {
        (self.warnings, self.deprecated_sandbox_fields)
    }
}

pub(crate) fn is_project(layer: ConfigSourceLayer) -> bool {
    layer == ConfigSourceLayer::Project
}
