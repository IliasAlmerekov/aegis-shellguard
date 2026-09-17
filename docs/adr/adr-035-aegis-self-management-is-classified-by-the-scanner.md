# ADR-035 — Aegis self-management is classified by the scanner, not the hook

## Status

Accepted.

## Context

Until now the only thing standing between an agent and `aegis off` was the
Claude Code / Codex hook. The hook denies a command whose **first word** is
literally `aegis` and that is not read-only (#333). Everything else was handed
to the scanner, which had no rule for Aegis' own commands and therefore
returned `Safe`:

```
$ aegis --output json --command 'echo hi && aegis off'
  "risk": "safe",
  "decision": "auto_approve",
```

The same held for `/usr/bin/aegis off`, `env aegis off` and `command aegis off`.
A first-word string check is the wrong shape for this: it cannot see a launcher
prefix, an absolute path, or a second command after `&&`. The deny reason it
prints — these commands are "reserved for the human operator" — was therefore
only true for one spelling out of many.

## Decision

Classify Aegis' own state-changing commands in the scanner, where launcher and
absolute-path normalization (ADR-014) already resolves the Effective program and
where every segment of a compound command is scanned.

Add to the domain glossary the term **`Self-management command`** and a
`Category::Aegis` that holds nothing else, so an audit entry naming the category
says what was touched without the reader mapping a rule ID.

Six token-prefix rules:

- `AEG-001` — `aegis off`, `Danger`. Turning the toggle off removes
  classification, confirmation, and snapshots from every command that follows.
- `AEG-002` — `aegis rollback`, `Danger`. Restoring a snapshot overwrites the
  files it covers.
- `AEG-003` — `aegis snapshot prune --yes`, `Danger`. Pruned artifacts cannot be
  restored, and `aegis rollback` then has nothing to restore. The `--yes` token
  is part of the pattern rather than a negative condition, because prune without
  it is already a dry-run preview.
- `AEG-004` — `aegis config init`, `Warn`.
- `AEG-005` — `aegis install-hooks` (and its `install` alias), `Warn`.
- `AEG-006` — `aegis setup-shell`, `Warn`.

`Danger` and not `Block` throughout. Aegis runs as the operator's `$SHELL`, so a
`Block` on `aegis off` would take away the operator's own escape hatch; the point
is a confirmation prompt, not a wall.

Commands that only read state or only tighten enforcement carry no rule and stay
`Safe`: `on`, `status`, `audit`, `snapshot list`, `config show`,
`config validate`, and `snapshot prune` without `--yes`.

## The hook deny stays

The hook keeps denying non-read-only `aegis` first-word commands. It is now a
fast path in front of the scanner rather than the boundary, and it is the only
layer that can refuse without a prompt in a non-interactive agent session. The
scanner rules are what make the claim in its deny reason true for every spelling.

## Consequences

- `echo hi && aegis off`, `/usr/bin/aegis off`, `env aegis off` and
  `sudo aegis off` reach `Danger` and prompt.
- `aegis` is seeded as a quick-scan keyword. The ADR-034 safe-path budget is
  unchanged: one more literal in the existing Aho-Corasick automaton.
- Adding `Category::Aegis` extends the config JSON schema and the `category`
  string in policy JSON output with the value `aegis`.
- An operator who wants no prompt on their own machine can turn the toggle off
  from a terminal Aegis does not proxy.
