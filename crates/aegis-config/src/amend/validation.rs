use super::AppendOutcome;
use crate::AegisConfig;
use crate::allowlist::ConfigSourceLayer;

pub(super) fn config_layer_from_path(config_path: &std::path::Path) -> ConfigSourceLayer {
    if config_path.file_name().is_some_and(|n| n == ".aegis.toml") {
        ConfigSourceLayer::Project
    } else {
        ConfigSourceLayer::Global
    }
}

pub(super) fn check_dedup_and_conflict(
    config: &AegisConfig,
    pattern: &str,
    cwd: &str,
    target_table: super::TableKind,
    location: ConfigSourceLayer,
) -> Option<AppendOutcome> {
    match target_table {
        super::TableKind::Allow => {
            if config
                .allowlist
                .iter()
                .any(|r| r.pattern == pattern && r.cwd.as_deref() == Some(cwd) && r.user.is_none())
            {
                return Some(AppendOutcome::SkippedDuplicate);
            }
            if config
                .blocklist
                .iter()
                .any(|r| r.pattern == pattern && r.cwd.as_deref() == Some(cwd))
            {
                return Some(AppendOutcome::Conflict {
                    pattern: pattern.to_string(),
                    existing_location: location,
                });
            }
        }
        super::TableKind::Blocklist => {
            if config
                .blocklist
                .iter()
                .any(|r| r.pattern == pattern && r.cwd.as_deref() == Some(cwd) && r.user.is_none())
            {
                return Some(AppendOutcome::SkippedDuplicate);
            }
            if config
                .allowlist
                .iter()
                .any(|r| r.pattern == pattern && r.cwd.as_deref() == Some(cwd))
            {
                return Some(AppendOutcome::Conflict {
                    pattern: pattern.to_string(),
                    existing_location: location,
                });
            }
        }
    }
    None
}
