#[cfg(test)]
use super::{AegisConfig, ConfigLayerPath};
#[cfg(test)]
use crate::allowlist::ConfigSourceLayer;

mod audit;
mod context;
mod direction;
mod docker;
mod language;
mod policy;
mod provenance;
mod prune;
mod sandbox;
mod supabase;
mod targets;
mod warning;

pub(crate) use context::RatchetContext;
pub(crate) use warning::{DeprecatedSandboxFieldWarning, SecurityRatchetWarning};

pub(super) use audit::merge_audit;
pub(super) use docker::custom_docker_scope;
pub(super) use language::merge_language_analysis;
pub(super) use policy::custom_rules;
pub(super) use prune::merge_prune;
pub(super) use sandbox::merge_sandbox;
pub(super) use supabase::custom_supabase_snapshot;
pub(super) use targets::{custom_mysql_snapshot, custom_postgres_snapshot, custom_sqlite_snapshot};
pub(super) use warning::RatchetSink;

#[cfg(test)]
impl AegisConfig {
    pub(crate) fn project_security_ratchet_warnings(
        base: &Self,
        layer: &ConfigLayerPath,
    ) -> std::result::Result<Vec<SecurityRatchetWarning>, crate::error::ConfigError> {
        if layer.source_layer != ConfigSourceLayer::Project {
            return Ok(Vec::new());
        }
        let (_, warnings) = Self::merge_layer_path_with_warnings(base.clone(), layer)?;
        Ok(warnings)
    }
}

pub(super) use direction::{
    Ratchet, Tighten, Unratcheted, append, bool_true_is_stricter, format_debug, format_display,
};
pub(super) use provenance::Provenance;
