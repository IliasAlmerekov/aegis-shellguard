# ADR-025 — Recursive chmod over system roots is target-keyed

## Status

Accepted

## Context

Existing `chmod` detections key on the mode: `PS-005` detects literal `777`,
and `FS-007` detects world-writable `x77` modes. That leaves a destructive
recursive rewrite of a system tree undetected when its mode is otherwise
ordinary: `chmod -R 755 /usr` can remove setuid bits and alter package-managed
permissions just as surely as `chmod -R 000 /` can lock a machine out.

This is a program-led command, so its normal delivery follows ADR-014: the
scanner resolves the `Effective program` and applies a token-prefix rule after
launcher stripping, absolute-path basename normalization, and compound-command
segmentation. The destructive property is not the program verb or a particular
mode; it is recursion applied to a system-root target.

## Decision

Add `FS-019`, a `Danger` Filesystem token-prefix rule. It requires `chmod`, the
case-sensitive recursive option (`-R`, including a short-flag cluster, or
`--recursive`), and one literal target from this system-root set:

- `/`
- `/usr` and `/usr/`
- `/etc` and `/etc/`
- `/bin` and `/bin/`
- `/sbin` and `/sbin/`
- `/lib` and `/lib/`
- `/var` and `/var/`
- `/boot` and `/boot/`

The target spellings are alternatives inside `FS-019`; trailing-slash handling
does not change the shared token comparison. A shared normalization would alter
every existing prefix rule without a corresponding rule-by-rule narrowness
review.

`FS-019` is `Danger`, not an `Intrinsic Block`: recursive permission changes to
a system root can be intentional inside a throwaway container, so Aegis must
request human approval rather than make the operation unoverridable.

`PS-005` retains its ID and matching behavior but changes category from
`Process` to `Filesystem`. The ID remains stable because audit entries and
allowlist configuration use it as their contract. `chmod -R 777 /` may match
both rules; both have `Danger` risk, and matches are preserved rather than
collapsed.

The accepted non-coverage is literal and bounded: glob targets such as `/*`
and targets made relative by an earlier `cd` do not match. Resolving them would
require shell expansion or evaluation, which remains outside the scanner's
heuristic contract.

## Consequences

- `chmod -R 000 /`, clustered `chmod -Rf 000 /`, and long-form
  `chmod --recursive 000 /` receive a `Danger` confirmation.
- Mode changes do not weaken detection: `chmod -R 755 /usr` and
  `chmod -R 700 /etc` are covered.
- `chmod -r 000 /`, non-recursive `chmod 000 /etc/passwd`, and ordinary
  application targets stay outside `FS-019`.
- The rule shares ADR-014's launcher, absolute-path, and compound-command
  behavior while leaving the safe quick-scan path unchanged.
