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

---

## [0.0.4-stage0.1.4] — 2026-11-27

### Subphase 0.1.4 — Define MVP and Deferred Features (Phase 0.1 Complete)
- **Added:** `docs/SCOPE_AND_MVP.md` (`FF-SCP-001` v1.0.0) defining explicit release milestones (`MVP-v0.1`, `v1.0-GA`, `v1.x+`), `P0`/`P1`/`P2` requirement mappings, the 12-item Deferred-Features & Non-Goals Register (`DEF-001` through `DEF-012`), and the 5-point Scope Admission Gate.
- **Changed:** Cross-referenced `docs/SCOPE_AND_MVP.md` in `REQUIREMENTS.md`; updated `docs/PROGRESS.md` (including verified `4e6d610` commit reference for `0.1.3`) and appended Entry 004 to `TODO.md`.

---

## [0.0.5-stage0.2.1] — 2026-11-27

### Subphase 0.2.1 — Define System Architecture
- **Added:** `ARCHITECTURE.md` (`FF-ARCH-001` v1.0.0) establishing the 5-plane system architecture, forbidden-coupling rules, 8-step request lifecycle with atomic staging and no-op verification, dual Tokio/Rayon resource governance, 4 security trust boundaries, cross-platform integration topologies, and structured observability architecture.
- **Changed:** Updated `docs/PROGRESS.md` (including verified `df8c41b` commit reference for `0.1.4`) and appended Entry 005 to `TODO.md`.

---

## [0.0.6-stage0.2.2] — 2026-11-27

### Subphase 0.2.2 — Define Module Boundaries
- **Added:** `docs/MODULE_BOUNDARIES.md` (`FF-MOD-001` v1.0.0) specifying the complete monorepo ownership map, 16-crate Rust workspace architecture (`L0`–`L6`), acyclic dependency DAG, 6 enforceable forbidden-coupling rules (`MOD-RULE-01`..`MOD-RULE-06`), and Desktop/Mobile/WASM Cargo feature-flag matrix.
- **Changed:** Cross-referenced `docs/MODULE_BOUNDARIES.md` in `ARCHITECTURE.md`; updated `docs/PROGRESS.md` (including verified `d6fb2b5` commit reference for `0.2.1`) and appended Entry 006 to `TODO.md`.

---

## [0.0.7-stage0.2.3] — 2026-11-27

### Subphase 0.2.3 — Define Rust and Application Contracts
- **Added:** `docs/CONTRACTS.md` (`FF-CTR-001` v1.0.0) establishing canonical error codes (`FF-ERR-*`), security/resource limits, inspection/progress DTOs, core Rust engine/validator traits, SQLite WAL persistence tables, and cross-platform interface contracts (`flutter-bridge`, Web/WASM Worker, CLI, and `/api/v1` REST API).
- **Changed:** Cross-referenced `docs/CONTRACTS.md` in `ARCHITECTURE.md` and `docs/MODULE_BOUNDARIES.md`; updated `docs/PROGRESS.md` (including verified `606553b` commit reference for `0.2.2`) and appended Entry 007 to `TODO.md`.

---

## [0.0.8-stage0.2.4] — 2026-11-27

### Subphase 0.2.4 — Create Architecture Decision Records (Phase 0.2 Complete)
- **Added:** `docs/DECISION_LOG.md` (`FF-ADR-001` v1.0.0) recording `ADR-001` through `ADR-010` across Rust core modularization, Two-Tier license isolation, dual-pool concurrency, Flutter/FRB, SvelteKit 5/WASM, SQLite/Cloud persistence, atomic staging/No-Op Guard, security isolation, opt-in AI privacy, and CI/Podman topology.
- **Changed:** Cross-referenced `docs/DECISION_LOG.md` in `ARCHITECTURE.md`; updated `docs/PROGRESS.md` (including verified `fff6c24` commit reference for `0.2.3`) and appended Entry 008 to `TODO.md`.

---

## [0.0.9-stage0.3.1] — 2026-11-27

### Subphase 0.3.1 — Create GitHub Repository
- **Added:** Public GitHub repository `https://github.com/dayashimoga/filefusion` created via authenticated GitHub integration and linked as `origin` for branch `main` and all `stage-0.*` tags.
- **Changed:** Updated `docs/PROGRESS.md` (including verified `2a65c5f` commit reference for `0.2.4`) and appended Entry 009 to `TODO.md`.

---

## [0.0.10-stage0.3.2] — 2026-11-27

### Subphase 0.3.2 — Establish Monorepo
- **Added:** Monorepo engineering configuration roots (`.gitignore`, `.gitattributes`, `.editorconfig`, `.cargo/config.toml`, `deny.toml`), root `Cargo.toml` workspace with non-decorative `crates/file-core` anchor crate + unit tests, and `docs/MONOREPO_SETUP.md` (`FF-MONO-001`).
- **Changed:** Updated `docs/PROGRESS.md` (including verified `2450a3e` commit reference for `0.3.1`) and appended Entry 010 to `TODO.md`.

---

## [0.0.11-stage0.3.3] — 2026-11-27

### Subphase 0.3.3 — Configure Antigravity Rules and Skills
- **Added:** Root `AGENTS.md`, 12 concise persistent rules in `.agents/rules/*.md`, and 14 reusable workflow skills in `.agents/skills/*/SKILL.md` conforming to the active Antigravity Customization specification.
- **Changed:** Updated `docs/PROGRESS.md` (including verified `99794ae` commit reference for `0.3.2`) and appended Entry 011 to `TODO.md`.

---

## [0.0.12-stage0.3.4] — 2026-11-27

### Subphase 0.3.4 — Configure Specialized Agents
- **Added:** 12 specialist agent `SKILL.md` definitions (`agent-01-principal-architect` through `agent-12-independent-reviewer`) in `.agents/skills/` covering responsibilities, permitted tools, implementation standards, verification commands, and completion criteria.
- **Changed:** Updated `docs/PROGRESS.md` (including verified `4c9c283` commit reference for `0.3.3`) and appended Entry 012 to `TODO.md`.

---

## [0.0.13-stage0.3.5] — 2026-11-27

### Subphase 0.3.5 — Create Documentation Foundation (Phase 0.3 Complete)
- **Added:** Complete documentation foundation suite: `docs/ENGINEERING_CONTRACT.md`, `docs/ROADMAP.md`, `README.md`, `CONTRIBUTING.md`, `LICENSE` (`Apache-2.0 OR MIT`), `SECURITY.md`, `PERFORMANCE.md`, `LICENSES.md`, `CODEBASE_GUIDE.md`, `USER_GUIDE.md`, and `DEVELOPER_GUIDE.md`.
- **Changed:** Updated `docs/PROGRESS.md` (including verified `e613fa1` commit reference for `0.3.4`) and appended Entry 013 to `TODO.md`.

---

## [0.0.14-stage0.4.1] — 2026-11-27

### Subphase 0.4.1 — Configure GitHub Actions
- **Added:** `.github/ISSUE_TEMPLATE/` (`bug_report.yml`, `feature_request.yml`, `config.yml`), `.github/pull_request_template.md`, `.github/dependabot.yml`, `.github/workflows/build-apps.yml` (SHA-pinned CI baseline with `workflow_dispatch` inputs, Rust core verification, and `cargo-deny` gate), and `.github/workflows/release.yml` (`v*.*.*` tagged release pipeline).
- **Changed:** Updated `docs/PROGRESS.md` (including verified `53dada6` commit reference for `0.3.5`) and appended Entry 014 to `TODO.md`.
