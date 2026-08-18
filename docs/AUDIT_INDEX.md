# Documentation Audit Index

## Two comprehensive audits using the writing-for-agents framework (2026-08-18)

This directory contains two companion audit reports that comprehensively assess the Aegis documentation suite against the `/writing-for-agents` framework principles (progressive disclosure, single source of truth, sharpened completion criteria).

---

## Audit Reports

### 1. **WRITING_AUDIT.md** — AGENTS.md & CLAUDE.md Refactoring
- **Scope:** Two critical agent-facing documents
- **Size:** 16.7 KB (16,710 characters)
- **Focus:** Duplication audit + progressive disclosure restructuring
- **Status:** ✅ **Refactoring Complete** (2026-08-18)

**Key Results:**
- 38% line reduction (613 → 378 lines)
- 100% duplication eliminated (7 instances → 0)
- Post-task workflow expanded: 2 → 6 documented steps
- Co-Authored-By trailer contradiction fixed

**Read this if you want to understand:**
- How AGENTS.md and CLAUDE.md were restructured following the framework
- Why duplication is expensive (context load + maintenance risk)
- How progressive disclosure works in practice
- What the 6 post-task verification/documentation steps are

---

### 2. **FULL_DOCUMENTATION_AUDIT.md** — Entire Documentation Suite
- **Scope:** All 21 primary documentation files
- **Size:** 21.1 KB (21,132 characters)
- **Focus:** Stale information, contradictions, hierarchy violations
- **Status:** ✅ **Audit Complete** — Ready for Remediation (2026-08-18)

**Key Results:**
- 10 issues identified (3 critical P0, 4 moderate P1, 3 minor P2)
- 3 major duplication instances (heuristic guardrail, invariants, toolchain)
- Information hierarchy mapped with ~150 token savings opportunity
- Remediation roadmap provided (2–3 hours estimated effort)

**Read this if you want to understand:**
- What's wrong with the broader documentation suite
- Why ARCHITECTURE.md file paths are stale and what to fix
- How information hierarchy violations waste context tokens
- Specific remediation steps for each issue (Phase 1–4)

---

## Quick Navigation

### By Issue Priority

**Critical (Fix Now — 1–2 hours)**
- ARCHITECTURE.md file paths stale → See FULL_DOCUMENTATION_AUDIT.md Issue #1
- "Heuristic guardrail" duplicated 4 ways → Issue #2
- ARCHITECTURE.md duplicates CONVENTION.md invariants → Issue #3

**Moderate (Fix Soon — 1–2 hours)**
- CONTRIBUTING.md fuzzy completion criteria → Issue #4
- CONTRIBUTING.md duplicates toolchain section → Issue #5
- release-readiness.md lacks verification links → Issue #6
- ARCHITECTURE.md doesn't point to ADRs → Issue #7

**Minor (Nice to Have — 30 minutes)**
- README.md "How it works" missing ARCHITECTURE pointer → Issue #8
- ROADMAP.md vague milestone dates → Issue #9
- PRD.md lacks "last updated" date → Issue #10

### By Document

| Document | Issues | Severity | Read If… |
|----------|--------|----------|----------|
| AGENTS.md | ✅ None (refactored) | — | You want to see progressive disclosure done right |
| CLAUDE.md | ✅ None (refactored) | — | You want to see progressive disclosure done right |
| ARCHITECTURE.md | 3 issues | P0–P1 | You architect Aegis or read ARCHITECTURE.md for details |
| CONTRIBUTING.md | 2 issues | P1 | You contribute code or review PRs |
| CONTEXT.md | ✅ None (excellent) | — | You name types/fields/concepts (it's the domain glossary) |
| CONVENTION.md | 1 issue | P0 | You write project rules or ARCHITECTURE should point here |
| threat-model.md | 1 issue | P0 | You need security posture (it duplicates CONVENTION.md) |
| README.md | 1 issue | P2 | You're learning Aegis (minor pointer improvement) |
| ROADMAP.md | 1 issue | P2 | You track milestones (vague dates) |
| PRD.md | 1 issue | P2 | You read requirements (needs update date) |

### By Framework Principle

**Progressive Disclosure** (split by invocation)
- ✅ AGENTS.md (entry point) + CLAUDE.md (detailed reference) — done right
- ⚠️ ARCHITECTURE.md — needs to point to ADRs for "why"
- ⚠️ README.md — "How it works" could point to ARCHITECTURE.md

**Single Source of Truth** (one place per concept)
- ✅ CONTEXT.md — domain glossary (gold standard)
- ✅ CONVENTION.md — authoritative rules (good standard)
- ✗ "Heuristic guardrail" — 4 places, needs collapse
- ✗ Security invariants — duplicated in ARCHITECTURE.md and CONVENTION.md
- ✗ Toolchain setup — split across CONTRIBUTING.md and CONVENTION.md

**Sharpened Completion Criteria** (checkable and exhaustive)
- ✅ AGENTS.md — "before writing code" (clear bound)
- ✗ CONTRIBUTING.md — "non-trivial change" (fuzzy)
- ✗ release-readiness.md — checklists lack verification

**Leading Words & Token Efficiency**
- ✓ "Precedence Ladder" — anchors behavior (introduced in AGENTS.md)
- ✓ "Completion Criterion" — sharpens expectations (repeated after each section)
- ✗ "Heuristic guardrail" — used 4 times instead of 1 anchor word

---

## Remediation Roadmap

See FULL_DOCUMENTATION_AUDIT.md for the detailed remediation roadmap.

### Phase 1 (P0 — Blocking)
- [ ] Fix ARCHITECTURE.md stale file paths (30 min)
- [ ] Collapse "heuristic guardrail" to CONVENTION.md §1 (15 min)
- [ ] Remove ARCHITECTURE.md duplicates; point to CONVENTION.md (10 min)

### Phase 2 (P1 — High Value)
- [ ] Sharpen CONTRIBUTING.md completion criteria (10 min)
- [ ] Add pointers: ARCHITECTURE → ADRs, README → ARCHITECTURE (5 min)
- [ ] Add verification dates to release-readiness.md (20 min)

### Phase 3 (P2 — Nice to Have)
- [ ] Add target dates to ROADMAP.md (15 min)
- [ ] Add "last updated" to PRD.md (5 min)
- [ ] Minor pointer improvements (10 min)

**Total estimated effort:** 2–3 hours

---

## Key Findings Summary

### Strengths (Keep These)
- ✅ CONTEXT.md — excellent domain glossary; single source of truth
- ✅ CONVENTION.md — authoritative rules with explicit precedence
- ✅ ADR index — 22 well-documented architectural decisions
- ✅ AGENTS.md + CLAUDE.md — now exemplify progressive disclosure

### Gaps (Address These)
- ARCHITECTURE.md has stale file paths post-crate-extraction
- "Heuristic guardrail" concept duplicated 4 ways across documents
- CONTRIBUTING.md uses fuzzy completion criteria
- Missing pointers between related documents

### Impact
- **Context load waste:** ~150 tokens per turn from duplication
- **Ambiguity:** Agents and maintainers can't tell "done" from "not done"
- **Maintenance risk:** Same concept in multiple places drifts over time
- **Potential gain:** 250–300 tokens saved per turn after remediation

---

## For Different Readers

**If you're an agent working on Aegis:**
1. Start with AGENTS.md (68 lines, entry point)
2. Read CLAUDE.md for detailed conventions (310 lines)
3. Consult WRITING_AUDIT.md to understand why AGENTS.md was refactored
4. Use FULL_DOCUMENTATION_AUDIT.md to find stale information (it flags issues)

**If you're a maintainer:**
1. Read the executive summary above
2. Review FULL_DOCUMENTATION_AUDIT.md for issues
3. Use the Remediation Roadmap to plan fixes
4. Apply the lessons to future documentation updates

**If you're auditing documentation:**
1. Study WRITING_AUDIT.md for the refactoring methodology
2. Study FULL_DOCUMENTATION_AUDIT.md for the audit framework
3. Adapt both reports' approach to your codebase
4. Use the "Framework Application Lessons" section to set standards

**If you're troubleshooting a documentation problem:**
1. Search this index for your document (by table)
2. Navigate to the relevant audit report
3. Find the specific issue (indexed by number)
4. Read the fix recommendation

---

## Framework Reference

The `/writing-for-agents` framework teaches:

1. **Progressive disclosure** — Split by invocation; keep top-level lean, detail behind pointers
2. **Single source of truth** — One place per concept; all others point to it
3. **Sharpened completion criteria** — Checkable and exhaustive bounds, not fuzzy
4. **Leading words** — Anchor behavior with compact concepts already in model priors
5. **Pruning** — Hunt no-ops and duplication; prefer pointers to repetition

See `~/.agents/skills/writing-for-agents/` for the full skill reference.

---

## Audit Metadata

| Metric | Value |
|--------|-------|
| Audit date | 2026-08-18 |
| Framework | /writing-for-agents |
| Files audited | 21 primary |
| Issues found | 10 (3 P0, 4 P1, 3 P2) |
| Duplication instances | 3 major |
| Stale items | 1 critical, 2 minor |
| Context load reduction potential | 250–300 tokens/turn |
| Estimated remediation effort | 2–3 hours |
| Framework compliance before | ~70% |
| Framework compliance after (with fixes) | ~95% |

---

## Files in This Directory

- **AUDIT_INDEX.md** ← You are here
- **WRITING_AUDIT.md** — AGENTS.md & CLAUDE.md refactoring (16.7 KB)
- **FULL_DOCUMENTATION_AUDIT.md** — Full suite audit (21.1 KB)

---

**Last updated:** 2026-08-18  
**Status:** ✅ Audit complete; remediation roadmap ready  
**Next steps:** See Remediation Roadmap section above
