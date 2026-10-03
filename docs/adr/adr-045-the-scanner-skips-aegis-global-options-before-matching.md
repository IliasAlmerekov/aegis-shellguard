# ADR-045: The scanner skips Aegis global options before matching AEG-* rules

## Status

Accepted (extends ADR-035, mirrors ADR-041).

## Context

The `AEG-*` Self-management rules (ADR-035) are token-prefix rules anchored
at `["aegis", "<subcommand>"]`. The `aegis` binary accepts global options
before its subcommand: `-c`/`--command <value>`, `--output <value>`,
`--verbosity <value>`, `--quiet`, and `-v`/`--verbose`. Clap parses
`aegis --quiet off` exactly like `aegis off`, but the option moves `off` to
position 2, so no `AEG-*` rule matched and the command scored `Safe`.

Verified on 0.6.10: `aegis --output text off` and `aegis --verbosity quiet
off` were auto-approved. With `aegis uninstall` (ADR-044) the same shape
reached `aegis --quiet uninstall --purge-data`, which removes every
integration and deletes all snapshots without a prompt.

## Decision

Add a git-style rescan for `aegis` slices, as ADR-041 did for git:

1. `aegis_parser::aegis_option_subcommand_start` takes a slice whose first
   token is `aegis` and returns the subcommand position once known global
   options are skipped. It handles long options with a glued `=value` or a
   separate value, and short clusters such as `-vc <value>` or `-c<value>`.
2. The option table is closed. Unlike git, Aegis owns its own CLI, and clap
   exits with a usage error on an unknown option before any subcommand runs.
   The walk therefore stops at an unknown option and reports nothing.
3. The scanner re-runs the `aegis` prefix rules on `aegis` followed by the
   tokens from that position, and reports the subcommand-onward span as the
   matched text.
4. A unit test in the binary walks every global argument clap defines on
   `Cli` and asserts the parser skips each spelling. Adding a global option
   without updating the table fails that test.

## Consequences

- `aegis <global options> <subcommand>` gets the same risk as the bare
  subcommand for every `AEG-*` rule, current and future.
- Cost is one table walk per `aegis` slice whose second token starts with
  `-`. Other programs never enter this path.
- Print-and-exit options (`--help`, `--version`) are outside the table, so
  `aegis --help off` stays `Safe`. Clap prints help and runs nothing, so no
  state changes.
