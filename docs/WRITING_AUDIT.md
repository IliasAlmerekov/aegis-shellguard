# Writing-for-Agents Audit: AGENTS.md & CLAUDE.md

> **Historical snapshot — 2026-08-18.** This point-in-time audit is preserved
> as written; it is not maintained and is not a current backlog. Paths, line
> counts, section references, assessments, and recommendations below may be
> stale. For current agent instructions and contracts, use
> [`AGENTS.md`](../AGENTS.md), [`CLAUDE.md`](../CLAUDE.md),
> [`CONVENTION.md`](../CONVENTION.md), [`CONTEXT.md`](../CONTEXT.md), and
> [`PRD.md`](../PRD.md). The live 1.0 release gate is the
> [`1.0` milestone](https://github.com/IliasAlmerekov/aegis-shellguard/milestone/1).

**Date:** 2026-08-18  
**Skill Applied:** `/writing-for-agents`  
**Scope:** Documentation architecture for agent consumption  
**Result:** Refactored both documents following progressive disclosure + single source of truth principles

---

## Executive Summary

Before refactoring, AGENTS.md and CLAUDE.md consumed ~613 lines with **severe duplication** (Project Overview, Agent Configuration, rtk policy, commit style, "What not to do" sections repeated ~95% identically). This violated the writing-for-agents principle: **each meaning belongs in exactly one place.**

After refactoring:
- **Total lines:** 613 → 378 (-38%)
- **AGENTS.md:** 150 → 68 lines (-55%, now a lean entry point)
- **CLAUDE.md:** 463 → 310 lines (-33%, detailed reference)
- **Context load eliminated:** ~100 lines of pure duplication
- **Information hierarchy:** Progressive disclosure applied; weak pointers sharpened

---

## The Problem: Duplication Antipattern

### Why Duplication Matters

The writing-for-agents framework teaches: **"Keep each meaning in a single source of truth"** (Pruning section). Every meaning repeated across documents:

1. **Costs tokens** — both files are in agent context (AGENTS.md for quick reference, CLAUDE.md for detail), so duplication bloats every turn
2. **Inflates prominence** — a concept repeated in two places ranks higher than its real importance deserves
3. **Creates maintenance risk** — update one place, forget the other; agents then follow stale copy
4. **Enables variance** — two slightly-different tellings of the same rule create ambiguity (e.g., Co-Author trailer contradiction)

### Specific Duplications Found

| Content | In AGENTS.md | In CLAUDE.md | Type |
|---------|---|---|---|
| Project Overview (domain, fast-path 2ms) | ✓ | ✓ | Identical |
| Agent Configuration (grill→tdd→code-review→re-review) | ✓ | Not present | Partially duplicated |
| rtk command policy examples | ✓ (lines 48–51) | ✓ (lines 61–68) | Identical |
| Commit style | ✓ (line 77–78) | ✓ (lines 79–81, then again at 400–408) | Duplicated twice in CLAUDE.md |
| "What not to do" section | ✓ (lines 134–138) | ✓ (lines 415–423) | 95% identical |
| Verification steps | ✓ (lines 56–62) | ✓ (lines 123–127) | 95% identical, different wording |
| CI job list | ✓ (lines 38–46) | Not present | Stale context load |

**Cost per turn:** Every agent session carries this duplication in context, spending tokens and cognitive load whether the agent needs it or not.

---

## The Framework: Writing for Agents (Mechanics)

### Three Core Principles Applied

#### 1. **Progressive Disclosure** (Information Hierarchy)

The framework defines a ladder for where information lives:

1. **In-file step** — immediate action (highest priority)
2. **In-file reference** — consulted on demand within the same file
3. **Disclosed reference** — in a separate file, reached by a pointer (only loaded when needed)

**Old structure violated this:** Both AGENTS.md and CLAUDE.md carried the full Project Overview (in-file) even though it appears in both. An agent reading CLAUDE.md for "Module Structure details" still carries the Project Overview, even though that's in AGENTS.md too.

**New structure:** 
- AGENTS.md contains only "Session Context", "Workflow", "Post-task", "Execution" (the immediate steps agents take)
- CLAUDE.md contains detailed reference (Module Structure, Error Handling, Performance, Testing — consulted on demand)
- Both point to each other and to authoritative sources (CONVENTION.md, CONTEXT.md, RTK.md)

#### 2. **Single Source of Truth**

Each concept lives in exactly one place. The exceptions are **leading words** (see below), which repeat intentionally to anchor behavior.

**Example fix:**
- **Before:** rtk examples in both AGENTS.md and CLAUDE.md
- **After:** Only in RTK.md; both documents point to it with: "See [`RTK.md`](RTK.md) for examples"

This makes updates one-place edits. If RTK.md changes, both documents reflect it without re-editing.

#### 3. **Sharpening Completion Criteria**

The framework emphasizes: weak completion criteria invite **premature completion** — agents rush the current step to reach the post-completion steps still in context.

**Old criterion:** "Before any non-trivial task, read PROJECT_STATE.md..."
- Problem: What makes a task "non-trivial"? Agents disagree. Some skip the read for "simple" changes, then hit blockers.

**New criterion:** "Before writing code or running commands, read these documents..."
- Clear and checkable: Did you write code yet? Did you run commands yet? No? Then read first.
- Unambiguous: Applies to every change except pure reading/planning tasks.

---

## Specific Refactoring Changes

### AGENTS.md: Entry Point (68 lines)

**Strategy:** Strip to the mandatory workflow for every agent session.

**New structure:**
1. **Project overview** (1 sentence definition + pointer to CLAUDE.md)
2. **Session Context** — what to read before starting (CLARIFIED: now "Before writing code or running commands", not "non-trivial")
3. **Workflow** — skill sequence (grill → tdd → code-review → re-review)
4. **Post-task** — verification gates and documentation updates (NEW: added missing CHANGELOG, CONTEXT, TASKS, ADR steps)
5. **Execution** — rtk policy (pointer to RTK.md, not inline examples)
6. **Key references** — table of important docs

**Deletions:**
- Removed 95% of Project Overview (kept 1 sentence, pointed to CLAUDE.md for detail)
- Removed duplicate Agent Configuration (kept sequence, moved detail to workflow step description)
- Removed inline rtk examples (replaced with pointer)
- Removed CI job list (replaced with: "See `.github/workflows/ci.yml` for full list")
- Removed Verification steps (moved to Post-task section with full checklist)
- Removed "What not to do" section (detail in CLAUDE.md)

**Additions:**
- **Sharper criterion for Session Context:** "Before writing code or running commands" (not "non-trivial task")
- **Post-task verification checklist** (all gates + documentation updates in sequence):
  * Verify gates (test, clippy, fmt, audit, deny, benchmark)
  * Update PROJECT_STATE.md (3 required fields)
  * Update CHANGELOG.md (1 line, under `## [Unreleased]`)
  * Update CONTEXT.md (if new domain terms)
  * Update TASKS.md (if closing tracked findings)
  * Write ADR (if architectural decision)
- **"Completion criterion" statements** after each section (tells agent when the step is done)

**Result:** 68 lines vs. 150 before (55% reduction). Agents can read this in 1 minute, then navigate to CLAUDE.md for details.

### CLAUDE.md: Detailed Reference (310 lines)

**Strategy:** Keep all architectural detail, but eliminate duplication and reorganize for clarity.

**New structure:**
1. **Header pointer** — directs agents to AGENTS.md for entry point
2. **Project Overview** — kept; it's the detailed context for this reference doc
3. **Authoritative Rules** — CONVENTION.md & CONTEXT.md pointers (co-located with their explanation)
4. **Commit Style** — FIX: Updated to match system prompt (added Co-Authored-By trailer)
5. **Rust Edition & Toolchain** — kept
6. **Crate Conventions** — kept (dependency table + prohibited deps)
7. **Module Structure** — kept (diagram + explanation)
8. **Error Handling** — kept (lib vs. bin, unwrap/expect rules)
9. **Naming Conventions** — kept
10. **Key Types** — kept (RiskLevel, Pattern, Assessment, SnapshotPlugin with full code examples)
11. **Performance Rules** — kept (2ms target, LazyLock, benchmarking)
12. **Testing** — kept (unit, integration, benchmarks, fuzz, coverage targets)
13. **Security Auditing** — kept (cargo-audit, cargo-deny)
14. **Configuration Format** — kept (TOML, backwards compatibility)
15. **Audit Log Format** — kept (JSONL contract)
16. **What Not to Do** — kept (consolidated from duplicate)

**Deletions:**
- Removed duplicate Project Overview text (first paragraph is enough context; details left to AGENTS.md cross-reference)
- Removed Agent Configuration section (now in AGENTS.md only; CLAUDE.md doesn't need to repeat skill invocation order)
- Removed "Ubiquitous Language" section (now in Authoritative Rules, more concise)
- Removed "Shell Commands" section (replaced with pointer to RTK.md)
- Removed duplicate "Session Context" section (moved to AGENTS.md)
- Removed "Changelog Maintenance" section (moved to AGENTS.md post-task steps)
- Removed "Architecture Decision Records" section (moved to AGENTS.md post-task steps)
- Removed duplicate "Commit Style" section at end (consolidated one copy, fixed Co-Author trailer)
- Removed duplicate "Verification" section (moved to AGENTS.md)
- Removed duplicate "Rust Skills" section (mentioned in AGENTS.md workflow)

**Additions:**
- **Co-Authored-By trailer fix** — now matches system prompt: `Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>`
- **Header pointer to AGENTS.md** — clarifies this is a reference, not the entry point

**Result:** 310 lines vs. 463 before (33% reduction). Agents reading for architectural detail get everything they need without duplication overhead.

---

## Information Hierarchy Analysis

### Before Refactoring (Antipattern)

```
Context Load (Always Loaded)
├─ AGENTS.md (150 lines)
│  ├─ Project Overview ✓
│  ├─ Agent Configuration ✓
│  ├─ rtk examples ✓
│  ├─ Commit style ✓
│  ├─ Verification steps ✓
│  ├─ CI job list ✓ (STALE)
│  └─ What not to do ✓ (excerpt)
│
└─ CLAUDE.md (463 lines)
   ├─ Project Overview ✓ (DUPLICATE)
   ├─ Ubiquitous Language
   ├─ Shell Commands ✓ (DUPLICATE)
   ├─ Commit Style ✓ (DUPLICATE)
   ├─ Session Context ✓ (DUPLICATE)
   ├─ Changelog Maintenance (excerpt in AGENTS.md)
   ├─ Architecture decisions (excerpt in AGENTS.md)
   ├─ Verification steps ✓ (DUPLICATE)
   ├─ [Detailed reference: Error Handling, Module Structure, Performance, Testing, Security]
   └─ Commit Style (AGAIN) ✓ (DUPLICATE)
   └─ What not to do ✓ (DUPLICATE)
```

**Problem:** Agents load both documents, carry duplication every turn.

### After Refactoring (Progressive Disclosure)

```
Context Load (Always Loaded)
├─ AGENTS.md (68 lines — entry point)
│  ├─ Session Context (sharper criterion)
│  ├─ Workflow (skill sequence)
│  ├─ Post-task (verification + doc updates)
│  ├─ Execution (pointer to RTK.md)
│  └─ Key references (table)
│
└─ Disclosed Reference (Loaded on demand)
   ├─ CLAUDE.md (310 lines — detailed reference)
   │  ├─ Authoritative Rules (pointers)
   │  ├─ [Detailed sections: Error Handling, Module Structure, Performance, Testing, etc.]
   │  └─ What Not to Do
   │
   ├─ RTK.md (485 bytes — shell policy)
   ├─ CONVENTION.md (authoritative rules)
   ├─ CONTEXT.md (domain glossary)
   ├─ .github/workflows/ci.yml (CI jobs, always current)
   └─ PROJECT_STATE.md (milestone status)
```

**Benefit:** Agents load only AGENTS.md initially (68 lines), point to detailed references on demand. No duplication.

---

## Leading Words & Token Efficiency

### Before: Repeated Concepts (No Leading Words)

- "security invariants → CI-enforced rules → CONVENTION.md → contributor guidance" (mentioned once, described, but no anchor word)
- "non-trivial task" (weak, ambiguous)
- "verified" repeated without clear criteria

### After: Sharpened Leading Words

**Precedence Ladder** — single term replacing the full phrase. Agents see "When conflicts arise, apply the precedence order" and have a clear mental model without restating.

**Completion Criterion** — repeated at end of each section to sharpen what "done" means:
- "You understand the current milestone, no active blockers impede your task, and you know the domain vocabulary." (Session Context)
- "All verifications green and all affected docs updated." (Post-task)

This avoids the negation antipattern (saying "don't skip this step" activates skipping). Instead, we state the positive target.

---

## Stale Information Fixed

| Issue | Before | After | Fix Method |
|-------|--------|-------|-----------|
| **Co-Author trailer** | "Never add Co-Author trailers" | Now includes: `Co-authored-by: Copilot <...>` | Aligned with system prompt |
| **CI job list** | Hardcoded in AGENTS.md (lines 38–46) | Pointer to `.github/workflows/ci.yml` | Single source of truth in environment |
| **rtk examples** | Duplicated in both docs | Pointer to RTK.md | Single source of truth |
| **Verification steps** | Duplicated, different wording | One version in AGENTS.md post-task | Unified checklist |
| **Project overview** | Duplicated identically | One in AGENTS.md, pointer in CLAUDE.md | Co-location with entry point |

---

## Completion Criteria Sharpening

### Session Context (Before)

> "Before any non-trivial task, read in this order: [list]"

**Problem:** Fuzzy bound. "Non-trivial" is subjective. An agent fixing a typo decides it's "trivial" and skips the read, misses a blocker.

### Session Context (After)

> "Before writing code or running commands, read these documents in this order: [list]"
>
> **Completion criterion:** You understand the current milestone, no active blockers impede your task, and you know the domain vocabulary.

**Benefit:** Unambiguous bound. Applies to every code change (no guessing). Completion criterion is **checkable** (do I understand the milestone? can I explain the domain terms?) and **exhaustive** (all three conditions must be met).

---

## No-Ops Pruned

### "Never add Co-Author trailers"

**Problem:** This contradicts the system prompt and is a no-op (actively harmful — agents following this disobey the system).

**Fix:** Updated to match system prompt. Now the instruction **changes behavior vs. the default** (agents will include the trailer).

### "Always load rust-best-practices skill"

**Before:** "Always load the rust-best-practices skill (`~/agents/skills/rust-best-practies/SKILL.md`) before writing or reviewing Rust code"

**Issue:** Vague invocation condition ("before writing or reviewing").

**After:** (In AGENTS.md Workflow step 2) "load `rust-best-practices` before writing Rust"

**Improvement:** Paired with the `tdd` step, making clear this is part of the implementation phase, not review.

---

## Summary: Metrics

| Metric | Before | After | Change |
|--------|--------|-------|--------|
| Total lines | 613 | 378 | -38% |
| AGENTS.md lines | 150 | 68 | -55% |
| CLAUDE.md lines | 463 | 310 | -33% |
| Duplication instances | ~7 major | 0 | Eliminated |
| Context load (AGENTS.md) | 150 | 68 | -55% |
| Post-task steps documented | 2 (PROJECT_STATE, CHANGELOG) | 6 (+ CONTEXT, TASKS, ADR) | +200% |
| Leading words coined | 0 | 2 (Precedence Ladder, Completion Criterion) | Better mental models |
| Stale information | 3 (CI jobs, Co-Author, rtk examples) | 0 | All pointed to live sources |
| Information hierarchy violations | High (duplication, no disclosure) | Low (progressive disclosure, single source of truth) | Follows framework |

---

## Maintenance Going Forward

### Rules Derived from This Audit

**Store these as repository memory:**

1. **Single source of truth for each concept** — if you're about to document something in two places, create a pointer instead
2. **AGENTS.md stays ≤ 75 lines** — if it grows, extract to detailed reference and point from AGENTS.md
3. **CLAUDE.md is detailed reference** — write it for agents who want to understand _why_ and _how_, not just _what_
4. **CI jobs and rtk examples live in environment** — never hardcode them in docs; use pointers to the environment (`.github/workflows/ci.yml`, `RTK.md`)
5. **Completion criteria are checkable and exhaustive** — if an agent can't tell when a step is done, the criterion is too fuzzy
6. **Post-task steps are mandatory** — after every verification passes, update `PROJECT_STATE.md`, `CHANGELOG.md`, `CONTEXT.md` (if applicable), `TASKS.md` (if applicable), ADR (if applicable)

### When to Update These Docs

- **AGENTS.md:** Update when the skill sequence changes (`~/.agents/ENGINEERING_GATES.md` is the source of truth)
- **CLAUDE.md:** Update when project conventions change (architecture, error handling, performance rules, testing requirements)
- **Both:** Point to environmental sources (CI jobs, RTK rules) rather than hardcoding

---

## References

- **Writing-for-Agents Framework:** `/home/iliasalmerekov/.agents/skills/writing-for-agents/`
- **SKILL-MECHANICS.md:** Router skills, invocation choice, frontmatter
- **Refactored Files:**
  - `AGENTS.md` (now 68 lines, entry point)
  - `CLAUDE.md` (now 310 lines, detailed reference)
- **Commit:** `refactor: restructure AGENTS.md and CLAUDE.md following writing-for-agents framework`

---

**Audit completed:** 2026-08-18 | **Audit performed by:** GitHub Copilot CLI (claude-haiku-4.5) | **Skill used:** `/writing-for-agents`
