pub const RELEASE_GRAMMARS: &[&str] = &["python", "javascript", "typescript", "bash"];

#[cfg(feature = "grammars")]
pub const SUPPORTED_LANGUAGE_ABI: u32 = tree_sitter::LANGUAGE_VERSION as u32;
#[cfg(not(feature = "grammars"))]
pub const SUPPORTED_LANGUAGE_ABI: u32 = 15;

#[cfg(feature = "grammars")]
pub const MIN_SUPPORTED_LANGUAGE_ABI: u32 = tree_sitter::MIN_COMPATIBLE_LANGUAGE_VERSION as u32;
#[cfg(not(feature = "grammars"))]
pub const MIN_SUPPORTED_LANGUAGE_ABI: u32 = 13;

pub const RELEASE_TARGETS: &[&str] = &[
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-musl",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrammarEntry {
    pub language: &'static str,
    pub crate_name: &'static str,
    pub version: &'static str,
    pub upstream: &'static str,
    pub license_spdx: &'static str,
    pub abi: u32,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ManifestError {
    #[error("grammar `{language}` has no pinned version")]
    Unpinned { language: &'static str },

    #[error("grammar `{language}` is missing license evidence")]
    MissingLicense { language: &'static str },

    #[error("grammar `{language}` declares unsupported Tree-sitter ABI {abi}")]
    UnsupportedAbi { language: &'static str, abi: u32 },

    #[error("grammar `{language}` is absent from the release manifest")]
    AbsentFromRelease { language: &'static str },
}

pub fn validate_entry(
    entry: &GrammarEntry,
    release_grammars: &[&str],
) -> Result<(), ManifestError> {
    if entry.version.is_empty() || entry.version == "*" {
        return Err(ManifestError::Unpinned {
            language: entry.language,
        });
    }
    if entry.license_spdx.is_empty() {
        return Err(ManifestError::MissingLicense {
            language: entry.language,
        });
    }
    if entry.abi < MIN_SUPPORTED_LANGUAGE_ABI || entry.abi > SUPPORTED_LANGUAGE_ABI {
        return Err(ManifestError::UnsupportedAbi {
            language: entry.language,
            abi: entry.abi,
        });
    }
    if !release_grammars.contains(&entry.language) {
        return Err(ManifestError::AbsentFromRelease {
            language: entry.language,
        });
    }
    Ok(())
}

pub fn validate_manifest(
    manifest: &[GrammarEntry],
    release_grammars: &[&str],
) -> Result<(), Vec<ManifestError>> {
    let errors: Vec<ManifestError> = manifest
        .iter()
        .filter_map(|e| validate_entry(e, release_grammars).err())
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

pub const BUILTIN_MANIFEST: &[GrammarEntry] = &[
    GrammarEntry {
        language: "python",
        crate_name: "tree-sitter-python",
        version: "0.25.0",
        upstream: "https://github.com/tree-sitter/tree-sitter-python",
        license_spdx: "MIT",
        abi: SUPPORTED_LANGUAGE_ABI,
    },
    GrammarEntry {
        language: "javascript",
        crate_name: "tree-sitter-javascript",
        version: "0.25.0",
        upstream: "https://github.com/tree-sitter/tree-sitter-javascript",
        license_spdx: "MIT",
        abi: SUPPORTED_LANGUAGE_ABI,
    },
    GrammarEntry {
        language: "typescript",
        crate_name: "tree-sitter-typescript",
        version: "0.23.2",
        upstream: "https://github.com/tree-sitter/tree-sitter-typescript",
        license_spdx: "MIT",
        abi: 14,
    },
    GrammarEntry {
        language: "bash",
        crate_name: "tree-sitter-bash",
        version: "0.25.1",
        upstream: "https://github.com/tree-sitter/tree-sitter-bash",
        license_spdx: "MIT",
        abi: SUPPORTED_LANGUAGE_ABI,
    },
];

#[cfg(all(test, feature = "grammars"))]
mod tests {
    use super::*;

    #[test]
    fn fallback_abi_constants_match_pinned_tree_sitter_runtime() {
        assert_eq!(SUPPORTED_LANGUAGE_ABI, 15);
        assert_eq!(MIN_SUPPORTED_LANGUAGE_ABI, 13);
    }
}
