use super::{AnalysisQueue, PushOutcome, QueueBudget, QueueTarget};
use aegis_language::SourceLanguage;
use aegis_types::DegradationReason;
use std::time::{Duration, Instant};

fn tgt(lang: SourceLanguage, source: &str, depth: u32) -> QueueTarget {
    QueueTarget::new(lang, source.to_string(), depth)
}

#[test]
fn root_target_is_accepted_and_poppable() {
    let mut q = AnalysisQueue::new(QueueBudget::L1_DEFAULT);
    assert_eq!(
        q.push(tgt(SourceLanguage::Python, "os.remove('a')", 0)),
        PushOutcome::Accepted,
    );
    assert_eq!(q.accepted_count(), 1);
    let popped = q.pop().expect("root target was just pushed");
    assert_eq!(popped.language, SourceLanguage::Python);
    assert!(q.is_empty());
}

#[test]
fn nested_literal_target_recurses() {
    let mut q = AnalysisQueue::new(QueueBudget::L1_DEFAULT);
    q.push(tgt(
        SourceLanguage::Python,
        "subprocess.run(['bash','-c','rm x'])",
        0,
    ));
    let _ = q.pop();
    assert_eq!(
        q.push(tgt(SourceLanguage::Bash, "rm x", 1)),
        PushOutcome::Accepted,
    );
    assert_eq!(q.accepted_count(), 2);
}

#[test]
fn duplicate_target_hash_is_skipped_not_reanalyzed() {
    let mut q = AnalysisQueue::new(QueueBudget::L1_DEFAULT);
    q.push(tgt(SourceLanguage::Python, "os.remove('a')", 0));
    assert_eq!(
        q.push(tgt(SourceLanguage::Python, "os.remove('a')", 0)),
        PushOutcome::DuplicateSkipped,
    );
    assert_eq!(
        q.accepted_count(),
        1,
        "a duplicate must not inflate the accepted count",
    );
}

#[test]
fn cycle_is_broken_by_dedup() {
    let mut q = AnalysisQueue::new(QueueBudget::L1_DEFAULT);
    q.push(tgt(SourceLanguage::Bash, "bash -c 'bash -c x'", 0));
    let _ = q.pop();
    assert_eq!(
        q.push(tgt(SourceLanguage::Bash, "bash -c 'bash -c x'", 1)),
        PushOutcome::DuplicateSkipped,
    );
}

#[test]
fn cross_language_nesting_is_allowed() {
    let mut q = AnalysisQueue::new(QueueBudget::L1_DEFAULT);
    q.push(tgt(SourceLanguage::Python, "shared", 0));
    assert_eq!(
        q.push(tgt(SourceLanguage::Bash, "shared", 1)),
        PushOutcome::Accepted,
    );
    assert_eq!(q.accepted_count(), 2);
}

#[test]
fn depth_ceiling_8_is_enforced() {
    let mut q = AnalysisQueue::new(QueueBudget::L1_DEFAULT);
    assert_eq!(
        q.push(tgt(SourceLanguage::Python, "d8", 8)),
        PushOutcome::Accepted,
    );
    assert_eq!(
        q.push(tgt(SourceLanguage::Python, "d9", 9)),
        PushOutcome::DepthExceeded,
    );
}

#[test]
fn target_count_cap_is_enforced() {
    let budget = QueueBudget {
        max_targets: 2,
        ..QueueBudget::L1_DEFAULT
    };
    let mut q = AnalysisQueue::new(budget);
    assert_eq!(
        q.push(tgt(SourceLanguage::Python, "a", 0)),
        PushOutcome::Accepted
    );
    assert_eq!(
        q.push(tgt(SourceLanguage::Python, "b", 0)),
        PushOutcome::Accepted
    );
    assert_eq!(
        q.push(tgt(SourceLanguage::Python, "c", 0)),
        PushOutcome::CountExceeded,
    );
}

#[test]
fn aggregate_bytes_cap_is_enforced() {
    let budget = QueueBudget {
        max_aggregate_bytes: 5,
        ..QueueBudget::L1_DEFAULT
    };
    let mut q = AnalysisQueue::new(budget);
    assert_eq!(
        q.push(tgt(SourceLanguage::Python, "aaaa", 0)),
        PushOutcome::Accepted
    );
    assert_eq!(
        q.push(tgt(SourceLanguage::Python, "bbb", 0)),
        PushOutcome::BytesExceeded,
    );
    assert_eq!(q.accepted_bytes(), 4);
}

#[test]
fn deadline_in_past_rejects_push() {
    let past = Instant::now()
        .checked_sub(Duration::from_secs(1))
        .expect("now is well after a 1s offset");
    let budget = QueueBudget {
        deadline: Some(past),
        ..QueueBudget::L1_DEFAULT
    };
    let mut q = AnalysisQueue::new(budget);
    assert_eq!(
        q.push(tgt(SourceLanguage::Python, "x", 0)),
        PushOutcome::DeadlineExceeded,
    );
}

#[test]
fn cap_exceeded_outcomes_map_to_limit_exceeded_degradation() {
    assert_eq!(
        PushOutcome::DepthExceeded.degradation_reason(),
        Some(DegradationReason::LimitExceeded),
    );
    assert_eq!(
        PushOutcome::CountExceeded.degradation_reason(),
        Some(DegradationReason::LimitExceeded),
    );
    assert_eq!(
        PushOutcome::BytesExceeded.degradation_reason(),
        Some(DegradationReason::LimitExceeded),
    );
    assert_eq!(
        PushOutcome::DeadlineExceeded.degradation_reason(),
        Some(DegradationReason::LimitExceeded),
    );
    assert_eq!(PushOutcome::DuplicateSkipped.degradation_reason(), None);
    assert_eq!(PushOutcome::Accepted.degradation_reason(), None);
}

#[test]
fn l1_default_budget_matches_adr_022_ceilings() {
    let b = QueueBudget::L1_DEFAULT;
    assert_eq!(b.max_depth, 8, "recursion-depth ceiling is 8");
    assert_eq!(
        b.max_aggregate_bytes,
        1024 * 1024,
        "aggregate-source ceiling is 1 MiB",
    );
    assert!(
        b.deadline.is_none(),
        "deadline is caller-set, not a static default",
    );
    assert!(
        b.max_targets > b.max_depth as usize,
        "max_targets ({}) must exceed max_depth ({}) so depth is the binding recursion guard",
        b.max_targets,
        b.max_depth,
    );
}

#[test]
fn queue_target_hash_matches_source_reader_hex_sha256_format() {
    let t = QueueTarget::new(SourceLanguage::Python, "abc".to_string(), 0);
    assert_eq!(
        t.source_hash, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        "SHA-256 of \"abc\" in lowercase hex",
    );
}

#[test]
fn pop_is_lifo_depth_first() {
    let mut q = AnalysisQueue::new(QueueBudget::L1_DEFAULT);
    q.push(tgt(SourceLanguage::Python, "first", 0));
    q.push(tgt(SourceLanguage::Bash, "second", 0));
    assert_eq!(
        q.pop().expect("two targets were pushed").source,
        "second",
        "LIFO: the last pushed target pops first",
    );
    assert_eq!(q.pop().expect("one target remains").source, "first",);
    assert!(q.is_empty());
}

#[test]
fn cap_rejected_target_does_not_poison_dedup() {
    let mut q = AnalysisQueue::new(QueueBudget::L1_DEFAULT);
    assert_eq!(
        q.push(tgt(SourceLanguage::Python, "rm x", 9)),
        PushOutcome::DepthExceeded,
    );
    assert_eq!(
        q.push(tgt(SourceLanguage::Python, "rm x", 0)),
        PushOutcome::Accepted,
        "a cap-rejected target must not preclude the same source at a valid depth",
    );
    assert_eq!(q.accepted_count(), 1);
}
