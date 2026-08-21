# AGENTS.md — Aegis (Codex instructions)

**Aegis** is a lightweight Rust CLI that acts as a `$SHELL` proxy for AI coding agents, intercepting commands and requiring human confirmation before destructive operations. It must stay fast (< 2ms for safe paths), correct, and minimal.

See [`CLAUDE.md`](CLAUDE.md) for detailed project conventions; this document is the entry point.

---

## Session Context — read before any code change

Before writing code or running commands, read these documents in order:

1. [`PROJECT_STATE.md`](PROJECT_STATE.md) — last session's work and open blockers
2. [`CONVENTION.md`](CONVENTION.md) — authoritative rules (precedence: security invariants → CI gates → architecture → style)
3. [`CONTEXT.md`](CONTEXT.md) — domain glossary; use canonical terms in code and commits
4. The [`1.0` milestone](https://github.com/IliasAlmerekov/aegis-shellguard/milestone/1) — the live release gate: what still blocks 1.0, and in what order (native blocked-by relationships between its issues). It is the only gate ([ADR-027](docs/adr/adr-027-one-1-0-release-gate-lives-in-the-issue-tracker.md)); [`PRD.md`](PRD.md) is the normative promise it is measured against, and [`TASKS.md`](TASKS.md) is the historical registry of security findings, not a backlog.

**Completion criterion:** You know what the milestone still holds, no active blockers impede your task, and you know the domain vocabulary.

---

## Workflow: Skills in sequence

Before starting any code task, use global skills from `~/.agents/skills/` in this order:

1. **`grill-me`** (or **`grill-with-docs`** when a spec exists) — interview the task
2. **`tdd`** — red-green implementation (load **`rust-best-practices`** before writing Rust)
3. **`code-review`** — Standards and Spec axes  
4. **`re-review`** — verify findings and fixes (max 2 rounds; see `~/.agents/ENGINEERING_GATES.md`)

Only push once `re-review` reports green.

---

## Post-task: Verify, then update docs

After code passes all gates, update in this order:

1. **Verification gates:** `rtk cargo test --workspace`, `rtk cargo clippy -- -D warnings`, `rtk cargo fmt --check`, `rtk cargo audit`, `rtk cargo deny check` (benchmark if hot path was touched). Wait for all to pass.

2. **Update `PROJECT_STATE.md`:** Last updated date, Last session summary, Open blockers. Release status is not recorded here — it is the state of the `1.0` milestone.

3. **Update `CHANGELOG.md`:** Prepend one line under `## [Unreleased]` (category: Added/Changed/Fixed/Removed/Security; reference the issue or ADR).

4. **Update `CONTEXT.md`** (if needed): If the task introduces or sharpens a domain term, update glossary in the same change.

5. **Close the issue** (if applicable): when the work satisfies its acceptance criteria and verification is linked, close the issue. `TASKS.md` carries no status to flip — touch it only to add a finding or point it at a new issue.

6. **Write ADR** (if needed): For significant architecture, API, or security model changes, write `docs/adr/adr-NNN-slug.md` (required sections: Status, Context, Decision, Consequences; update `docs/adr/README.md` index).

**Completion criterion:** All verifications green and all affected docs updated.

---

## Execution

- Route all shell commands through `rtk` — see [`RTK.md`](RTK.md) for examples
- Respect denied Aegis decisions; do not propose bypasses
- For Rust code, apply `rust-best-practices` skill before writing

---

## Key references

- **`PRD.md`** — the normative Aegis 1.0 product promise; every other document is derived from it and keeps no 1.0 checklist of its own
- **`CLAUDE.md`** — detailed conventions (error handling, performance, module structure, testing, naming, key types, approved dependencies)
- **`CONVENTION.md`** — authoritative rules with precedence order
- **`.github/workflows/ci.yml`** — required branch-protection status checks
- **`~/.agents/ENGINEERING_GATES.md`** — Definition-of-Done, traceability, branch policy
