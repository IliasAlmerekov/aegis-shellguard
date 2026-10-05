# ADR-046: Token-prefix matches respect quote boundaries and `--`

## Status

Accepted.

## Context

Each logical segment reaches the scanner as its words joined by single spaces
(`logical_segments`). The scanner re-tokenizes that string for the
token-prefix rules, so a quoted argument with a space splits again. For
`git push origin 'x --force'`, git receives one argument `x --force`, but the
prefix rules saw a standalone `--force` and GIT-003 fired. The same happened
to every rule with `AnyStar` before a flag (GIT-002, GIT-003, FS-020 and
others) (#484).

A second false match came from `--`. After `--`, getopt, git's parse-options,
popt and argparse treat every later word as an operand. `AnyStar` skipped
past `--` anyway, so `git push origin -- --force` matched GIT-003.

The lossy re-split is not only a defect. A command string handed to a
program, such as a git alias body in `git -c alias.x='!git push --force' x`,
reaches the prefix rules only because its words split again. Dropping the
re-split would turn those matches into false negatives.

## Decision

1. `aegis_parser::logical_scan_segments` returns each segment as a
   `ScanSegment` holding both the normalized string and the
   quote-preserving tokens. `logical_segments` keeps its old output.
2. The scanner still matches prefix rules against the re-split tokens. After
   each target, it removes a token-prefix match only when both of these hold:
   - the same rule does not match the segment's quote-preserving tokens,
     through the same launcher stripping and git/aegis option skipping;
   - no quoted token that re-splits into several words contains the name of
     a program the rule is indexed under.
3. `AnyStar` stops at a `--` token when the next pattern element only
   matches flags and the rule is anchored at a program in
   `DOUBLE_DASH_PROGRAMS`: `git`, `rm`, `rsync`, `sgdisk` and `wipefs`. Each
   was run with a flag after `--` and treated it as an operand. Before an
   operand element, such as PS-008's `/`, `AnyStar` still skips `--`.
   The stop applies only to quote-preserving tokens, through
   `matches_prefix_ending_options`. The main scan runs on the re-split
   tokens, where a `--` may come from a quoted or escaped argument
   (`git push origin 'a --' --force`), so there `AnyStar` skips `--` as
   before. The filter in decision 2 then applies the stop: it runs for every
   segment, including one whose quoted tokens equal the re-split tokens, and
   drops a match that the quoted tokens do not support.

## Consequences

- The main scan finds the same matches as before #484. The filter then removes
  only a match the quote-preserving tokens contradict, by a quoted word or by
  a real `--`. A segment with no quote-preserving tokens, or two segments that
  normalize alike with different tokens, is not filtered and keeps every
  match. A `--` hidden inside a quoted argument therefore cannot hide a flag.
  A differential run over 7130 commands from the test suite and
  rule examples changed no verdict except the #484 cases.
- Some false positives remain. They fail safe and prompt.
  - The recursive path (heredoc, process substitution, backticks, `eval`)
    keeps no quote-preserving tokens, so it does not filter.
  - `strip_env_prefix` also joins words with spaces, so a target stripped of
    an env prefix may have no quote-preserving tokens and is not filtered.
  - A quoted argument that names the program, such as
    `git commit -m 'revert git push --force'`, keeps its match.
- `aws`, `gsutil` and `gcloud` are not in the list, because they could not
  be run here. `aws s3 rb s3://b -- --force` still matches CL-011. Adding a
  program to the list needs the same check.
