// Issue #396, #432: which heredoc shapes are a `Data consumer` target.
use super::super::*;

// 49. Issue #396: a same-line pipe out of the consumer means something else
// reads whatever it prints — never inert, whatever the consumer is.
#[test]
fn heredoc_jq_piped_to_shell_is_not_flagged_as_data_consumer() {
    let cmd = "jq -r .a <<'JSON' | sh\n{\"a\": \"echo hi\"}\nJSON";
    let bodies = extract_heredoc_bodies(cmd);
    assert!(!bodies[0].is_data_consumer_target);
}

// 50. `xargs` is not on the `Data consumer` list — it turns its stdin into
// argv for whatever program it runs.
#[test]
fn heredoc_xargs_is_not_flagged_as_data_consumer() {
    let cmd = "xargs <<'EOF'\npython3 ./x\nEOF";
    let bodies = extract_heredoc_bodies(cmd);
    assert!(!bodies[0].is_data_consumer_target);
}

// 51. Issue #396: a `$(...)` that is the right-hand side of an assignment
// is not a trusted context. Later text can run the captured variable in
// too many ways (`$((x))`, `${x@P}`, zsh `${(e)x}`) for a predicate to rule
// them all out, so the body stays scanned.
#[test]
fn heredoc_inside_assignment_command_substitution_is_not_flagged() {
    for cmd in [
        "out=$(cat <<'EOF'\nsome text\nEOF\n)",
        "body=$(jq -c . <<'JSON'\n{}\nJSON\n)",
    ] {
        let bodies = extract_heredoc_bodies(cmd);
        assert!(!bodies[0].is_data_consumer_target, "{cmd}");
    }
}

// 52. Issue #396: a `$(...)` that is the value of a `git`/`gh` message flag
// is a trusted context: the `gh pr create --body "$(cat <<'EOF' ...)"` idiom.
#[test]
fn heredoc_cat_inside_gh_argument_command_substitution_is_flagged() {
    let cmd = "gh pr create --body \"$(cat <<'EOF'\nsome text\nEOF\n)\"";
    let bodies = extract_heredoc_bodies(cmd);
    assert!(bodies[0].is_data_consumer_target);
}

// 53. Issue #396: a `$(...)` argument of any other command is not a trusted
// context — routing cannot vouch for what that command does with the text.
#[test]
fn heredoc_cat_inside_untrusted_command_argument_substitution_is_not_flagged() {
    let cmd = "ssh host \"$(cat <<'EOF'\nsome text\nEOF\n)\"";
    let bodies = extract_heredoc_bodies(cmd);
    assert!(!bodies[0].is_data_consumer_target);
}

// 54. Issue #396: a backtick command-substitution position is never a
// trusted context.
#[test]
fn heredoc_cat_inside_backtick_substitution_is_not_flagged() {
    let cmd = "eval `cat <<'EOF'\nsome text\nEOF\n`";
    let bodies = extract_heredoc_bodies(cmd);
    assert!(!bodies[0].is_data_consumer_target);
}

// 55. Issue #396: message-flag values of `git`/`gh`, standalone or glued,
// are trusted.
#[test]
fn heredoc_cat_inside_git_gh_message_values_is_flagged() {
    let cases = [
        "git commit -m \"$(cat <<'EOF'\nsome text\nEOF\n)\"",
        "git tag -a v1 --message \"$(cat <<'EOF'\nsome text\nEOF\n)\"",
        "git tag -a v1 -m \"$(cat <<'EOF'\nsome text\nEOF\n)\"",
        "gh issue create --title \"$(cat <<'EOF'\nsome text\nEOF\n)\"",
        "gh pr create --body=\"$(cat <<'EOF'\nsome text\nEOF\n)\"",
        "gh issue comment 1 --body \"$(cat <<'EOF'\nsome text\nEOF\n)\"",
        "cat >&2 <<'EOF'\nsome text\nEOF",
        "cat 2>&1 <<'EOF'\nsome text\nEOF",
    ];
    for cmd in cases {
        let bodies = extract_heredoc_bodies(cmd);
        assert!(bodies[0].is_data_consumer_target, "command {cmd:?}");
    }
}

// 56. Issue #396: every other shape that can hand the consumer's output to
// a shell is not a data consumer: a `$(` opened on an earlier line, a
// process substitution, a line continuation, and `git`/`gh` arguments that
// are not message values.
#[test]
fn heredoc_data_consumer_output_reaching_a_shell_is_not_flagged() {
    let cases = [
        "$(\ncat <<'EOF'\nsome text\nEOF\n)",
        "bash -c \"$(\ncat <<'EOF'\nsome text\nEOF\n)\"",
        "cat <<'EOF' > >(sh)\nsome text\nEOF",
        "tee >(sh) <<'EOF'\nsome text\nEOF",
        "cat <<'EOF' \\\n| sh\nsome text\nEOF",
        "git -c \"alias.x=!$(cat <<'EOF'\nsome text\nEOF\n)\" x",
        "git config alias.y \"!$(cat <<'EOF'\nsome text\nEOF\n)\"",
        "gh alias set --shell x \"$(cat <<'EOF'\nsome text\nEOF\n)\"",
        "git commit -m \"prefix $(cat <<'EOF'\nsome text\nEOF\n)\"",
        // Issue #396 review (finding 1): `git`'s message flag is only
        // `-m`/`--message` — `-t` is `--template=<file>` and `-b` names a
        // branch, neither a message.
        "git commit -t \"$(cat <<'EOF'\nsome text\nEOF\n)\"",
        "git checkout -b \"$(cat <<'EOF'\nsome text\nEOF\n)\"",
        // Issue #396 review (finding 1): a global option before the
        // subcommand occupies the position the gate reads as the
        // subcommand, so it must not skip past it to `commit`.
        "git -c alias.x=y commit -m \"$(cat <<'EOF'\nsome text\nEOF\n)\"",
        // Issue #396 review (finding 2): `gh`'s message-flag trust is
        // gated by subcommand for path (a) too — `-b` names a branch under
        // `pr checkout`, not a message.
        "gh pr checkout 1 -b \"$(cat <<'EOF'\nsome text\nEOF\n)\"",
        "exec 3< <(\ncat <<'EOF'\nsome text\nEOF\n)",
        "(cat <<'EOF'\nsome text\nEOF\n) | sh",
        "x=$( (cat <<'EOF'\nsome text\nEOF\n) | sh )",
        "x=$(case a in a) cat <<'EOF'\nsome text\nEOF\n;; esac)",
        "cat <<'EOF' >&3\nsome text\nEOF",
        "cat <<'EOF' >&12\nsome text\nEOF",
        "cat <<'EOF' >&$fd\nsome text\nEOF",
        "cat <<'EOF' > /dev/fd/3\nsome text\nEOF",
    ];
    for cmd in cases {
        let bodies = extract_heredoc_bodies(cmd);
        assert!(!bodies[0].is_data_consumer_target, "command {cmd:?}");
    }
}

// 57. Issue #396: a compound command around the marker can pipe or redirect
// the consumer's output after the terminator line (`} | sh`, `done >&3`), so
// an open `{` group or a compound-command keyword before the marker is not a
// data consumer.
#[test]
fn heredoc_data_consumer_inside_compound_command_is_not_flagged() {
    let cases = [
        "{ true; cat <<'EOF'\nsome text\nEOF\n} | sh",
        "exec 3> >(sh)\n{ true; cat <<'EOF'\nsome text\nEOF\n} >&3",
        "f() { true; cat <<'EOF'\nsome text\nEOF\n}; f | sh",
        "for i in 1; do true; cat <<'EOF'\nsome text\nEOF\ndone | sh",
        "while true; do true; cat <<'EOF'\nsome text\nEOF\nbreak; done | sh",
        "until false; do true; cat <<'EOF'\nsome text\nEOF\nbreak; done | sh",
        "if true; then true; cat <<'EOF'\nsome text\nEOF\nfi | sh",
        "if false; then :; else true; cat <<'EOF'\nsome text\nEOF\nfi | sh",
        "select x in a; do true; cat <<'EOF'\nsome text\nEOF\ndone | sh",
        "coproc true; cat <<'EOF'\nsome text\nEOF",
        // A `}` or `)` in a comment closes nothing in the shell, so a comment
        // before the marker must not close the group around it.
        "{ true; # closed } here\ncat <<'EOF'\nsome text\nEOF\n} >&3",
        "( true # closed ) here\ncat <<'EOF'\nsome text\nEOF\n) | sh",
        "true # ; cat\ncat <<'EOF'\nsome text\nEOF",
        // An earlier `exec` can point stdout itself at a shell, so a plain
        // consumer with no redirect of its own still feeds one.
        "exec > >(sh)\ncat <<'EOF'\nsome text\nEOF",
        "exec 1> >(sh)\ncat <<'EOF'\nsome text\nEOF",
        "exec >&3\ncat <<'EOF'\nsome text\nEOF",
    ];
    for cmd in cases {
        let bodies = extract_heredoc_bodies(cmd);
        assert!(!bodies[0].is_data_consumer_target, "command {cmd:?}");
    }
}

// 58. Issue #396: a `{` inside quotes or glued to a word opens no group, and
// a group closed before the marker line no longer encloses it.
#[test]
fn heredoc_data_consumer_after_quoted_or_closed_brace_is_flagged() {
    let cases = [
        "jq '. | { a }' <<'JSON'\n{\"a\":1}\nJSON",
        "jq -c '{a: .b}' <<'JSON'\n{\"b\":1}\nJSON",
        "echo a{b,c}; cat <<'EOF'\nsome text\nEOF",
        "{ true; }\ncat <<'EOF'\nsome text\nEOF",
    ];
    for cmd in cases {
        let bodies = extract_heredoc_bodies(cmd);
        assert!(bodies[0].is_data_consumer_target, "command {cmd:?}");
    }
}

// 59. Issue #396 (capture-then-execute): a capture is never trusted, so the
// body stays scanned whatever later text does with the variable, including
// the shapes that broke the old forwarding allowlist in PR #463 review.
#[test]
fn heredoc_capture_then_run_the_variable_is_not_flagged() {
    let cases = [
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\n$x",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\neval \"$x\"",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\necho \"$x\" | sh",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\necho $((x))",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\necho ${x@P}",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\necho ${(e)x}",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\nn=$(printf '\\170'); echo $(( $n ))",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\ne$'x'ec \"$x\"",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\ngh pr create --body \"$x\"",
        "set -euo pipefail\nout=$(cat <<'EOF'\nsome text\nEOF\n)",
        "out=$(cat <<'EOF'\nsome text\nEOF\n); true",
    ];
    for cmd in cases {
        let bodies = extract_heredoc_bodies(cmd);
        assert!(!bodies[0].is_data_consumer_target, "command {cmd:?}");
    }
}

// PR #463 review (BLOCKER 1): `$'\''` is bash's ANSI-C escape for an
// embedded `'`, not a quote toggle. `open_frames` and
// `owning_simple_command_start` both toggle a quote on every `'`, so this
// line reads as `cat`, a closed nowdoc marker `<<'EOF'`, and nothing else —
// while bash actually reads two ANSI-C strings and a bare word, with no
// heredoc operator in the text at all. The body must stay scanned.
#[test]
fn heredoc_ansi_c_quote_escape_before_marker_is_not_flagged() {
    let cmd = "cat $'\\'' ' <<'EOF'\nx'; rm -rf /; : '\nEOF\n'";
    let bodies = extract_heredoc_bodies(cmd);
    assert!(!bodies[0].is_data_consumer_target);
}

// PR #463 review (BLOCKER 2): a quote opened on an earlier physical line and
// never closed on it carries into the marker line. `open_frames` only
// tracks `$(`/backtick/paren/brace/comment frames, so it sees an empty
// frame list here and `heredoc_marker_context` read that as `TopLevel` —
// but bash actually closes the pending quote at the delimiter's own opening
// `'` and reopens a fresh one at its closing `'`, so the `<<'EOF'` this walk
// finds is not a heredoc operator to bash at all.
#[test]
fn heredoc_quote_left_open_from_earlier_line_is_not_flagged() {
    let cmd = "echo 'start\ncat <<'EOF'\n'; rm -rf /; : '\nEOF\n'";
    let bodies = extract_heredoc_bodies(cmd);
    assert!(!bodies[0].is_data_consumer_target);
}

// Same shape, a double quote left open instead of a single one.
#[test]
fn heredoc_double_quote_left_open_from_earlier_line_is_not_flagged() {
    let cmd = "echo \"start\ncat <<'EOF'\n\"; rm -rf /; : \"\nEOF\n\"";
    let bodies = extract_heredoc_bodies(cmd);
    assert!(!bodies[0].is_data_consumer_target);
}

// Same shape again, a backtick command substitution left open.
#[test]
fn heredoc_backtick_left_open_from_earlier_line_is_not_flagged() {
    let cmd = "echo `start\ncat <<'EOF'\n`; rm -rf /; : `\nEOF\n`";
    let bodies = extract_heredoc_bodies(cmd);
    assert!(!bodies[0].is_data_consumer_target);
}

// Safe idiom: an apostrophe inside the body itself never reaches the prefix
// checks above, since heredoc bodies are excluded from them
// (`embedded_scripts::walk_heredocs`'s `command_text`).
#[test]
fn heredoc_body_with_apostrophe_stays_flagged_as_data_consumer() {
    let cmd = "cat <<'EOF'\ndon't rm -rf /\nEOF";
    let bodies = extract_heredoc_bodies(cmd);
    assert!(bodies[0].is_data_consumer_target);
}

// Safe idiom: a prefix line with a quote opened and closed on the same line
// leaves no quote open at the marker, so the fix above must not flag it.
#[test]
fn heredoc_after_balanced_quote_prefix_line_stays_flagged() {
    let cmd = "echo 'a'; cat <<'EOF'\nrm -rf /\nEOF";
    let bodies = extract_heredoc_bodies(cmd);
    assert!(bodies[0].is_data_consumer_target);
}

// PR #463 review (round 3): a word denylist always lags the shell. Each
// prefix below rebinds `cat` through a shell table, a computed command word,
// or an expansion that assigns, and none of them names a denylisted word.
// Only prefixes built from known inert commands keep the body trusted.
#[test]
fn heredoc_after_prefix_that_can_rebind_the_consumer_is_not_flagged() {
    let prefixes = [
        "BASH_CMDS[cat]=/bin/bash",
        "shopt -s expand_aliases; BASH_ALIASES[cat]=bash",
        "functions[cat]=sh",
        "commands[cat]=/bin/sh",
        "BASH_CMDS+=([cat]=/bin/bash)",
        "path=(/tmp/p $path)",
        "set -A path /tmp/p",
        "print -v path /tmp/p",
        "autoload -Uz cat",
        "$(echo eval) git '; BASH_CMDS[cat]=/bin/bash'",
        "\"$(echo eval)\" git '; BASH_CMDS[cat]=/bin/bash'",
        "git log ${PATH::=/tmp/p}",
        "git log $[PATH=0]",
        "git log $a[PATH=0]",
        "git log && BASH_CMDS[cat]=/bin/bash",
        "echo x | BASH_CMDS[cat]=/bin/bash",
        "{ BASH_CMDS[cat]=/bin/bash; }",
        ": <<X\n$((PATH=0))\nX",
        "{ true; } always { functions[cat]=sh; }",
        "true &>/dev/null alias cat=sh",
    ];
    for prefix in prefixes {
        let cmd = format!("{prefix}\ncat <<'EOF'\nrm -rf /\nEOF");
        let bodies = extract_heredoc_bodies(&cmd);
        let body = bodies.last().expect("heredoc body");
        assert!(!body.is_data_consumer_target, "trusted after {prefix:?}");
    }
}

// Same gap inside the trusted `$(...)` of a message flag: the command
// before `cat` inside the substitution must be inert too. The capture cases
// stay here as a guard for the follow-up that may trust captures again.
#[test]
fn heredoc_after_rebinding_inside_trusted_substitution_is_not_flagged() {
    for cmd in [
        "git commit -m \"$(BASH_CMDS[cat]=/bin/sh; cat <<'EOF'\nrm -rf /\nEOF\n)\"",
        "x=$(functions[cat]=sh; cat <<'EOF'\nrm -rf /\nEOF\n)",
        "BASH_CMDS[cat]=/bin/sh\nx=$(cat <<'EOF'\nrm -rf /\nEOF\n)",
    ] {
        let bodies = extract_heredoc_bodies(cmd);
        assert!(!bodies[0].is_data_consumer_target, "trusted: {cmd:?}");
    }
}

// Safe idioms: a prefix made only of inert commands keeps the trust,
// including a second message heredoc after a first one.
#[test]
fn heredoc_after_inert_command_prefix_stays_flagged() {
    for cmd in [
        "git add -A && git commit -m \"$(cat <<'EOF'\nmsg\nEOF\n)\"",
        "cd /repo && gh pr create --title \"t\" --body \"$(cat <<'EOF'\nbody\nEOF\n)\"",
        "git commit -m \"$(cat <<'EOF'\nmsg\nEOF\n)\"\ngit push 2>&1\ngh pr create --body \"$(cat <<'EOF'\nbody\nEOF\n)\"",
        "cd \"$HOME/repo\"; cat <<'EOF'\ntext\nEOF",
    ] {
        let bodies = extract_heredoc_bodies(cmd);
        let body = bodies.last().expect("heredoc body");
        assert!(body.is_data_consumer_target, "untrusted: {cmd:?}");
    }
}

// `set` stays trusted only as a plain error-handling option list.
#[test]
fn heredoc_after_set_that_can_assign_is_not_flagged() {
    for prefix in ["set -A path /tmp/p", "set -k", "set -o posix", "set -e $x"] {
        let cmd = format!("{prefix}\ncat <<'EOF'\nrm -rf /\nEOF");
        let bodies = extract_heredoc_bodies(&cmd);
        assert!(
            !bodies[0].is_data_consumer_target,
            "trusted after {prefix:?}"
        );
    }
}
