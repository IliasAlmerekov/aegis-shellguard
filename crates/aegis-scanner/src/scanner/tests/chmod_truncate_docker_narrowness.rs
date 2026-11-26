use super::*;

#[test]
fn m5_fs019_non_recursive_and_non_literal_root_targets_do_not_fire() {
    let s = scanner();
    for cmd in [
        "chmod -r 000 /",
        "chmod -R 000 ./build",
        "chmod 000 /etc/passwd",
        "chmod -R 000 /*",
        "cd /usr && chmod -R 000 .",
    ] {
        let assessment = s.assess(cmd);
        assert!(
            !assessment
                .matched
                .iter()
                .any(|matched| matched.pattern.id.as_ref() == "FS-019"),
            "FS-019 must not fire for {cmd:?}: {:?}",
            assessment
                .matched
                .iter()
                .map(|matched| matched.pattern.id.as_ref())
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn m5_db009_bare_truncate_ident_does_not_fire() {
    let s = scanner();
    let assessment = s.assess("TRUNCATE users");
    assert!(
        !assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "DB-009"),
        "DB-009 must not fire for bare 'TRUNCATE users': {:?}",
        assessment
            .matched
            .iter()
            .map(|m| m.pattern.id.as_ref())
            .collect::<Vec<_>>()
    );
    assert!(
        assessment.risk < RiskLevel::Danger,
        "bare 'TRUNCATE users' must not reach Danger (got {:?})",
        assessment.risk
    );
}

#[test]
fn m5_db009_coreutils_truncate_stays_safe() {
    let s = scanner();
    for cmd in ["truncate -s 0 app.log", "truncate --size=0 app.log"] {
        let assessment = s.assess(cmd);
        assert!(
            !assessment
                .matched
                .iter()
                .any(|m| m.pattern.id.as_ref() == "DB-009"),
            "DB-009 must not fire for coreutils {cmd:?}: {:?}",
            assessment
                .matched
                .iter()
                .map(|m| m.pattern.id.as_ref())
                .collect::<Vec<_>>()
        );
        assert!(
            assessment.risk < RiskLevel::Danger,
            "coreutils {cmd:?} must not reach Danger (got {:?})",
            assessment.risk
        );
    }
}

#[test]
fn m5_db009_prose_does_not_fire() {
    let s = scanner();
    for cmd in [
        "git commit -m \"truncate long log lines\"",
        "git commit -m \"truncate logs, rebuild index\"",
        "echo \"we truncate stdout, then exit\"",
        "git commit -m \"truncate only the last line\"",
    ] {
        let assessment = s.assess(cmd);
        assert!(
            !assessment
                .matched
                .iter()
                .any(|m| m.pattern.id.as_ref() == "DB-009"),
            "DB-009 must not fire for prose {cmd:?}: {:?}",
            assessment
                .matched
                .iter()
                .map(|m| m.pattern.id.as_ref())
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn m5_db009_bare_comma_list_does_not_fire() {
    let s = scanner();
    let assessment = s.assess("TRUNCATE orders, order_items");
    assert!(
        !assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "DB-009"),
        "DB-009 must not fire for a bare comma list 'TRUNCATE orders, order_items': {:?}",
        assessment
            .matched
            .iter()
            .map(|m| m.pattern.id.as_ref())
            .collect::<Vec<_>>()
    );
    assert!(
        assessment.risk < RiskLevel::Danger,
        "bare comma list must not reach Danger (got {:?})",
        assessment.risk
    );
}

#[test]
fn m5_db009_spaced_quoted_identifier_does_not_fire() {
    let s = scanner();
    let assessment = s.assess("TRUNCATE \"my table\" CASCADE");
    assert!(
        !assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "DB-009"),
        "DB-009 must not fire for a spaced quoted identifier 'TRUNCATE \"my table\" CASCADE': {:?}",
        assessment
            .matched
            .iter()
            .map(|m| m.pattern.id.as_ref())
            .collect::<Vec<_>>()
    );
}

#[test]
fn m5_docker_volume_ls_and_inspect_do_not_fire_dk007() {
    let s = scanner();
    for cmd in ["docker volume ls", "docker volume inspect pgdata"] {
        let assessment = s.assess(cmd);
        assert!(
            !assessment
                .matched
                .iter()
                .any(|m| m.pattern.id.as_ref() == "DK-007"),
            "DK-007 must not fire for {cmd:?}: {:?}",
            assessment
                .matched
                .iter()
                .map(|m| m.pattern.id.as_ref())
                .collect::<Vec<_>>()
        );
        assert!(
            assessment.risk < RiskLevel::Danger,
            "{cmd:?} must not reach Danger (got {:?})",
            assessment.risk
        );
    }
}

#[test]
fn m5_docker_rm_v_is_not_detected_as_dk007() {
    let s = scanner();
    let assessment = s.assess("docker rm -v my-container");
    assert!(
        !assessment
            .matched
            .iter()
            .any(|m| m.pattern.id.as_ref() == "DK-007"),
        "DK-007 must not fire for 'docker rm -v my-container': {:?}",
        assessment
            .matched
            .iter()
            .map(|m| m.pattern.id.as_ref())
            .collect::<Vec<_>>()
    );
}
