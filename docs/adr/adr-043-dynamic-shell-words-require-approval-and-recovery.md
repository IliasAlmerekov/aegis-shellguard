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
`$EDITOR`, `$VISUAL`, `$PAGER`, and `$SHELL` remain interactive-launch
exceptions. Other variable forms, including `${...}`, do not inherit that
exception.

A variable supplied as argv to a program outside the read-only command set
also degrades. This check runs for every stage, including one whose nested
source already produced another routed target. An `IFS` assignment degrades
because it changes how later shell words split. The router does not evaluate
the variable, infer its value from earlier assignments, or execute shell code.
The read-only exception does not cover a dynamic `rg --pre` value because
that option runs a subprocess. Both separate and `=` option forms are checked;
an ordinary dynamic search pattern remains read-only. An `IFS` assignment
inside a launcher prefix also degrades, including after `sudo` options.

An unresolved execution sets Effect-opaque execution on the Assessment.
Policy therefore requires confirmation and requests recovery under the
existing Snapshot policy. This is independent of RiskLevel and does not turn
a known Block into an approvable command. A stage containing only single-quoted
or escaped expansion markers does not degrade for them. Because tokenization
removes quoting, a literal marker can still degrade conservatively if another
word in the same stage has active expansion.

## Consequences

Some safe commands with dynamic argv now require approval and a Snapshot.
The narrow read-only command set preserves common data-reading commands on
the fast path. Runtime shell expansion remains a non-goal under ADR-010;
uncertainty is recorded instead of claiming to know the expanded command.
