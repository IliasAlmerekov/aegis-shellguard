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

// 51. Issue #396: a `$(...)` that is the right-hand side of a plain
// assignment is a trusted context.
#[test]
fn heredoc_cat_inside_assignment_command_substitution_is_flagged() {
    let cmd = "OUT=$(cat <<'EOF'\nsome text\nEOF\n)";
    let bodies = extract_heredoc_bodies(cmd);
    assert!(bodies[0].is_data_consumer_target);
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
        "gh issue create --title \"$(cat <<'EOF'\nsome text\nEOF\n)\"",
        "gh pr create --body=\"$(cat <<'EOF'\nsome text\nEOF\n)\"",
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
