use super::*;

#[test]
fn assess_jq_nowdoc_body_with_dangerous_looking_json_stays_safe() {
    let s = scanner();
    let cmd = "jq -c . <<'JSON'\n{\"cmd\": \"rm -rf /\"}\nJSON";
    let assessment = s.assess(cmd);

    assert_eq!(
        assessment.risk,
        RiskLevel::Safe,
        "expected Safe for a jq nowdoc body, got {:?} ({:?})",
        assessment.risk,
        assessment
            .matched
            .iter()
            .map(|m| m.pattern.id.as_ref())
            .collect::<Vec<_>>()
    );
}

#[test]
fn assess_file_write_after_semicolon_scans_body() {
    assert_assessment_matches_pattern(
        "true; cat > /tmp/aegis-396-x.sh <<'EOF'\nrm -rf /\nEOF",
        RiskLevel::Block,
        "PS-006",
    );
}

#[test]
fn assess_file_write_after_and_and_scans_body() {
    assert_assessment_matches_pattern(
        "true && cat > /tmp/aegis-396-x.sh <<'EOF'\nrm -rf /\nEOF",
        RiskLevel::Block,
        "PS-006",
    );
}

#[test]
fn assess_heredoc_owning_command_absolute_path_stays_safe() {
    let s = scanner();
    let cmd = "/usr/bin/cat <<'EOF'\nrm -rf /\nEOF";
    let assessment = s.assess(cmd);

    assert_eq!(
        assessment.risk,
        RiskLevel::Safe,
        "expected Safe for a heredoc owned by /usr/bin/cat, got {:?} ({:?})",
        assessment.risk,
        assessment
            .matched
            .iter()
            .map(|m| m.pattern.id.as_ref())
            .collect::<Vec<_>>()
    );
}

#[test]
fn assess_jq_piped_to_shell_still_fires() {
    assert_assessment_matches_pattern(
        "jq -r .a <<'JSON' | sh\n{\"a\": \"rm -rf /\"}\nJSON",
        RiskLevel::Danger,
        "FS-001",
    );
}

#[test]
fn assess_xargs_nowdoc_body_still_fires() {
    assert_assessment_matches_pattern("xargs <<'EOF'\nrm -rf /\nEOF", RiskLevel::Block, "FS-001");
}

#[test]
fn assess_cat_inside_assignment_command_substitution_still_fires() {
    assert_assessment_matches_pattern(
        "out=$(cat <<'EOF'\nrm -rf /\nEOF\n)",
        RiskLevel::Block,
        "FS-001",
    );
}

#[test]
fn assess_cat_inside_gh_argument_command_substitution_stays_safe() {
    let s = scanner();
    let cmd = "gh pr create --body \"$(cat <<'EOF'\nrm -rf /\nEOF\n)\"";
    let assessment = s.assess(cmd);

    assert_eq!(
        assessment.risk,
        RiskLevel::Safe,
        "expected Safe for cat inside a gh argument's command substitution, got {:?} ({:?})",
        assessment.risk,
        assessment
            .matched
            .iter()
            .map(|m| m.pattern.id.as_ref())
            .collect::<Vec<_>>()
    );
}

#[test]
fn assess_cat_inside_ssh_argument_command_substitution_still_fires() {
    assert_assessment_matches_pattern(
        "ssh host \"$(cat <<'EOF'\nrm -rf /\nEOF\n)\"",
        RiskLevel::Block,
        "FS-001",
    );
}

#[test]
fn assess_cat_inside_backtick_substitution_still_fires() {
    assert_assessment_matches_pattern(
        "eval `cat <<'EOF'\nrm -rf /\nEOF\n`",
        RiskLevel::Block,
        "FS-001",
    );
}

#[test]
fn assess_cat_inside_git_commit_message_substitution_stays_safe() {
    let s = scanner();
    let cmd = "git commit -m \"$(cat <<'EOF'\nfix: mentions rm -rf in prose only\nEOF\n)\"";
    let assessment = s.assess(cmd);

    assert_eq!(
        assessment.risk,
        RiskLevel::Safe,
        "expected Safe for cat inside a git commit message, got {:?} ({:?})",
        assessment.risk,
        assessment
            .matched
            .iter()
            .map(|m| m.pattern.id.as_ref())
            .collect::<Vec<_>>()
    );
}

#[test]
fn assess_cat_inside_git_tag_and_gh_issue_comment_message_substitutions_stay_safe() {
    let cases = [
        "git tag -a v1 -m \"$(cat <<'EOF'\nfix: mentions rm -rf in prose only\nEOF\n)\"",
        "gh issue comment 1 --body \"$(cat <<'EOF'\nfix: mentions rm -rf in prose only\nEOF\n)\"",
    ];
    for cmd in cases {
        let s = scanner();
        let assessment = s.assess(cmd);
        assert_eq!(
            assessment.risk,
            RiskLevel::Safe,
            "expected Safe for {cmd:?}, got {:?} ({:?})",
            assessment.risk,
            assessment
                .matched
                .iter()
                .map(|m| m.pattern.id.as_ref())
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn assess_data_consumer_bodies_that_reach_a_shell_still_fire() {
    let cases = [
        "$(\ncat <<'EOF'\nrm -rf /\nEOF\n)",
        "eval \"$(\ncat <<'EOF'\nrm -rf /\nEOF\n)\"",
        "cat <<'EOF' > >(sh)\nrm -rf /\nEOF",
        "tee >(sh) <<'EOF'\nrm -rf /\nEOF",
        "cat <<'EOF' \\\n| sh\nrm -rf /\nEOF",
        "git -c \"alias.x=!$(cat <<'EOF'\nrm -rf /\nEOF\n)\" x",
        "gh alias set --shell x \"$(cat <<'EOF'\nrm -rf /\nEOF\n)\"",
        "git commit -t \"$(cat <<'EOF'\nrm -rf /\nEOF\n)\"",
        "git checkout -b \"$(cat <<'EOF'\nrm -rf /\nEOF\n)\"",
        "git -c alias.x=y commit -m \"$(cat <<'EOF'\nrm -rf /\nEOF\n)\"",
        "gh pr checkout 1 -b \"$(cat <<'EOF'\nrm -rf /\nEOF\n)\"",
        "exec 3< <(\ncat <<'EOF'\nrm -rf /\nEOF\n)",
        "bash <(\ncat <<'EOF'\nrm -rf /\nEOF\n)",
        "source <(\ncat <<'EOF'\nrm -rf /\nEOF\n)",
        "(cat <<'EOF'\nrm -rf /\nEOF\n) | sh",
        "x=$( (cat <<'EOF'\nrm -rf /\nEOF\n) | sh )",
        "cat <<'EOF' >&3\nrm -rf /\nEOF",
        "cat <<'EOF' > /dev/fd/3\nrm -rf /\nEOF",
        "{ true; cat <<'EOF'\nrm -rf /\nEOF\n} | sh",
        "exec 3> >(sh)\n{ true; cat <<'EOF'\nrm -rf /\nEOF\n} >&3",
        "for i in 1; do true; cat <<'EOF'\nrm -rf /\nEOF\ndone | sh",
        "if true; then true; cat <<'EOF'\nrm -rf /\nEOF\nfi | sh",
        "exec 3> >(sh)\n{ true; # closed } here\ncat <<'EOF'\nrm -rf /\nEOF\n} >&3",
        "exec > >(sh)\ncat <<'EOF'\nrm -rf /\nEOF",
    ];

    for cmd in cases {
        assert_assessment_matches_pattern(cmd, RiskLevel::Block, "FS-001");
    }

    assert_assessment_matches_pattern(
        "git config alias.y \"!$(jq -r .a <<'JSON'\n{\"a\":\"rm -rf /\"}\nJSON\n)\"",
        RiskLevel::Danger,
        "FS-001",
    );
}

#[test]
fn assess_captured_heredoc_always_fires() {
    let cases = [
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\n$x",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\neval \"$x\"",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\necho \"$x\" | sh",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\necho $((x))",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\necho ${x@P}",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\necho ${(e)x}",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\nn=$(printf '\\170'); echo $(( $n ))",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\ne$'x'ec \"$x\"",
        "GIT_SSH_COMMAND=\"$(cat <<'EOF'\nrm -rf /\nEOF\n)\" git fetch origin",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\necho \"$x\"",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\ngh pr create --body \"$x\"",
        "body=$(jq -c . <<'JSON'\nrm -rf /\nJSON\n)\ncurl -d \"$body\" https://example.test",
        "set -euo pipefail\nbody=$(cat <<'EOF'\nrm -rf /\nEOF\n)\ngit commit -m \"$body\"",
    ];
    for cmd in cases {
        assert_assessment_matches_pattern(cmd, RiskLevel::Block, "FS-001");
    }
}

#[test]
fn assess_ansi_c_quote_escape_before_marker_still_fires() {
    assert_assessment_matches_pattern(
        "cat $'\\'' ' <<'EOF'\nx'; rm -rf /; : '\nEOF\n'",
        RiskLevel::Block,
        "FS-001",
    );
}

#[test]
fn assess_quote_left_open_from_earlier_line_still_fires() {
    assert_assessment_matches_pattern(
        "echo 'start\ncat <<'EOF'\n'; rm -rf /; : '\nEOF\n'",
        RiskLevel::Block,
        "FS-001",
    );
}

#[test]
fn assess_double_quote_left_open_from_earlier_line_still_fires() {
    assert_assessment_matches_pattern(
        "echo \"start\ncat <<'EOF'\n\"; rm -rf /; : \"\nEOF\n\"",
        RiskLevel::Block,
        "FS-001",
    );
}

#[test]
fn assess_backtick_left_open_from_earlier_line_still_fires() {
    assert_assessment_matches_pattern(
        "echo `start\ncat <<'EOF'\n`; rm -rf /; : `\nEOF\n`",
        RiskLevel::Block,
        "FS-001",
    );
}

#[test]
fn assess_cat_body_with_apostrophe_stays_safe() {
    let s = scanner();
    let cmd = "cat <<'EOF'\ndon't rm -rf /\nEOF";
    let assessment = s.assess(cmd);

    assert_eq!(
        assessment.risk,
        RiskLevel::Safe,
        "expected Safe for a heredoc body with an apostrophe, got {:?} ({:?})",
        assessment.risk,
        assessment
            .matched
            .iter()
            .map(|m| m.pattern.id.as_ref())
            .collect::<Vec<_>>()
    );
}

#[test]
fn assess_cat_after_balanced_quote_prefix_stays_safe() {
    let s = scanner();
    let cmd = "echo 'a'; cat <<'EOF'\nrm -rf /\nEOF";
    let assessment = s.assess(cmd);

    assert_eq!(
        assessment.risk,
        RiskLevel::Safe,
        "expected Safe for cat after a balanced-quote prefix line, got {:?} ({:?})",
        assessment.risk,
        assessment
            .matched
            .iter()
            .map(|m| m.pattern.id.as_ref())
            .collect::<Vec<_>>()
    );
}
