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

1. **Verification gates:** CI runs them, not you. Do not run `cargo test --workspace`, clippy, `cargo audit` or `cargo deny check` locally. Run `scripts/lint.sh fmt` before every push (the pre-push hook does it); it compiles nothing. Run only the targeted test for the code you changed (`rtk cargo test --test <name>` or `rtk cargo test -p <crate>`). Push, then wait for every CI check to go green (`gh pr checks`, `gh run watch`). Benchmark if a hot path was touched.

2. **Update `CHANGELOG.md`:** Prepend one line under `## [Unreleased]` (category: Added/Changed/Fixed/Removed/Security; reference the issue or ADR).

3. **Update `CONTEXT.md`** (if needed): If the task introduces or sharpens a domain term, update glossary in the same change.

4. **Close the issue** (if applicable): when the work satisfies its acceptance criteria and verification is linked, close the issue. Write the PR description from `.github/pull_request_template.md` by the PR body rule in `CONVENTION.md`. Session notes and follow-ups go in an issue comment.

5. **Write ADR** (if needed): For significant architecture, API, or security model changes, write `docs/adr/adr-NNN-slug.md` (required sections: Status, Context, Decision, Consequences; update `docs/adr/README.md` index).

**Completion criterion:** All verifications green and all affected docs updated.

---

## Agent pipeline

The agent-pipeline plugin carries one ticket through triage, interview, handoff, implement, review loop, ship, PR gate, merge, and retro. Commands: `/ticket`, `/handoff`, `/implement`, `/review-loop`, `/ship`, `/pr-gate`, `/retro`.

- Config: `.agents/pipeline.json` (gate commands, path-to-package map, review tiers). It is the one tracked file under `.agents/`.
- `/implement` works in a worktree under `.worktrees/`; a Stop gate checks the changed files before the agent may finish.
- Gates stay per package and per test file (`cargo check -p`, `cargo test -p <pkg> --lib`, `cargo test --test <stem>`, `scripts/lint.sh fmt`, `scripts/test-tamper-check.sh`). Full-workspace tests, clippy, audit, and deny stay in CI.
- Ticket state lives in `.scratch/<ticket>/` in the main checkout: local and gitignored.
- `.github/workflows/pipeline.yml` adds informational PR checks (tamper check, diff coverage, mutants), outside the Merge admission check.

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
