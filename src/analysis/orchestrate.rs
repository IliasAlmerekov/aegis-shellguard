//! Parent-side language analysis orchestration (ADR-022 §2/§6/§7, L1
//! Iteration 6 Slice C).
//!
//! [`run`] routes a command's analyzable source targets, spawns the ephemeral
//! worker, and drains the parent-owned [`AnalysisQueue`]: pop a target →
//! analyze it via one `Request::Analyze` → map the response through the
//! in-process [`map_adapter_result`] → push any recursive targets back onto
//! the queue → repeat until the queue empties or a budget cap fires. Per-target
//! [`LanguageAnalysisResult`]s are folded into the baseline [`Assessment`]
//! via a single aggregated [`merge_analysis`] call.
//!
//! Iteration 9 wires this orchestration into live planning for every routed
//! target, including bounded script-file reads, typed unresolved-source
//! degradation, trusted aliases, and effective queue/session budgets. One
//! worker is spawned per queue pop (≤ `max_targets` spawns per session):
//! `Worker::analyze` closes stdin and reaps the child every session, so worker
//! reuse across pops remains a deferred performance optimization.

use std::path::Path;
use std::time::{Duration, Instant};

#[cfg(feature = "language-analysis")]
use aegis_language::SourceLanguage;
#[cfg(feature = "language-analysis")]
use aegis_language::protocol::Response;
#[cfg(feature = "language-analysis")]
use aegis_types::SourceOrigin;
use aegis_types::{
    AnalysisStatus, Assessment, DegradationReason, LanguageAnalysisResult, merge_analysis,
};

use super::AnalysisCwd;
#[cfg(feature = "language-analysis")]
use super::mapping::map_adapter_result;
#[cfg(feature = "language-analysis")]
use super::queue::{AnalysisQueue, QueueBudget, QueueTarget};
use super::router::{Resolution, RoutedTarget, resolve_for_analysis, route_with_home};
#[cfg(feature = "language-analysis")]
use super::shell_scan::{fold_result, scan_shell_source};
#[cfg(feature = "language-analysis")]
use super::worker_client::{RequestKind, TargetRequest, TargetResult, Worker, WorkerError};
use aegis_scanner::Scanner;

/// The outcome of [`run`]: distinguishes "no routed analysis targets — no
/// subprocess spawned" from "the worker ran and results were folded in".
#[derive(Debug, Clone)]
pub enum Outcome {
    /// `route` yielded no target. No subprocess was spawned;
    /// the `baseline` is returned unchanged (`analysis` still `None`). This is
    /// the ADR-022 §0 contract: language analysis never starts without an
    /// analyzable source target.
    NotStarted {
        /// The baseline `Assessment`, untouched.
        baseline: Assessment,
    },
    /// Routing started and analysis or typed degradation was produced.
    /// Per-target [`LanguageAnalysisResult`]s have been folded into `baseline`
    /// via a single aggregated [`merge_analysis`] call.
    Analyzed {
        /// The merged `Assessment` (baseline + aggregated language analysis).
        assessment: Assessment,
        /// The number of targets analyzed, top-level + recursive (one
        /// `Request::Analyze` each, one worker spawn each).
        target_count: usize,
    },
}

/// Effective, hard-bounded budgets for one language-analysis session.
#[derive(Debug, Clone, Copy)]
pub struct OrchestrationBudget {
    /// Maximum bytes accepted from one inline source body.
    pub inline_source_limit_bytes: usize,
    /// Maximum bytes read from one script file.
    pub script_file_limit_bytes: u64,
    /// Maximum top-level script files inspected.
    pub max_script_files: usize,
    /// Maximum recursive depth.
    pub max_depth: u32,
    /// Maximum distinct top-level and recursive targets.
    pub max_targets: usize,
    /// Maximum aggregate source bytes.
    pub max_aggregate_bytes: usize,
    /// Total wall-clock budget for the complete command.
    pub total_timeout: Duration,
}

impl OrchestrationBudget {
    /// ADR-022 §7 initial bounded defaults.
    pub const L1_DEFAULT: Self = Self {
        inline_source_limit_bytes: 16 * 1024,
        script_file_limit_bytes: 256 * 1024,
        max_script_files: 8,
        max_depth: 8,
        max_targets: 16,
        max_aggregate_bytes: 1024 * 1024,
        total_timeout: Duration::from_millis(100),
    };
}

/// Run language analysis with the effective configuration budgets.
///
/// Resolves relative targets against the Aegis process working directory — a
/// convenience for callers with no separate command working directory. Production
/// callers pass their own [`AnalysisCwd`] to [`run_with_budget_in_cwd`], because an
/// unresolved command working directory must degrade rather than silently mean `.`
/// (ADR-022 §6). Shell/Bash targets are scanned with the built-in Scanner;
/// production callers pass their config-aware Scanner instead.
pub async fn run_with_budget(
    command: &str,
    baseline: &Assessment,
    aegis_path: Option<&str>,
    trusted_aliases: &[(&str, &str)],
    budget: OrchestrationBudget,
) -> Outcome {
    let builtin_scanner = aegis_scanner::scanner_for(&[])
        .inspect_err(|error| {
            tracing::warn!(%error, "built-in Scanner unavailable; Bash targets degrade");
        })
        // None degrades Bash targets to GrammarUnavailable instead of trusting the narrower adapter.
        .ok();
    run_with_budget_in_cwd(
        command,
        AnalysisCwd::Resolved(Path::new(".")),
        baseline,
        aegis_path,
        trusted_aliases,
        budget,
        builtin_scanner.as_deref(),
    )
    .await
}

/// Run language analysis with effective budgets and a command working directory.
///
/// Relative script-file and direct-exec targets are resolved against
/// `command_cwd`; absolute paths and inline sources are unaffected.
///
/// `shell_scanner` is applied to the source of every Shell/Bash target (see
/// [`scan_shell_source`]). With `None`, a Bash target records
/// [`DegradationReason::GrammarUnavailable`] instead of trusting the Bash
/// adapter's narrower view of the script.
pub async fn run_with_budget_in_cwd(
    command: &str,
    command_cwd: AnalysisCwd<'_>,
    baseline: &Assessment,
    aegis_path: Option<&str>,
    trusted_aliases: &[(&str, &str)],
    budget: OrchestrationBudget,
    shell_scanner: Option<&Scanner>,
) -> Outcome {
    let session_deadline = Instant::now() + budget.total_timeout;
    let home = aegis_config::home_dir();
    let routed = route_with_home(command, trusted_aliases, home.as_deref());
    if routed.is_empty() {
        return Outcome::NotStarted {
            baseline: baseline.clone(),
        };
    }
    let mut unresolved_execution = routed.iter().any(|target| {
        matches!(
            target,
            RoutedTarget::Unresolved {
                reason: DegradationReason::DynamicSource
            } | RoutedTarget::Dynamic {
                reason: DegradationReason::DynamicSource,
                ..
            }
        )
    });

    #[cfg(feature = "language-analysis")]
    let mut queue = AnalysisQueue::new(QueueBudget {
        max_depth: budget.max_depth,
        max_targets: budget.max_targets,
        max_aggregate_bytes: budget.max_aggregate_bytes,
        deadline: Some(session_deadline),
    });
    #[cfg(not(feature = "language-analysis"))]
    let mut any_resolved = false;
    let mut per_target: Vec<LanguageAnalysisResult> = Vec::new();
    let mut script_files = 0usize;
    // Speculative `LauncherOperand` candidates stay off `max_script_files`; 4x caps stat/shebang I/O (#437).
    let mut launcher_operand_candidates = 0usize;
    let max_launcher_operand_candidates = budget.max_script_files.saturating_mul(4);

    for target in routed {
        #[cfg(feature = "language-analysis")]
        let (origin, file_path);
        let (is_script_file, is_launcher_operand) = match &target {
            RoutedTarget::Unresolved { reason } => {
                per_target.push(degraded(*reason));
                continue;
            }
            RoutedTarget::Inline { source, .. } => {
                if source.len() > budget.inline_source_limit_bytes {
                    per_target.push(degraded(DegradationReason::LimitExceeded));
                    continue;
                }
                #[cfg(feature = "language-analysis")]
                {
                    (origin, file_path) = (SourceOrigin::Inline, None);
                }
                (false, false)
            }
            RoutedTarget::ScriptFile { path, .. } | RoutedTarget::DirectExec { path } => {
                #[cfg(feature = "language-analysis")]
                {
                    (origin, file_path) = (
                        SourceOrigin::ScriptFile,
                        Some(path.to_string_lossy().into_owned()),
                    );
                }
                #[cfg(not(feature = "language-analysis"))]
                // Path provenance is recorded only by the language-analysis worker path.
                let _ = path;
                (true, false)
            }
            RoutedTarget::LauncherOperand { path } => {
                launcher_operand_candidates += 1;
                if launcher_operand_candidates > max_launcher_operand_candidates {
                    per_target.push(degraded(DegradationReason::LimitExceeded));
                    continue;
                }
                #[cfg(feature = "language-analysis")]
                {
                    (origin, file_path) = (
                        SourceOrigin::ScriptFile,
                        Some(path.to_string_lossy().into_owned()),
                    );
                }
                #[cfg(not(feature = "language-analysis"))]
                // Path provenance is recorded only by the language-analysis worker path.
                let _ = path;
                (false, true)
            }
            RoutedTarget::Dynamic { .. } => {
                #[cfg(feature = "language-analysis")]
                {
                    (origin, file_path) = (SourceOrigin::Stdin, None);
                }
                (false, false)
            }
        };
        if is_script_file {
            script_files += 1;
            if script_files > budget.max_script_files {
                per_target.push(degraded(DegradationReason::LimitExceeded));
                continue;
            }
        }

        let Some(remaining) = session_deadline.checked_duration_since(Instant::now()) else {
            per_target.push(degraded(DegradationReason::LimitExceeded));
            break;
        };
        let resolution = match tokio::time::timeout(
            remaining,
            resolve_for_analysis(target, command_cwd, budget.script_file_limit_bytes),
        )
        .await
        {
            Ok(resolution) => resolution,
            Err(_) => {
                per_target.push(degraded(DegradationReason::LimitExceeded));
                break;
            }
        };
        match resolution {
            Resolution::Resolved {
                #[cfg(feature = "language-analysis")]
                language,
                #[cfg(feature = "language-analysis")]
                source,
                #[cfg(feature = "language-analysis")]
                source_hash,
                #[cfg(feature = "language-analysis")]
                source_byte_offset,
                ..
            } => {
                // A launcher operand only proves itself a real script here
                // (verified shebang); only now does it compete with
                // ScriptFile/DirectExec for the same max_script_files budget.
                if is_launcher_operand {
                    script_files += 1;
                    if script_files > budget.max_script_files {
                        per_target.push(degraded(DegradationReason::LimitExceeded));
                        continue;
                    }
                }
                #[cfg(feature = "language-analysis")]
                push_with_degradation(
                    &mut queue,
                    QueueTarget::new(language, source, 0)
                        .with_source_hash(source_hash)
                        .with_source_byte_offset(source_byte_offset)
                        .with_provenance(origin, file_path),
                    &mut per_target,
                );
                #[cfg(not(feature = "language-analysis"))]
                {
                    any_resolved = true;
                }
            }
            Resolution::Degraded(reason) => {
                unresolved_execution |= reason == DegradationReason::DynamicSource;
                per_target.push(degraded(reason));
            }
            // A direct executable without a verified shebang is not an
            // analyzable source target and does not claim safety.
            Resolution::NotApplicable => {}
        }
    }

    #[cfg(not(feature = "language-analysis"))]
    {
        // Only the language-analysis worker path uses aegis_path and shell_scanner.
        let _ = (aegis_path, shell_scanner);
        if per_target.is_empty() {
            if any_resolved {
                let mut assessment = baseline.clone();
                assessment.effect_opaque = true;
                return Outcome::Analyzed {
                    assessment,
                    target_count: 0,
                };
            }
            return Outcome::NotStarted {
                baseline: baseline.clone(),
            };
        }
        unresolved_execution |= any_resolved;
    }

    #[cfg(feature = "language-analysis")]
    {
        // Worker::analyze closes stdin and reaps the child per session, so each pop spawns one worker.
        while let Some(target) = queue.pop() {
            let Some(remaining) = session_deadline.checked_duration_since(Instant::now()) else {
                per_target.push(degraded(DegradationReason::LimitExceeded));
                break;
            };
            let request = TargetRequest {
                request_id: 1,
                language: target.language,
                source: target.source.as_bytes().to_vec(),
                kind: RequestKind::Analyze,
            };
            let results: Vec<TargetResult> = match Worker::spawn(aegis_path).await {
                Ok(mut worker) => worker.analyze(vec![request], remaining).await,
                // Spawn failure degrades as WorkerFailure rather than claiming safety (ADR-022 §2).
                Err(_) => vec![TargetResult::Failed(WorkerError::Closed)],
            };
            let result = results
                .into_iter()
                .next()
                .unwrap_or(TargetResult::Failed(WorkerError::Closed));
            let (mut analysis, recursive_targets) = map_target_result(result, &target);
            if target.language == SourceLanguage::Bash {
                let shell_scan = match shell_scanner {
                    Some(scanner) => scan_shell_source(scanner, &target.source),
                    None => degraded(DegradationReason::GrammarUnavailable),
                };
                fold_result(&mut analysis, shell_scan);
            }
            per_target.push(analysis);
            for recursive in recursive_targets {
                push_with_degradation(&mut queue, recursive, &mut per_target);
            }
        }
    }

    // Merge once so earlier targets' degradation reasons are not overwritten.
    let aggregated = aggregate(&per_target);
    let mut assessment = merge_analysis(baseline, &aggregated);
    assessment.effect_opaque |= unresolved_execution;
    Outcome::Analyzed {
        assessment,
        target_count: per_target.len(),
    }
}

fn degraded(reason: DegradationReason) -> LanguageAnalysisResult {
    LanguageAnalysisResult {
        status: AnalysisStatus::Degraded,
        matches: Vec::new(),
        degradation_reasons: vec![reason],
    }
}

/// Push `target` onto `queue`, recording a [`DegradationReason::LimitExceeded`]
/// degradation into `per_target` when a budget cap rejects it (ADR-022 §7:
/// preserve Matches already produced, record the limit). A duplicate or an
/// acceptance records nothing — a duplicate was already analyzed, an
/// acceptance is normal flow.
#[cfg(feature = "language-analysis")]
fn push_with_degradation(
    queue: &mut AnalysisQueue,
    target: QueueTarget,
    per_target: &mut Vec<LanguageAnalysisResult>,
) {
    if let Some(reason) = queue.push(target).degradation_reason() {
        per_target.push(LanguageAnalysisResult {
            status: AnalysisStatus::Degraded,
            matches: Vec::new(),
            degradation_reasons: vec![reason],
        });
    }
}

/// Map one worker [`TargetResult`] into a per-target [`LanguageAnalysisResult`]
/// and the recursive targets its literal execution-sink payloads produced.
///
/// - `Responded(Analyzed{result})` → [`map_adapter_result`] with the target's
///   own `depth` and `source_hash` so recursive targets land at `depth + 1`
///   with correct provenance. Returns `(analysis, recursive_targets)`.
/// - `Responded(UnsupportedLanguage)` → `Degraded` with `[GrammarUnavailable]`.
/// - `Responded(Parsed|ParseFailed)` (defensive — unreachable when sending
///   `Analyze`) → `Degraded` with `[WorkerFailure]`.
/// - `Failed(WorkerError)` → `Degraded` with `[WorkerFailure]`.
#[cfg(feature = "language-analysis")]
fn map_target_result(
    result: TargetResult,
    target: &QueueTarget,
) -> (LanguageAnalysisResult, Vec<QueueTarget>) {
    match result {
        TargetResult::Responded(Response::Analyzed { result }) => {
            let mut outcome = map_adapter_result(
                &result,
                target.language,
                target.source_origin,
                target.file_path.clone(),
                Some(target.source_hash.clone()),
                target.depth,
            );
            if target.source_byte_offset > 0 {
                for matched in &mut outcome.analysis.matches {
                    if let aegis_types::MatchEvidence::LanguageRule { provenance, .. } =
                        &mut matched.evidence
                        && let Some(span) = &mut provenance.span
                    {
                        span.byte_start += target.source_byte_offset;
                        span.byte_end += target.source_byte_offset;
                        if span.line == 1 {
                            span.column += target.source_byte_offset as u32;
                        }
                    }
                }
            }
            (outcome.analysis, outcome.recursive_targets)
        }
        TargetResult::Responded(Response::UnsupportedLanguage) => (
            LanguageAnalysisResult {
                status: AnalysisStatus::Degraded,
                matches: Vec::new(),
                degradation_reasons: vec![DegradationReason::GrammarUnavailable],
            },
            Vec::new(),
        ),
        // Defensive: Parsed/ParseFailed are unreachable when sending Analyze,
        // since the worker routes Analyze to Analyzed/UnsupportedLanguage only.
        TargetResult::Responded(_) => (
            LanguageAnalysisResult {
                status: AnalysisStatus::Degraded,
                matches: Vec::new(),
                degradation_reasons: vec![DegradationReason::WorkerFailure],
            },
            Vec::new(),
        ),
        TargetResult::Failed(err) => (
            LanguageAnalysisResult {
                status: AnalysisStatus::Degraded,
                matches: Vec::new(),
                degradation_reasons: vec![DegradationReason::from(err)],
            },
            Vec::new(),
        ),
    }
}

/// Aggregate per-target results into ONE [`LanguageAnalysisResult`] for a
/// single [`merge_analysis`] call.
///
/// Matches are concatenated; degradation reasons are deduplicated across
/// targets; status is `Degraded` if any target degraded, else `Complete` if
/// any target produced a match, else `NotApplicable`. This preserves earlier
/// targets' reasons rather than letting a per-target `merge_analysis` clobber
/// them (D3).
fn aggregate(per_target: &[LanguageAnalysisResult]) -> LanguageAnalysisResult {
    let mut matches = Vec::new();
    let mut reasons: Vec<DegradationReason> = Vec::new();
    let mut any_degraded = false;
    for r in per_target {
        matches.extend(r.matches.iter().cloned());
        for reason in &r.degradation_reasons {
            if !reasons.contains(reason) {
                reasons.push(*reason);
            }
        }
        if r.status == AnalysisStatus::Degraded {
            any_degraded = true;
        }
    }
    let status = if any_degraded {
        AnalysisStatus::Degraded
    } else if !matches.is_empty() {
        AnalysisStatus::Complete
    } else {
        AnalysisStatus::NotApplicable
    };
    LanguageAnalysisResult {
        status,
        matches,
        degradation_reasons: reasons,
    }
}
