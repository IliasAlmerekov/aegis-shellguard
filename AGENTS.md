# AGENTS.md — Aegis (Codex instructions)

**Aegis** is a lightweight Rust CLI that acts as a `$SHELL` proxy for AI coding agents, intercepting commands and requiring human confirmation before destructive operations. It must stay fast (< 2ms for safe paths), correct, and minimal.

See [`CONVENTION.md`](CONVENTION.md) for detailed project conventions; this document is the entry point.

---

## Session Context — read before any code change

Before writing code or running commands, read these documents in order:

1. [`CONVENTION.md`](CONVENTION.md) — authoritative rules (precedence: security invariants → CI gates → architecture → style)
2. [`CONTEXT.md`](CONTEXT.md) — domain glossary; use canonical terms in code and commits
3. The [`1.0` milestone](https://github.com/IliasAlmerekov/aegis-shellguard/milestone/1) — the live release gate: what still blocks 1.0, and in what order (native blocked-by relationships between its issues). It is the only gate ([ADR-027](docs/adr/adr-027-one-1-0-release-gate-lives-in-the-issue-tracker.md)); [`PRD.md`](PRD.md) is the normative promise it is measured against.
4. The current issue — the task at hand and its acceptance criteria

**Completion criterion:** You know what the milestone still holds, no active blockers impede your task, and you know the domain vocabulary.

---

## Workflow: Skills in sequence

Before starting any code task, use global skills from `~/.agents/skills/` in this order:

1. **`grill-me`** (or **`grill-with-docs`** when a spec exists) — interview the task
2. **`tdd`** — red-green implementation (load **`rust-best-practices`** before writing Rust)
3. **`code-review`** — Standards and Spec axes

Only push once every `code-review` finding is fixed or explicitly waived by the user.

---

## Post-task: Verify, then update docs

After code passes all gates, update in this order:

1. **Verification gates:** `rtk cargo test --workspace`, `scripts/lint.sh` (fmt + clippy, CI toolchain), `rtk cargo audit`, `rtk cargo deny check` (benchmark if hot path was touched). Wait for all to pass.

2. **Update `CHANGELOG.md`:** Prepend one line under `## [Unreleased]` (category: Added/Changed/Fixed/Removed/Security; reference the issue or ADR).

3. **Update `CONTEXT.md`** (if needed): If the task introduces or sharpens a domain term, update glossary in the same change.

4. **Close the issue** (if applicable): when the work satisfies its acceptance criteria and verification is linked, close the issue. Put the session summary and verification results in the PR description or an issue comment, not in a tracked file.

5. **Write ADR** (if needed): For significant architecture, API, or security model changes, write `docs/adr/adr-NNN-slug.md` (required sections: Status, Context, Decision, Consequences; update `docs/adr/README.md` index).

**Completion criterion:** All verifications green and all affected docs updated.

---

## Execution

- Route shell commands through `rtk` when it is installed. Examples live in a local, gitignored `RTK.md`; the repository does not ship one
- Respect denied Aegis decisions; do not propose bypasses

---

## Key references

- **`PRD.md`** — the normative Aegis 1.0 product promise; every other document is derived from it and keeps no 1.0 checklist of its own
- **`CONVENTION.md`** — authoritative rules with precedence order
- **`.github/workflows/ci.yml`** — required branch-protection status checks
- **`~/.agents/ENGINEERING_GATES.md`** — Definition-of-Done, traceability, branch policy
