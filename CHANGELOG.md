# FileFusion — Changelog (APPEND-ONLY)

> **GOVERNANCE RULE:** This file is strictly **APPEND-ONLY**. All notable changes, completed subphases, architectural baselines, and releases are appended chronologically. Historical entries are never overwritten.

---

## [0.0.1-stage0.1.1] — 2026-11-27

### Subphase 0.1.1 — Finalize Product Requirements
- **Added:** `REQUIREMENTS.md` (`FF-REQ-001` v1.0.0) establishing the authoritative functional requirements across all 20 product capability domains (`FR-DET`, `FR-DOC`, `FR-IMG`, `FR-AV`, `FR-ARC`, `FR-CMP`, `FR-ADP`, `FR-PDF`, `FR-FID`, `FR-DUP`, `FR-REP`, `FR-MET`, `FR-BAT`, `FR-WFL`, `FR-AI`, `FR-LOC`, `FR-CLD`, `FR-DEV`, `FR-SEC`, `FR-VAL`), 8 target platforms (`PLAT-WIN`, `PLAT-LIN`, `PLAT-MAC`, `PLAT-AND`, `PLAT-IOS`, `PLAT-WEB`, `PLAT-CLI`, `PLAT-API`), non-functional requirements (`NFR-PERF`, `NFR-REL`, `NFR-SEC`, `NFR-QA`, `NFR-UX`, `NFR-OPS`), Stage 0–12 traceability matrix, and Two-Tier Engine Architecture.
- **Added:** `docs/PROGRESS.md` to track stage, phase, subphase status, time budgets, test evidence, and commit references under the strict one-subphase-at-a-time governance rule.
- **Added:** `TODO.md` (append-only) and `CHANGELOG.md` (append-only) initialized with Stage 0 baseline tracking.
- **Environment:** Audited Windows 11 host (16 GB RAM, 12 logical CPUs) and verified installed toolchains (`rustc 1.99.1`, `cargo 1.99.1`, `flutter 3.38.4`, `dart 3.10.3`, `node v25.8.2`, `pnpm`, `podman 6.1.4`, `git 2.56.0`). Initialized local Git repository on `main`.

---

## [0.0.2-stage0.1.2] — 2026-11-27

### Subphase 0.1.2 — Define Target Users and Use Cases
- **Added:** `docs/USE_CASES.md` (`FF-UC-001` v1.0.0) establishing 6 target user personas (`USR-01` through `USR-06`), complete 14-screen Core Navigation mapping, 16 end-to-end use cases (`UC-001` through `UC-016`) with explicit failure/cancellation flows and progressive-disclosure tiers, 4 cross-cutting UX/accessibility journeys (`UXJ-01` through `UXJ-04`), and 100% traceability to all 20 `FR-*` domains in `REQUIREMENTS.md`.
- **Changed:** Cross-referenced `docs/USE_CASES.md` in `REQUIREMENTS.md`; updated `docs/PROGRESS.md` (including verified `3e2650d` commit reference for `0.1.1`) and appended Entry 002 to `TODO.md`.

---

## [0.0.3-stage0.1.3] — 2026-11-27

### Subphase 0.1.3 — Define Supported Formats and Capabilities
- **Added:** `docs/FORMATS_AND_CAPABILITIES.md` (`FF-FMT-001` v1.0.0) establishing the complete format, codec, engine tier (`Y1` pure-Rust permissive vs `Y2` out-of-process license-isolated adapters), hardware acceleration, security bomb-protection, platform availability (`PLAT-*`), and conversion-graph routing matrices across all supported file families.
- **Changed:** Cross-referenced `docs/FORMATS_AND_CAPABILITIES.md` in `REQUIREMENTS.md`; updated `docs/PROGRESS.md` (including verified `84207b0` commit reference for `0.1.2`) and appended Entry 003 to `TODO.md`.
