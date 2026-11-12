//! Ceiling computation: turns the merged `[sandbox] allow_write` and the
//! resolved workspace into the roots of the `Trusted ceiling`, with a typed
//! warning for every entry that could not enter it.

use std::fs;
use std::path::{Path, PathBuf};

use super::workspace::{
    ResolverInputs, WorkspaceResolution, WorkspaceUnresolvedReason, resolve_workspace_with,
};
use super::{CeilingLayer, WriteRoot, contains, has_parent_dir, intersect};
use crate::model::SandboxSettings;

/// Why a path could not enter the ceiling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CeilingOmitReason {
    /// Config entry is not an absolute path.
    Relative,
    /// Config entry has a `..` component (never folded lexically).
    ParentDir,
    /// Path does not exist.
    NotFound,
    /// `.git` file names a gitdir that does not point back at the worktree.
    GitdirUnverified,
    /// Entry lies outside the trusted roots (merge clamp, or canonical escape
    /// at enforcement).
    OutsideTrustedCeiling,
}

impl CeilingOmitReason {
    /// Stable machine-readable name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Relative => "relative",
            Self::ParentDir => "parent_dir",
            Self::NotFound => "not_found",
            Self::GitdirUnverified => "gitdir_unverified",
            Self::OutsideTrustedCeiling => "outside_trusted_ceiling",
        }
    }
}

/// `trusted_ceiling_path_omitted { reason, layer, location, path }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathOmitted {
    /// Why the path was left out.
    pub reason: CeilingOmitReason,
    /// Layer the path came from.
    pub layer: CeilingLayer,
    /// Config file (or computed-root label) the path came from.
    pub location: String,
    /// The offending path, as written.
    pub path: PathBuf,
}

/// A typed ceiling warning. Surfacing never changes an exit code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CeilingWarning {
    /// `trusted_ceiling_path_omitted`.
    PathOmitted(PathOmitted),
    /// `workspace_unresolved { reason, cwd }`.
    WorkspaceUnresolved {
        /// Why there is no workspace.
        reason: WorkspaceUnresolvedReason,
        /// Working directory the search started from.
        cwd: PathBuf,
    },
}

/// The computed `Trusted ceiling`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TrustedCeiling {
    /// Writable roots, each with its provenance. Empty means no writable root.
    pub roots: Vec<WriteRoot>,
    /// Everything that could not enter the ceiling.
    pub warnings: Vec<CeilingWarning>,
}

/// Compute the ceiling for `cwd` with the real system inputs.
///
/// Call only when the Sandbox applies: this resolves the workspace (a `.git`
/// search) whenever the computed default is needed.
pub fn compute_trusted_ceiling(sandbox: &SandboxSettings, cwd: &Path) -> TrustedCeiling {
    compute_trusted_ceiling_with(sandbox, cwd, &ResolverInputs::system())
}

/// Compute the ceiling with injected inputs.
///
/// - Absent `allow_write`: the computed default (workspace, its verified
///   gitdirs, the temporary directories).
/// - Present list: an override, never an addition; the workspace is not
///   resolved. An explicit `[]`, or a list emptied by omissions, stays empty.
/// - Project list over an absent trusted list
///   (`allow_write_narrows_computed`): intersected with the computed default;
///   entries outside it are reported as `outside_trusted_ceiling`.
///
/// Nothing here is a startup error: every rejected path becomes a warning.
pub fn compute_trusted_ceiling_with(
    sandbox: &SandboxSettings,
    cwd: &Path,
    inputs: &ResolverInputs,
) -> TrustedCeiling {
    let mut out = TrustedCeiling::default();
    match &sandbox.allow_write {
        Some(list) if !sandbox.allow_write_narrows_computed => {
            out.roots = valid_entries(list, &mut out.warnings);
        }
        Some(list) => {
            let entries = valid_entries(list, &mut out.warnings);
            let default = computed_default(cwd, inputs, &mut out.warnings);
            for entry in &entries {
                if !default.iter().any(|root| contains(&root.path, &entry.path)) {
                    out.warnings
                        .push(omitted(CeilingOmitReason::OutsideTrustedCeiling, entry));
                }
            }
            out.roots = intersect(&default, &entries);
        }
        None => out.roots = computed_default(cwd, inputs, &mut out.warnings),
    }
    out
}

fn omitted(reason: CeilingOmitReason, root: &WriteRoot) -> CeilingWarning {
    CeilingWarning::PathOmitted(PathOmitted {
        reason,
        layer: root.layer,
        location: root.location.clone(),
        path: root.path.clone(),
    })
}

/// Entries that are absolute, free of `..`, and exist; the rest become warnings.
fn valid_entries(list: &[WriteRoot], warnings: &mut Vec<CeilingWarning>) -> Vec<WriteRoot> {
    let mut kept = Vec::new();
    for entry in list {
        let reason = if !entry.path.is_absolute() {
            Some(CeilingOmitReason::Relative)
        } else if has_parent_dir(&entry.path) {
            Some(CeilingOmitReason::ParentDir)
        } else if fs::metadata(&entry.path).is_err() {
            Some(CeilingOmitReason::NotFound)
        } else {
            None
        };
        match reason {
            Some(reason) => warnings.push(omitted(reason, entry)),
            None => kept.push(entry.clone()),
        }
    }
    kept
}

fn computed_default(
    cwd: &Path,
    inputs: &ResolverInputs,
    warnings: &mut Vec<CeilingWarning>,
) -> Vec<WriteRoot> {
    let computed = |path: &Path, location: &str| {
        WriteRoot::new(path.to_path_buf(), CeilingLayer::Computed, location)
    };
    let mut roots = Vec::new();
    match resolve_workspace_with(cwd, inputs) {
        WorkspaceResolution::Resolved(ws) => {
            roots.push(computed(&ws.root, "workspace"));
            roots.extend(ws.gitdirs.iter().map(|g| computed(g, "workspace gitdir")));
            if let Some(claimed) = &ws.unverified_gitdir {
                warnings.push(omitted(
                    CeilingOmitReason::GitdirUnverified,
                    &computed(claimed, "workspace"),
                ));
            }
        }
        WorkspaceResolution::Unresolved(reason) => {
            warnings.push(CeilingWarning::WorkspaceUnresolved {
                reason,
                cwd: cwd.to_path_buf(),
            });
        }
    }
    for dir in &inputs.temp_dirs {
        let root = computed(dir, "temporary directory");
        if fs::metadata(dir).is_ok() {
            roots.push(root);
        } else {
            warnings.push(omitted(CeilingOmitReason::NotFound, &root));
        }
    }
    roots
}
