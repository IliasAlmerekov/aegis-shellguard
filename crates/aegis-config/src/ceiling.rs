//! The `Trusted ceiling` of ADR-030 §2: the upper bound on the paths a
//! sandboxed command may write.
//!
//! This module holds the configured side of the ceiling ([`WriteRoot`] with
//! its provenance, and the component-wise path-tree intersection the project
//! ratchet uses). The runtime side — the workspace resolver and the ceiling
//! computation — lives in the sibling submodules.

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Where a ceiling root came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CeilingLayer {
    /// Entry of the global config file.
    Global,
    /// Entry of the project config file.
    Project,
    /// Root computed by Aegis (worktree root, verified gitdir, temporary
    /// directory), not read from any config file.
    Computed,
}

/// One writable root of the `Trusted ceiling`, with its provenance.
///
/// On the wire (TOML, JSON schema) this is a bare path; `layer` and `location`
/// are stamped by the layer merge and never serialised.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "PathBuf", into = "PathBuf")]
pub struct WriteRoot {
    /// The configured path, exactly as written (never lexically folded).
    pub path: PathBuf,
    /// Layer the entry came from.
    pub layer: CeilingLayer,
    /// Config file path (or other label) the entry came from.
    pub location: String,
}

impl From<PathBuf> for WriteRoot {
    fn from(path: PathBuf) -> Self {
        Self {
            path,
            layer: CeilingLayer::Global,
            location: String::new(),
        }
    }
}

impl From<WriteRoot> for PathBuf {
    fn from(root: WriteRoot) -> Self {
        root.path
    }
}

impl WriteRoot {
    /// Build a root stamped with its layer and location.
    pub fn new(path: PathBuf, layer: CeilingLayer, location: impl Into<String>) -> Self {
        Self {
            path,
            layer,
            location: location.into(),
        }
    }
}

pub(crate) fn has_parent_dir(path: &Path) -> bool {
    path.components().any(|c| matches!(c, Component::ParentDir))
}

/// Component-wise containment, no filesystem access. A path with a `..`
/// component is never contained: `..` is not folded lexically.
pub(crate) fn contains(outer: &Path, inner: &Path) -> bool {
    !has_parent_dir(inner) && inner.starts_with(outer)
}

/// Intersect two sets of path trees component-wise: for every pair where one
/// root contains the other, keep the narrower one (with its own provenance).
/// The result never admits a path that either side did not.
pub(crate) fn intersect(base: &[WriteRoot], overlay: &[WriteRoot]) -> Vec<WriteRoot> {
    let mut out: Vec<WriteRoot> = Vec::new();
    for a in base {
        for b in overlay {
            let narrower = if contains(&a.path, &b.path) {
                b
            } else if contains(&b.path, &a.path) {
                a
            } else {
                continue;
            };
            if !out.iter().any(|r| r.path == narrower.path) {
                out.push(narrower.clone());
            }
        }
    }
    out
}
