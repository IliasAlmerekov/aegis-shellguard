# ADR-042: A nowdoc read by a Data consumer is data

## Status

Accepted. Adds the `Data consumer` entry to `CONTEXT.md`. Fixes issue #396.
File writes from #357 and #432 remain scanned because written files can run
without an explicit command naming them. A heredoc inside a `NAME=$(...)`
capture is not trusted here; that case is deferred to a follow-up issue
(item 5).

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
   - the program of the simple command that owns `<<` is the bare word
     `cat` or `jq`, or one of their two absolute-path spellings,
     `/bin/<name>` or `/usr/bin/<name>`, both spelled exactly so, and both
     the paths a stock install actually puts the binary at (issue #396 review).
     The owning command starts after the last `;`, `&&`, `||`, `(`, `$(`,
     backtick or newline, not at the line's first word (#432). Any other
     path (`./cat`, `/tmp/p/cat`, `/usr/local/bin/cat`) can name a copied
     shell and `CAT` is another file on a case-sensitive filesystem, so
     neither counts. A leading assignment (`PATH=/tmp/p cat`) drops the
     trust too, since it can change which `cat` runs. `sed` and `awk` are
     left out on purpose: `sed` has the `e` command and `awk` has
     `system()`. `tee` is left out because it always writes a file, which
     the file-write condition below never trusts (PR #463 review);
   - the marker line has no `|`, quote, backtick or `$` after the delimiter,
     no `>(` or `<(`, and it does not end in a `\` continuation. A quote or
     substitution left open there (`cat <<'EOF' "`, `$(`, `${`) keeps the
     command going past the newline, so bash starts the body on a later
     line than this walk does and runs the lines in between (PR #463
     review);
   - the command text before the `<<`, earlier lines included, has no
     ANSI-C quoting (`$'...'`) and leaves no `'`, `"` or backtick open.
     Inside `$'...'` bash reads `\'` as an escaped quote, while the quote
     tracking here reads it as a close, so `cat $'\'' ' <<'EOF'` looks
     like a marker that bash reads as quoted text. A quote opened on an
     earlier line (`echo 'start` then `cat <<'EOF'`) makes the marker line
     part of a string for bash, and the lines this walk takes for a body
     run as commands (PR #463 review);
   - the consumer does not write to a descriptor above 2 or a variable one
     (`>&3`, `>&$fd`) or to a `/dev/fd/` or `/proc/` path, since an earlier
     `exec 3> >(sh)` can make that descriptor a pipe to a shell;
   - the consumer does not write a file (an unquoted `>` other than
     `>&1`/`>&2`). A written file may run implicitly on a later command,
     such as a Git hook during `git commit` or a startup file when a shell
     starts. No path-based exception can prove that a file is inert, so all
     file writes keep their bodies scanned. This reverses #357 and #432's
     data treatment by explicit request during PR #463 review;
   - an exact delimiter line closes the body, with no earlier line merely
     starting with the delimiter. An ambiguous line keeps the body scanned
     and stops heredoc suspension there, so bash's `EOF)` recovery cannot
     hide later commands;
   - the heredoc sits at top level, or inside exactly one `$(...)` that is
     the whole value of a `git` or `gh` message flag (`git commit -m`,
     `gh pr create --body`). The gates in
     `heredoc_data_consumer/message_flags.rs` decide which flag counts
     under which subcommand. For `git` that is `-m` or `--message` under
     `commit`, `tag`, `merge` or `notes`, with no global option such as
     `-c` or `-C` before the subcommand; `-t` is `--template=<file>` and
     `-b` names a branch, so neither counts. For `gh` it is `-m`,
     `--message`, `-t`, `--title`, `-b` or `--body` under `gh pr create`,
     `edit`, `comment`, `review` or `merge`, or `gh issue create`, `edit`
     or `comment`, read from the two words right after `gh`, so a flag
     before them (`gh -R o/r pr create`) makes the call untrusted. A
     `NAME=$(...)` capture is not a trusted frame (item 5). Any
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
     at a shell and a later plain `cat <<'EOF'` then feeds it. Anything
     before the marker that can change which program `cat` or `jq`
     names makes it untrusted as well. This check is an allowlist, not a
     word search: every simple command before the marker, on earlier lines
     and inside the trusted `$(...)`, must start with a literal command word
     from a fixed inert set (`cd`, `echo`, `false`, `gh`, `git`, `ls`,
     `mkdir`, `pwd`, `true`, and the Data consumer names), or be `set` with
     only `-e`, `-u`, `-v`, `-x` and `-o errexit|nounset|pipefail|verbose|xtrace`
     style options. An assignment, any other builtin, a command word built by
     an expansion (`$(echo eval) git '; ...'`, `$c`), and a word the walk
     cannot read all fail it. The prefix also may not hold a `${...}`,
     `$((...))`, `$[...]`, `$"..."` or subscripted (`$a[i]`) expansion, since
     each can assign a variable from an argument position, nor a heredoc with
     an unquoted delimiter, whose body the prefix leaves out and the shell
     expands (`: <<X` with `$((PATH=0))` in the body runs in the current
     shell). A function definition (`cat() { sh; }`) stays refused as well.
     An earlier version searched the prefix for denylisted words (`alias`,
     `hash`, `PATH`, `eval`, ...). PR #463 review showed that such a list
     trails the shell: `BASH_CMDS[cat]=/bin/bash`, `BASH_ALIASES[cat]=bash`,
     zsh `functions[cat]=sh`, `commands[cat]=/bin/sh` and `path=(...)` all
     rebind `cat` without a listed word.
3. When the predicate holds, the scanner blanks the body (line lengths kept)
   and the router masks it before tokenizing the stage. When it does not, both
   keep their previous behaviour. Unquoted heredocs and interpreter readers
   are not affected.
4. A `cat <<'EOF'` that prints to the terminal is data too. The terminal is
   not a shell. The `heredoc_body_block` case in
   `tests/fixtures/security_bypass_corpus.toml` now uses `bash <<'EOF'`, which
   still runs its body.
5. A heredoc inside a `NAME=$(...)` capture is not trusted in this ADR,
   and its body stays scanned as it was before #396. A draft of PR #463
   trusted the capture while every later use of the variable looked like a
   forward to `gh`, `git`, `curl`, `echo`, `printf` or `jq`, and while the
   name looked out of reach of programs that read variables from the
   environment (`GIT_SSH_COMMAND`, `PS4`, `BASH_ENV`). Each review round
   found another way for later text to run the value. Bash arithmetic
   (`echo $((x))`) evaluates the text as an expression, and an array index
   in it can hold a command substitution. `${x@P}` expands it as a prompt,
   and zsh `${(e)x}` expands it again. A name computed at run time
   (`n=$(printf '\170'); echo $(( $n ))`) never spells `$x` at all. A line
   walk cannot rule these out, so capture trust moves to a follow-up issue.

## Consequences

- A bare `cat` that only prints to stdout and the `git commit -m` /
  `gh --body` idioms are auto-approved whatever their body text says.
  File-writing #357 and #432 shapes are scanned instead, and so is the
  #396 `body=$(jq -c . <<'JSON' ...)` capture (item 5).
- Every shape in the Context list, plus `bash -c "$(cat <<'EOF' ...)"`,
  `eval "$(...)"`, `ssh host "$(...)"`, `echo "$(...)" | sh` and
  `gh alias set --shell`, keeps the body scanned. Tests in
  `crates/aegis-scanner/src/scanner/tests/issue_396.rs`,
  `crates/aegis-parser/src/tests/tokenizer_tests.rs` and
  `src/analysis/router/tests/issue_396.rs` pin both sides.
- The predicate reads lines, not a shell AST. A shape it cannot classify falls
  on the scanned side, so most parsing gaps cost a false positive, not a
  bypass. One kind does not: a line walk can disagree with bash about where a
  heredoc body ends, and when the walker's body runs longer than bash's, the
  extra text bash actually runs gets blanked as inert data instead of
  scanned. PR #463 review found this in bash's own `$(...)` heredoc
  recovery: a body with no line exactly equal to the delimiter, or a line
  that merely starts with it (`EOF)`), lets bash close the heredoc there and
  run whatever follows. The fix drops Data consumer trust for both shapes
  (no exact terminator line, and a delimiter-prefixed non-exact line)
  rather than modeling bash's recovery rule outright. Any other body-end
  disagreement between this walk and bash is the same kind of bypass, not a
  false positive, until it gets the same guard.
  Adding a program to the `Data consumer` list or a flag to the message list
  is a security decision and needs a test for each way its output could reach
  a shell.
- The inert-command allowlist costs a scanned body whenever the prefix
  holds any other command, such as an earlier assignment,
  `cargo build; cat <<'EOF'`, or `git log ${x}`. This false
  positive is accepted: a command outside the set can rebind the consumer
  through a shell table this ADR does not list. Adding a word to the set is
  a security decision and needs a test that it cannot change the shell's
  own state.
- Any file write keeps the body scanned, including `cat > f <<'EOF'` and
  `tee -a ~/.bashrc <<'EOF'`. A dangerous-looking body in #432's `cargo
  build; cat > loop.sh <<'EOF'` now prompts or blocks even though the write
  itself does not execute it. This false positive is accepted so implicit
  execution of a written file cannot inherit Data consumer trust.
- Who reads a written file no longer matters. On 0.6.10 two readers stayed
  out of reach: one started by an earlier Aegis invocation, such as
  `sh /tmp/f &` left waiting on a FIFO from a prior turn, and one in the same
  command that detaches without `&` (`setsid sh /tmp/f; cat > /tmp/f
  <<'EOF'`). The write rule scans the body before the file exists, so both
  readers, and a file that runs implicitly such as
  `cat > .git/hooks/pre-commit <<'EOF'`, get the scanned body. A file
  assembled through other writes or later edits still needs separate
  analysis. This rule does not prove the file will be safe when it runs.
- Capturing a heredoc into a variable keeps the body scanned, whether later
  text runs the variable or only forwards it. `body=$(jq -c . <<'JSON' ...)`
  followed by `gh api ... -f body="$body"` still prompts or blocks when the
  body mentions a dangerous command. This false positive is accepted until
  the follow-up issue lands. The same text passed as
  `gh pr create --body "$(cat <<'EOF' ...)"` or
  `git commit -m "$(cat <<'EOF' ...)"` stays trusted.
