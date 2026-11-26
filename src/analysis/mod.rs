use std::path::Path;

fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalysisCwd<'a> {
    Resolved(&'a Path),
    Unavailable,
}

#[cfg(feature = "language-analysis")]
mod classifier;
pub mod heredoc;
#[cfg(feature = "language-analysis")]
pub mod mapping;
mod merge;
pub mod orchestrate;
#[cfg(feature = "language-analysis")]
pub mod queue;
#[cfg(feature = "language-analysis")]
pub mod recursive;
pub mod router;
#[cfg(feature = "language-analysis")]
pub mod shell_scan;
pub mod source_reader;
#[cfg(feature = "language-analysis")]
pub mod worker_client;

pub use orchestrate::{OrchestrationBudget, Outcome, run_with_budget, run_with_budget_in_cwd};
#[cfg(feature = "language-analysis")]
pub use worker_client::{
    INTERNAL_LANGUAGE_WORKER_FLAG, RequestKind, TargetRequest, TargetResult, Worker, WorkerError,
    analyze,
};
