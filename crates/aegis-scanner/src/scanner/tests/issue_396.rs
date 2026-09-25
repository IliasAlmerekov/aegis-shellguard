use super::*;

// Issue #396: `jq` parses its stdin as JSON, never as commands. A
// dangerous-looking substring inside a JSON value (a shell command quoted as
// data) must not be treated as a live command just because it sits inside a
// nowdoc body.
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

// Issue #432: the heredoc's owning command is found after the last `;`, not
// the line's own first token — `true; cat > f <<'EOF'` must resolve to
// `cat`, so the body is treated as data at rest exactly like a bare
// `cat > f <<'EOF'` already is.
#[test]
fn assess_heredoc_owning_command_after_semicolon_stays_safe() {
    let s = scanner();
    let cmd = "true; cat > /tmp/aegis-396-x.sh <<'EOF'\nrm -rf /\nEOF";
    let assessment = s.assess(cmd);

    assert_eq!(
        assessment.risk,
        RiskLevel::Safe,
        "expected Safe for a heredoc owned by a chained cat, got {:?} ({:?})",
        assessment.risk,
        assessment
            .matched
            .iter()
            .map(|m| m.pattern.id.as_ref())
            .collect::<Vec<_>>()
    );
}

// Same fix, `&&`-chained instead of `;`-chained.
#[test]
fn assess_heredoc_owning_command_after_and_and_stays_safe() {
    let s = scanner();
    let cmd = "true && cat > /tmp/aegis-396-x.sh <<'EOF'\nrm -rf /\nEOF";
    let assessment = s.assess(cmd);

    assert_eq!(
        assessment.risk,
        RiskLevel::Safe,
        "expected Safe for a heredoc owned by an &&-chained cat, got {:?} ({:?})",
        assessment.risk,
        assessment
            .matched
            .iter()
            .map(|m| m.pattern.id.as_ref())
            .collect::<Vec<_>>()
    );
}

// A same-line pipe out of `jq` means `sh` reads whatever `jq` prints — never
// inert, whatever the JSON body says.
#[test]
fn assess_jq_piped_to_shell_still_fires() {
    assert_assessment_matches_pattern(
        "jq -r .a <<'JSON' | sh\n{\"a\": \"rm -rf /\"}\nJSON",
        RiskLevel::Danger,
        "FS-001",
    );
}

// `xargs` is not a `Data consumer` — it turns its stdin into argv for
// whatever program it runs, so a dangerous body must keep firing.
#[test]
fn assess_xargs_nowdoc_body_still_fires() {
    assert_assessment_matches_pattern("xargs <<'EOF'\nrm -rf /\nEOF", RiskLevel::Block, "FS-001");
}

// A `$(...)` that is the right-hand side of a plain assignment is a trusted
// context — the body stays inert.
#[test]
fn assess_cat_inside_assignment_command_substitution_stays_safe() {
    let s = scanner();
    let cmd = "OUT=$(cat <<'EOF'\nrm -rf /\nEOF\n)";
    let assessment = s.assess(cmd);

    assert_eq!(
        assessment.risk,
        RiskLevel::Safe,
        "expected Safe for cat inside an assignment's command substitution, got {:?} ({:?})",
        assessment.risk,
        assessment
            .matched
            .iter()
            .map(|m| m.pattern.id.as_ref())
            .collect::<Vec<_>>()
    );
}

// A `$(...)` that is the value of a `gh` message flag is a trusted context:
// the `gh pr create --body "$(cat <<'EOF' ...)"` idiom.
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

// A `$(...)` argument of any other command is not a trusted context — `ssh`
// runs whatever text it receives remotely, so the body must keep firing.
#[test]
fn assess_cat_inside_ssh_argument_command_substitution_still_fires() {
    assert_assessment_matches_pattern(
        "ssh host \"$(cat <<'EOF'\nrm -rf /\nEOF\n)\"",
        RiskLevel::Block,
        "FS-001",
    );
}

// A backtick command-substitution position is never a trusted context.
#[test]
fn assess_cat_inside_backtick_substitution_still_fires() {
    assert_assessment_matches_pattern(
        "eval `cat <<'EOF'\nrm -rf /\nEOF\n`",
        RiskLevel::Block,
        "FS-001",
    );
}

// The `git commit -m "$(cat <<'EOF' ...)"` idiom: a `-m` value is only ever
// stored as the commit message.
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

// Bodies that reach a shell through a shape the data-consumer predicate must
// reject: a `$(` opened on an earlier line, a process substitution, a line
// continuation into a pipe, and `git`/`gh` arguments that are not message
// values (a `!` alias runs a shell, `gh alias set --shell` stores one).
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
        // A `(` frame opened before the marker: process substitution,
        // subshell, or a subshell nested in an assignment's `$(...)`.
        "exec 3< <(\ncat <<'EOF'\nrm -rf /\nEOF\n)",
        "bash <(\ncat <<'EOF'\nrm -rf /\nEOF\n)",
        "source <(\ncat <<'EOF'\nrm -rf /\nEOF\n)",
        "(cat <<'EOF'\nrm -rf /\nEOF\n) | sh",
        "x=$( (cat <<'EOF'\nrm -rf /\nEOF\n) | sh )",
        // Output to a descriptor that may be a pipe opened earlier.
        "cat <<'EOF' >&3\nrm -rf /\nEOF",
        "cat <<'EOF' > /dev/fd/3\nrm -rf /\nEOF",
        // A compound command whose output leaves after the terminator line.
        "{ true; cat <<'EOF'\nrm -rf /\nEOF\n} | sh",
        "exec 3> >(sh)\n{ true; cat <<'EOF'\nrm -rf /\nEOF\n} >&3",
        "for i in 1; do true; cat <<'EOF'\nrm -rf /\nEOF\ndone | sh",
        "if true; then true; cat <<'EOF'\nrm -rf /\nEOF\nfi | sh",
        "exec 3> >(sh)\n{ true; # closed } here\ncat <<'EOF'\nrm -rf /\nEOF\n} >&3",
    ];

    for cmd in cases {
        assert_assessment_matches_pattern(cmd, RiskLevel::Block, "FS-001");
    }

    // The JSON quote right after `/` keeps PS-006 from matching, so FS-001
    // alone decides the level here.
    assert_assessment_matches_pattern(
        "git config alias.y \"!$(jq -r .a <<'JSON'\n{\"a\":\"rm -rf /\"}\nJSON\n)\"",
        RiskLevel::Danger,
        "FS-001",
    );
}
