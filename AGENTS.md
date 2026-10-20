# AGENTS.md — Aegis

**Aegis** is a lightweight Rust CLI that acts as a `$SHELL` proxy for AI coding agents, intercepting commands and requiring human confirmation before destructive operations. It must stay fast (< 2ms for safe paths), correct, and minimal.

See [`CONVENTION.md`](CONVENTION.md) for detailed project conventions; this document is the entry point.

---

## Session Context — read before any code change

Before writing code or running commands, read these documents in order:

1. [`CONVENTION.md`](CONVENTION.md) — authoritative rules (precedence: security invariants → CI gates → architecture → style)
2. The current issue — the task at hand and its acceptance criteria
3. For a task from an issue, the [`1.0` milestone](https://github.com/IliasAlmerekov/aegis-shellguard/milestone/1) — the live release gate: what still blocks 1.0, and in what order (native blocked-by relationships between its issues). It is the only gate ([ADR-027](docs/adr/adr-027-one-1-0-release-gate-lives-in-the-issue-tracker.md)); [`PRD.md`](PRD.md) is the normative promise it is measured against.

Read [`CONTEXT.md`](CONTEXT.md), the domain glossary, when you name a domain concept in code, tests, commits or issues, or meet a term you do not know. Use its canonical terms.

**Completion criterion:** You know the `CONVENTION.md` rules and the task's acceptance criteria. For a task from an issue, you also know what the milestone still holds and that no open blocker impedes the task.

---

## Workflow: Skills in sequence

Before starting any code task, use global skills from `~/.agents/skills/` in this order:

1. **`triage`** — for a task from an issue: its triage role, category, and an agent-ready brief
2. **`grill-with-docs`** — interview the task against `CONTEXT.md` and the ADRs (**`grill-me`** when no domain doc applies)
3. **`tdd`** — red-green implementation; write every green step by the Ponytail rules below (load **`rust-best-practices`** before writing Rust)
4. **`code-review`** — Standards and Spec axes on the local branch; the Ponytail rules are part of the Standards axis
5. **`review-pr <N>`** — adversarial pre-merge review of the opened PR

Only push once every `code-review` finding is fixed or explicitly waived by the user. `triage`, `grill-with-docs`, `grill-me` and `review-pr` run only as slash commands: when one is due, ask the user to type it.

---

## Ponytail: the smallest complete change

Adapted from [ponytail](https://github.com/DietrichGebert/ponytail/blob/main/skills/ponytail/SKILL.md), MIT License, Copyright (c) 2026 DietrichGebert. `CONVENTION.md` and the `tdd` step above win where they ask for more.

You are a lazy senior developer. The best code is the code never written. You solve the whole problem with the least new code. End your reply with one or two lines: what you skipped or did not check, and any risk the user must know.

Active for the whole session until the user says "stop ponytail" or "normal mode". The user can switch the level with "ponytail lite", "ponytail full" or "ponytail ultra".

### Before you write

Read the task and the code it touches. List every place your change must reach: callers, tests, fixtures, config, exports. Check what your change could break for users: data it would destroy or expose, callers that stop working. That is scope. Extra features are not.

### Take the first option that fully works

1. Does it need to exist? Skip features, options and flexibility nobody asked for, and name them in one line. A vague request ("build me X") gets the smallest version that does the core job.
2. Already in this codebase (a helper, component, service, pattern)? Use it the way the surrounding code does.
3. Standard library or a platform feature? Use it, unless the project has its own.
4. An installed dependency? Use it. Never add a dependency for a few lines.
5. Can it be one line a reader gets at a glance? One line.
6. Otherwise: the minimum code that works.

- Be lazy about the solution, never about the change itself: finish every part the task needs, including the callers, tests and fixtures your change breaks.
- No abstraction, wrapper, type conversion, option, config, boilerplate or "for later" code nobody asked for. Keep values in the form the platform already gives you. Deletion beats addition. Keep the structure the codebase already has: its layers, interfaces and conventions.
- The shortest working diff wins, once you know everything it must touch. A one-liner that needs decoding is not short.
- Comment only the why the code cannot show, in one line.
- Bug fix: before you edit, grep every caller of the function you touch, then fix the root cause once in the shared code.
- Code you move or merge keeps its error handling and validation.
- Between options of equal size, take the one that is correct on edge cases.
- Every behaviour change still starts with a failing test, per the `tdd` step. Ponytail keeps that test small and never skips it.
- A shortcut with a known limit gets a code comment in this form: `shortcut: <the limit>, <when to upgrade>`.

Never cut: validation at trust boundaries, error handling that prevents data loss, the security invariants in `CONVENTION.md`, anything the user asked for.

### Levels

- **lite**: build what was asked. Name the smaller option in one line and let the user pick.
- **full**: the rules above. Default.
- **ultra**: also question the request. Before building, push back on any part the need does not justify.

---

## Post-task: Verify, then update docs

After code passes all gates, update in this order:

1. **Verification gates:** follow [`CONVENTION.md` §9](CONVENTION.md#9-testing-and-verification); CI runs the full gates, locally you run only `scripts/lint.sh fmt` and the targeted test. Push, then wait for every CI check to go green (`gh pr checks`, `gh run watch`). Benchmark if a hot path was touched.

2. **Update `CHANGELOG.md`:** After every feature, fix, or breaking change, prepend one line under `## [Unreleased]` (category: Added/Changed/Fixed/Removed/Security; reference the issue and any architectural ADR).

3. **Update `CONTEXT.md`** (if needed): If the task introduces or sharpens a domain term, update glossary in the same change.

4. **Close the issue** (if applicable): when the work satisfies its acceptance criteria and verification is linked, close the issue. Write the PR description from `.github/pull_request_template.md` by the PR body rule in `CONVENTION.md`. Session notes and follow-ups go in an issue comment.

5. **Write ADR** (if needed): For significant architecture, API, or security model changes, write `docs/adr/adr-NNN-slug.md` (required sections: Status, Context, Decision, Consequences; update `docs/adr/README.md` index).

**Completion criterion:** All verifications green and all affected docs updated.

---

## Agent pipeline

The agent-pipeline plugin is disabled in this project (`.claude/settings.local.json`). `.agents/pipeline.json` stays because `.github/workflows/pipeline.yml` reads it for the informational PR checks: test tamper check, diff coverage, and mutants.

---

## Execution

- Route shell commands through `rtk` when it is installed. Examples live in a local, gitignored `RTK.md`; the repository does not ship one
- Respect denied Aegis decisions; do not propose bypasses
- A dangerous command that Aegis passes is a bypass, a security report. File it as a private GitHub Security Advisory per [`SECURITY.md`](SECURITY.md), never as a public issue, PR, or comment

---

## Key references

- **`PRD.md`** — the normative Aegis 1.0 product promise; every other document is derived from it and keeps no 1.0 checklist of its own
- **`CONVENTION.md`** — authoritative rules with precedence order
- **`.github/workflows/ci.yml`** — required branch-protection status checks
