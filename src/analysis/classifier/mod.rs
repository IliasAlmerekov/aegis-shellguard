use std::borrow::Cow;
use std::sync::Arc;

use aegis_types::{
    AnalysisProvenance, Category, DetectedOperation, DetectionSource, HighlightRange,
    LANGUAGE_AWARE_MATCH_LABEL, MatchEvidence, MatchResult, OperationKind, Pattern, PatternSource,
    RiskLevel,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Classification {
    category: Category,
    risk: RiskLevel,
    rule_id: &'static str,
    description: &'static str,
    safe_alt: Option<&'static str>,
}

fn classify(op: &DetectedOperation) -> Classification {
    let kind = op.kind;
    let mods = op.modifiers;

    match kind {
        OperationKind::FilesystemDelete => {
            let (rule_id, risk, description, safe_alt) = if mods.recursive && mods.forced {
                (
                    "LANG-FS-DEL-RF",
                    RiskLevel::Danger,
                    "Recursive, forced filesystem deletion detected in source",
                    Some("Remove a specific path with a bounded, non-recursive call"),
                )
            } else if mods.recursive {
                (
                    "LANG-FS-DEL-R",
                    RiskLevel::Danger,
                    "Recursive filesystem deletion detected in source",
                    Some("Remove a specific path with a bounded, non-recursive call"),
                )
            } else if mods.forced {
                (
                    "LANG-FS-DEL-F",
                    RiskLevel::Warn,
                    "Forced filesystem deletion detected in source",
                    Some("Remove the path without --force and confirm the target first"),
                )
            } else {
                (
                    "LANG-FS-DEL",
                    RiskLevel::Warn,
                    "Filesystem deletion detected in source",
                    Some("Confirm the exact path before deleting"),
                )
            };
            Classification {
                category: Category::Filesystem,
                risk,
                rule_id,
                description,
                safe_alt,
            }
        }
        OperationKind::FilesystemOverwrite => {
            let (rule_id, description) = if mods.destructive_mode {
                (
                    "LANG-FS-OVR-W",
                    "Destructive-mode file overwrite or truncation detected in source",
                )
            } else {
                ("LANG-FS-OVR", "File overwrite detected in source")
            };
            Classification {
                category: Category::Filesystem,
                risk: RiskLevel::Warn,
                rule_id,
                description,
                safe_alt: Some("Write to a new path or back up the existing file first"),
            }
        }
        OperationKind::PermissionOrOwnershipChange => Classification {
            category: Category::Filesystem,
            risk: RiskLevel::Danger,
            rule_id: "LANG-FS-CHMOD",
            description: "Permission or ownership change detected in source",
            safe_alt: Some("Apply the least-privileged mode to a specific path"),
        },
        OperationKind::DeviceOrCriticalWrite => Classification {
            category: Category::Filesystem,
            risk: RiskLevel::Danger,
            rule_id: "LANG-FS-DEV",
            description: "Write to a device file or critical-path target detected in source",
            safe_alt: Some("Avoid raw device writes; use the intended filesystem tool"),
        },
        OperationKind::DatabaseDestructive => Classification {
            category: Category::Database,
            risk: RiskLevel::Danger,
            rule_id: "LANG-DB-DEST",
            description: "Destructive database operation detected in source",
            safe_alt: Some("Run a reversible migration or back up the database first"),
        },
        OperationKind::CloudDestructive => Classification {
            category: Category::Cloud,
            risk: RiskLevel::Danger,
            rule_id: "LANG-CLOUD-DEST",
            description: "Destructive cloud-provider API call detected in source",
            safe_alt: Some("Target a specific named resource and confirm before deletion"),
        },
        OperationKind::ContainerDestructive => Classification {
            category: Category::Docker,
            risk: RiskLevel::Danger,
            rule_id: "LANG-DOCKER-DEST",
            description: "Destructive container-management operation detected in source",
            safe_alt: Some("Remove a specific named container or image after confirmation"),
        },
        OperationKind::PackageDestructive => Classification {
            category: Category::Package,
            risk: RiskLevel::Warn,
            rule_id: "LANG-PKG-DEST",
            description: "Destructive package-manager operation detected in source",
            safe_alt: Some("Remove a specific named package after confirmation"),
        },
        _ => Classification {
            category: Category::Process,
            risk: RiskLevel::Danger,
            rule_id: "LANG-EXEC",
            description: "Process, shell, or eval execution sink detected in source",
            safe_alt: Some("Run a known, bounded command instead of arbitrary code"),
        },
    }
}

#[must_use]
pub(super) fn language_match(
    op: &DetectedOperation,
    provenance: AnalysisProvenance,
    highlight_range: Option<HighlightRange>,
) -> MatchResult {
    let class = classify(op);
    let pattern = Arc::new(Pattern {
        id: Cow::Borrowed(class.rule_id),
        category: class.category,
        risk: class.risk,
        pattern: Cow::Borrowed(""),
        description: Cow::Borrowed(class.description),
        safe_alt: class.safe_alt.map(Cow::Borrowed),
        justification: None,
        source: PatternSource::Builtin,
    });
    MatchResult {
        pattern,
        matched_text: LANGUAGE_AWARE_MATCH_LABEL.to_string(),
        highlight_range,
        evidence: MatchEvidence::LanguageRule {
            source: DetectionSource::Builtin,
            operation: op.clone(),
            provenance,
        },
    }
}

#[cfg(test)]
mod tests;
