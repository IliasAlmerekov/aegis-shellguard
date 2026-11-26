use std::fs;
use std::path::{Path, PathBuf};

use super::workspace::{
    ResolverInputs, WorkspaceResolution, WorkspaceUnresolvedReason, resolve_workspace_with,
};
use super::{CeilingLayer, WriteRoot, contains, has_parent_dir, intersect};
use crate::model::SandboxSettings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CeilingOmitReason {
    Relative,
    ParentDir,
    NotFound,
    GitdirUnverified,
    OutsideTrustedCeiling,
}

impl CeilingOmitReason {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathOmitted {
    pub reason: CeilingOmitReason,
    pub layer: CeilingLayer,
    pub location: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CeilingWarning {
    PathOmitted(PathOmitted),
    ProjectEntryClamped {
        path: PathBuf,
        location: String,
    },
    WorkspaceUnresolved {
        reason: WorkspaceUnresolvedReason,
        cwd: PathBuf,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TrustedCeiling {
    pub roots: Vec<WriteRoot>,
    pub trusted: Vec<WriteRoot>,
    pub warnings: Vec<CeilingWarning>,
}

pub fn compute_trusted_ceiling(sandbox: &SandboxSettings, cwd: &Path) -> TrustedCeiling {
    compute_trusted_ceiling_with(sandbox, cwd, &ResolverInputs::system())
}

pub fn compute_trusted_ceiling_with(
    sandbox: &SandboxSettings,
    cwd: &Path,
    inputs: &ResolverInputs,
) -> TrustedCeiling {
    let mut out = TrustedCeiling::default();
    match &sandbox.allow_write {
        Some(list) if !sandbox.allow_write_narrows_computed => {
            out.roots = valid_entries(list, &mut out.warnings);
            out.trusted = match &sandbox.allow_write_trusted {
                Some(base) => {
                    let mut base_warnings = Vec::new();
                    let kept = valid_entries(base, &mut base_warnings);
                    for warning in base_warnings {
                        if !out.warnings.contains(&warning) {
                            out.warnings.push(warning);
                        }
                    }
                    kept
                }
                None => out.roots.clone(),
            };
        }
        Some(list) => {
            let entries = valid_entries(list, &mut out.warnings);
            let default = computed_default(cwd, inputs, &mut out.warnings);
            for entry in &entries {
                if !default.iter().any(|root| contains(&root.path, &entry.path)) {
                    out.warnings.push(CeilingWarning::ProjectEntryClamped {
                        path: entry.path.clone(),
                        location: entry.location.clone(),
                    });
                }
            }
            out.roots = intersect(&default, &entries);
            out.trusted = default;
        }
        None => {
            out.roots = computed_default(cwd, inputs, &mut out.warnings);
            out.trusted = out.roots.clone();
        }
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
