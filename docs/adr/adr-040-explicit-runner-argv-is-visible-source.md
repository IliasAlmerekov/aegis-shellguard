# ADR-040: Explicit runner argv is visible source

## Status

Accepted

## Context

ADR-022 routes explicit interpreters and script files, but its package/build
runner non-goal did not distinguish manifest expansion from a runner that names
its child program in argv. A command such as `uv run python -c 'print(1)'`
therefore lost the Effective program and could miss scanner and language-aware
Matches. The same gap applies to visible scripts behind other command runners.

## Decision

The parser recognizes the unambiguous prefixes `uv run`, `uv tool run`, `uvx`,
`poetry run`, `pipenv run`, `pipx run`, and `npx`, including path-qualified
runner programs. Its `Runner` type owns the shared prefix grammar. The scanner
sees the child as the Effective program; the router inspects its visible
inline source or script file. `uv run` and `pipx run` treat a bare `.py`
operand as Python even without a shebang, and `uv run --script` forces Python
script routing.

An option that prevents reliable child-argv recovery, including `npx -c`,
records Analysis degradation instead of parsing the command string as a known
child. For `npx`, `uvx`, `uv tool run` (the long form of `uvx`), and
package-style `pipx run`, a visible interpreter's source is still analyzed, but
uncertainty about the package-provided executable also records Analysis
degradation. A visible Match is retained alongside it. The same degradation
covers a direct-exec operand behind one of these runners (`npx ./x`, `uvx
./x`, `pipx run ./x`, `uv tool run ./x`): its content is still read and
matched when a shebang resolves it, but the runner's own executable-selection
uncertainty degrades the route regardless of what that content says. Relative
script paths under an unresolved runner cwd option degrade. No package
manifest, executable lookup, or dependency traversal is attempted.

## Consequences

Explicit child source can raise RiskLevel and request a Snapshot where it was
previously treated as Safe. A package-selected executable can still differ from
the visible name, so its source Match never cancels the degradation. Unsupported
runner options and manifest aliases remain outside the static source claim.
