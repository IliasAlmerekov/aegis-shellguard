# ADR-041: The scanner skips a git global option before matching GIT-* rules

## Status

Accepted

## Context

`GIT-001`..`GIT-008` (`crates/aegis-scanner/src/patterns/builtins_a.rs`) and
`GIT-009` (`builtins_b.rs`) are `Token-prefix rule`s anchored at
`["git", "<subcommand>", ...]`. `matches_prefix`
(`crates/aegis-parser/src/prefix_match.rs`) anchors at token 0 of whatever
tokens it is given, and ADR-014's `effective_tokens_at` keeps every token
after `git` unchanged. It only strips launcher prefixes and normalizes an
absolute path ahead of `git`, never anything git accepts as its own option.

`git` itself accepts a run of global options between the program name and
the subcommand: `-C <path>`, `-c <name>=<value>`, `--git-dir=...`,
`--no-pager`, and the rest of `git(1)`'s options section. `git -C . reset
--hard` puts `-C` at position 1 and `reset` at position 3, so every
`GIT-*` rule missed it and the command scored `Safe`. The same shape defeats
`git_push_deletes_remote_refs` (`GIT-009`, `crates/aegis-scanner/src/
scanner/prefix_rule.rs`), which reads `tokens[..2]` and assumes the second
token is literally `push`; and it defeats a `Danger`-level rule
(`GIT-004`, `git filter-branch`) exactly the way `sudo kill -9 1` defeated
`Block` before ADR-014. A command that should have stopped for review ran
in full instead. `git -C . filter-branch` skipped both the prompt and the
Snapshot that `Danger` should trigger (Snapshot policy itself is unchanged
by this ADR; see Consequences).

This is a different bypass than the one ADR-014 closed. ADR-014 normalizes
what comes *before* the program token (`sudo`, `env`, an absolute path).
This gap is git's own option grammar *after* the program token, which no
other prefix-rule family has, because no other family lets its own program
inject option tokens between itself and the "real" subcommand the way `git`
does.

## Decision

The scanner gains a second git-specific matching step. It does not touch
`EffectiveTokenSlice`, `effective_tokens_at`, or the router: those keep
seeing the slice exactly as ADR-014 already produces it, unchanged, because
the router still reads git `-c`/`--config-env` values off that same slice
and assumes it is a suffix of the stage tokens.

1. **A pure helper in `aegis-parser`** (`crates/aegis-parser/src/
   git_options.rs`, `git_option_subcommand_starts`) takes a token slice whose
   first token is `git` and returns every position its subcommand could
   start at, once known git global options are skipped. It returns nothing
   when the second token does not start with `-`, so a plain `git reset
   --hard` never enters this path.
2. **A fixed option table**, checked against `git(1)` 2.55's options section
   and local git 2.43.0 behavior, classifies each option token by how many
   further tokens it consumes: none (`--no-pager`, `-P`, ...), always the
   next token (`-C`, `-c`; git has no `=`-glued form for either), or the
   next token only when this one carries no `=` (`--git-dir`, `--work-tree`,
   `--namespace`, `--config-env`, `--attr-source`). A print-and-exit option
   (`--version`, `--help`, ...) does not stop the walk. Scanning continues
   past it, at the cost of an accepted false positive on `git --help reset
   --hard`. A glued short form git itself rejects (`-C/tmp`, `-cfoo=bar`)
   is not special-cased either; it falls to the unlisted-option rule below.
3. **An option outside that table gets two readings**: one where it takes no
   value, one where it takes the next token as its value. That is the same
   treatment ADR-014 already gives an unknown launcher flag, and for the
   same reason: git 2.43 exits 129 on a genuinely unknown option, and no
   listed global option takes more than one value, so between the two
   readings the real subcommand is never missed. The helper computes the
   *set* of reachable positions by walking the option chain as a small state
   graph, each position visited once, rather than the cross product of
   readings: `n` unlisted options in a row yield at most `n` extra candidate
   positions, not `2^n`.
4. **A cap of 16 candidates per git slice, fail closed.** Each candidate
   costs the scanner a full token-prefix and regex rescan (step 5 below), so
   without a cap, a chain of `n` unlisted options that each add a candidate
   makes that per-slice cost grow with `n`, and total scan cost with `n^2`;
   a review measured 2000 unlisted options at 180ms against 100 at 0.83ms.
   `git_option_subcommand_starts` returns `GitSubcommandStarts::TooMany`
   once the walk finds more than `MAX_GIT_OPTION_CANDIDATES` (16) starts,
   in place of the `Starts(Vec<usize>)` it returns otherwise, and stops
   walking at that point rather than draining the rest of the chain. The
   scanner does not scan any candidate for that slice when it sees
   `TooMany`. Instead it reports a Warn of its own, pattern id `SCAN-004`,
   alongside the existing synthetic `SCAN-001`..`SCAN-003` markers for an
   oversized command, an oversized inline script, and a depth-exceeded
   recursive parse. A command this unusual gets a prompt instead of a
   silent `Safe` and instead of the removed quadratic scan.
5. **The scanner step** (`Scanner::assess`, `crates/aegis-scanner/src/
   scanner/assessment.rs`) runs after `quick_scan`, only for an effective
   slice whose program is `git`, and only when the helper returns at least
   one position. For each position it builds a candidate token list (`git`
   plus the tokens from that position on) and runs both existing mechanisms
   against it: the git-keyed `Token-prefix rule`s, `GIT-009`'s dry-run/refspec
   check included, and the regex rescan (universal and `git`-indexed
   patterns, `Custom` ones from `aegis.toml` included) on the candidate
   joined with spaces. A match this step finds is deduplicated against every
   other match for the command by pattern id, the same as every other source
   `Scanner::assess` already merges.
6. **Evidence stays honest about what it can show.** The candidate string is
   synthetic: the skipped options are cut out of the middle of the original
   command, so a byte offset a regex found inside it is not a raw-command
   substring in general, even though it happens to validate as in-bounds. The
   step reports the subcommand-onward token span instead, which is always a
   real contiguous suffix of the original command; if that span cannot be
   found verbatim in the raw text either, the existing highlight-range
   fallback (`crates/aegis-scanner/src/scanner/highlighting.rs`) already
   drops the highlight rather than pointing at the wrong text.

## Consequences

`git -C . reset --hard`, `git -c x=y clean -fdx`, `git --git-dir=.git branch
-D x`, `git --no-pager push --force`, `git -C . filter-branch`, and the same
forms behind a launcher (`sudo git -C . reset --hard`) now reach their
intended `RiskLevel`, matching every other `GIT-*` command. `git -C repo push
--delete origin x` (already named publicly in the #431/PR #450 review) now
fires `GIT-009`.

A git command carrying more than 16 unrecognized global-option candidates
ahead of its subcommand now warns via `SCAN-004` instead of paying the
removed quadratic scan cost or silently skipping detection. This is a rare
shape for a real git invocation; the accepted cost is an extra prompt on it,
the same trade the `--help` false positive below already makes.

Snapshot targeting is unchanged: it still targets the process cwd
(`src/execution/mod.rs`) even for `git -C other ...`, so a `Danger` command
with a redirected working tree gets a prompt and an attempted Snapshot of the
wrong directory. That gap is tracked as a follow-up once this advisory
publishes, not fixed here.

`git --help reset --hard` now warns even though it only prints `reset`'s
manual page and exits. This is an accepted false positive, in the same
spirit as `git commit -m "drop table feature"` under ADR-015: an extra
prompt is cheaper than a real bypass, and correctly telling the two apart
would need knowledge of git's own runtime option semantics no static scan
has.

Nothing here changes `PrefixRule::match_examples` validation
(`crates/aegis-scanner/src/scanner/prefix_rule.rs`): it checks examples
through `matches_tokens` alone, without this step, so an option-prefixed
example added there would validate against the wrong mechanism.

## Related

Extends [ADR-014](adr-014-launcher-and-absolute-path-normalization-for-token-prefix-detection.md).
