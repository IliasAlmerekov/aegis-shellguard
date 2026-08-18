# AGENTS.md — Aegis (Codex instructions)

**Aegis** is a lightweight Rust CLI that acts as a `$SHELL` proxy for AI coding agents, intercepting commands and requiring human confirmation before destructive operations. It must stay fast (< 2ms for safe paths), correct, and minimal.

See [`CLAUDE.md`](CLAUDE.md) for detailed project conventions; this document is the entry point.

---

## Session Context — read before any code change

Before writing code or running commands, read these documents in order:

1. [`PROJECT_STATE.md`](PROJECT_STATE.md) — last session's work, milestone status, open blockers
2. [`CONVENTION.md`](CONVENTION.md) — authoritative rules (precedence: security invariants → CI gates → architecture → style)
3. [`CONTEXT.md`](CONTEXT.md) — domain glossary; use canonical terms in code and commits
4. [`TASKS.md`](TASKS.md) — open security findings blocking 1.0 (P0/P1/P2)

**Completion criterion:** You understand the current milestone, no active blockers impede your task, and you know the domain vocabulary.

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

2. **Update `PROJECT_STATE.md`:** Last updated date, Last session summary, Milestone status rows, Open blockers.

3. **Update `CHANGELOG.md`:** Prepend one line under `## [Unreleased]` (category: Added/Changed/Fixed/Removed/Security; reference milestone or ADR).

4. **Update `CONTEXT.md`** (if needed): If the task introduces or sharpens a domain term, update glossary in the same change.

5. **Update `TASKS.md`** (if applicable): Flip `[ ]` to `[x]` only if a tracked finding is closed and verified.

6. **Write ADR** (if needed): For significant architecture, API, or security model changes, write `docs/adr/adr-NNN-slug.md` (required sections: Status, Context, Decision, Consequences; update `docs/adr/README.md` index).

**Completion criterion:** All verifications green and all affected docs updated.

---

## Execution

- Route all shell commands through `rtk` — see [`RTK.md`](RTK.md) for examples
- Respect denied Aegis decisions; do not propose bypasses
- For Rust code, apply `rust-best-practices` skill before writing

---

## Key references

- **`CLAUDE.md`** — detailed conventions (error handling, performance, module structure, testing, naming, key types, approved dependencies)
- **`CONVENTION.md`** — authoritative rules with precedence order
- **`.github/workflows/ci.yml`** — required branch-protection status checks
- **`~/.agents/ENGINEERING_GATES.md`** — Definition-of-Done, traceability, branch policy
