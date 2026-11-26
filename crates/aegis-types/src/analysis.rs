use serde::{Deserialize, Serialize};

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(description = "Which detection mechanism produced a `Match`.

The three concrete mechanisms of the common Detection rule contract:
regex `Pattern`, `Token-prefix rule`, and Language-aware
rule. A `Match` always identifies exactly one mechanism.")]
pub enum DetectionMechanism {
    #[schemars(description = "A regex `Pattern` matched anywhere in the normalized command.")]
    RegexPattern,
    #[schemars(
        description = "A `Token-prefix rule` matched the command's `Effective program` + token prefix."
    )]
    TokenPrefixRule,
    #[schemars(
        description = "A built-in Language-aware rule matched via structural source analysis."
    )]
    LanguageRule,
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(
    description = "Whether a detection rule is built into Aegis or came from user config.

Distinct from `crate::PatternSource` (which lives on the legacy `Pattern`
struct): this is the common per-`Match` \"built in or custom\" flag every
`Match` must carry."
)]
pub enum DetectionSource {
    #[schemars(description = "Shipped with Aegis.")]
    Builtin,
    #[schemars(description = "Loaded from user configuration.")]
    Custom,
}

impl From<crate::PatternSource> for DetectionSource {
    fn from(source: crate::PatternSource) -> Self {
        match source {
            crate::PatternSource::Builtin => DetectionSource::Builtin,
            crate::PatternSource::Custom => DetectionSource::Custom,
        }
    }
}

#[non_exhaustive]
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Deserialize,
    Serialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[schemars(
    description = "How completely a `Detected operation`'s operand is known to static analysis.

Ordered by *decreasing* certainty: `Known < Partial < Dynamic`. A `Dynamic`
operand is never treated as evidence of safety — it records Analysis
degradation in addition to the visible operation."
)]
pub enum OperandCertainty {
    #[schemars(description = "The operand is a literal recoverable from the source.")]
    Known,
    #[schemars(
        description = "The operand is partially resolved (e.g. an alias or adjacent literal)."
    )]
    Partial,
    #[schemars(
        description = "The operand is computed, imported, or otherwise not statically recoverable."
    )]
    Dynamic,
}

#[non_exhaustive]
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Deserialize,
    Serialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[schemars(description = "The state of language-aware analysis for one target.

Ordered by *increasing* degradation: `NotApplicable < Complete < Degraded`,
so `max` of a set of statuses is the worst (most degraded) one, as required
by monotonic merge.")]
pub enum AnalysisStatus {
    #[schemars(
        description = "No language-aware analysis applies to this target (no analyzable source)."
    )]
    NotApplicable,
    #[schemars(description = "Analysis ran to completion with no degradation.")]
    Complete,
    #[schemars(
        description = "Analysis ran but produced typed degradation; prior results are retained."
    )]
    Degraded,
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(description = "Why language-aware analysis degraded for a target.

Variants mirror the seven degradation buckets. The
enum is `#[non_exhaustive]` so adapters may surface finer-grained reasons
without breaking serialization consumers.")]
pub enum DegradationReason {
    #[schemars(description = "The grammar for the target language is unsupported or unavailable.")]
    GrammarUnavailable,
    #[schemars(
        description = "The source could not be fully parsed (malformed or incomplete syntax)."
    )]
    IncompleteSyntax,
    #[schemars(
        description = "The source was unsafe or unavailable to read (symlink, FIFO, permissions, …)."
    )]
    UnsafeSource,
    #[schemars(description = "The source used an unsupported encoding (invalid UTF-8, UTF-16, …).")]
    UnsupportedEncoding,
    #[schemars(description = "A size, file-count, recursion-depth, or timeout limit was exceeded.")]
    LimitExceeded,
    #[schemars(
        description = "The source or working directory was dynamic and could not be resolved."
    )]
    DynamicSource,
    #[schemars(
        description = "The analysis worker or its protocol failed (crash, timeout, bad frame)."
    )]
    WorkerFailure,
}

impl DegradationReason {
    #[must_use]
    pub fn phrase(self) -> &'static str {
        match self {
            Self::DynamicSource => {
                "a command word, argument, or working directory is only known at run time"
            }
            Self::UnsafeSource => "a source file is missing, unreadable, or unsafe to read",
            Self::LimitExceeded => "a size, depth, or time limit was reached",
            Self::UnsupportedEncoding => "a source file uses an unsupported text encoding",
            Self::IncompleteSyntax => "a script could not be fully parsed",
            Self::GrammarUnavailable => "this build has no parser for the script's syntax",
            Self::WorkerFailure => "the analysis worker failed",
        }
    }
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(
    description = "The kind of destructive effect or execution sink a language-aware rule
detected.

`CodeExecution` is the canonical kind for a recognized process, shell, or
eval sink: it always emits a `CodeExecution` Match regardless of payload
certainty. The enum is `#[non_exhaustive]` so adapters may
add operation kinds without breaking serialization consumers. It carries
no inherent severity ordering — the shared classifier maps kind + modifiers
+ certainty to `Category`/`RiskLevel`, not this enum's declaration order."
)]
pub enum OperationKind {
    #[schemars(
        description = "Recursive or single filesystem deletion (`os.remove`, `fs.rmSync`, …)."
    )]
    FilesystemDelete,
    #[schemars(
        description = "Overwrite or truncation of an existing file (`open('w')`, `truncate`, …)."
    )]
    FilesystemOverwrite,
    #[schemars(
        description = "A dangerous permission or ownership change (`chmod 000`, `os.chown`, …)."
    )]
    PermissionOrOwnershipChange,
    #[schemars(description = "A write to a device file or other critical-path target.")]
    DeviceOrCriticalWrite,
    #[schemars(
        description = "A destructive database operation (`DROP TABLE`, `DELETE` without where, …)."
    )]
    DatabaseDestructive,
    #[schemars(
        description = "A recognized process, shell, or eval sink (`subprocess.run`, `eval`, …)."
    )]
    CodeExecution,
    #[schemars(description = "A destructive cloud-provider API call.")]
    CloudDestructive,
    #[schemars(description = "A destructive container-management operation.")]
    ContainerDestructive,
    #[schemars(description = "A destructive package-manager operation.")]
    PackageDestructive,
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Deserialize, Serialize, schemars::JsonSchema,
)]
#[schemars(description = "Modifiers that refine an [`OperationKind`].

All flags default to `false`; an operation carries only the modifiers that
apply to it.")]
pub struct OperationModifiers {
    #[schemars(description = "The operation is recursive (`rm -r`, `shutil.rmtree`, …).")]
    pub recursive: bool,
    #[schemars(description = "The operation is forced (`rm -f`, `--force`, …).")]
    pub forced: bool,
    #[schemars(
        description = "The operation is in an explicitly destructive mode (e.g. a destructive
open flag or overwrite mode)."
    )]
    pub destructive_mode: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, schemars::JsonSchema)]
#[schemars(
    description = "A language-neutral operation detected from source syntax.

Each adapter emits `DetectedOperation`s rather than assigning `RiskLevel`
directly from an API spelling. A shared classifier maps `kind`, `modifiers`,
and `certainty` into the existing `Category`, `RiskLevel`, explanation,
safer alternative, and `Match` vocabulary. A `Dynamic` operand never
authorizes treating the operation as safe."
)]
pub struct DetectedOperation {
    #[schemars(description = "What effect or execution sink was detected.")]
    pub kind: OperationKind,
    #[schemars(description = "Modifiers refining the kind (recursive, forced, destructive mode).")]
    pub modifiers: OperationModifiers,
    #[schemars(description = "How completely the operand is known to static analysis.")]
    pub certainty: OperandCertainty,
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(
    description = "Where a language-aware analysis target's source came from.

Drives routing and recovery: only `Inline` / `Heredoc` / `ScriptFile` / `Stdin`
/ `Pipe` shapes can become analysis targets, and each carries different
Effect-opaque / Required-recovery consequences."
)]
pub enum SourceOrigin {
    #[schemars(description = "An inline interpreter body (`-c` / `-e`).")]
    Inline,
    #[schemars(description = "A heredoc body supplied to the command.")]
    Heredoc,
    #[schemars(description = "A script file named in argv.")]
    ScriptFile,
    #[schemars(description = "Interpreter stdin.")]
    Stdin,
    #[schemars(description = "A pipe-to-shell sink.")]
    Pipe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize, schemars::JsonSchema)]
#[schemars(description = "A concrete byte span inside analyzed source.

Carries line/column for human display and byte offsets for mapping back to
the original bytes. It references position only — it never carries the
source text itself.")]
pub struct ByteSpan {
    #[schemars(description = "1-based line number.")]
    pub line: u32,
    #[schemars(description = "1-based column number (in bytes).")]
    pub column: u32,
    #[schemars(description = "Inclusive start byte offset within the source.")]
    pub byte_start: usize,
    #[schemars(description = "Exclusive end byte offset within the source.")]
    pub byte_end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, schemars::JsonSchema)]
#[schemars(
    description = "Metadata recording where a language-aware result came from.

Provenance persists metadata only. It MUST NOT carry script contents, full
snippets, imported source, variable values, or syntax trees — the TUI may
render a short in-memory snippet, but provenance never does.
The `privacy_test_*` tests below pin that invariant at the serialization
boundary so a later field addition cannot silently leak source."
)]
pub struct AnalysisProvenance {
    #[schemars(description = "The detected language (e.g. `\"python\"`), when applicable.")]
    pub language: Option<String>,
    #[schemars(description = "Where the analyzed source came from.")]
    pub source_origin: SourceOrigin,
    #[schemars(description = "The stable detection rule ID that fired, when applicable.")]
    pub rule_id: Option<String>,
    #[schemars(description = "The detected operation, when a language-aware rule matched.")]
    pub operation: Option<DetectedOperation>,
    #[schemars(description = "The analyzed file path, when the target was a `ScriptFile`.")]
    pub file_path: Option<String>,
    #[schemars(description = "A hex digest of the original analyzed bytes (never the bytes).")]
    pub source_hash: Option<String>,
    #[schemars(description = "The span in the source the result refers to, when localized.")]
    pub span: Option<ByteSpan>,
    #[schemars(description = "The operand certainty of the result.")]
    pub certainty: OperandCertainty,
    #[schemars(description = "The analysis status for this target.")]
    pub status: AnalysisStatus,
    #[schemars(description = "The degradation reason, when `status` is `Degraded`.")]
    pub degradation_reason: Option<DegradationReason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, schemars::JsonSchema)]
#[schemars(description = "Per-target language-aware analysis result.

One `TargetAnalysis` per analysis target. `status` is ordered by increasing
degradation (`NotApplicable < Complete < Degraded`), so the worst target
drives the merged `Assessment` status. `degradation_reasons` carries zero or
more typed reasons; an empty vector is valid only when `status` is not
`Degraded`.")]
pub struct TargetAnalysis {
    #[schemars(description = "The analysis status for this target.")]
    pub status: AnalysisStatus,
    #[schemars(description = "Typed degradation reasons (non-empty iff `status == Degraded`).")]
    pub degradation_reasons: Vec<DegradationReason>,
    #[schemars(description = "Provenance for this target, when analysis ran.")]
    pub provenance: Option<AnalysisProvenance>,
}

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
#[schemars(description = "Typed evidence carried by every `Match`.

The variant encodes the detection mechanism, so an impossible combination
(e.g. a regex `Pattern` carrying a `DetectedOperation`) is unconstructable.
Only `LanguageRule` carries a `DetectedOperation` and `AnalysisProvenance`;
regex `Pattern` and `Token-prefix rule` matches carry just their source.
Every variant also records whether the rule is built in or custom.

`DetectionMechanism` remains the lightweight standalone tag for consumers
that need only the mechanism (audit projection, deterministic sort);
[`MatchEvidence::mechanism`] projects this enum to that tag.")]
pub enum MatchEvidence {
    #[schemars(description = "A regex `Pattern` matched anywhere in the normalized command.")]
    RegexPattern {
        #[schemars(description = "Whether the pattern is built in or custom.")]
        source: DetectionSource,
    },
    #[schemars(
        description = "A `Token-prefix rule` matched the command's `Effective program` + prefix."
    )]
    TokenPrefixRule {
        #[schemars(description = "Whether the rule is built in or custom.")]
        source: DetectionSource,
    },
    #[schemars(
        description = "A built-in Language-aware rule matched via structural source analysis."
    )]
    LanguageRule {
        #[schemars(
            description = "Whether the rule is built in (Language-aware rules are always built
in — project config cannot define custom Tree-sitter queries —
but the field is retained so the common per-Match
\"built in or custom\" contract is uniform)."
        )]
        source: DetectionSource,
        #[schemars(description = "The detected operation that fired.")]
        operation: DetectedOperation,
        #[schemars(description = "Where the result came from (metadata only — no source body).")]
        provenance: AnalysisProvenance,
    },
}

impl MatchEvidence {
    pub fn mechanism(&self) -> DetectionMechanism {
        match self {
            MatchEvidence::RegexPattern { .. } => DetectionMechanism::RegexPattern,
            MatchEvidence::TokenPrefixRule { .. } => DetectionMechanism::TokenPrefixRule,
            MatchEvidence::LanguageRule { .. } => DetectionMechanism::LanguageRule,
        }
    }

    pub fn source(&self) -> DetectionSource {
        match self {
            MatchEvidence::RegexPattern { source }
            | MatchEvidence::TokenPrefixRule { source }
            | MatchEvidence::LanguageRule { source, .. } => *source,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LanguageAnalysisResult {
    pub status: AnalysisStatus,
    pub matches: Vec<crate::MatchResult>,
    pub degradation_reasons: Vec<DegradationReason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(
    description = "Summary of language-aware analysis carried on a merged `Assessment`.

`status` + `degradation_reasons` are orthogonal to `RiskLevel`:
degradation may coexist with `Safe` and never authorizes auto-execution."
)]
pub struct AnalysisSummary {
    #[schemars(description = "Overall language-aware analysis status.")]
    pub status: AnalysisStatus,
    #[schemars(description = "Typed degradation reasons (non-empty iff `status == Degraded`).")]
    pub degradation_reasons: Vec<DegradationReason>,
}

pub fn merge_analysis(
    baseline: &crate::Assessment,
    language: &LanguageAnalysisResult,
) -> crate::Assessment {
    let mut merged_matched = baseline.matched.clone();
    let baseline_ids: std::collections::HashSet<&str> = baseline
        .matched
        .iter()
        .map(|m| m.pattern.id.as_ref())
        .collect();
    for m in &language.matches {
        if !baseline_ids.contains(m.pattern.id.as_ref()) {
            merged_matched.push(m.clone());
        }
    }

    let risk = merged_matched
        .iter()
        .map(|m| m.pattern.risk)
        .chain(language.matches.iter().map(|m| m.pattern.risk))
        .max()
        .map(|mx| mx.max(baseline.risk))
        .unwrap_or(baseline.risk);

    crate::Assessment {
        risk,
        effect_opaque: baseline.effect_opaque,
        matched: merged_matched,
        highlight_ranges: baseline.highlight_ranges.clone(),
        command: baseline.command.clone(),
        analysis: Some(AnalysisSummary {
            status: language.status,
            degradation_reasons: language.degradation_reasons.clone(),
        }),
    }
}

#[cfg(test)]
mod merge_tests;
#[cfg(test)]
mod tests;

pub mod classifier;

#[cfg(test)]
mod classifier_tests;
