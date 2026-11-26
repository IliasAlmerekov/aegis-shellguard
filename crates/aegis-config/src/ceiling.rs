use std::path::{Component, Path, PathBuf};

mod compute;
#[cfg(test)]
mod compute_tests;
mod workspace;
#[cfg(test)]
mod workspace_tests;

pub use compute::{
    CeilingOmitReason, CeilingWarning, PathOmitted, TrustedCeiling, compute_trusted_ceiling,
    compute_trusted_ceiling_with,
};
pub use workspace::{
    DeviceFn, ResolverInputs, Workspace, WorkspaceResolution, WorkspaceUnresolvedReason,
    resolve_workspace, resolve_workspace_with,
};

use aegis_types::CeilingLayer;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "PathBuf", into = "PathBuf")]
pub struct WriteRoot {
    pub path: PathBuf,
    pub layer: CeilingLayer,
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

pub(crate) fn is_lexically_invalid(path: &Path) -> bool {
    !path.is_absolute() || has_parent_dir(path)
}

pub(crate) fn contains(outer: &Path, inner: &Path) -> bool {
    !has_parent_dir(inner) && inner.starts_with(outer)
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_root_converts_into_its_path() {
        let root = WriteRoot::new(PathBuf::from("/srv/app"), CeilingLayer::Global, "cfg");
        assert_eq!(PathBuf::from(root), PathBuf::from("/srv/app"));
    }
}
