//! Compatibility fixtures pinning `Assessment` (`risk`, matched pattern ID,
//! `DecisionSource`, and `effect_opaque`) for a representative command corpus (ADR-022 §4).

use super::*;

/// Expected scanner output for one representative command.
struct Case {
    /// The raw command under test.
    cmd: &'static str,
    /// The expected `RiskLevel`.
    risk: RiskLevel,
    /// A pattern ID that must be among the matched patterns (`None` ⇒ no match).
    key_id: Option<&'static str>,
    /// The expected `DecisionSource`.
    source: DecisionSource,
    /// The expected `effect_opaque` flag.
    effect_opaque: bool,
}

const CASES: &[Case] = &[
    // Safe — no keyword hits, quick scan returns false immediately.
    Case {
        cmd: "echo hello",
        risk: RiskLevel::Safe,
        key_id: None,
        source: DecisionSource::Fallback,
        effect_opaque: false,
    },
    // Danger — regex `Pattern` (filesystem recursive force delete, FS-001).
    Case {
        cmd: "rm -rf /tmp/build",
        risk: RiskLevel::Danger,
        key_id: Some("FS-001"),
        source: DecisionSource::BuiltinPattern,
        effect_opaque: false,
    },
    // Warn — `Token-prefix rule` (git reset --hard, GIT-001).
    Case {
        cmd: "git reset --hard HEAD~1",
        risk: RiskLevel::Warn,
        key_id: Some("GIT-001"),
        source: DecisionSource::BuiltinPattern,
        effect_opaque: false,
    },
    // Block — regex `Pattern` (mkfs, FS-006).
    Case {
        cmd: "mkfs.ext4 /dev/sda1",
        risk: RiskLevel::Block,
        key_id: Some("FS-006"),
        source: DecisionSource::BuiltinPattern,
        effect_opaque: false,
    },
    // Effect-opaque `Script-file execution` — Safe to the scanner, but the
    // destructive effect lives in the unread file, so effect_opaque is set.
    Case {
        cmd: "sh ./cleanup.sh",
        risk: RiskLevel::Safe,
        key_id: None,
        source: DecisionSource::Fallback,
        effect_opaque: true,
    },
    // `Inline script` — the `-c` body is extracted and scanned in its own
    // right, so the embedded `rm -rf` trips FS-001 and the command is *not*
    // effect-opaque (CONTEXT.md: an Inline script stops being effect-opaque
    // once its body is extracted and assessed).
    Case {
        cmd: "bash -c 'rm -rf /tmp/x'",
        risk: RiskLevel::Danger,
        key_id: Some("FS-001"),
        source: DecisionSource::BuiltinPattern,
        effect_opaque: false,
    },
];

#[test]
fn compatibility_corpus_risk_is_unchanged() {
    for case in CASES {
        let assessment = scanner().assess(case.cmd);
        assert_eq!(
            assessment.risk, case.risk,
            "command {:?}: risk drifted",
            case.cmd,
        );
    }
}

#[test]
fn compatibility_corpus_key_pattern_id_is_present() {
    for case in CASES {
        let assessment = scanner().assess(case.cmd);
        let ids: Vec<&str> = assessment
            .matched
            .iter()
            .map(|m| m.pattern.id.as_ref())
            .collect();
        match case.key_id {
            Some(expected) => {
                assert!(
                    ids.contains(&expected),
                    "command {:?}: expected pattern {} among {:?}",
                    case.cmd,
                    expected,
                    ids,
                );
            }
            None => {
                assert!(
                    assessment.matched.is_empty(),
                    "command {:?}: expected no matches, got {:?}",
                    case.cmd,
                    ids,
                );
            }
        }
    }
}

#[test]
fn compatibility_corpus_decision_source_is_unchanged() {
    for case in CASES {
        let assessment = scanner().assess(case.cmd);
        assert_eq!(
            assessment.decision_source(),
            case.source,
            "command {:?}: decision_source drifted",
            case.cmd,
        );
    }
}

#[test]
fn compatibility_corpus_effect_opaque_is_unchanged() {
    for case in CASES {
        let assessment = scanner().assess(case.cmd);
        assert_eq!(
            assessment.effect_opaque, case.effect_opaque,
            "command {:?}: effect_opaque drifted",
            case.cmd,
        );
    }
}
