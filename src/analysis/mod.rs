use std::path::Path;
use std::time::Duration;

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

pub use orchestrate::{Outcome, run_with_budget, run_with_budget_in_cwd};
#[cfg(feature = "language-analysis")]
pub use worker_client::{
    INTERNAL_LANGUAGE_WORKER_FLAG, RequestKind, TargetRequest, TargetResult, Worker, WorkerError,
    analyze,
};
