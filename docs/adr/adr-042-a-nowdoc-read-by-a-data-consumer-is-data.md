# ADR-042: A nowdoc read by a Data consumer is data

## Status

Accepted. Adds the `Data consumer` entry to `CONTEXT.md`. Fixes issue #396.
File writes from #357 and #432 remain scanned because written files can run
without an explicit command naming them.

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
     `/bin/<name>` or `/usr/bin/<name>` — both spelled exactly so, and both
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
     at a shell and a later plain `cat <<'EOF'` then feeds it. Anything
     before the marker that can change which program `cat` or `jq`
     names makes it untrusted as well: a function definition (`cat() { sh;
     }`), `alias`, `hash`, `enable`, `eval`, `source` or a `.` in command
     position, the word `PATH`, the declare and read families and `getopts`
     (which can write a computed name such as `declare -n r="${x}TH"`),
     `printf -v`, and arithmetic `((` (`(( $n = 5 ))` makes `PATH` the
     relative directory `5`). The word check ignores quotes and drops the
     quote characters first, so `PA""TH` still reads as `PATH`, and a quoted
     keyword costs a scanned body, never a skipped one. The function and
     alias checks are deliberately not scoped to the names `cat` or `jq`: a
     script can build the name through quoting (`c""at`) or `eval` before
     defining or aliasing it, so narrowing the check to those two names would just move the gap there; the only cost of checking every
     name is a body that gets scanned instead of trusted.
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

- The #396 `jq` capture shape, a bare `cat` that only prints to stdout, and
  the `git commit -m` / `gh --body` idioms are auto-approved whatever their
  body text says. File-writing #357 and #432 shapes are scanned instead.
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
