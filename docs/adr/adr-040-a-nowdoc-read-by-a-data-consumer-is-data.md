# ADR-040: A nowdoc read by a Data consumer is data

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
     either the right side of `NAME=` or the whole value of a `git`/`gh`
     message flag (`-m`, `--message`, `-t`, `--title`, `-b`, `--body`). Any
     other open frame before the marker (a subshell `(`, a process
     substitution `<(`/`>(`, a backtick, a second `$(`) makes it untrusted,
     because its output can reach a pipe or a shell after the terminator line
     (`(cat <<'EOF' ... EOF` then `) | sh`). The frames are read from every
     command line before the marker, not only the marker's own line. An open
     `{ ...; }` group counts as a frame too (`{ true; cat <<'EOF'` then
     `} >&3`). A compound-command keyword anywhere before the marker (`case`,
     `coproc`, `do`, `elif`, `else`, `for`, `function`, `if`, `select`,
     `then`, `until`, `while`) also makes it untrusted: `done | sh` or
     `fi >&3` after the terminator can take the output, and a `case`
     pattern's `)` would close the wrong frame. The keyword check ignores
     quotes, so a quoted keyword costs a scanned body, never a skipped one.
3. When the predicate holds, the scanner blanks the body (line lengths kept)
   and the router masks it before tokenizing the stage. When it does not, both
   keep their previous behaviour. Unquoted heredocs and interpreter readers
   are not affected.
4. A `cat <<'EOF'` that prints to the terminal is data too. The terminal is
   not a shell. The `heredoc_body_block` case in
   `tests/fixtures/security_bypass_corpus.toml` now uses `bash <<'EOF'`, which
   still runs its body.

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
