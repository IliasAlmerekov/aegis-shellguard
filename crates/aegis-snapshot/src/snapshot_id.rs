//! The one owner of `snapshot_id` encoding and parsing (ADR-026 §2).
//!
//! New ids are `v3:<component>:<component>...`. Path and name components are
//! lowercase hex of their bytes, so a path holding `:` or a tab cannot break
//! the split. Providers keep their own component layout and semantic checks.
//! Ids written before `v3` (tab separated) stay readable forever, because the
//! audit log is append-only.

use std::path::{Path, PathBuf};

use crate::error::SnapshotError;

type Result<T> = std::result::Result<T, SnapshotError>;

/// Version prefix of every id written today.
pub(crate) const V3_VERSION: &str = "v3";
/// Separator between the components of a `v3` id.
pub(crate) const V3_SEP: char = ':';
/// Separator of every id written before `v3`.
pub(crate) const LEGACY_SEP: char = '\t';
/// Version prefix of the tab separated `v2` ids.
const V2_VERSION: &str = "v2";

fn malformed_encoding(label: &str, encoded: &str) -> SnapshotError {
    SnapshotError::Snapshot(format!(
        "malformed snapshot_id: invalid {label} encoding {encoded:?}"
    ))
}

/// Lowercase hex of `bytes`.
pub(crate) fn encode_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Hex of a path: raw OS bytes on Unix, the UTF-8 form elsewhere.
pub(crate) fn encode_path(path: &Path) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        encode_bytes(path.as_os_str().as_bytes())
    }

    #[cfg(not(unix))]
    {
        encode_bytes(path.to_string_lossy().as_bytes())
    }
}

fn decode_bytes(encoded: &str, label: &str) -> Result<Vec<u8>> {
    let digits = encoded.as_bytes();
    if !digits.len().is_multiple_of(2) {
        return Err(malformed_encoding(label, encoded));
    }
    digits
        .chunks_exact(2)
        .map(|pair| {
            let high = (pair[0] as char).to_digit(16);
            let low = (pair[1] as char).to_digit(16);
            match (high, low) {
                (Some(high), Some(low)) => Ok((high * 16 + low) as u8),
                _ => Err(malformed_encoding(label, encoded)),
            }
        })
        .collect()
}

/// Inverse of [`encode_path`].
pub(crate) fn decode_path(encoded: &str, label: &str) -> Result<PathBuf> {
    let bytes = decode_bytes(encoded, label)?;

    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        Ok(PathBuf::from(std::ffi::OsString::from_vec(bytes)))
    }

    #[cfg(not(unix))]
    {
        String::from_utf8(bytes)
            .map(PathBuf::from)
            .map_err(|_| malformed_encoding(label, encoded))
    }
}

/// Hex decode to UTF-8 text.
pub(crate) fn decode_string(encoded: &str, label: &str) -> Result<String> {
    String::from_utf8(decode_bytes(encoded, label)?).map_err(|_| malformed_encoding(label, encoded))
}

/// True when `snapshot_id` carries the `v3` prefix. A `v3:` id never falls
/// through to a legacy parser.
pub(crate) fn is_v3(snapshot_id: &str) -> bool {
    snapshot_id
        .strip_prefix(V3_VERSION)
        .is_some_and(|rest| rest.starts_with(V3_SEP))
}

/// Join already encoded components into a `v3` id.
pub(crate) fn join_v3(components: &[&str]) -> String {
    let mut id = String::from(V3_VERSION);
    for component in components {
        id.push(V3_SEP);
        id.push_str(component);
    }
    id
}

fn split_versioned<'a>(
    snapshot_id: &'a str,
    sep: char,
    version: &str,
    components: usize,
) -> Result<Vec<&'a str>> {
    let mut parts = snapshot_id.split(sep);
    let found = parts.next().unwrap_or_default();
    if found != version {
        return Err(SnapshotError::Snapshot(format!(
            "unsupported snapshot_id version {found:?}"
        )));
    }
    let parts: Vec<_> = parts.collect();
    if parts.len() != components {
        return Err(SnapshotError::Snapshot(format!(
            "malformed snapshot_id: {snapshot_id:?}"
        )));
    }
    Ok(parts)
}

/// Split a `v3` id into exactly `components` parts after the version.
pub(crate) fn split_v3(snapshot_id: &str, components: usize) -> Result<Vec<&str>> {
    split_versioned(snapshot_id, V3_SEP, V3_VERSION, components)
}

/// True when `snapshot_id` carries the tab separated `v2` prefix.
pub(crate) fn is_v2(snapshot_id: &str) -> bool {
    snapshot_id
        .strip_prefix(V2_VERSION)
        .is_some_and(|rest| rest.starts_with(LEGACY_SEP))
}

/// Split a `v2` id into exactly `components` parts after the version.
pub(crate) fn split_v2(snapshot_id: &str, components: usize) -> Result<Vec<&str>> {
    split_versioned(snapshot_id, LEGACY_SEP, V2_VERSION, components)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_bytes_writes_lowercase_hex() {
        assert_eq!(encode_bytes(b"/tmp/a.db"), "2f746d702f612e6462");
        assert_eq!(encode_bytes(&[0xAB, 0x00]), "ab00");
    }

    #[test]
    fn path_round_trips_ascii() {
        let path = Path::new("/tmp/db.sqlite");
        assert_eq!(decode_path(&encode_path(path), "p").unwrap(), path);
    }

    #[test]
    fn path_round_trips_colon_and_tab() {
        let path = Path::new("/tmp/a:b\tc");
        let encoded = encode_path(path);
        assert!(!encoded.contains(':') && !encoded.contains('\t'));
        assert_eq!(decode_path(&encoded, "p").unwrap(), path);
    }

    #[cfg(unix)]
    #[test]
    fn path_round_trips_non_utf8_bytes() {
        use std::os::unix::ffi::OsStrExt;
        let path = Path::new(std::ffi::OsStr::from_bytes(b"/tmp/\xff\xfe"));
        assert_eq!(encode_path(path), "2f746d702ffffe");
        assert_eq!(decode_path("2f746d702ffffe", "p").unwrap(), path);
    }

    #[test]
    fn decode_string_round_trips_utf8() {
        assert_eq!(
            decode_string(&encode_bytes("db:é".as_bytes()), "d").unwrap(),
            "db:é"
        );
    }

    #[test]
    fn decode_rejects_odd_length_and_non_hex() {
        for bad in ["abc", "zz", "+f", "0g", "é1"] {
            let error = decode_path(bad, "dump path").unwrap_err().to_string();
            assert!(error.contains("malformed snapshot_id"), "{bad}: {error}");
            assert!(decode_string(bad, "host").is_err(), "{bad}");
        }
    }

    #[test]
    fn decode_string_rejects_invalid_utf8() {
        assert!(decode_string("ff", "host").is_err());
    }

    #[test]
    fn join_v3_uses_colon_separator() {
        assert_eq!(join_v3(&["ab", "5432"]), "v3:ab:5432");
    }

    #[test]
    fn split_v3_returns_components() {
        assert_eq!(split_v3("v3:ab:cd", 2).unwrap(), ["ab", "cd"]);
    }

    #[test]
    fn split_v3_rejects_wrong_count() {
        for id in ["v3:ab", "v3:ab:cd:ef", "v3"] {
            let error = split_v3(id, 2).unwrap_err().to_string();
            assert!(error.contains("malformed snapshot_id"), "{id}: {error}");
        }
    }

    #[test]
    fn split_v3_rejects_wrong_version() {
        let error = split_v3("v4:ab:cd", 2).unwrap_err().to_string();
        assert!(error.contains("unsupported snapshot_id version"), "{error}");
    }

    #[test]
    fn is_v3_needs_prefix_and_separator() {
        assert!(is_v3("v3:ab"));
        assert!(!is_v3("v3"));
        assert!(!is_v3("v3x:ab"));
        assert!(!is_v3("v2\tab"));
    }

    #[test]
    fn split_v2_splits_on_tab() {
        assert_eq!(split_v2("v2\tab\tcd", 2).unwrap(), ["ab", "cd"]);
        assert!(is_v2("v2\tab"));
        assert!(!is_v2("v3:ab"));
    }

    #[test]
    fn split_v2_rejects_wrong_count_and_version() {
        assert!(split_v2("v2\tab", 2).is_err());
        assert!(split_v2("v9\tab\tcd", 2).is_err());
    }
}
