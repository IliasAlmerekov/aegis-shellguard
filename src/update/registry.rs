use std::process::Command;

use serde_json::Value;

use super::error::UpdateError;
use super::state::Channel;

const REGISTRY_TIMEOUT_SECS: u64 = 5;
const MAX_RESPONSE_BYTES: usize = 64 * 1024;
const USER_AGENT: &str = concat!("aegis-update-check/", env!("CARGO_PKG_VERSION"));

fn registry_url(channel: Channel) -> &'static str {
    match channel {
        Channel::Npm => "https://registry.npmjs.org/@iliasalmerekov%2Faegis/latest",
    }
}

pub fn fetch_latest_version(channel: Channel) -> Result<String, UpdateError> {
    let output = Command::new("curl")
        .args(["--silent", "--show-error", "--fail", "--location"])
        .arg("--max-time")
        .arg(REGISTRY_TIMEOUT_SECS.to_string())
        .arg("--max-filesize")
        .arg(MAX_RESPONSE_BYTES.to_string())
        .args(["-A", USER_AGENT])
        .arg(registry_url(channel))
        .output()
        .map_err(|err| UpdateError::NetworkUnavailable {
            detail: format!("failed to run curl: {err}"),
        })?;

    if !output.status.success() {
        return Err(UpdateError::NetworkUnavailable {
            detail: format!("curl exited with {}", output.status),
        });
    }

    parse_latest_version(&output.stdout)
}

pub(super) fn parse_latest_version(body: &[u8]) -> Result<String, UpdateError> {
    if body.len() > MAX_RESPONSE_BYTES {
        return Err(UpdateError::MalformedRegistryResponse {
            detail: "response exceeded the size cap".to_string(),
        });
    }

    let value: Value =
        serde_json::from_slice(body).map_err(|err| UpdateError::MalformedRegistryResponse {
            detail: err.to_string(),
        })?;

    value
        .get("version")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| UpdateError::MalformedRegistryResponse {
            detail: "response has no string \"version\" field".to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_version_field_out_of_a_full_manifest() {
        let body = br#"{"name":"@iliasalmerekov/aegis","version":"0.6.8","readme":"..."}"#;
        assert_eq!(parse_latest_version(body).unwrap(), "0.6.8");
    }

    #[test]
    fn rejects_a_response_with_no_version_field() {
        let body = br#"{"name":"@iliasalmerekov/aegis"}"#;
        assert!(matches!(
            parse_latest_version(body),
            Err(UpdateError::MalformedRegistryResponse { .. })
        ));
    }

    #[test]
    fn rejects_a_non_string_version_field() {
        let body = br#"{"version": 68}"#;
        assert!(matches!(
            parse_latest_version(body),
            Err(UpdateError::MalformedRegistryResponse { .. })
        ));
    }

    #[test]
    fn rejects_malformed_json() {
        assert!(matches!(
            parse_latest_version(b"not json"),
            Err(UpdateError::MalformedRegistryResponse { .. })
        ));
    }

    #[test]
    fn rejects_a_response_over_the_size_cap() {
        let oversized = vec![b'a'; MAX_RESPONSE_BYTES + 1];
        assert!(matches!(
            parse_latest_version(&oversized),
            Err(UpdateError::MalformedRegistryResponse { .. })
        ));
    }

    #[test]
    fn registry_url_is_https_and_scoped() {
        let url = registry_url(Channel::Npm);
        assert!(url.starts_with("https://registry.npmjs.org/"));
        assert!(url.contains("@iliasalmerekov%2Faegis"));
    }
}
