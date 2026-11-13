//! Shared sandbox support and test-injection hook.
//!
//! Platform modules (`linux`, `macos`, `windows`, `unsupported`) route their
//! "sandbox unavailable" code paths through [`run_unavailable_result`] and
//! [`run_unavailable_result`] so typed behavior stays consistent across targets.

use std::path::PathBuf;

use crate::{SandboxConfig, SandboxError, SandboxResult, WriteRoot};

// ── Test injection ────────────────────────────────────────────────────────────
//
// The forced-unavailable hook is only exercised by the Linux and macOS sandbox
// modules' "sandbox unavailable" code paths. Gate it to those targets so it is
// not dead code on native Windows (which routes to `unsupported.rs` and never
// consults the hook).
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

// ── Shared unavailable/bypass helpers ─────────────────────────────────────────

pub(crate) fn run_unavailable_result() -> Result<SandboxResult, SandboxError> {
    Err(SandboxError::Required)
}

/// Why a root was left out at enforcement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OmitReason {
    /// The root could not be canonicalized (it does not exist).
    NotFound,
    /// The canonical root lies outside every canonical trusted root.
    OutsideTrustedCeiling,
}

impl OmitReason {
    /// Stable machine-readable name, equal to the config crate's.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::OutsideTrustedCeiling => "outside_trusted_ceiling",
        }
    }
}

/// A root left out at enforcement, with its provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OmittedRoot {
    /// Why it was left out.
    pub reason: OmitReason,
    /// The root as configured, with its layer and location.
    pub root: WriteRoot,
}

/// The canonical writable paths enforcement binds, and what it left out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolvedAllowWrite {
    /// Canonical roots that passed. Empty stays empty: there is no fallback.
    pub paths: Vec<PathBuf>,
    /// Roots left out; surfaced by `status`, never printed by `-c`.
    pub omitted: Vec<OmittedRoot>,
}

/// Canonicalize the effective roots and the trusted roots, and keep an
/// effective root only if its canonical path lies under a canonical trusted
/// root. Shared by bwrap, Landlock and Seatbelt, which bind the resolved path,
/// never the configured one, to avoid symlink confusion.
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

// ── Shared test helpers ────────────────────────────────────────────────────────

#[cfg(test)]
pub(crate) mod test_helpers {
    /// RAII guard that clears the forced-unavailable flag on drop.
    ///
    /// Only meaningful where the forced-unavailable hook exists (Linux/macOS);
    /// gate it to those targets so the module compiles cleanly on native
    /// Windows, which routes to `unsupported.rs` and never sets the flag.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub(crate) struct ForceUnavailableGuard;

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    impl Drop for ForceUnavailableGuard {
        fn drop(&mut self) {
            super::set_force_sandbox_unavailable(false);
        }
    }
}

// ── Common (platform-agnostic) tests ───────────────────────────────────────────

#[cfg(test)]
mod tests {
    use crate::support::run_unavailable_result;
    use crate::{SandboxConfig, SandboxError, SandboxExecutor, SandboxProfile, WriteRoot};
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

    // ── SandboxConfig defaults ────────────────────────────────────────────────

    #[test]
    fn sandbox_config_default_has_empty_allow_write() {
        assert!(SandboxConfig::default().allow_write.is_empty());
    }

    #[test]
    fn sandbox_config_default_disallows_network() {
        assert!(!SandboxConfig::default().allow_network);
    }

    // ── SandboxProfile / SandboxExecutor construction ─────────────────────────

    #[test]
    fn sandbox_profile_builds_from_config() {
        let _profile = SandboxProfile::from_config(&SandboxConfig::default());
    }

    #[test]
    fn sandbox_executor_new_accepts_profile() {
        let profile = SandboxProfile::from_config(&SandboxConfig::default());
        let _executor = SandboxExecutor::new(profile);
    }

    // ── run_unavailable_result behavior ───────────────────────────────────────

    #[test]
    fn run_unavailable_result_always_returns_required_error() {
        assert!(matches!(
            run_unavailable_result(),
            Err(SandboxError::Required)
        ));
    }

    // ── SandboxError::Display ─────────────────────────────────────────────────

    #[test]
    fn sandbox_error_required_display_mentions_required_or_unavailable() {
        let msg = SandboxError::Required.to_string().to_lowercase();
        assert!(msg.contains("required") || msg.contains("unavailable"));
    }

    // ── Legacy test names (kept for backwards compatibility) ─────────────────

    #[test]
    fn test_sandbox_config_default_allows_no_write_paths() {
        assert!(SandboxConfig::default().allow_write.is_empty());
    }

    #[test]
    fn test_sandbox_config_default_disallows_network() {
        assert!(!SandboxConfig::default().allow_network);
    }

    #[test]
    fn test_sandbox_profile_builds_from_config() {
        let _profile = SandboxProfile::from_config(&SandboxConfig::default());
    }

    #[test]
    fn test_sandbox_executor_new_returns_executor() {
        let profile = SandboxProfile::from_config(&SandboxConfig::default());
        let _executor = SandboxExecutor::new(profile);
    }

    #[test]
    fn test_sandbox_error_implements_display() {
        let msg = SandboxError::Required.to_string();
        assert!(!msg.is_empty());
        let lower = msg.to_lowercase();
        assert!(lower.contains("required") || lower.contains("unavailable"));
    }

    #[test]
    fn test_run_non_linux_unavailable_logic() {
        match run_unavailable_result() {
            Err(SandboxError::Required) => {}
            other => {
                panic!("run_unavailable_result() must return Err(Required), got {other:?}")
            }
        }
    }

    // ── resolve_allow_write ───────────────────────────────────────────────────

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
