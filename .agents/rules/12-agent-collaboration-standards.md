# Rule 12 — Agent Collaboration Standards

- **Strict One-Subphase-At-A-Time Rule:** Execute only the active subphase in `docs/PROGRESS.md`. Never auto-advance to the next subphase without explicit user confirmation.
- **Contracts Before Parallelism:** Lock interfaces in `docs/CONTRACTS.md` before delegating parallel implementation tasks.
- **No Unapproved Scope Drift:** Check every proposed feature or library against `REQUIREMENTS.md`, `ARCHITECTURE.md`, and `docs/SCOPE_AND_MVP.md` (5-point Scope Admission Gate). Record deferrals in `DEF-*`.
- **Mandatory Independent Review:** Agent 12 (Independent Reviewer) must audit code, tests, security, documentation, and subphase boundaries before marking any subphase `COMPLETE`.
- **Time-Budget Discipline:** Honor subphase time limits (Trivial 30m, Simple 1h, Moderate 2h, Complex 4h, Very Complex 6h). If exceeded, stop scope expansion, report blockers, and re-scope with user approval.
