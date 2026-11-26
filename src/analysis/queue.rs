use std::collections::HashSet;
use std::time::Instant;

use aegis_language::SourceLanguage;
use aegis_types::{DegradationReason, SourceOrigin};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueBudget {
    pub max_depth: u32,
    pub max_targets: usize,
    pub max_aggregate_bytes: usize,
    pub deadline: Option<Instant>,
}

impl QueueBudget {
    pub const L1_DEFAULT: QueueBudget = QueueBudget {
        max_depth: 8,
        max_targets: 16,
        max_aggregate_bytes: 1024 * 1024,
        deadline: None,
    };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueTarget {
    pub language: SourceLanguage,
    pub source: String,
    pub depth: u32,
    pub source_hash: String,
    pub source_origin: SourceOrigin,
    pub file_path: Option<String>,
    pub source_byte_offset: usize,
}

impl QueueTarget {
    #[must_use]
    pub fn new(language: SourceLanguage, source: String, depth: u32) -> Self {
        let source_hash = super::sha256_hex(&Sha256::digest(source.as_bytes()));
        Self {
            language,
            source,
            depth,
            source_hash,
            source_origin: SourceOrigin::Inline,
            file_path: None,
            source_byte_offset: 0,
        }
    }

    #[must_use]
    pub fn with_provenance(
        mut self,
        source_origin: SourceOrigin,
        file_path: Option<String>,
    ) -> Self {
        self.source_origin = source_origin;
        self.file_path = file_path;
        self
    }

    #[must_use]
    pub fn with_source_hash(mut self, source_hash: Option<String>) -> Self {
        if let Some(source_hash) = source_hash {
            self.source_hash = source_hash;
        }
        self
    }

    #[must_use]
    pub fn with_source_byte_offset(mut self, source_byte_offset: usize) -> Self {
        self.source_byte_offset = source_byte_offset;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushOutcome {
    Accepted,
    DuplicateSkipped,
    DepthExceeded,
    CountExceeded,
    BytesExceeded,
    DeadlineExceeded,
}

impl PushOutcome {
    #[must_use]
    pub fn degradation_reason(self) -> Option<DegradationReason> {
        match self {
            PushOutcome::DepthExceeded
            | PushOutcome::CountExceeded
            | PushOutcome::BytesExceeded
            | PushOutcome::DeadlineExceeded => Some(DegradationReason::LimitExceeded),
            PushOutcome::Accepted | PushOutcome::DuplicateSkipped => None,
        }
    }
}

pub struct AnalysisQueue {
    budget: QueueBudget,
    seen: HashSet<(&'static str, String)>,
    pending: Vec<QueueTarget>,
    accepted_count: usize,
    accepted_bytes: usize,
}

impl AnalysisQueue {
    #[must_use]
    pub fn new(budget: QueueBudget) -> Self {
        AnalysisQueue {
            budget,
            seen: HashSet::new(),
            pending: Vec::new(),
            accepted_count: 0,
            accepted_bytes: 0,
        }
    }

    pub fn push(&mut self, target: QueueTarget) -> PushOutcome {
        let key = (target.language.id(), target.source_hash.clone());
        if self.seen.contains(&key) {
            return PushOutcome::DuplicateSkipped;
        }
        if self
            .budget
            .deadline
            .is_some_and(|deadline| Instant::now() > deadline)
        {
            return PushOutcome::DeadlineExceeded;
        }
        if target.depth > self.budget.max_depth {
            return PushOutcome::DepthExceeded;
        }
        if self.accepted_count >= self.budget.max_targets {
            return PushOutcome::CountExceeded;
        }
        let new_bytes = self.accepted_bytes + target.source.len();
        if new_bytes > self.budget.max_aggregate_bytes {
            return PushOutcome::BytesExceeded;
        }

        self.seen.insert(key);
        self.accepted_count += 1;
        self.accepted_bytes = new_bytes;
        self.pending.push(target);
        PushOutcome::Accepted
    }

    pub fn pop(&mut self) -> Option<QueueTarget> {
        self.pending.pop()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    #[must_use]
    pub fn accepted_count(&self) -> usize {
        self.accepted_count
    }

    #[must_use]
    pub fn accepted_bytes(&self) -> usize {
        self.accepted_bytes
    }
}

#[cfg(test)]
#[path = "queue_tests.rs"]
mod tests;
