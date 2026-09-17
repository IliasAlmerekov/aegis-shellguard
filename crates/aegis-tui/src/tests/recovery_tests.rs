use super::*;
use std::io::{self, Write};

use aegis_types::RecoveryDegradation;

const NO_SNAPSHOT: RecoveryDegradation = RecoveryDegradation::NoSnapshotAvailable;
const PARTIAL: RecoveryDegradation = RecoveryDegradation::PartialSnapshotCoverage;

struct FailingWriter;

#[derive(Default)]
struct FlushFailingWriter(Vec<u8>);

impl Write for FailingWriter {
    fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("render failed"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::other("flush failed"))
    }
}

impl Write for FlushFailingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::other("flush failed"))
    }
}

#[test]
fn recovery_prompt_run_once_choice_is_not_persistable() {
    let decision =
        show_recovery_override_with_input(NO_SNAPSHOT, true, &mut b"r\n".as_ref(), &mut Vec::new());

    assert_eq!(decision, RecoveryPromptDecision::RunOnceWithoutRecovery);
}

#[test]
fn recovery_prompt_explains_missing_required_recovery() {
    let mut output = Vec::new();
    let _ = show_recovery_override_with_input(NO_SNAPSHOT, true, &mut b"n\n".as_ref(), &mut output);
    let text = strip_ansi(&String::from_utf8_lossy(&output));

    assert!(
        text.contains("could not determine the eventual effect")
            && text.contains("No required Snapshot was created")
            && text.contains("without the ADR-016 recovery backstop"),
        "Recovery prompt must explain the complete degradation: {text}"
    );
}

#[test]
fn recovery_prompt_does_not_claim_zero_snapshots_on_partial_coverage() {
    let mut output = Vec::new();
    let _ = show_recovery_override_with_input(PARTIAL, true, &mut b"n\n".as_ref(), &mut output);
    let text = strip_ansi(&String::from_utf8_lossy(&output));

    assert!(
        text.contains("At least one applicable snapshot provider failed")
            && !text.contains("No required Snapshot was created"),
        "Partial coverage must not be reported as a total snapshot failure: {text}"
    );
}

#[test]
fn recovery_prompt_does_not_offer_persisted_approval() {
    let decision = show_recovery_override_with_input(
        NO_SNAPSHOT,
        true,
        &mut b"always\n".as_ref(),
        &mut Vec::new(),
    );

    assert_eq!(decision, RecoveryPromptDecision::Deny);
}

#[test]
fn noninteractive_recovery_degradation_denies_even_with_run_input() {
    let decision = show_recovery_override_with_input(
        NO_SNAPSHOT,
        false,
        &mut b"r\n".as_ref(),
        &mut Vec::new(),
    );

    assert_eq!(decision, RecoveryPromptDecision::Deny);
}

#[test]
fn recovery_prompt_render_failure_denies() {
    let decision = show_recovery_override_with_input(
        NO_SNAPSHOT,
        true,
        &mut b"r\n".as_ref(),
        &mut FailingWriter,
    );

    assert_eq!(decision, RecoveryPromptDecision::Deny);
}

#[test]
fn recovery_prompt_flush_failure_denies() {
    let decision = show_recovery_override_with_input(
        NO_SNAPSHOT,
        true,
        &mut b"r\n".as_ref(),
        &mut FlushFailingWriter::default(),
    );

    assert_eq!(decision, RecoveryPromptDecision::Deny);
}
