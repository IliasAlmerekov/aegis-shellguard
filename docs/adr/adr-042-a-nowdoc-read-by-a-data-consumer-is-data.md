# ADR-042: A nowdoc read by a Data consumer is data

## Status

Accepted. Adds the `Data consumer` entry to `CONTEXT.md`. Fixes issues #396
and #432 and replaces the file-write-only rule from #357.

## Context

The scanner matches `Pattern`s against the whole command string, and a heredoc
body is part of that string. Since #344 a quoted-delimiter heredoc (nowdoc)
read by a program that is not an interpreter only had its `$(`/backtick
markers masked. Since #357 the body was blanked completely, but only when the
reader was `cat` or `tee` writing to a file, and only when that `cat` or `tee`
was the first word of the line.

Agents build request bodies and commit messages from nowdocs all the time:

- `body=$(jq -c . <<'JSON' ... JSON)` for an HTTP request (#396);
- `git commit -m "$(cat <<'EOF' ... EOF)"` and `gh pr create --body "$(cat <<'EOF' ...)"`;
- `cargo build; cat > f <<'EOF' ... EOF` (#432).

When the text described a dangerous command, as bug reports and test fixtures
for Aegis do, `FS-001` fired on the body and the non-interactive hook denied a
command that runs nothing. A second layer failed the same way. The router's
unclaimed-interpreter net tokenized the heredoc body as if it were argv, so a
JSON string such as `"python3 ./x"` read as an interpreter operand and the
command prompted.

The body is not always inert, though. Probes and review found shapes where a
naive "cat, tee or jq means data" rule lets a shell run the text:
`jq -r .a <<'JSON' | sh`, `cat <<'EOF' > >(sh)`, a `$(` or `<(` opened on the
line before the marker, a subshell, `{ ...; }` group or loop piped to `sh`
after the terminator, `cat <<'EOF' >&3`, and `git -c "alias.x=!$(cat <<'EOF' ...)"`, where git runs
a `!` alias through the shell. `xargs <<'EOF'` also turns stdin
into argv, so the router cannot drop body lines for every heredoc.

## Decision

1. One predicate, `heredoc_target_is_data_consumer` in
   `crates/aegis-parser/src/heredoc_data_consumer.rs`, decides whether a
   heredoc is read by a `Data consumer`. `walk_heredocs` computes it once per
   marker. The scanner (`mask_inert_heredoc_substitution_markers`) and the
   router (`unclaimed_interpreter_net`) both use its result, so the two layers
   cannot disagree.
2. The body is data only when every condition holds:
   - the delimiter is quoted, so bash expands nothing in the body;
   - the program of the simple command that owns `<<` is `cat`, `tee` or `jq`.
     The owning command starts after the last `;`, `&&`, `||`, `(`, `$(`,
     backtick or newline, not at the line's first word (#432). `sed` and `awk`
     are left out on purpose: `sed` has the `e` command and `awk` has
     `system()`;
   - no `|` follows the delimiter on the marker line, the line has no `>(` or
     `<(`, and it does not end in a `\` continuation;
   - the consumer does not write to a descriptor above 2 or a variable one
     (`>&3`, `>&$fd`) or to a `/dev/fd/` or `/proc/` path, since an earlier
     `exec 3> >(sh)` can make that descriptor a pipe to a shell;
   - the heredoc sits at top level, or inside exactly one `$(...)` that is
     either the right side of `NAME=` or the whole value of a `git` or `gh`
     message flag in a position item 5's table trusts for that program
     (`git commit -m`, `gh pr create --body`; one gate in `forwarding.rs`
     serves both this rule and item 5, so they cannot disagree). Any
     other open frame before the marker (a subshell `(`, a process
     substitution `<(`/`>(`, a backtick, a second `$(`) makes it untrusted,
     because its output can reach a pipe or a shell after the terminator line
     (`(cat <<'EOF' ... EOF` then `) | sh`). The frames are read from every
     command line before the marker, not only the marker's own line. An open
     `{ ...; }` group counts as a frame too (`{ true; cat <<'EOF'` then
     `} >&3`). So does an unquoted `#` comment, which never closes: the shell
     ignores a `}` or `)` after it, and the frame reader does not model that.
     A compound-command keyword anywhere before the marker (`case`,
     `coproc`, `do`, `elif`, `else`, `for`, `function`, `if`, `select`,
     `then`, `until`, `while`) also makes it untrusted: `done | sh` or
     `fi >&3` after the terminator can take the output, and a `case`
     pattern's `)` would close the wrong frame. An `exec` anywhere before
     the marker does the same, because `exec > >(sh)` points stdout itself
     at a shell and a later plain `cat <<'EOF'` then feeds it. The word
     check ignores quotes, so a quoted keyword costs a scanned body, never a
     skipped one.
3. When the predicate holds, the scanner blanks the body (line lengths kept)
   and the router masks it before tokenizing the stage. When it does not, both
   keep their previous behaviour. Unquoted heredocs and interpreter readers
   are not affected.
4. A `cat <<'EOF'` that prints to the terminal is data too. The terminal is
   not a shell. The `heredoc_body_block` case in
   `tests/fixtures/security_bypass_corpus.toml` now uses `bash <<'EOF'`, which
   still runs its body.
5. `AssignmentRhs` trust (rule 2's last bullet, the `NAME=$(...)` case) holds
   only while every later reference to the captured variable is provably
   forwarding only. This is an allowlist, not a blocklist of run shapes
   (`crates/aegis-parser/src/heredoc_data_consumer/forwarding.rs`): a
   reference forwards when it sits inside a top-level simple command whose
   program, basename normalized, is `gh`, `git`, `curl`, `echo`, `printf`
   or `jq`; that command carries no pipe and no redirect other than
   `2>&1` or `>&2`; the reference is not laundered through a leading
   `NAME=value` assignment ahead of the program; and the reference sits in
   a position where that program treats it as data it sends or prints:

   | Program  | Trusted positions for the reference |
   |----------|-------------------------------------|
   | `git`    | value of `-m` or `--message`, only under `git commit`, `tag`, `merge` or `notes` with no global option before the subcommand |
   | `gh`     | value of `-m`, `--message`, `-t`, `--title`, `-b` or `--body`, only under `gh pr create`, `edit`, `comment`, `review` or `merge`, or `gh issue create`, `edit` or `comment`; value of `-f`/`--raw-field` (`key="$x"`), only under `gh api` |
   | `curl`   | value of `--data-raw`; value of `-d`, `--data`, `--data-binary`, `--data-urlencode` or `--json` only when the captured body does not start with `@` |
   | `jq`     | value of `--arg NAME` or `--argjson NAME` |
   | `echo`   | any argument |
   | `printf` | any argument from index 1 onward, and only when the first argument does not start with `-` |

   Positions outside the table read the value as a path, a URL, code or a
   config, or give a flag a meaning the table does not cover: `gh api
   --input`, `gh -F`/`--field` (which reads `@file`), `gh --body-file`,
   `curl -T`, `-K`, `--config`, `-o`, `-F`, a curl URL, `jq -f`,
   `--rawfile`, `--slurpfile` and the positional jq filter (`jq -n "$x"`
   runs the body as a jq program). A body that starts with `@` turns a
   curl data flag into a file read (`-d @/etc/shadow`), so `walk_heredocs`
   passes that fact to the check. `gh alias`, `gh extension`, and every
   `gh` subcommand outside the table's list (`gh pr checkout`, `gh repo
   create`, `gh workflow run`, ...) are excluded even though `gh` is on
   the list, since a message flag or `-f` can mean something other than a
   stored message there, or nothing at all. `gh`'s subcommand is the two
   words right after `gh`, so a flag before them (`gh -R o/r pr create`)
   makes the call untrusted: a value-taking flag would otherwise shift which
   word reads as the action. For `git`, `-t` is `--template=<file>` and
   `-b` names a branch, so neither is trusted, and a global option such as
   `-c` or `-C` before the subcommand makes the call untrusted. `printf -v`,
   `printf --`, or any other leading option makes the whole call
   untrusted too, since an option shifts where the format string actually
   falls and `-v` sends the value to a second variable this check does not
   follow.
   Everything else keeps the body scanned: a wrapper (`sudo $x`, `env $x`,
   `command $x`, `nohup $x`, `time $x`, `find ... -exec $x \;`), a
   grouping or compound construct (`($x)`, `{ $x; }`, an
   `if`/`for`/`while`/`until`/`select` body), a parameter expansion
   (`${x:-}`, `${x%%foo}`), a second layer of capture (`z=$($x)`,
   `` z=`$x` ``, `arr=($x)`), a nameref (`declare -n r=x` then `$r`), a
   here-string (`bash <<< "$x"`), a store into `alias`, `trap` or
   `PROMPT_COMMAND`, `eval`/`source`/a bare `.` re-parsing the text, a
   write followed by a run of the file (`echo "$x" > f.sh; sh f.sh`), and
   a program outside that six-name list, whatever it is.
   `assignment_variable_runs_later` in `heredoc_data_consumer.rs` checks
   the text after the heredoc's terminator line for all of this;
   `walk_heredocs` in `embedded_scripts.rs` threads that text through. The
   `body=$(jq -c . <<'JSON' ...)` then `gh api ... -f body="$body"` idiom
   from the Context list keeps its trust.

## Consequences

- The #396, #432 and #357 shapes, and the `git commit -m` / `gh --body`
  idioms, are auto-approved whatever their text says.
- Every shape in the Context list, plus `bash -c "$(cat <<'EOF' ...)"`,
  `eval "$(...)"`, `ssh host "$(...)"`, `echo "$(...)" | sh` and
  `gh alias set --shell`, keeps the body scanned. Tests in
  `crates/aegis-scanner/src/scanner/tests/issue_396.rs`,
  `crates/aegis-parser/src/tests/tokenizer_tests.rs` and
  `src/analysis/router/tests/issue_396.rs` pin both sides.
- The predicate reads lines, not a shell AST. A shape it cannot classify falls
  on the scanned side, so a parsing gap costs a false positive, not a bypass.
  Adding a program to the `Data consumer` list or a flag to the message list
  is a security decision and needs a test for each way its output could reach
  a shell.
- Writing a script to disk with `jq` and running it in the same command
  (`jq -r .a > f <<'JSON' && bash f`) now prompts instead of matching the
  body, because the router cannot read the file before it exists. It is not
  auto-approved.
- Capturing a heredoc into a variable and then running that variable, be it
  `x=$(cat <<'EOF' ...)` then `$x`, `eval "$x"`, `bash -c "$x"`, `$x` piped
  or fed into `<(...)`/`>(...)`, or any of the wrapper, grouping,
  parameter-expansion, second-capture, nameref, here-string, alias/trap and
  write-then-run shapes item 5 lists, keeps the body scanned instead of
  trusting the assignment. A security review before this ADR's forwarding
  allowlist found that an earlier blocklist of run shapes let `sudo $x`,
  `($x)`, `{ $x; }`, `if`/`for`/`while` wrapping, `find -exec`, a nameref
  via `declare -n`, `${x:-}` and other parameter-expansion forms, and more
  slip through as auto-approved; the allowlist closes that gap by trusting
  only the shapes item 5 names, not by naming the ways to bypass it. The
  check is still a text scan, not a shell parser: a chain that merely
  contains both a pipe and a reference to the variable counts as running it
  even when the two are in different pipeline stages. The accepted false
  positives are the same shape: a captured value handed to a program
  outside `gh`, `git`, `curl`, `echo`, `printf` and `jq`, a `cp` writing it
  to a file among them, keeps the body scanned even though that program may
  never run it as code. A listed program that gets the value outside the
  item 5 table does the same, for example a `gh` positional argument or a
  `printf` format string.
