use super::*;

struct Case {
    cmd: &'static str,
    risk: RiskLevel,
    key_id: Option<&'static str>,
    source: DecisionSource,
    effect_opaque: bool,
}

const CASES: &[Case] = &[
    Case {
        cmd: "echo hello",
        risk: RiskLevel::Safe,
        key_id: None,
        source: DecisionSource::Fallback,
        effect_opaque: false,
    },
    Case {
        cmd: "rm -rf /tmp/build",
        risk: RiskLevel::Danger,
        key_id: Some("FS-001"),
        source: DecisionSource::BuiltinPattern,
        effect_opaque: false,
    },
    Case {
        cmd: "git reset --hard HEAD~1",
        risk: RiskLevel::Warn,
        key_id: Some("GIT-001"),
        source: DecisionSource::BuiltinPattern,
        effect_opaque: false,
    },
    Case {
        cmd: "mkfs.ext4 /dev/sda1",
        risk: RiskLevel::Block,
        key_id: Some("FS-006"),
        source: DecisionSource::BuiltinPattern,
        effect_opaque: false,
    },
    Case {
        cmd: "sh ./cleanup.sh",
        risk: RiskLevel::Safe,
        key_id: None,
        source: DecisionSource::Fallback,
        effect_opaque: true,
    },
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
