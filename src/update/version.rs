//! Strict SemVer parsing and comparison for the update notice.
//!
//! Every entry point here fails closed: a version that does not parse never
//! recommends an update, it just disables the notice for that comparison.

use semver::Version;

use super::error::UpdateError;

/// Parse `raw` as a strict SemVer 2.0 version.
pub fn parse_strict(raw: &str) -> Result<Version, UpdateError> {
    Version::parse(raw.trim()).map_err(|err| UpdateError::InvalidVersion {
        version: raw.to_string(),
        detail: err.to_string(),
    })
}

/// `Some(latest)` when `latest` strictly outranks `installed`; `None` when
/// `latest` is equal to, older than, or fails to parse against `installed`
/// (fail closed — a malformed registry version never triggers a notice).
pub fn newer_version(installed: &str, latest: &str) -> Option<Version> {
    let installed = parse_strict(installed).ok()?;
    let latest = parse_strict(latest).ok()?;
    (latest > installed).then_some(latest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_parse_rejects_a_leading_v() {
        assert!(parse_strict("v1.2.3").is_err());
    }

    #[test]
    fn strict_parse_accepts_plain_semver() {
        assert_eq!(parse_strict("1.2.3").unwrap(), Version::new(1, 2, 3));
    }

    #[test]
    fn newer_version_detects_an_available_update() {
        let newer = newer_version("0.6.7", "0.6.8").unwrap();
        assert_eq!(newer, Version::new(0, 6, 8));
    }

    #[test]
    fn newer_version_is_none_when_equal() {
        assert!(newer_version("0.6.7", "0.6.7").is_none());
    }

    #[test]
    fn newer_version_is_none_when_installed_is_ahead() {
        assert!(newer_version("0.6.8", "0.6.7").is_none());
    }

    #[test]
    fn newer_version_fails_closed_on_malformed_registry_version() {
        assert!(newer_version("0.6.7", "not-a-version").is_none());
    }

    #[test]
    fn newer_version_fails_closed_on_malformed_installed_version() {
        assert!(newer_version("not-a-version", "0.6.8").is_none());
    }

    #[test]
    fn newer_version_respects_prerelease_ordering() {
        assert!(newer_version("0.6.8-rc.1", "0.6.8").is_some());
        assert!(newer_version("0.6.8", "0.6.8-rc.1").is_none());
    }
}
