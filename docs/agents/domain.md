# Domain docs

## Before exploring, read these

- `CONTEXT.md` at the repo root: the domain glossary. It plays the role other skills call `GLOSSARY.md`; write new terms here, never into a new `GLOSSARY.md`.
- `ARCHITECTURE.md` at the repo root: the crate map, the fail-closed rule, and the project config ratchet. A structural change edits it in the same pull request and explains the change in the PR body.
- Before you move, gate or delete a module, check the advisory index in `SECURITY.md` for fixes that live in it.

## Use the glossary's vocabulary

Name domain concepts with the `CONTEXT.md` term in issue titles, proposals, hypotheses, and test names. A concept missing from the glossary is either invented language (reconsider) or a real gap (note it for `/domain-modeling`).

## Flag conflicts

When your output contradicts `CONTEXT.md`, `ARCHITECTURE.md`, or `PRD.md`, say so explicitly:

> _Contradicts the one-release-gate rule in `AGENTS.md`, but worth reopening because…_
