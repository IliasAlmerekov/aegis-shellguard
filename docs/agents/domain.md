# Domain docs

## Before exploring, read these

- `CONTEXT.md` at the repo root: the domain glossary. It plays the role other skills call `GLOSSARY.md`; write new terms here, never into a new `GLOSSARY.md`.
- `docs/adr/`: read the ADRs that touch the area you work in. `docs/adr/README.md` indexes them. New ADRs are `docs/adr/adr-NNN-slug.md` with Status, Context, Decision, Consequences, plus an index line.

## Use the glossary's vocabulary

Name domain concepts with the `CONTEXT.md` term in issue titles, proposals, hypotheses, and test names. A concept missing from the glossary is either invented language (reconsider) or a real gap (note it for `/domain-modeling`).

## Flag ADR conflicts

When your output contradicts an ADR, say so explicitly:

> _Contradicts ADR-027 (one 1.0 release gate), but worth reopening because…_
