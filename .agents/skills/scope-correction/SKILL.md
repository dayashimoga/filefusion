---
name: scope-correction
description: >-
  Detects and prevents scope creep, unapproved technology changes, or premature
  work outside the active subphase. Use whenever a new feature, crate, or external
  dependency is proposed mid-subphase.
---

# Scope Correction

## Steps

1. Evaluate the proposed addition against the 5-point Scope Admission Gate in `docs/SCOPE_AND_MVP.md` §5:
   - Product vision fit (`REQUIREMENTS.md`)
   - Active subphase alignment (`docs/PROGRESS.md`)
   - Architecture & `MOD-RULE` impact (`docs/MODULE_BOUNDARIES.md`)
   - Measurable benefit vs resource/binary cost
   - ADR (`docs/DECISION_LOG.md`) and explicit user approval requirement
2. If out of scope for the active subphase, reject immediate implementation and record it either in `TODO.md` (for its designated future stage) or in `docs/SCOPE_AND_MVP.md` `DEF-*` register.
3. If an architectural change is justified, draft a comparative ADR before writing implementation code and request user confirmation.

## Validation

- Active subphase diff contains zero unrelated features or unapproved dependencies.
