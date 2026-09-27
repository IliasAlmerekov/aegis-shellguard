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
// assignment to a name with no uppercase letter is a trusted context.
#[test]
fn heredoc_cat_inside_assignment_command_substitution_is_flagged() {
    let cmd = "out=$(cat <<'EOF'\nsome text\nEOF\n)";
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

// 59. Issue #396 (capture-then-execute): `AssignmentRhs` trust assumes the
// captured value stays data. When the command later runs `$NAME`/`${NAME}`
// itself, the marker must fall back to `Untrusted` so the body stays
// scanned — a `NAME=$(cat <<'EOF' ... EOF)` capture followed by any of these
// shapes must not be flagged as a data consumer.
#[test]
fn heredoc_capture_then_run_the_variable_is_not_flagged() {
    let cases = [
        // `$x` as the command word, on its own line.
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\n$x",
        // `; $x` — command word right after a `;`.
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\ntrue; $x",
        // `${x}` — braced form as the command word.
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\n${x}",
        // `"$x"` — quoted command word.
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\n\"$x\"",
        // `eval "$x"`.
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\neval \"$x\"",
        // `bash -c "$x"`.
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\nbash -c \"$x\"",
        // `sh -c "$x"`.
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\nsh -c \"$x\"",
        // `source <(echo "$x")`.
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\nsource <(echo \"$x\")",
        // `echo "$x" | sh`.
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\necho \"$x\" | sh",
    ];
    for cmd in cases {
        let bodies = extract_heredoc_bodies(cmd);
        assert!(!bodies[0].is_data_consumer_target, "command {cmd:?}");
    }
}

// 60. Issue #396 (capture-then-execute): the negative case — a captured
// value only ever handed to `echo` or stored as a `git`/`gh` message/flag
// value never runs, so the body stays a data consumer.
#[test]
fn heredoc_capture_then_only_forward_the_variable_is_still_flagged() {
    let cases = [
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\necho \"$x\"\ngh pr create --body \"$x\"",
        "body=$(jq -c . <<'JSON'\n{\"cmd\": \"rm -rf /\"}\nJSON\n)\ngh api repos/o/r/issues -f body=\"$body\"",
        "body=$(jq -c . <<'JSON'\n{\"cmd\": \"rm -rf /\"}\nJSON\n)\ncurl -d \"$body\" https://example.test",
    ];
    for cmd in cases {
        let bodies = extract_heredoc_bodies(cmd);
        assert!(bodies[0].is_data_consumer_target, "command {cmd:?}");
    }
}

// 61. Commit c696273's forwarding allowlist (ADR-042 item 5): a captured
// heredoc variable run through a compound-command wrapper, a grouping
// construct, a parameter expansion, a second capture layer, an alias/trap
// store, a nameref, a here-string, or a write-then-run must not be flagged
// as a data consumer — none of these is a provable forward.
#[test]
fn heredoc_capture_then_run_the_variable_via_allowlist_gaps_is_not_flagged() {
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
        let bodies = extract_heredoc_bodies(cmd);
        assert!(!bodies[0].is_data_consumer_target, "command {cmd:?}");
    }
}

// 62. Commit c696273's forwarding allowlist (ADR-042 item 5): a captured
// heredoc variable handed to `gh`/`curl`/`git` as a plain argument or a
// message-flag value is still flagged, whether standalone or alongside the
// existing `echo`/`jq` idioms.
#[test]
fn heredoc_capture_then_forward_the_variable_via_allowlist_programs_is_still_flagged() {
    let cases = [
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\necho \"$x\"",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\ngh pr create --body \"$x\"",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\ngh api repos/o/r/issues -f body=\"$x\"",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\ncurl -d \"$x\" https://example.com",
        "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\ngit commit -m \"$x\"",
    ];
    for cmd in cases {
        let bodies = extract_heredoc_bodies(cmd);
        assert!(bodies[0].is_data_consumer_target, "command {cmd:?}");
    }
}

// 63. Issue #396 review follow-up: the forwarding allowlist's per-program
// narrowing to data flags. A captured value handed to a flag that reads a
// file or another program's text — `jq -n`/`-f`, `gh --input`/`-F`, `curl
// -T`/`-K`/`-o`/a bare URL, `printf`'s own format string — must not be
// flagged as a data consumer, whatever the body says.
#[test]
fn heredoc_capture_then_narrow_data_flag_violations_is_not_flagged() {
    let cases = [
        "x=$(cat <<'EOF'\nsome text\nEOF\n)\njq -n \"$x\"",
        "x=$(cat <<'EOF'\nsome text\nEOF\n)\njq -f \"$x\"",
        "x=$(cat <<'EOF'\nsome text\nEOF\n)\ngh api -X POST --input \"$x\" /repos/x/y/issues",
        "x=$(cat <<'EOF'\nsome text\nEOF\n)\ngh api -F body=\"$x\" /repos/x/y/issues",
        "x=$(cat <<'EOF'\nsome text\nEOF\n)\ncurl -T \"$x\" https://example.com",
        "x=$(cat <<'EOF'\nsome text\nEOF\n)\ncurl -K \"$x\"",
        "x=$(cat <<'EOF'\nsome text\nEOF\n)\ncurl -o \"$x\" https://example.com",
        "x=$(cat <<'EOF'\nsome text\nEOF\n)\ncurl \"$x\"",
        "x=$(cat <<'EOF'\nsome text\nEOF\n)\nprintf \"$x\"",
        // Issue #396 review (finding 3): `-R`'s own value shifts the
        // action word off position 1, so it must not resolve to `create`.
        "x=$(cat <<'EOF'\nsome text\nEOF\n)\ngh issue -R create delete --body \"$x\"",
        // Issue #396 review (finding 2 and 3): a flag before `pr` makes
        // both subcommand slots unresolved, so `-b` cannot ride along.
        "x=$(cat <<'EOF'\nsome text\nEOF\n)\ngh -R o/r pr create -b \"$x\"",
    ];
    for cmd in cases {
        let bodies = extract_heredoc_bodies(cmd);
        assert!(!bodies[0].is_data_consumer_target, "command {cmd:?}");
    }
}

// 64. Issue #396 review follow-up: `curl -d`/`--data`* trust the captured
// value only while its own first body line does not start with `@` — curl
// reads an `@`-prefixed value as a file path to upload instead of sending it
// literally.
#[test]
fn heredoc_capture_then_curl_data_flag_with_at_prefixed_body_is_not_flagged() {
    let cases = [
        "x=$(cat <<'EOF'\n@/etc/shadow\nsome text\nEOF\n)\ncurl -d \"$x\" https://example.com",
        "x=$(cat <<'EOF'\n@/etc/shadow\nsome text\nEOF\n)\ncurl --data \"$x\" https://example.com",
    ];
    for cmd in cases {
        let bodies = extract_heredoc_bodies(cmd);
        assert!(!bodies[0].is_data_consumer_target, "command {cmd:?}");
    }
}

// 65. Issue #396 review follow-up: the allowlist's trusted side under the
// narrowed rules — `jq --arg`, `curl --data-raw` (even with an
// `@`-prefixed body, since it is always trusted), and `gh`'s message flags.
#[test]
fn heredoc_capture_then_narrow_data_flag_matches_is_still_flagged() {
    let cases = [
        "x=$(cat <<'EOF'\nsome text\nEOF\n)\njq -n --arg b \"$x\" '{b:$b}'",
        "x=$(cat <<'EOF'\nsome text\nEOF\n)\ncurl --data-raw \"$x\" https://example.com",
        "x=$(cat <<'EOF'\n@/etc/shadow\nsome text\nEOF\n)\ncurl --data-raw \"$x\" https://example.com",
        "x=$(cat <<'EOF'\nsome text\nEOF\n)\ngh issue create -t \"$x\" -b \"$x\"",
    ];
    for cmd in cases {
        let bodies = extract_heredoc_bodies(cmd);
        assert!(bodies[0].is_data_consumer_target, "command {cmd:?}");
    }
}

// 66. Issue #396 review follow-up (BLOCKER/MAJOR findings on PR #463):
// `printf -v`/`--` and a `gh` subcommand outside the message-flag/`api`
// allowlist must not be flagged as a data consumer either — none of these
// is a provable forward under the narrowed rules.
#[test]
fn heredoc_capture_then_printf_option_or_gh_unlisted_subcommand_is_not_flagged() {
    let cases = [
        "x=$(cat <<'EOF'\nsome text\nEOF\n)\nprintf -- \"$x\"",
        "x=$(cat <<'EOF'\nsome text\nEOF\n)\nprintf -v y \"$x\"",
        "x=$(cat <<'EOF'\nsome text\nEOF\n)\ngh pr checkout 1 -b \"$x\"",
        "x=$(cat <<'EOF'\nsome text\nEOF\n)\ngh repo create -d \"$x\"",
        "x=$(cat <<'EOF'\nsome text\nEOF\n)\ngh workflow run w -f a=\"$x\"",
    ];
    for cmd in cases {
        let bodies = extract_heredoc_bodies(cmd);
        assert!(!bodies[0].is_data_consumer_target, "command {cmd:?}");
    }
}

// 67. Issue #396 review follow-up: `printf` with no leading option still
// trusts its post-format-string argument, and `gh issue comment`/`gh pr
// merge` join the earlier `pr create`/`issue create` cases as message-flag
// subcommands the narrowed rule keeps trusting.
#[test]
fn heredoc_capture_then_printf_or_gh_listed_subcommand_is_still_flagged() {
    let cases = [
        "x=$(cat <<'EOF'\nsome text\nEOF\n)\nprintf '%s' \"$x\"",
        "x=$(cat <<'EOF'\nsome text\nEOF\n)\ngh issue comment 1 -b \"$x\"",
        "x=$(cat <<'EOF'\nsome text\nEOF\n)\ngh pr merge 1 -b \"$x\"",
    ];
    for cmd in cases {
        let bodies = extract_heredoc_bodies(cmd);
        assert!(bodies[0].is_data_consumer_target, "command {cmd:?}");
    }
}

// PR #463 review: git, bash, and editors read some variables without a
// `$NAME` in the command, so the capture stays scanned when the name has an
// uppercase letter, when the assignment is an env prefix, or when the name
// may be exported.
#[test]
fn heredoc_capture_that_may_run_unnamed_is_not_flagged_as_data_consumer() {
    let cases = [
        "GIT_SSH_COMMAND=$(cat <<'EOF'\nsome text\nEOF\n)",
        "Out=$(cat <<'EOF'\nsome text\nEOF\n)",
        "out=\"$(cat <<'EOF'\nsome text\nEOF\n)\" git fetch",
        "out=$(cat <<'EOF'\nsome text\nEOF\n) other=1 git fetch",
        "out=$(cat <<'EOF'\nsome text\nEOF\n)\nexport out",
        "out=$(cat <<'EOF'\nsome text\nEOF\n)\nex''port out",
        "out=$(cat <<'EOF'\nsome text\nEOF\n)\ntypeset -x out",
        "set -ea\nout=$(cat <<'EOF'\nsome text\nEOF\n)",
        "builtin set -a\nout=$(cat <<'EOF'\nsome text\nEOF\n)",
        "out=$(cat <<'EOF'\nsome text\nEOF\n)\nset -o allexport",
        "set${IFS}-a\nout=$(cat <<'EOF'\nsome text\nEOF\n)",
        "set -$flags\nout=$(cat <<'EOF'\nsome text\nEOF\n)",
        "path=$(cat <<'EOF'\n/tmp/bin\nEOF\n)\nls",
        "fpath=$(cat <<'EOF'\n/tmp/fn\nEOF\n)",
    ];
    for cmd in cases {
        let bodies = extract_heredoc_bodies(cmd);
        assert!(!bodies[0].is_data_consumer_target, "{cmd:?}");
    }
}

// The trusted side: a separator after the `)` ends the assignment, and a
// `set` option cluster without `a` does not export.
#[test]
fn heredoc_capture_that_stays_in_the_shell_is_flagged_as_data_consumer() {
    let cases = [
        "out=$(cat <<'EOF'\nsome text\nEOF\n); true",
        "out=\"$(cat <<'EOF'\nsome text\nEOF\n)\" && true",
        "set -euo pipefail\nout=$(cat <<'EOF'\nsome text\nEOF\n)",
        "out=$(cat <<'EOF'\nsome text\nEOF\n)\nset -- a b",
        "out=$(cat <<'EOF'\nsome text\nEOF\n)\nreset; ls -la",
    ];
    for cmd in cases {
        let bodies = extract_heredoc_bodies(cmd);
        assert!(bodies[0].is_data_consumer_target, "{cmd:?}");
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

// PR #463 review: the same `$'` toggle bug reaches an `AssignmentRhs`
// capture's forwarding check through `following_text`, so a reference can
// splice a keyword past it: `e$'x'ec "$x"` reads as `exec "$x"` to bash.
#[test]
fn heredoc_capture_then_run_variable_via_ansi_c_quote_splice_is_not_flagged() {
    let cmd = "x=$(cat <<'EOF'\nrm -rf /\nEOF\n)\ne$'x'ec \"$x\"";
    let bodies = extract_heredoc_bodies(cmd);
    assert!(!bodies[0].is_data_consumer_target);
}

// Safe idiom: an apostrophe inside the body itself never reaches any of the
// prefix/following-text checks above, since heredoc bodies are excluded
// from both (`embedded_scripts::walk_heredocs`'s `command_text`).
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
    ];
    for prefix in prefixes {
        let cmd = format!("{prefix}\ncat <<'EOF'\nrm -rf /\nEOF");
        let bodies = extract_heredoc_bodies(&cmd);
        let body = bodies.last().expect("heredoc body");
        assert!(!body.is_data_consumer_target, "trusted after {prefix:?}");
    }
}

// Same gap inside the trusted `$(...)` of a message flag or an assignment:
// the command before `cat` inside the substitution must be inert too.
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
