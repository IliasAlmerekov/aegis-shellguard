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

#[derive(Debug, Clone)]
pub enum Outcome {
    NotStarted {
        baseline: Assessment,
    },
    Analyzed {
        assessment: Assessment,
        target_count: usize,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct OrchestrationBudget {
    pub inline_source_limit_bytes: usize,
    pub script_file_limit_bytes: u64,
    pub max_script_files: usize,
    pub max_depth: u32,
    pub max_targets: usize,
    pub max_aggregate_bytes: usize,
    pub total_timeout: Duration,
}

impl OrchestrationBudget {
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
            Resolution::NotApplicable => {}
        }
    }

    #[cfg(not(feature = "language-analysis"))]
    {
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
