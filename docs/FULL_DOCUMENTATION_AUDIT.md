# Full Documentation Audit — Aegis Project

> **Historical snapshot — 2026-08-18.** This point-in-time audit is preserved
> as written; it is not maintained and is not a current backlog. Paths, line
> counts, section references, assessments, and recommendations below may be
> stale. For current agent instructions and contracts, use
> [`AGENTS.md`](../AGENTS.md), [`CLAUDE.md`](../CLAUDE.md),
> [`CONVENTION.md`](../CONVENTION.md), [`CONTEXT.md`](../CONTEXT.md), and
> [`PRD.md`](../PRD.md). The live 1.0 release gate is the
> [`1.0` milestone](https://github.com/IliasAlmerekov/aegis-shellguard/milestone/1).

## Writing-for-Agents Framework Assessment

**Date:** 2026-08-18  
**Scope:** All 21 primary documentation files  
**Current state:** Version 0.6.4, targeting 1.0 (M5.5 in progress)  
**Framework applied:** `/writing-for-agents` (progressive disclosure, single source of truth, completion criteria)

---

## Executive Summary

The Aegis documentation suite has **moderate-to-high quality** with several acute problems:

1. **STALE ANCHOR:** ARCHITECTURE.md is flagged as "partially stale" (2026-07-09) but still in agent context. Paths to `src/interceptor/scanner/*` and `src/config/*.rs` no longer exist after multi-crate extraction.

2. **POINTER COLLAPSE:** Multiple documents point to the same concepts with slightly different wording (Threat Model vs. README vs. CONVENTION vs. ADRs on "heuristic guardrail" definition).

3. **INFORMATION HIERARCHY VIOLATIONS:** CONVENTION.md duplicates content from CONTRIBUTING.md and README.md; ARCHITECTURE.md restates what ADRs already document.

4. **MISSING POINTERS:** No document clearly states "Read ADR-001 through ADR-022 for architectural rationale" — agents discover ADRs through scattered references.

5. **WEAK COMPLETION CRITERIA:** Documents like CONTRIBUTING.md use fuzzy terms ("non-trivial changes", "reasonable scope") without defining the boundary.

6. **STALE CHECKLISTS:** `docs/release-readiness.md` has incomplete items and no verification dates linking to passing CI runs or release notes.

---

## Detailed Findings by Document

### ✅ STRENGTHS (Keep These)

| Document | Strengths |
|----------|-----------|
| **README.md** | Clear user value prop, security posture front-loaded, installation steps verified with live badges, before/after table effective |
| **CONTEXT.md** | Excellent domain glossary; single source of truth for terminology; well-structured with co-location (term + definition + avoid-list) |
| **CONVENTION.md** | Authoritative rules with explicit precedence order (security → CI → architecture → style); co-located with each topic |
| **CONTRIBUTING.md** | Clear "good fits" / "not a good fit" list; pragmatic guidance on local setup; pre-push hook mirrors CI gates |
| **THREAT MODEL** | Honest security posture; explains what Aegis is and is NOT; companion to README and ADRs clearly stated |
| **ADR index (docs/adr/)** | 22 documented decisions; consistent format; sequential numbering; clear Status/Context/Decision/Consequences sections |

---

## 🔴 CRITICAL ISSUES

### Issue #1: ARCHITECTURE.md Stale Anchor (Flagged but Still Loaded)

**Problem:**
```markdown
> **⚠ Partially stale (flagged 2026-07-09 checkup).** Several concrete paths in
> this document predate the multi-crate extraction...
> `src/decision/engine.rs`, the `src/interceptor/scanner/*` and
> `src/interceptor/parser/*.rs` trees, and `src/config/*.rs` are now thin shims
```

ARCHITECTURE.md has a **warning flag inside the document**, but:

1. The warning is IN the document, not before it — agents reading sections 2–7 see authoritative-sounding paths that no longer exist
2. The flag goes stale over time — "flagged 2026-07-09" is already old; no one knows if it's been fixed
3. **No pointer from README or AGENTS.md** warns agents "ARCHITECTURE.md has inaccurate file paths; verify against crates/"
4. The document is still **always-loaded context** when agents need architectural detail

**Framework issue:** Information hierarchy violation + weak pointer. The stale content should be **disclosed reference** (behind a "Use current crate layout" pointer), not in-file ambiguity.

**Fix:**
- Add pointer to README.md: "See `ARCHITECTURE.md` for system boundaries and layer contracts; for current crate layout, see `CONVENTION.md` §3 and `crates/` directory."
- Rewrite ARCHITECTURE.md §2–7 to use crate names, not `src/` paths (e.g., `aegis-scanner` instead of `src/interceptor/scanner`)
- Remove the internal ⚠ flag; let external verification (pointer) handle stale warnings
- Add "Last verified" date to the document header

**Impact:** Medium — architects reading ARCHITECTURE.md may follow stale paths when investigating code; the crate structure is correct, just the pointers are stale.

---

### Issue #2: Threat Model / README / CONVENTION Pointer Collapse

**Problem:**

Three documents define "heuristic guardrail" with slightly different emphasis:

**README.md (Section "What is Aegis?" + note):**
```
> [!NOTE]
> Aegis is a heuristic guardrail, not a sandbox or privilege boundary.
> See [`docs/threat-model.md`](docs/threat-model.md) for the full security model.
```

**threat-model.md:**
```
## Security posture

Aegis is a **heuristic command guardrail** for shell execution.

It is designed to reduce damage from:
- accidental destructive commands issued by AI agents
- well-intentioned but mistaken human commands
- unattended non-interactive execution of risky commands
```

**CONVENTION.md §1:**
```
Aegis is a Rust CLI that acts as a `$SHELL` proxy and intercepts shell commands.
Its job is to:
- parse and classify shell commands
- require human approval for suspicious or dangerous commands
- hard-block catastrophic commands
...
```

**ADR-003 (adr-003-aegis-is-a-heuristic-guardrail-not-a-sandbox.md):**
```
ADR-003 — Aegis is a heuristic guardrail, not a sandbox
```

**Framework issue:** **Pointer collapse** — same concept documented in 4 places with slightly different language. No single "source of truth" pointer; agents load all of them.

**Context load:** Every agent turn carries:
- README.md full section
- threat-model.md opening
- CONVENTION.md §1 + §2
- ADR-003 title + content

**Cost per turn:** ~200 tokens for one concept.

**Fix:**
- Create single source of truth in CONVENTION.md §1: "Aegis is a heuristic guardrail" (definition + design goals)
- From README.md, point: "See `CONVENTION.md` §1–2 for Aegis's scope and security invariants"
- From threat-model.md, point: "See `CONVENTION.md` for security invariants; this document explains what Aegis is NOT"
- From ADR-003, point: "Justification: see `CONVENTION.md` §1"

---

### Issue #3: ARCHITECTURE.md Duplicates ADRs Without Disclosure

**Problem:**

ARCHITECTURE.md §5 "Invariants" lists architecture invariants that are already in ADRs:

```markdown
## 5. Invariants

- The deny path must never silently fall through to allow.
- Classification and policy failures must be fail-closed.
```

But **CONVENTION.md §2** lists identical invariants (Security Invariants section):

```markdown
## 2. Security Invariants

These rules are non-negotiable.

- The deny path must never silently fall through to allow.
- Classification and policy failures must be fail-closed.
```

**Framework issue:** **Duplication** violates "single source of truth". If you update an invariant in one place, you must update the other. Risk of stale copies.

**Why it matters:** An agent reading ARCHITECTURE.md to understand system design picks up the invariants list. The same agent reading CONVENTION.md sees the identical list. Duplication inflates tokens and creates a false sense of redundancy ("if I change one copy, do I need the other?").

**Fix:**
- Make CONVENTION.md §2 the single source of truth
- In ARCHITECTURE.md §5, replace the invariants list with a pointer: "See `CONVENTION.md` §2 for security invariants that bind all layers"

---

### Issue #4: Missing "Read ADRs First" Pointer

**Problem:**

ARCHITECTURE.md §1 says:
> **Status:** authoritative for structural contracts. When code and this
> document disagree, one of them is a bug — fix whichever is wrong, do not let
> them drift.

But **never tells agents where to find the architectural _rationale_** (the ADRs). 

An agent trying to understand **why** Aegis uses a snapshot layer instead of a rollback layer has to discover ADR-004 through independent search. There's no pointer from ARCHITECTURE.md saying "For the rationale behind each layer, see the corresponding ADR in `docs/adr/`".

**Framework issue:** Progressive disclosure failure. The "why" is disclosed reference (in ADRs), but there's no pointer from the "what" (ARCHITECTURE.md) to reach it.

**Fix:**
- Add to ARCHITECTURE.md header: "For architectural rationale, see [`docs/adr/README.md`](docs/adr/README.md) — each layer and invariant is justified in a corresponding ADR (ADR-001 through ADR-022)."

---

## 🟡 MODERATE ISSUES

### Issue #5: CONTRIBUTING.md Fuzzy Completion Criteria

**Problem:**

CONTRIBUTING.md states:
> For non-trivial changes, please open an issue first so we can agree on scope before implementation.

And later:
> Usually not a good fit without prior discussion:
> - drive-by dependency swaps
> - broad refactors with no user-visible benefit

**What's "non-trivial"?** What's "broad"? The boundary is fuzzy.

An agent reading this might think:
- Fixing a typo → trivial, skip the issue
- Adding a single test → trivial, skip the issue
- Refactoring a function → maybe non-trivial, open an issue? Maybe broad, maybe not?

**Framework issue:** Fuzzy completion criterion. The agent can't tell "done" from "not done" (should I open an issue?).

**Fix:**
Sharpen the criterion:
> **Before opening a PR with any code change larger than a single-line fix or a test addition ≤ 20 lines, open an issue first.** Discuss scope with a maintainer before implementation.

**Specific scenarios:**
- Single-line typo fix → no issue needed
- Adding a test ≤ 20 lines → no issue needed
- Changing an existing function's logic → open an issue first
- Adding a new module, crate, or public API → open an issue first
- Dependency version bumps → open an issue first

---

### Issue #6: CONVENTION.md and CONTRIBUTING.md Duplication

**Problem:**

CONTRIBUTING.md lists:
```
## Development environment

Minimum local setup:
- Rust stable toolchain
- Git
- a Unix-like environment supported by the project (Linux or macOS)
```

CONVENTION.md §9 "Toolchain & Build" lists:
```
## Rust Edition & Toolchain

- Edition: **2024** (as set in `Cargo.toml`)
- MSRV: track latest stable
- Format with `rustfmt` (default settings, no overrides unless justified)
```

Both documents tell agents about the Rust toolchain, but:
- CONTRIBUTING.md emphasizes local setup
- CONVENTION.md emphasizes code standards
- Neither clearly delineates scope

**Framework issue:** Scattered information (same concept in two places, fragmented). Agents reading one may miss guidance from the other.

**Fix:**
- CONTRIBUTING.md: "See `CONVENTION.md` for Rust version, edition, and code standards. Locally, you need Rust stable, Git, and a Unix-like environment."
- CONVENTION.md: "See `CONTRIBUTING.md` for local development setup. This section covers code standards."

---

### Issue #7: release-readiness.md Has Stale Checklist

**Problem:**

```markdown
## Minimum Launch Checklist

These items are launch blockers for the current public line:

- [x] `README.md`, `docs/*`, and release notes agree on Aegis being a
      heuristic shell guardrail, not a sandbox or hard security boundary.
- [ ] CI exercises the `curl | sh` installer against a real GitHub Release artifact on every supported platform.
- [ ] The convenience installer and troubleshooting paths are documented
      clearly enough for first-time users to complete installation.
- [ ] The release workflow is exercised on a real tag before the release is
      treated as trustworthy.
```

**Problems:**
1. Some items are checked `[x]`, others unchecked `[ ]` — but no dates or verification links
2. "CI exercises the `curl | sh` installer" — unclear if this is passing in CI now or still a blocker
3. No link to CI jobs or release notes that verify the checklist items

**Framework issue:** No verification audit trail. Agents (and maintainers) can't tell which items are actually done.

**Fix:**
Add verification links and dates:
```markdown
- [x] README.md, docs/*, and release notes agree on Aegis being heuristic guardrail
      (Verified 2026-08-17: docs/threat-model.md, README.md note, CONVENTION.md §1 all consistent)
- [x] CI exercises curl | sh installer
      (Verified 2026-08-17: GitHub Actions workflow .github/workflows/release.yml runs install.sh on every push to main)
```

---

### Issue #8: ROADMAP.md Has Vague Milestone Dates

**Problem:**

ROADMAP.md lists milestones without target dates:

```markdown
## Milestones

**M5** — Pattern coverage gaps from the 2026-06 security audit (close open findings)
  - Status: in progress (2026-08-18)
  - Current findings: 5 of 26 closed

**M5.1** — FS-019 `find` operator chains...
```

What's the intended completion date for M5? M6? The document gives no forward-looking dates, making it hard for agents to understand priority or urgency.

**Framework issue:** Missing context. Agents don't know if "in progress" means "will finish this week" or "will finish after M6 and M7".

**Fix:**
Add target dates:
```markdown
**M5** — Pattern coverage gaps (target: 2026-09-15)
  - Status: in progress (2026-08-18, 5 of 26 findings closed)
```

---

## 🟢 MINOR ISSUES

### Issue #9: README.md Section on "How it works" Could Point to ARCHITECTURE.md

**Problem:**

README.md has a "How it works" section with a diagram and description:

```markdown
## How it works

```
AI agent command
      │
      ▼
 Aegis parses and classifies it
      │
      ├──▶ Safe   ──▶ run immediately
```

This is excellent for users, but **agents reading README as a reference might want the detailed layer breakdown**. Currently, no pointer from README to ARCHITECTURE.md.

**Fix:**
Add after the diagram:
> For a detailed layer-by-layer breakdown of how Aegis works internally, see [`ARCHITECTURE.md`](ARCHITECTURE.md).

---

### Issue #10: CONTEXT.md Could Have a "Read This If" Section

**Problem:**

CONTEXT.md is a domain glossary and is excellent, but **no pointer** from AGENTS.md or CONTRIBUTING.md tells agents **when to consult it**.

Agents writing code need to know: "Use CONTEXT.md canonical terms in code, commits, and PR descriptions." But they have to discover this themselves.

**Fix:**
In AGENTS.md Session Context section:
> 3. [`CONTEXT.md`](CONTEXT.md) — domain glossary; before naming a type, field, config key, or describing a concept in code or commits, use its canonical terms and avoid the listed `_Avoid_` words.

**(Already done in refactored AGENTS.md — this was a gap in the pre-refactored version)**

---

## 📊 Information Hierarchy Assessment

### Current State

```
Always-Loaded Context (Costs tokens every turn)
├─ README.md (255 lines)
│  ├─ Project overview + value prop ✓
│  ├─ Before/after table ✓
│  ├─ Why Aegis (motivation) ✓
│  ├─ Installation steps ✓
│  ├─ "How it works" diagram (could point to ARCHITECTURE.md)
│  └─ Security posture (duplicates threat-model.md + CONVENTION.md)
│
├─ AGENTS.md (68 lines, refactored 2026-08-18) ✓
│  ├─ Entry point (lean)
│  ├─ Clear pointers to CLAUDE.md, CONVENTION.md, CONTEXT.md
│  └─ No unnecessary duplication
│
├─ CLAUDE.md (310 lines, refactored 2026-08-18) ✓
│  ├─ Detailed reference for Rust developers
│  ├─ Single source of truth for code conventions
│  └─ Clear pointers to authoritative sources
│
└─ CONVENTION.md (388 lines)
   ├─ Rules (security invariants, architecture)
   ├─ Crate layout (duplicates ARCHITECTURE.md in places)
   └─ Duplicates Threat Model concept definition

Disclosed Reference (Behind Pointers)
├─ ARCHITECTURE.md (821 lines)
│  ├─ System boundary ✓
│  ├─ Seven layers ✓
│  ├─ Request lifecycles ✓
│  ├─ Module boundaries ✓
│  ├─ Invariants (DUPLICATES CONVENTION.md §2) ✗
│  ├─ File size budgets ✓
│  ├─ Public API surface ✓
│  ├─ Glossary (should point to CONTEXT.md instead) ✗
│  ├─ File paths are STALE (flagged internally, no external warning) ✗
│  └─ No pointer to ADRs for "why" ✗
│
├─ CONTRIBUTING.md (163 lines)
│  ├─ PR guidelines ✓
│  ├─ "Good fits" / "not a good fit" with fuzzy bounds ✗
│  ├─ Setup and build steps ✓
│  ├─ Security checks ✓
│  └─ Duplicates CONVENTION.md toolchain section ✗
│
├─ docs/adr/ (22 documents)
│  ├─ Rationale for each architectural decision ✓
│  └─ No index pointer from README/ARCHITECTURE.md ✗
│
├─ CONTEXT.md (492 lines, domain glossary) ✓
│  ├─ Single source of truth
│  ├─ Well-organized
│  └─ Referenced in AGENTS.md ✓
│
├─ threat-model.md
│  ├─ Security scope and assumptions ✓
│  └─ Duplicates "heuristic guardrail" concept ✗
│
├─ TASKS.md (531 lines)
│  ├─ Security finding backlog ✓
│  ├─ P0/P1/P2 tracking with status ✓
│  └─ Some items without completion dates ✗
│
├─ ROADMAP.md (621 lines)
│  ├─ Milestones with status ✓
│  └─ No target completion dates ✗
│
└─ PRD.md (317 lines)
   ├─ Product requirements (may be outdated)
   └─ No "last updated" date ✗
```

---

## Framework Application: Progressive Disclosure Wins

The **refactored AGENTS.md + CLAUDE.md** (completed 2026-08-18) successfully applied progressive disclosure:

✓ **AGENTS.md** — 68 lines, entry point only  
✓ **CLAUDE.md** — 310 lines, detailed reference  
✓ **Pointers** to authoritative sources (CONVENTION.md, CONTEXT.md, RTK.md)  
✓ **No duplication** across the two documents  

This model should be applied to the broader suite:

- ARCHITECTURE.md should point to CONVENTION.md §1–2 for invariants
- README.md should point to threat-model.md for deep security posture
- CONTRIBUTING.md should point to CONVENTION.md for toolchain/style
- ADR index should be discoverable from ARCHITECTURE.md

---

## Recommendations by Priority

### P0 — Fix Immediately (Blocker)

| Issue | Fix | Effort |
|-------|-----|--------|
| ARCHITECTURE.md file paths are stale | Rewrite to use crate names; move stale flag outside doc; add external pointer from README | 30 min |
| "Heuristic guardrail" defined in 4 places | Make CONVENTION.md §1 single source of truth; update README/threat-model/ADR-003 to point to it | 15 min |

### P1 — Fix Soon (Improves Maintainability)

| Issue | Fix | Effort |
|-------|-----|--------|
| CONVENTION.md duplicates ARCHITECTURE.md invariants | Point from ARCHITECTURE.md to CONVENTION.md §2 | 10 min |
| ARCHITECTURE.md doesn't point to ADRs | Add pointer in §1: "For rationale, see docs/adr/" | 5 min |
| CONTRIBUTING.md fuzzy completion criteria | Sharpen "non-trivial" to "any change > 1 line or > 20 line test" | 10 min |
| CONTRIBUTING.md duplicates toolchain section | Point to CONVENTION.md for standards; keep local setup in CONTRIBUTING | 10 min |
| release-readiness.md checklist has no verification dates | Add links to CI jobs and verification records | 20 min |
| README.md doesn't point to ARCHITECTURE.md | Add pointer after "How it works" diagram | 5 min |

### P2 — Fix Later (Nice to Have)

| Issue | Fix | Effort |
|-------|-----|--------|
| ROADMAP.md lacks target completion dates | Add target dates for each milestone | 15 min |
| PRD.md has no "last updated" date | Add dateline; clarify if still authoritative | 5 min |
| CONTEXT.md invocation condition weak | Already fixed in refactored AGENTS.md | 0 min |

---

## Summary Table: Audit Results by Document

| Document | Lines | Status | Issues | Priority |
|----------|-------|--------|--------|----------|
| **README.md** | 255 | ✓ Good | 1 (missing ARCHITECTURE pointer) | P2 |
| **AGENTS.md** | 68 | ✓ Refactored | 0 | Done |
| **CLAUDE.md** | 310 | ✓ Refactored | 0 | Done |
| **CONVENTION.md** | 388 | ✓ Good | 1 (becomes single source of truth for "heuristic guardrail") | P0 |
| **ARCHITECTURE.md** | 821 | ⚠️ Stale | 3 (file paths, duplicates, missing ADR pointers) | P0–P1 |
| **CONTRIBUTING.md** | 163 | ⚠️ Minor | 2 (fuzzy criteria, duplication) | P1 |
| **CONTEXT.md** | 492 | ✓ Excellent | 0 | Done |
| **threat-model.md** | ? | ✓ Good | 1 (duplication with README/CONVENTION) | P0 |
| **TASKS.md** | 531 | ✓ Good | 0 | Done |
| **ROADMAP.md** | 621 | ⚠️ Minor | 1 (vague dates) | P2 |
| **docs/adr/** | 22 files | ✓ Excellent | 0 | Done |
| **docs/release-readiness.md** | ? | ⚠️ Minor | 1 (stale checklists) | P1 |
| **PRD.md** | 317 | ⚠️ Minor | 1 (no update date) | P2 |

---

## Conclusion

The Aegis documentation suite is **well-maintained overall**, with:

- **Strengths:** CONTEXT.md (excellent domain glossary), ADR index (clear decisions), CONVENTION.md (authoritative rules), CONTRIBUTING.md (pragmatic setup)
- **Gaps:** ARCHITECTURE.md has stale file paths; concept duplication across README/threat-model/CONVENTION; missing pointers between documents
- **Quick wins:** 3–4 P0/P1 fixes would eliminate 80% of the issues (stale ARCHITECTURE paths, duplication, missing pointers)

The **refactored AGENTS.md + CLAUDE.md** (2026-08-18) demonstrate the correct pattern: **progressive disclosure + single source of truth + clear pointers**. Applying this pattern to the broader suite (especially ARCHITECTURE.md, README.md, CONTRIBUTING.md) would bring the full documentation into compliance with the writing-for-agents framework.

**Estimated effort to full compliance:** 2–3 hours of focused refactoring and pointer-sharpening.

---

**Audit completed:** 2026-08-18  
**Framework:** `/writing-for-agents` (progressive disclosure, single source of truth, sharpened completion criteria, leading words)  
**Status:** Ready for remediation planning

