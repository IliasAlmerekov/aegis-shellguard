use std::path::PathBuf;

use crate::{SandboxConfig, WriteRoot};

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[cfg(test)]
thread_local! {
    static FORCE_SANDBOX_UNAVAILABLE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[cfg(test)]
pub(crate) fn set_force_sandbox_unavailable(val: bool) {
    FORCE_SANDBOX_UNAVAILABLE.with(|c| c.set(val));
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) fn is_forced_sandbox_unavailable() -> bool {
    #[cfg(test)]
    return FORCE_SANDBOX_UNAVAILABLE.with(|c| c.get());
    #[cfg(not(test))]
    return false;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OmitReason {
    NotFound,
    OutsideTrustedCeiling,
}

impl OmitReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::OutsideTrustedCeiling => "outside_trusted_ceiling",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OmittedRoot {
    pub reason: OmitReason,
    pub root: WriteRoot,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolvedAllowWrite {
    pub paths: Vec<PathBuf>,
    pub omitted: Vec<OmittedRoot>,
}

pub fn resolve_allow_write(config: &SandboxConfig) -> ResolvedAllowWrite {
    let trusted: Vec<PathBuf> = config
        .trusted_roots
        .iter()
        .filter_map(|root| root.path.canonicalize().ok())
        .collect();
    let mut out = ResolvedAllowWrite::default();
    for root in &config.allow_write {
        let reason = match root.path.canonicalize() {
            Err(_) => OmitReason::NotFound,
            Ok(canonical) => {
                let inside = trusted.iter().any(|t| canonical.starts_with(t));
                if inside {
                    if !out.paths.contains(&canonical) {
                        out.paths.push(canonical);
                    }
                    continue;
                }
                OmitReason::OutsideTrustedCeiling
            }
        };
        out.omitted.push(OmittedRoot {
            reason,
            root: root.clone(),
        });
    }
    out
}

#[cfg(test)]
pub(crate) mod test_helpers {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub(crate) struct ForceUnavailableGuard;

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    impl Drop for ForceUnavailableGuard {
        fn drop(&mut self) {
            super::set_force_sandbox_unavailable(false);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{SandboxConfig, SandboxError, WriteRoot};
    use aegis_types::CeilingLayer;

    #[test]
    fn omit_reason_names_are_the_stable_wire_strings() {
        use crate::support::OmitReason;
        assert_eq!(OmitReason::NotFound.as_str(), "not_found");
        assert_eq!(
            OmitReason::OutsideTrustedCeiling.as_str(),
            "outside_trusted_ceiling"
        );
    }

    #[test]
    fn sandbox_config_default_has_empty_allow_write() {
        assert!(SandboxConfig::default().allow_write.is_empty());
    }

    #[test]
    fn sandbox_config_default_disallows_network() {
        assert!(!SandboxConfig::default().allow_network);
    }

    #[test]
    fn sandbox_error_required_display_mentions_required_or_unavailable() {
        let msg = SandboxError::Required.to_string().to_lowercase();
        assert!(msg.contains("required") || msg.contains("unavailable"));
    }

    #[test]
    fn test_sandbox_config_default_allows_no_write_paths() {
        assert!(SandboxConfig::default().allow_write.is_empty());
    }

    #[test]
    fn test_sandbox_config_default_disallows_network() {
        assert!(!SandboxConfig::default().allow_network);
    }

    #[test]
    fn test_sandbox_error_implements_display() {
        let msg = SandboxError::Required.to_string();
        assert!(!msg.is_empty());
        let lower = msg.to_lowercase();
        assert!(lower.contains("required") || lower.contains("unavailable"));
    }

    fn root(path: &std::path::Path, layer: CeilingLayer, location: &str) -> WriteRoot {
        WriteRoot {
            path: path.to_path_buf(),
            layer,
            location: location.into(),
        }
    }

    #[test]
    fn symlink_inside_the_ceiling_that_resolves_outside_is_omitted() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().canonicalize().unwrap();
        let ws = base.join("ws");
        let outside = base.join("outside");
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, ws.join("link")).unwrap();
        let link = ws.join("link");

        let resolved = super::resolve_allow_write(&SandboxConfig {
            allow_write: vec![root(&link, CeilingLayer::Project, "/p/.aegis.toml")],
            trusted_roots: vec![root(&ws, CeilingLayer::Global, "/g/config.toml")],
            ..Default::default()
        });

        assert!(resolved.paths.is_empty());
        assert_eq!(
            resolved.omitted,
            vec![super::OmittedRoot {
                reason: super::OmitReason::OutsideTrustedCeiling,
                root: root(&link, CeilingLayer::Project, "/p/.aegis.toml"),
            }]
        );
    }

    #[test]
    fn missing_root_is_omitted_as_not_found_not_an_error() {
        let resolved = super::resolve_allow_write(&SandboxConfig {
            allow_write: vec![WriteRoot::new("/nonexistent_aegis_test_path_xyz")],
            ..Default::default()
        });
        assert_eq!(resolved.omitted[0].reason, super::OmitReason::NotFound);
    }

    #[test]
    fn a_config_without_trusted_roots_admits_no_root() {
        let resolved = super::resolve_allow_write(&SandboxConfig {
            allow_write: vec![WriteRoot::new("/tmp")],
            ..Default::default()
        });
        assert!(resolved.paths.is_empty());
        assert_eq!(resolved.omitted.len(), 1);
    }

    #[test]
    fn root_inside_a_trusted_root_is_kept_canonical() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().canonicalize().unwrap();
        std::fs::create_dir_all(base.join("src")).unwrap();
        let resolved = super::resolve_allow_write(&SandboxConfig {
            allow_write: vec![WriteRoot::new(base.join("src"))],
            trusted_roots: vec![WriteRoot::new(&base)],
            ..Default::default()
        });
        assert_eq!(resolved.paths, vec![base.join("src")]);
    }
}
