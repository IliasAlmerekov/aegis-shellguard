# ADR-032 — Outbound irreversible actions are a named threat class

## Status

Accepted.

## Context

Every rule in the built-in set guards destruction of local or otherwise
controlled state: files wiped, databases truncated, docker volumes removed.
`npm publish` and `npm unpublish` are different in kind. They destroy nothing
locally, but once directed outward against the package registry they cannot be
undone — npm forbids republishing a version and heavily restricts unpublishing.
Until now no `npm` rule existed at all, so an agent could publish under the
user's credentials without any prompt.

## Decision

Name the class and detect its first two members. Add to the domain glossary the
term **`Outbound irreversible action`** — a command that destroys nothing locally
but, once directed outward, cannot be undone — with an `_Avoid_` list ("publish",
"deploy", "destructive") that keeps the term distinct from existing vocabulary.

Add two token-prefix rules:

- `PKG-006` — `npm publish`, `Warn`, category `Package`. Publishing is a normal
  intended act that must not happen unattended; `Warn` rather than `Danger`
  because it is not destruction.
- `PKG-007` — `npm unpublish`, `Danger`, category `Package`. Strictly worse than
  publishing: it breaks every consumer already depending on the version.

`PKG-006` declares the negative condition `--dry-run` so the rehearsal an agent
runs first stays `Safe`, in any flag position. `PKG-007` declares none.

## Negative-condition mechanism

A `Token-prefix rule` matches a prefix of the token list and ignores the tail,
so a rule on `npm publish` also fires on `npm publish --dry-run`. A rule that
shouts at a dry run is a rule the user disables on day one, so the matcher
supports a `suppressed_by` list: tokens whose presence anywhere in the command's
tokens suppress the rule.

The cost is a second way a rule can fail closed. A `suppressed_by` token can
silence a rule, so the list must stay short and semantically unambiguous — every
token on it must mean "this invocation does not perform the action" on its own,
regardless of what else is on the command line. Blank tokens are rejected by
validation because a blank entry reads as a guard that is not there.

## Precedent

`Outbound irreversible action` sets the class for future candidates that destroy
nothing locally but cannot be undone: `gh release create`, `terraform apply`.
New members should be filed as their own rules under the glossary term rather
than broadened into a generic "network" rule.

## Consequences

- `npm publish` reaches `Warn` and prompts; `npm publish --dry-run` stays `Safe`.
- `npm unpublish` reaches `Danger`.
- `npm` is seeded as a quick-scan keyword; the ADR-002 safe-path budget is
  unchanged because the added keyword is one literal in the existing automaton.
- Aegis' own release workflow publishes on a GitHub runner where Aegis is not
  installed as `$SHELL`, so CI is unaffected.
