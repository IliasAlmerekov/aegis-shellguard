# ADR-043: Dynamic shell words require approval and recovery

## Status

Accepted

## Context

ADR-022 treated a dynamic program word without an operand as inert. Shell
expansion can make the program and its flags visible only at execution time.
An argument supplied through a variable can also change the operation of a
static program. A Safe assessment with Analysis degradation prompts, but did
not by itself request a Snapshot.

## Decision

The router records DynamicSource degradation for a program word built by
expansion even when it has no operand or only flags. Exact, argument-less
`$EDITOR`, `$VISUAL`, and `$PAGER` remain interactive-launch
exceptions only when the entire command, apart from surrounding whitespace,
is exactly that one bare word. Additional segments, assignments, launchers,
quotes, comments, arguments, and redirections revoke the exception. The router
does not enumerate variable writers or infer shell state: namerefs and
assignment expansions such as `${EDITOR:=reboot}` make that inference
incomplete. Other variable forms, including `${...}`, do not inherit the
exception.

`$SHELL` always records unresolved execution, including when argument-less.
A shell can execute stdin from a pipe, redirection, heredoc, or here-string;
an argument-less invocation does not prove interactive use. The remaining
interactive-launch exceptions trust the inherited `EDITOR`, `VISUAL`, and
`PAGER` environment values. They do not prove that the expanded program is
read-only.

A variable supplied as argv to a program outside the read-only command set
also degrades. This check runs for every stage, including one whose nested
source already produced another routed target. An `IFS` assignment degrades
because it changes how later shell words split. The router does not evaluate
the variable, infer its value from earlier assignments, or execute shell code.
The read-only exception for `printf` covers dynamic data arguments only when
its format is a fixed, non-state-writing string. A dynamic leading option,
format, `-v` target, or `%n` target degrades because `printf` can write shell
state. Dynamic `rg --pre` or `rg --hostname-bin` values also degrade because
those options run another program. Both separate and `=` option forms are
checked. A dynamic ripgrep word before the first literal positional operand
also degrades because expansion can turn that word into an executor option.
Dynamic data consumed by a known data-only option, and dynamic paths after a
literal positional pattern, remain read-only. Every plausible effective
program behind an ambiguous launcher option is checked. An `IFS`
assignment inside a launcher prefix also degrades, including after `sudo`
options, but a same-shaped
operand after the program does not. Variables in an `env -S` split string are
treated as dynamic even when shell quoting prevents the outer shell from
expanding them. Markers in shell comments are not expansion.

The read-only command policy lives beside the stage-wide check. `file` is
excluded because its `-C` option writes compiled magic files. Dynamic `file`
argv therefore degrades, including an ordinary dynamic pathname, until a
command-specific grammar can prove a narrower data-only position.

Dynamic stdin is also treated as unresolved when it can become executable
input, including `xargs` here-strings, dynamic plain input redirection for
`xargs` or a recognized interpreter, and active expansion in a pipeline
producer feeding `xargs`.

A write redirection target built by active expansion also degrades before
redirection syntax is stripped from argv. This includes `>`, `>>`, `>|`,
`>&`, and fd-prefixed write redirects. Bash can treat a non-numeric target
after `>&` as a filename. An unresolved target therefore degrades even when
it could instead become an fd number; literal fd numbers and `-` do not
trigger this check. A single-quoted or escaped marker remains a
literal target.

An unresolved execution, including a dynamically sourced interpreter, sets
Effect-opaque execution on the Assessment.
Policy therefore requires confirmation and requests recovery under the
existing Snapshot policy. This is independent of RiskLevel and does not turn
a known Block into an approvable command. A stage containing only single-quoted
or escaped expansion markers does not degrade for them. Because tokenization
removes quoting, a literal marker can still degrade conservatively if another
word in the same stage has active expansion.

## Consequences

Some safe commands with dynamic argv now require approval and a Snapshot.
Accepted false positives include common agent forms such as
`git commit -m "$MSG"`, `cd "$DIR"`, `cargo test $ARGS`, and
`git log --grep "$x"`, `[ -n "$X" ]`, and `test -n "$X"` until
command-specific grammars prove those dynamic
positions are data-only. An ANSI-C quoted `printf` argument can also degrade
conservatively because the current tokenizer does not preserve that quote form;
it may encode a state-writing option or format.
The narrow read-only command set preserves common data-reading commands on
the fast path. Runtime shell expansion remains a non-goal under ADR-010;
uncertainty is recorded instead of claiming to know the expanded command.

This tightening ships without a bypass flag. Accepted false positives retain
human confirmation and the configured Snapshot policy. Release regressions
hold promotion and require a forward fix with data-only grammar coverage;
downgrading restores known auto-approval gaps and is not a security-preserving
rollback.
