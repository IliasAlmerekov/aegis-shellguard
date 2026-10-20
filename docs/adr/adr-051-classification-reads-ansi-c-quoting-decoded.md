# ADR-051: Classification reads ANSI-C quoting decoded

## Status

Accepted.

## Context

Bash expands ANSI-C quoting (`$'...'`) before it runs a word. Escapes such as
`\NNN`, `\xHH`, `\uHHHH` and `\cX` produce characters, and `\'` puts a quote
inside the piece without ending it. `$'\162m'` therefore runs `rm`.

The tokenizer read `$'...'` as a `$` followed by a single-quoted string. It
kept the escapes as written, so the program word did not match any rule.
Every raw-text walk (segment splitters, substitution and heredoc walkers,
`[[rules]]` compound detection, the router) toggles quote state on each `'`.
A `\'` inside a piece moved the quote edges those walks saw, so a `;` that
bash treats as a separator could read as quoted text.

ADR-043 recorded that an ANSI-C quoted `printf` argument degrades because the
tokenizer did not preserve that quote form. The router's Rule C degraded any
unclaimed stage with unquoted `$'` for the same reason.

## Decision

1. `aegis_parser::split_tokens` decodes `$'...'` the way bash's `ansicstr`
   does. Locale quoting (`$"..."`) reads as plain double quotes. A NUL ends
   the decoded piece, as in bash. The other backslash rules follow bash as
   well: inside double quotes a backslash goes only before `$`, backtick,
   `"`, `\` and newline (`"r\m"` stays `r\m`), and a backslash-newline outside
   single quotes is a line continuation that joins the word (`r\<newline>m`
   is `rm`).
2. `aegis_parser::canonical_ansi_c_quoting` rewrites every unquoted
   `$'...'` piece as the plain single-quoted word it decodes to (`'\''` for an
   embedded quote). `Scanner::assess`, `Scanner::git_stage_report` and
   `[[rules]]` evaluation read this canonical text, so every raw-text walk
   sees the same words and boundaries as bash. The Assessment keeps the
   command as written in `ParsedCommand::raw` for audit, and highlight
   ranges are recomputed against it.
3. The router reads `aegis_parser::canonical_ansi_c_quoting_marked`: the
   same text, but each rewritten piece keeps an empty `$''` in front
   (`$'\162m'` reads as `$'''rm'`). Bash runs the same word, the quote edges
   stay correct, and the router keeps its conservative rules for ANSI-C
   quoting unchanged: Rule C degrades any unclaimed stage that used `$'`, and
   the dynamic-argument net treats an ANSI-C quoted argument like a dynamic
   one (ADR-043). Decoding does not relax any routing decision.
4. A piece that does not decode exactly fails closed. It never closes, holds a
   NUL, or decodes to bytes that are not valid UTF-8 (including a `\u`/`\U`
   value that is not a Unicode scalar). `aegis_parser::ansi_c_quoting_is_inexact`
   reports it. The canonical text keeps its raw spelling, the scanner adds the
   synthetic `SCAN-005` Warn and marks the Assessment Effect-opaque, and
   router Rule C degrades the stage.
5. A `[[block]]` entry matches the command as written or its unquoted form:
   the tokenizer's words (quotes removed, escapes and ANSI-C quoting decoded)
   joined by single spaces. `"rm" -rf x`, `$'rm' -rf x` and `r''m -rf x`
   therefore hit an entry for `rm -rf *`. An `[[allow]]` entry matches
   only the command as written, as before. Matching the unquoted form there
   would let a quoted spelling reach an entry its author never wrote for it,
   and the allowlist must never gain a match through decoding.

## Consequences

- An ANSI-C or locale-quoted spelling classifies like the word bash runs,
  including inside `$(...)`, after `&&`, behind launchers and in `bash -c`
  bodies.
- Routing stays as conservative as before: `printf $'a\n'` and
  `strace -o $'x' ./pyx` still degrade. What changes is that an escaped
  quote inside a piece no longer hides the segments after it, so a later
  interpreter launch is routed. The ADR-043 note about ANSI-C quoted
  `printf` arguments stays in force.
- Command substitution (`$(...)` or backticks) starts a fresh quoting
  context in the canonical text, even inside double quotes, so a piece in
  `"$(... $'...' ...)"` is rewritten too and the substitution body splits
  like bash. A `)` that closes a `case` pattern can end that context early;
  the rest of the body then keeps its raw spelling, as before this decision.
- The safe path pays one substring check for `$'` and `$"`.
