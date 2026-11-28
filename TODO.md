# FileFusion — Master Project TODO Log (APPEND-ONLY)

> **GOVERNANCE RULE:** This file is strictly **APPEND-ONLY**. Historical entries must never be deleted or overwritten. Status updates and newly discovered tasks are appended chronologically by subphase.

---

## [2026-11-27] Entry 001 — Subphase 0.1.1 Baseline Initialization

### Completed in Subphase 0.1.1
- [x] `[0.1.1]` Audit local development environment, hardware constraints (Windows 11, 16 GB RAM, 12 logical processors, 65.59 GB free on `H:\`), installed toolchains (`rustc 1.99.1`, `cargo 1.99.1`, `flutter 3.38.4`, `node v25.8.2`, `pnpm`, `podman 6.1.4`, `git 2.56.0`), and GitHub authentication status.
- [x] `[0.1.1]` Initialize local Git repository on `main` branch in `h:\ufi`.
- [x] `[0.1.1]` Author authoritative `REQUIREMENTS.md` covering all 20 product capability domains (`FR-DET` through `FR-VAL`), 8 target platforms (`PLAT-WIN`, `PLAT-LIN`, `PLAT-MAC`, `PLAT-AND`, `PLAT-IOS`, `PLAT-WEB`, `PLAT-CLI`, `PLAT-API`), non-functional requirements (`NFR-*`), Two-Tier Engine Architecture, and Stage 0–12 traceability matrix.
- [x] `[0.1.1]` Initialize `docs/PROGRESS.md`, `TODO.md` (append-only), and `CHANGELOG.md` (append-only).

### Queued for Remaining Stage 0 Subphases
- [ ] `[0.1.2]` Define target personas, user journeys, and concrete workflows in `REQUIREMENTS.md` / `docs/USE_CASES.md`.
- [ ] `[0.1.3]` Define the complete format & codec compatibility matrix (input/output formats, lossless/lossy modes, Tier 1 native Rust vs Tier 2 external engine support).
- [ ] `[0.1.4]` Formalize MVP vs Full Release vs Deferred Features scope boundary and scope-control register.
- [ ] `[0.2.1]` Author `ARCHITECTURE.md` defining the layered system architecture, data flow, and security isolation boundaries.
- [ ] `[0.2.2]` Define exact Cargo crate boundaries, app packages, and dependency DAG rules (preventing circular or bloated dependencies).
- [ ] `[0.2.3]` Define Rust trait contracts, error taxonomy, progress/cancellation channels, and `flutter_rust_bridge` / REST / CLI API contracts.
- [ ] `[0.2.4]` Create initial Architecture Decision Records (ADRs) in `docs/DECISION_LOG.md`.
- [ ] `[0.3.1]` Create remote GitHub repository `filefusion` (pending user confirmation of repository visibility and GitHub CLI/token setup).
- [ ] `[0.3.2]` Establish full monorepo directory skeleton with functional configuration roots.
- [ ] `[0.3.3]` Create project-level Antigravity rules (`.agents/rules/`) and reusable workflow skills (`.agents/skills/`).
- [ ] `[0.3.4]` Create all 12 specialist agent `SKILL.md` definitions in `.agents/skills/`.
- [ ] `[0.3.5]` Author core governance documents (`docs/ENGINEERING_CONTRACT.md`, `CODEBASE_GUIDE.md`, `USER_GUIDE.md`, `DEVELOPER_GUIDE.md`, `SECURITY.md`, `PERFORMANCE.md`, `LICENSES.md`, `docs/ROADMAP.md`, `README.md`, `CONTRIBUTING.md`, `LICENSE`).
- [ ] `[0.4.1–0.4.5]` Implement and validate `.github/workflows/build-apps.yml`, release workflow, and local `scripts/build.ps1` + `scripts/build.sh`.

---

## [2026-11-27] Entry 002 — Subphase 0.1.2 Target Users & Use Cases

### Completed in Subphase 0.1.2
- [x] `[0.1.2]` Author `docs/USE_CASES.md` (`FF-UC-001` v1.0.0) defining 6 target user personas (`USR-01`..`USR-06`), 14-screen core navigation mapping, 16 concrete end-to-end use cases (`UC-001`..`UC-016`), 4 cross-cutting UX/accessibility journeys (`UXJ-01`..`UXJ-04`), and a complete traceability matrix back to all 20 capability domains in `REQUIREMENTS.md`.
- [x] `[0.1.2]` Cross-link `docs/USE_CASES.md` from `REQUIREMENTS.md` and synchronize `docs/PROGRESS.md`, `TODO.md`, and `CHANGELOG.md`.

---

## [2026-11-27] Entry 003 — Subphase 0.1.3 Supported Formats & Capabilities

### Completed in Subphase 0.1.3
- [x] `[0.1.3]` Author `docs/FORMATS_AND_CAPABILITIES.md` (`FF-FMT-001` v1.0.0) specifying the Tier 1 vs Tier 2 engine & license isolation model, magic-byte & container disambiguation matrix, image/video/audio/document/ebook/PDF/OCR/archive capability matrices, platform availability matrix, and multi-hop conversion graph rules.
- [x] `[0.1.3]` Cross-link `docs/FORMATS_AND_CAPABILITIES.md` in `REQUIREMENTS.md` and synchronize `docs/PROGRESS.md` (backfilling `84207b0` for `0.1.2`), `TODO.md`, and `CHANGELOG.md`.

---

## [2026-11-27] Entry 004 — Subphase 0.1.4 MVP & Deferred Features (Phase 0.1 Closure)

### Completed in Subphase 0.1.4
- [x] `[0.1.4]` Author `docs/SCOPE_AND_MVP.md` (`FF-SCP-001` v1.0.0) defining Milestone A (`MVP-v0.1`: Stages 1–3 + CLI), Milestone B (`v1.0-GA`: Stages 4–8 + 11–12), Milestone C (`v1.x+`: Stages 9–10 optional AI & Cloud), Deferred-Features & Non-Goals Register (`DEF-001`..`DEF-012`), and the 5-point Scope Admission Gate.
- [x] `[0.1.4]` Cross-link `docs/SCOPE_AND_MVP.md` in `REQUIREMENTS.md`, backfill `4e6d610` for `0.1.3` in `docs/PROGRESS.md`, and complete Phase 0.1 (Product Definition).

---

## [2026-11-27] Entry 005 — Subphase 0.2.1 System Architecture

### Completed in Subphase 0.2.1
- [x] `[0.2.1]` Author `ARCHITECTURE.md` (`FF-ARCH-001` v1.0.0) defining the 5-plane layered architecture, Mermaid architecture topology, 8-step invariant processing lifecycle, dual Tokio/Rayon concurrency & memory-permit governor, 4 security trust boundaries (zero-shell-injection subprocess isolation + opt-in network gate), cross-platform deployment topologies, and `tracing`/Criterion observability design.
- [x] `[0.2.1]` Backfill `df8c41b` (`stage-0.1.4`) in `docs/PROGRESS.md` and append Subphase `0.2.1` entries to `TODO.md` and `CHANGELOG.md`.

---

## [2026-11-27] Entry 006 — Subphase 0.2.2 Module Boundaries & Crate DAG

### Completed in Subphase 0.2.2
- [x] `[0.2.2]` Author `docs/MODULE_BOUNDARIES.md` (`FF-MOD-001` v1.0.0) defining monorepo directory ownership, all 16 Rust workspace crates across tiers `L0`–`L6`, the acyclic dependency DAG diagram, 6 forbidden-coupling rules (`MOD-RULE-01`..`06`), and Desktop/Mobile/WASM Cargo feature-flag boundaries.
- [x] `[0.2.2]` Cross-link `docs/MODULE_BOUNDARIES.md` in `ARCHITECTURE.md`, backfill `d6fb2b5` (`stage-0.2.1`) in `docs/PROGRESS.md`, and append Subphase `0.2.2` entries to `TODO.md` and `CHANGELOG.md`.

---

## [2026-11-27] Entry 007 — Subphase 0.2.3 Rust & Application Contracts

### Completed in Subphase 0.2.3
- [x] `[0.2.3]` Author `docs/CONTRACTS.md` (`FF-CTR-001` v1.0.0) defining the canonical error taxonomy (`FF-ERR-*`), `ResourceLimits`, `FileInspectionReport`, `ProgressEvent`, core Rust traits (`CompressionEngine`, `ConversionEngine`, `ExternalToolAdapter`, `QualityEvaluator`, `OutputValidator`), SQLite WAL schema, and Plane 2 transport contracts (`flutter-bridge`, Web/WASM Worker, CLI exit codes/`--json`, `/api/v1` REST envelope).
- [x] `[0.2.3]` Cross-link `docs/CONTRACTS.md` in `ARCHITECTURE.md` and `docs/MODULE_BOUNDARIES.md`, backfill `606553b` (`stage-0.2.2`) in `docs/PROGRESS.md`, and append Subphase `0.2.3` entries to `TODO.md` and `CHANGELOG.md`.

---

## [2026-11-27] Entry 008 — Subphase 0.2.4 Architecture Decision Records (Phase 0.2 Closure)

### Completed in Subphase 0.2.4
- [x] `[0.2.4]` Author `docs/DECISION_LOG.md` (`FF-ADR-001` v1.0.0) establishing `ADR-001` through `ADR-010` with context, alternatives rejected, consequences, and ADR governance policy.
- [x] `[0.2.4]` Cross-link `docs/DECISION_LOG.md` in `ARCHITECTURE.md`, backfill `fff6c24` (`stage-0.2.3`) in `docs/PROGRESS.md`, append Subphase `0.2.4` entries to `TODO.md` and `CHANGELOG.md`, and complete Phase 0.2 (Architecture).

---

## [2026-11-27] Entry 009 — Subphase 0.3.1 GitHub Repository Creation

### Completed in Subphase 0.3.1
- [x] `[0.3.1]` Confirm `Public` repository visibility preference with user and create `https://github.com/dayashimoga/filefusion` using authenticated Git Credential Manager + GitHub REST API without exposing any credentials or tokens.
- [x] `[0.3.1]` Configure `origin` remote (`https://github.com/dayashimoga/filefusion.git`), synchronize `docs/PROGRESS.md` (with backfilled `2a65c5f` for `0.2.4`), and append Subphase `0.3.1` entries to `TODO.md` and `CHANGELOG.md`.

---

## [2026-11-27] Entry 010 — Subphase 0.3.2 Monorepo Setup

### Completed in Subphase 0.3.2
- [x] `[0.3.2]` Establish monorepo engineering configuration roots (`.gitignore`, `.gitattributes`, `.editorconfig`, `.cargo/config.toml`, `deny.toml`).
- [x] `[0.3.2]` Initialize root `Cargo.toml` workspace (`edition = "2024"`, `unsafe_code = "forbid"`, strict clippy lints) with non-decorative `crates/file-core` anchor crate and passing unit test suite.
- [x] `[0.3.2]` Author `docs/MONOREPO_SETUP.md` (`FF-MONO-001` v1.0.0), backfill `2450a3e` (`stage-0.3.1`) in `docs/PROGRESS.md`, and append Subphase `0.3.2` entries to `TODO.md` and `CHANGELOG.md`.

---

## [2026-11-27] Entry 011 — Subphase 0.3.3 Antigravity Rules & Reusable Skills

### Completed in Subphase 0.3.3
- [x] `[0.3.3]` Author root `AGENTS.md` and 12 project-level persistent rules in `.agents/rules/` (`01-global-engineering-contract.md` through `12-agent-collaboration-standards.md`) following `agy-customizations` format and token-budget guidelines.
- [x] `[0.3.3]` Author all 14 reusable workflow skills in `.agents/skills/*/SKILL.md` (`project-initialization` through `subphase-completion-verification`).
- [x] `[0.3.3]` Backfill `99794ae` (`stage-0.3.2`) in `docs/PROGRESS.md` and append Subphase `0.3.3` entries to `TODO.md` and `CHANGELOG.md`.

---

## [2026-11-27] Entry 012 — Subphase 0.3.4 Specialized Agent Skills

### Completed in Subphase 0.3.4
- [x] `[0.3.4]` Create dedicated `SKILL.md` files for all 12 specialist agents (`agent-01-principal-architect` through `agent-12-independent-reviewer`) in `.agents/skills/` specifying Responsibilities, Permitted Tools, Implementation Standards, Verification Commands, and Completion Criteria.
- [x] `[0.3.4]` Backfill `4c9c283` (`stage-0.3.3`) in `docs/PROGRESS.md` and append Subphase `0.3.4` entries to `TODO.md` and `CHANGELOG.md`.

---

## [2026-11-27] Entry 013 — Subphase 0.3.5 Documentation Foundation (Phase 0.3 Closure)

### Completed in Subphase 0.3.5
- [x] `[0.3.5]` Author core governance and reference documentation: `docs/ENGINEERING_CONTRACT.md`, `docs/ROADMAP.md`, `README.md`, `CONTRIBUTING.md`, `LICENSE` (`Apache-2.0 OR MIT`), `SECURITY.md`, `PERFORMANCE.md`, `LICENSES.md`, `CODEBASE_GUIDE.md`, `USER_GUIDE.md`, and `DEVELOPER_GUIDE.md`.
- [x] `[0.3.5]` Backfill `e613fa1` (`stage-0.3.4`) in `docs/PROGRESS.md`, append Subphase `0.3.5` entries to `TODO.md` and `CHANGELOG.md`, and close Phase 0.3 (Engineering Environment).

---

## [2026-11-27] Entry 014 — Subphase 0.4.1 GitHub Actions Foundation

### Completed in Subphase 0.4.1
- [x] `[0.4.1]` Create `.github/ISSUE_TEMPLATE/` (`bug_report.yml`, `feature_request.yml`, `config.yml`), `.github/pull_request_template.md`, and `.github/dependabot.yml`.
- [x] `[0.4.1]` Create `.github/workflows/build-apps.yml` baseline CI (push/PR/`workflow_dispatch` input schema, `validate-inputs`, `rust-core-checks`, `license-and-security-audit`) and `.github/workflows/release.yml` (`v*.*.*`), with least-privilege permissions and live-verified immutable action commit SHAs.
- [x] `[0.4.1]` Backfill `53dada6` (`stage-0.3.5`) in `docs/PROGRESS.md` and append Subphase `0.4.1` entries to `TODO.md` and `CHANGELOG.md`.

---

## [2026-11-27] Entry 015 — Subphase 0.4.2 Selectable Application Builds

### Completed in Subphase 0.4.2
- [x] `[0.4.2]` Expand `.github/workflows/build-apps.yml` with dynamic target matrix resolution (`all`, `desktop`, `windows`, `linux`, `macos`, `mobile`, `android`, `ios`, `web`, `cli`, `api`), skipping unrelated jobs on targeted dispatch runs.
- [x] `[0.4.2]` Add native-runner build jobs (`build-cli`, `build-flutter`, `build-web` with `wasm32-unknown-unknown`, `build-api`), SHA-pinned Flutter/pnpm/Java actions, and Stage-aware honest execution paths.
- [x] `[0.4.2]` Backfill `8a4a67e` (`stage-0.4.1`) in `docs/PROGRESS.md` and append Subphase `0.4.2` entries to `TODO.md` and `CHANGELOG.md`.

---

## [2026-11-27] Entry 016 — Subphase 0.4.3 Independent Artifact Uploads

### Completed in Subphase 0.4.3
- [x] `[0.4.3]` Implement independent per-target staging (`dist/<artifact_name>/`) without nested ZIPs, bundling `LICENSE`, `LICENSES.md`, `build-metadata.json`, and `SHA256SUMS.txt` alongside real build outputs.
- [x] `[0.4.3]` Wire separate `actions/upload-artifact` (`v7.0.2` SHA-pinned) steps across `build-cli`, `build-flutter`, `build-web`, and `build-api`, plus `$GITHUB_STEP_SUMMARY` and `if: failure()` error reporting.
- [x] `[0.4.3]` Backfill `a108774` (`stage-0.4.2`) in `docs/PROGRESS.md` and append Subphase `0.4.3` entries to `TODO.md` and `CHANGELOG.md`.

---

## [2026-11-27] Entry 017 — Subphase 0.4.4 Runner & Platform Matrix Validation

### Completed in Subphase 0.4.4
- [x] `[0.4.4]` Audit live GitHub Actions runs across `ubuntu-24.04`, `windows-latest`, and `macos-latest` runners and diagnose `EmbarkStudios/cargo-deny-action` input ordering (`error: invalid value 'check'`).
- [x] `[0.4.4]` Fix `cargo-deny-action` inputs (`command: check`, `arguments: --all-features`, `command-arguments: advisories licenses bans sources`) in `.github/workflows/build-apps.yml` and `.github/workflows/release.yml`.
- [x] `[0.4.4]` Backfill `73c22a7` (`stage-0.4.3`) in `docs/PROGRESS.md` and append Subphase `0.4.4` entries to `TODO.md` and `CHANGELOG.md`.

---

## [2026-11-27] Entry 018 — Subphase 0.4.5 Local Build Scripts (Stage 0 Closure)

### Completed in Subphase 0.4.5
- [x] `[0.4.5]` Author `scripts/build.ps1` and `scripts/build.sh` supporting all 11 selectable targets, `release`/`debug` profiles, `IncludeTests`, `EnableOptionalCodecs`, sequential execution for 16 GB RAM hosts, independent `dist/local/<artifact_name>/` staging + `SHA256SUMS.txt`, and explicit cross-OS unavailability handling.
- [x] `[0.4.5]` Document local vs CI-delegated target support in `DEVELOPER_GUIDE.md`, backfill `9f3c441` (`stage-0.4.4`) in `docs/PROGRESS.md`, and append Subphase `0.4.5` entries to `TODO.md` and `CHANGELOG.md`.

---

## [2026-11-27] Entry 019 — Stage 1 Subphase 1.1.1 Cargo Workspace

### Completed in Subphase 1.1.1
- [x] `[1.1.1]` Expand root `Cargo.toml` workspace with `crates/file-core` (`L0`), `crates/codec-registry` (`L1`), `crates/validation-engine` (`L1`), `crates/job-manager` (`L5`), and `apps/cli` (`filefusion-cli` -> `filefusion` binary).
- [x] `[1.1.1]` Implement real baseline contracts (`ResourceLimits::for_host`, `ProgressEvent`, `CliExitCode`, `DoctorReport`, `validate_archive_member_path`, `validate_expansion_budget`, `CancellationToken`, and `filefusion doctor [--json]` / `--version`) with 12 passing unit & CLI integration tests and zero Clippy warnings.
- [x] `[1.1.1]` Backfill `d297150` (`stage-0.4.5`) in `docs/PROGRESS.md`, add Stage 1 queue, and append Subphase `1.1.1` entries to `TODO.md` and `CHANGELOG.md`.

---

## [2026-11-27] Entry 020 — Stage 1 Subphase 1.1.2 Shared Error & Logging System

### Completed in Subphase 1.1.2
- [x] `[1.1.2]` Implement `FileFusionError`, `ErrorDiagnostic`, `ErrorCategory`, and all 11 canonical `ErrorCode` (`FF-ERR-*`) mappings in `crates/file-core/src/error.rs`, plus `From<std::io::Error>`, `From<PathSafetyError>`, and `From<BombGuardError>`.
- [x] `[1.1.2]` Implement structured `tracing` initialization (`LoggingConfig`, `LogLevel`, `LogFormat::Pretty|Json` to `stderr`, `redact_path_for_log`) in `crates/file-core/src/logging.rs` with `native-logging` / `wasm` feature gating, `job-manager` tracing spans, and CLI `--log-level` / `--log-json` flags.
- [x] `[1.1.2]` Backfill `33bec50` (`stage-1.1.1`) in `docs/PROGRESS.md` and append Subphase `1.1.2` entries to `TODO.md` and `CHANGELOG.md`.

---

## [2026-11-27] Entry 021 — Stage 1 Subphase 1.1.3 Configuration Management (Phase 1.1 Closure)

### Completed in Subphase 1.1.3
- [x] `[1.1.3]` Implement versioned `FileFusionConfig`, `PrivacyConfig` (`offline_only = true`, `telemetry_enabled = false`, `external_ai_opt_in = false`), `BatchDefaults`, `CollisionPolicy`, and `PathConfig` in `crates/file-core/src/config.rs` with TOML parsing/serialization, deterministic layering (`defaults -> TOML -> FILEFUSION_* env -> CLI`), validation, and atomic `save_config_atomic` persistence.
- [x] `[1.1.3]` Wire `load_runtime_config` and `run_doctor_probe_with_config` (`DoctorReport.effective_limits`) through `codec-registry` and `job-manager`, and add CLI `--config`, `--workers`, `--max-memory-mb`, `--quiet`, `--no-color`, plus `filefusion config show|path|init|validate` subcommands with exit code `2` on invalid config.
- [x] `[1.1.3]` Backfill `e27daed` (`stage-1.1.2`) in `docs/PROGRESS.md` and append Subphase `1.1.3` entries to `TODO.md` and `CHANGELOG.md`.

---

## [2026-11-27] Entry 022 — Stage 1 Subphase 1.2.1 File Signature Detection (Phase 1.2 Begin)

### Completed in Subphase 1.2.1
- [x] `[1.2.1]` Expand `MediaKind` and `FormatId` in `crates/file-core/src/lib.rs` with MIME type and canonical extension tables across image, audio, video, document/ebook, archive, and executable formats.
- [x] `[1.2.1]` Implement bounded-I/O multi-offset magic-byte, structural container probing (`RIFF`, `ISO-BMFF`, `EBML`, `Ogg`, ZIP local/central-directory subformats), UTF-8/markup text heuristics, and extension spoofing enforcement (`FF-ERR-SEC-SPOOFED`) in `crates/file-core/src/signature.rs`.
- [x] `[1.2.1]` Wire `probe_file_signature` into `job-manager`, add `filefusion inspect [--strict] [--json]` in `apps/cli` + integration tests, backfill `24e9bc1` (`stage-1.1.3`) in `docs/PROGRESS.md`, and append Subphase `1.2.1` entries to `TODO.md` and `CHANGELOG.md`.

---

## [2026-11-27] Entry 023 — Stage 1 Subphase 1.2.2 Metadata Inspection

### Completed in Subphase 1.2.2
- [x] `[1.2.2]` Add `blake3`, `sha2`, and `xxhash-rust` (`xxh64`) streaming dependencies to `Cargo.toml` / `crates/file-core/Cargo.toml` under `deny.toml` permissive license compliance.
- [x] `[1.2.2]` Implement `crates/file-core/src/metadata.rs` (`FileInspectionReport`, `DigestSet`, `StreamDescriptor`, `MetadataSummary`, `compute_digests_from_reader`, `inspect_reader`, `inspect_path`, `inspect_bytes`) with fixed `64 KB` streaming I/O and structural/privacy inspection for PNG, JPEG, WebP, GIF, BMP, WAV, FLAC, MP3, PDF, ZIP/OOXML/EPUB/ODF, and UTF-8 text.
- [x] `[1.2.2]` Expose `job_manager::inspect_file` with privacy-safe `tracing` spans, upgrade `filefusion inspect [--signature-only] [--strict] [--json]` + integration tests, backfill `9b99e2c` (`stage-1.2.1`) in `docs/PROGRESS.md`, and append Subphase `1.2.2` entries to `TODO.md` and `CHANGELOG.md`.

---

## [2026-11-27] Entry 024 — Stage 1 Subphase 1.2.3 File Classification

### Completed in Subphase 1.2.3
- [x] `[1.2.3]` Implement Shannon entropy calculation (`compute_entropy_milli_bits`), `ProcessingDomain`, `CompressibilityTier`, `FileClassification`, and `classify_inspection_report` in `crates/file-core/src/classification.rs`.
- [x] `[1.2.3]` Integrate `FileClassification` and deterministic `recommended_actions` (security review, privacy scrub, zero-byte/no-op guards, domain compression recommendations) into `FileInspectionReport` and `job_manager`.
- [x] `[1.2.3]` Render classification & recommended actions in `filefusion inspect` CLI/JSON + E2E smoke tests, backfill `b9f9ea5` (`stage-1.2.2`) in `docs/PROGRESS.md`, and append Subphase `1.2.3` entries to `TODO.md` and `CHANGELOG.md`.

---

## [2026-11-27] Entry 025 — Stage 1 Subphase 1.2.4 Capability Registry (Phase 1.2 Closure)

### Completed in Subphase 1.2.4
- [x] `[1.2.4]` Implement Two-Tier `CapabilityRegistry` and `ToolAvailability` in `crates/codec-registry/src/lib.rs` registering all 3 active Tier 1 permissive capabilities and all 7 Tier 2 out-of-process tools (`ffmpeg`, `qpdf`, `tesseract`, `libreoffice`, `pandoc`, `calibre`, `7z`) with SPDX metadata, `LicenseClass`, and `capabilities_for_format` / `require_available` (`FF-ERR-ENG-MISSING`, exit code `5`).
- [x] `[1.2.4]` Gate native PATH/version probing cleanly behind `native-probe` with zero-warning `wasm` stub support across `codec-registry` and `job-manager`, expose `tool_availability` on `DoctorReport`, and verify `filefusion doctor --json`.
- [x] `[1.2.4]` Backfill `9ef27ca` (`stage-1.2.3`) in `docs/PROGRESS.md` and append Subphase `1.2.4` entries to `TODO.md` and `CHANGELOG.md`.

---

## [2026-11-27] Entry 026 — Stage 1 Subphase 1.3.1 Processing API (Phase 1.3 Begin)

### Completed in Subphase 1.3.1
- [x] `[1.3.1]` Implement shared `L0` Processing API contracts in `crates/file-core/src/pipeline.rs` (`CancellationToken`, `ProgressSink`, `JobContext`, `CompressionProfile`, `CompressionStatus`, `ProcessingOperation`, `ProcessingRequest`, `ValidatedSource`, `StagedPath` with `.filefusion.tmp.<id>` rollback-on-drop, `resolve_output_destination`, `ItemOutcome`, `JobReceipt`, `ProcessingHandler`).
- [x] `[1.3.1]` Consolidate `CancellationToken` in `file-core` (`MOD-RULE-01`), implement `JobManager::execute_batch` in `crates/job-manager/src/lib.rs`, backfill `a185199` (`stage-1.2.4`) in `docs/PROGRESS.md`, and append Subphase `1.3.1` entries to `TODO.md` and `CHANGELOG.md`.

---

## [2026-11-28] Entry 027 — Stage 1 Subphase 1.3.2 Plugin/Backend Contracts

### Completed in Subphase 1.3.2
- [x] `[1.3.2]` Define canonical backend, conversion, quality, validation, and subprocess traits (`CompressionEngine`, `ConversionEngine`, `ExternalToolAdapter`, `IsolatedProcessRunner`, `QualityEvaluator`, `OutputValidator`) + DTOs in `crates/file-core/src/contracts.rs` and inject `process_runner` into `JobContext`.
- [x] `[1.3.2]` Implement `MagicByteOutputValidator` (`OutputValidator`) in `crates/validation-engine/src/lib.rs` and `NativeIsolatedProcessRunner` (env-cleared, private temp dir, timeout & cancellation kill) + `BackendCatalog` in `crates/job-manager/src/backends.rs`.
- [x] `[1.3.2]` Backfill `8ec6dd2` (`stage-1.3.1`) in `docs/PROGRESS.md` and append Subphase `1.3.2` entries to `TODO.md` and `CHANGELOG.md`.


---

## [2026-11-28] Entry 028 — Stage 1 Subphase 1.3.3 Job Scheduler

### Completed in Subphase 1.3.3
- [x] `[1.3.3]` Add bounded `tokio` and `rayon` workspace dependencies with `native-scheduler`, `rayon-parallel`, and `wasm` feature gating (`FF-MOD-001` §6, `ADR-003`).
- [x] `[1.3.3]` Implement `JobScheduler`, `SchedulerController`, `JobPriority`, `ScheduledItemSpec`, and `SchedulerStats` in `crates/job-manager/src/scheduler.rs` enforcing `max_workers`, `max_memory_mb` permits (`FF-ERR-RES-MEMORY`), priority + FIFO ties, pause/resume, per-item cancellation, and `< 500 ms` global abort (`FR-BAT-001`, `NFR-PERF-003`, `NFR-REL-002`).
- [x] `[1.3.3]` Wire `JobManager::execute_batch` / `execute_scheduled_batch` in `crates/job-manager/src/lib.rs`, verify 1,200-item concurrency & WASM parity tests, backfill `b26ae47` (`stage-1.3.2`), and append entries to `TODO.md` and `CHANGELOG.md`.

---

## [2026-11-28] Entry 029 — Stage 1 Subphase 1.3.4 Cancellation and Progress

### Completed in Subphase 1.3.4
- [x] `[1.3.4]` Upgrade `CancellationToken` in `crates/file-core/src/pipeline.rs` with hierarchical parent-child linking (`child()`), reactive cancellation callbacks (`subscribe_on_cancel` / RAII `CancelSubscription`), `CallbackProgressSink`, `MonotonicProgressSink`, `ThrottledProgressSink`, and `JobContext::emit_progress_estimate`.
- [x] `[1.3.4]` Implement `ProgressBus`, `ProgressSubscription`, `ProgressBusStats`, rolling empirical ETA estimation, and `ndjson_line` formatting in `crates/job-manager/src/progress.rs`.
- [x] `[1.3.4]` Add `ActiveJobGuard`, `JobManager::cancel_job`, `JobManager::active_job_ids`, reactive scheduler wake hooks, monotonic lifecycle progress stages, and live `< 500 ms` cancellation tests across scheduler and isolated subprocess execution; backfill `02e86d7` (`stage-1.3.3`) in `docs/PROGRESS.md` and append to `TODO.md` and `CHANGELOG.md`.

---

## [2026-11-28] Entry 030 — Stage 1 Subphase 1.3.5 Resource Limits (Stage 1 Close)

### Completed in Subphase 1.3.5
- [x] `[1.3.5]` Implement runtime resource governance (`DeviceProfile`, `MemoryGovernor`, RAII `MemoryReservation`, `MemoryBudgetSnapshot`, `recommended_chunk_bytes`, `clamp_workers_for_job`, `validate_file_budget`, `parse_proc_meminfo_mb`, and `copy_bounded_streaming`) in `crates/file-core/src/limits.rs`.
- [x] `[1.3.5]` Inject shared `MemoryGovernor` into `JobContext` (`reserve_working_set_mb`), `JobScheduler` (`WorkerCompletion` RAII permit reservations + `SchedulerController::memory_snapshot`), and `JobManager` (`memory_governor` / `memory_stats`), closing Stage 1 Phase 1.3.
- [x] `[1.3.5]` Backfill `15651ec` (`stage-1.3.4`, `4aadd0a` lock sync) in `docs/PROGRESS.md` and append Subphase `1.3.5` entries to `TODO.md` and `CHANGELOG.md`.

---

## [2026-11-28] Entry 031 — Stage 2 Subphase 2.1.1 Compression Core & Quality Engine (Stage 2 Begin)

### Completed in Subphase 2.1.1
- [x] `[2.1.1]` Materialize `crates/quality-engine` (`L1`, `#![forbid(unsafe_code)]`, WASM-ready) implementing windowed SSIM, PSNR dB (`LOSSLESS_PSNR_CEILING_DB`), `diff_pixel_buffers`, `files_are_bit_exact` streaming comparator, and `BitExactQualityEvaluator` (`QualityEvaluator`) registered in `BackendCatalog::with_defaults()`.
- [x] `[2.1.1]` Materialize `crates/compression-core` (`L2`, `#![forbid(unsafe_code)]`, WASM-ready) implementing `CompressionProfileConfig` (all 5 `FR-CMP-001` presets), `prescreen_incompressible_noop`, `finalize_staged_compression` (mandatory `FR-CMP-002` No-Op inflation guard + `OutputValidator` + optional `min_ssim`/`target_max_bytes` checks), and atomic `commit_compression_stage`.
- [x] `[2.1.1]` Add Stage 2 execution queue to `docs/PROGRESS.md`, backfill `9db02eb` (`stage-1.3.5`), and append Subphase `2.1.1` entries to `TODO.md` and `CHANGELOG.md`.

---

## [2026-11-28] Entry 032 — Stage 2 Subphase 2.1.2 Hardware Acceleration Probe

### Completed in Subphase 2.1.2
- [x] `[2.1.2]` Materialize L1 `crates/hardware-acceleration` (`#![forbid(unsafe_code)]`, `hw-accel` vs zero-warning `wasm` no-op stub) implementing `SimdFeatures::detect()` (`SSE4.2`/`AVX2`/`AVX-512F`/`NEON`/`wasm_simd128`), GPU vendor discovery (`NVENC`, `QSV`, `AMF`, `VideoToolbox`, `MediaCodec`, `VAAPI`), `parse_ffmpeg_hwaccels`, `IsolatedProcessRunner`-backed `probe_ffmpeg_hwaccels`, and `select_video_backend` CPU fallback router (`FR-ADP-002`).
- [x] `[2.1.2]` Expose `probe_hardware_acceleration` and `JobManager::probe_hardware` in `crates/job-manager`, backfill `333a66f` (`stage-2.1.1`) in `docs/PROGRESS.md`, and append Subphase `2.1.2` entries to `TODO.md` and `CHANGELOG.md`.


---

## [2026-11-28] Entry 033 — Stage 2 Subphase 2.2.1 Image Compression Crate (`compression-image`)

### Completed in Subphase 2.2.1
- [x] `[2.2.1]` Materialize L3 `crates/compression-image` (`#![forbid(unsafe_code)]`, `FF-MOD-001` §4 DAG `L2 -> L3`, `FR-IMG-001`..`003`) implementing pure-Rust decode (`JPEG`, `PNG`, `WebP`, `GIF`, `BMP`, `TIFF`, `ICO`) & encode (`JPEG`, `PNG` via `oxipng`, lossless `WebP`, `AVIF` via `ravif`, plus `GIF`/`BMP`/`TIFF`/`ICO`).
- [x] `[2.2.1]` Implement EXIF TIFF orientation (`0x0112`, values `1..=8`) detection and normalization (`detect_exif_orientation`, `apply_exif_orientation`), SIMD resizing via `fast_image_resize` (`ResizeFilter`, `ImageResizeSpec`), `RasterQualityEvaluator` (empirical SSIM/PSNR via `quality_engine`), and `RasterCompressionEngine` wired with `JobContext` memory reservations and `compression_core::finalize_staged_compression` (`FR-CMP-002` No-Op guard).
- [x] `[2.2.1]` Backfill `a702338` (`stage-2.1.2`) in `docs/PROGRESS.md` and append Subphase `2.2.1` entries to `TODO.md` and `CHANGELOG.md`.

---

## [2026-11-28] Entry 034 — Stage 2 Subphase 2.2.2 Archive Compression Crate (`compression-archive`)

### Completed in Subphase 2.2.2
- [x] `[2.2.2]` Materialize L3 `crates/compression-archive` (`#![forbid(unsafe_code)]`, `FF-MOD-001` §4 DAG `L2 -> L3`, `FR-ARC-001`..`003`, `FR-SEC-001`..`002`, `FR-REP-001`) supporting pure/permissive multi-format archive create/list/extract across `Zip`, `Tar`, `TarGz`, `TarZst`, `TarXz`, `TarBz2`, `TarLz4`, `TarBrotli`, and `SevenZ`, plus `native-process` Tier 2 `7z` `IsolatedProcessRunner` helpers.
- [x] `[2.2.2]` Implement `StreamCodec` (`Zstd`, `Lz4`, `Brotli`, `Gzip`, `Xz` via `lzma-rust2`, `Bzip2` via `oxiarc-bzip2`), `.ffenc` authenticated encryption (`AES-256-GCM` & `ChaCha20-Poly1305` + `Argon2id` + CSPRNG salt/nonce + header AAD), `repair_truncated_zip` local-header recovery + Central Directory reconstruction, and `ArchiveCompressionEngine` (`FR-CMP-002` No-Op guard & bit-exact BLAKE3 `ArchiveQualityEvaluator`).
- [x] `[2.2.2]` Enforce Zip-Slip traversal/symlink rejection (`ErrorCode::SecTraversal`) and mid-stream decompression-bomb ceilings (`ErrorCode::SecBomb`) with partial directory cleanup, backfill `25d97e5` (`stage-2.2.1`) in `docs/PROGRESS.md`, and append Subphase `2.2.2` entries to `TODO.md` and `CHANGELOG.md`.

---

## [2026-11-28] Entry 035 — Stage 2 Subphase 2.2.3 Audio Compression Crate (`compression-audio`)

### Completed in Subphase 2.2.3
- [x] `[2.2.3]` Materialize L3 `crates/compression-audio` (`#![forbid(unsafe_code)]`, `FF-MOD-001` §4 DAG `L2 -> L3`, `FR-AV-002`, `FR-AV-003`, `FR-CMP-001`..`002`, `FR-MET-001`..`002`) implementing pure-Rust decoding (`hound`, `claxon`, `symphonia`) and lossless `WAV`/`FLAC` (`flacenc`) encoding tuned to `CompressionProfile`.
- [x] `[2.2.3]` Implement sample-rate conversion (`resample_pcm`), channel remapping (`remap_channels`), ITU-R BS.1770 / EBU R128 K-weighted gated loudness measurement & peak-clamped normalization (`measure_integrated_lufs`, `normalize_loudness_lufs`), `ID3v1`/`ID3v2` metadata reading/writing/stripping (`tags.rs`), `AudioQualityEvaluator`, `AudioCompressionEngine`, and `native-process` Tier 2 `FFmpeg` `IsolatedProcessRunner` helpers.
- [x] `[2.2.3]` Backfill `ba2658d` (`stage-2.2.2`) in `docs/PROGRESS.md` and append Subphase `2.2.3` entries to `TODO.md` and `CHANGELOG.md`.


---

## [2026-11-28] Entry 036 — Stage 2 Subphase 2.2.4 Video Compression Crate (`compression-video`)

### Completed in Subphase 2.2.4
- [x] `[2.2.4]` Materialize L3 `crates/compression-video` (`#![forbid(unsafe_code)]`, `FF-MOD-001` §4 DAG `L3_VID -> L2_CMP & L1_REG & L1_HW`, `FR-AV-001`, `FR-AV-003`, `FR-ADP-002`, `FR-CMP-001`..`002`) with zero new third-party dependencies in `Cargo.lock`.
- [x] `[2.2.4]` Implement `ffprobe` JSON inventory parsing (`parse_ffprobe_json_probe`), lossless container fast-path remux compatibility (`can_fast_path_remux`), unfinalized container atom health check (`diagnose_unfinalized_video_container`), pure-Rust SRT <-> WebVTT subtitle parser & converter (`subtitles.rs`), hardware-vs-CPU encoder routing (`resolve_video_route`), explicit `-f <muxer>` staged `FFmpeg` `IsolatedProcessRunner` command generation, `libvmaf` parser, and `VideoCompressionEngine` (`FR-CMP-002` No-Op Guard).
- [x] `[2.2.4]` Backfill `ee5957b` (`stage-2.2.3`) in `docs/PROGRESS.md` and append Subphase `2.2.4` entries to `TODO.md` and `CHANGELOG.md`.


---

## [2026-11-28] Entry 037 — Stage 2 Subphase 2.2.5 PDF & Office Compression Crates (`compression-pdf`, `compression-office`)

### Completed in Subphase 2.2.5
- [x] `[2.2.5]` Materialize L3 `crates/compression-pdf` (`#![forbid(unsafe_code)]`, `FF-MOD-001` §4 DAG `L3_PDF -> L2_CMP & L1_REG & L3_IMG`, `FR-PDF-001`..`002`, `FR-REP-001`..`002`, `FR-MET-002`) with MIT-licensed `lopdf` `0.42`, shared `compression-image::optimize_raster_in_memory`, Flate stream compaction, object pruning, `/Info`/`XMP` privacy scrubbing, honest `PdfRepairAssessment`, and `native-process` Tier 2 `qpdf` `IsolatedProcessRunner` helpers.
- [x] `[2.2.5]` Materialize L3 `crates/compression-office` (`#![forbid(unsafe_code)]`, `FF-MOD-001` §4 DAG `L3_OFF -> L2_CMP & L1_REG & L3_IMG`, `FR-DOC-002`..`003`, `FR-SEC-001`, `FR-MET-002`) supporting `DOCX`, `XLSX`, `PPTX`, `ODT`, `ODS`, `ODP`, and `EPUB` with Zip-Slip & decompression-bomb guards, embedded raster compression via `compression-image`, lossless XML minification, privacy metadata scrubbing, `mimetype`-first `Stored` packaging, and `FR-CMP-002` No-Op Guard.
- [x] `[2.2.5]` Backfill `2660aed` (`stage-2.2.4`) in `docs/PROGRESS.md` and append Subphase `2.2.5` entries to `TODO.md` and `CHANGELOG.md`.


---

## [2026-11-28] Entry 038 — Stage 2 Subphase 2.3.1 Adaptive Optimization Engine (`optimization-engine`)

### Completed in Subphase 2.3.1
- [x] `[2.3.1]` Materialize L4 `crates/optimization-engine` (`#![forbid(unsafe_code)]`, `FF-MOD-001` §4 DAG `L4_OPT -> L1_HW & L3_IMG & L3_VID & L3_AUD & L3_PDF & L3_OFF & L3_ARC`, `FR-ADP-001`..`002`, `FR-CMP-003`, `FR-DUP-002`, `FR-DET-004`) with zero new external dependencies in `Cargo.lock`.
- [x] `[2.3.1]` Implement content-aware `recommend_codec` (`recommender.rs`), quality-constrained quantizer binary-search solver (`solver.rs`), FastCDC Gear-hash chunking & `dHash`/2D-DCT `pHash` similarity hashing (`dedup.rs`), and unified `OptimizationDispatcher` (`dispatcher.rs`) across all six L3 engines.
- [x] `[2.3.1]` Backfill `19c5a04` (`stage-2.2.5`) in `docs/PROGRESS.md` and append Subphase `2.3.1` entries to `TODO.md` and `CHANGELOG.md`.


---

## [2026-11-28] Entry 039 — Stage 2 Subphase 2.3.2 Stage 2 CLI & Criterion Benchmarks

### Completed in Subphase 2.3.2
- [x] `[2.3.2]` Wire L5 `crates/job-manager` to L4 `crates/optimization-engine` cleanly (`FF-MOD-001` §4 DAG `L5_JOB -> L0 & L1_REG & L4_OPT`), exposing `JobManager::compress_file`, `create_archive`, `list_archive`, `extract_archive`, and `run_quick_benchmark`.
- [x] `[2.3.2]` Implement `filefusion compress`, `filefusion archive {create,list,extract}`, and `filefusion bench` CLI subcommands (`apps/cli/src/main.rs`) with `--json` output and full E2E CLI integration coverage (`apps/cli/tests/cli_smoke.rs`).
- [x] `[2.3.2]` Add permanent Criterion benchmark suite (`Criterion 0.6.0`, `tests/benchmarks/stage2_compression.rs`), execute host CPU benchmarks, record baselines in `PERFORMANCE.md`, backfill `c0bfe41` (`stage-2.3.1`) in `docs/PROGRESS.md`, and append Subphase `2.3.2` entries to `TODO.md` and `CHANGELOG.md`.


---

## [2026-11-28] Entry 040 — Stage 3 Subphase 3.1.1 Conversion Core Foundation, Multi-Hop Router & Cross-Media Conversion (`conversion-core`)

### Completed in Subphase 3.1.1
- [x] `[3.1.1]` Materialize L4 `crates/conversion-core` (`#![forbid(unsafe_code)]`, `FF-MOD-001` §4 DAG `L4_CNV -> L1_REG & L1_VAL & L3_*` with optional `compression-video` gated behind `native-process`) with zero new external crates in `Cargo.lock`.
- [x] `[3.1.1]` Implement `ConversionGraph` weighted Dijkstra direct-first / lossless-hub router (`graph.rs`), pre-flight compatibility warnings + structural markup inventory & `ConversionFidelityReport` builder (`preflight.rs`, `FR-FID-001`..`002`), and `UniversalConversionEngine` implementing `ConversionEngine` (`lib.rs`, `FF-CTR-001` §3.2) for pure-Rust cross-raster, cross-archive, and cross-audio (`-> WAV`/`FLAC`) conversions plus `native-process`-gated Tier 2 `FFmpeg` dispatch.
- [x] `[3.1.1]` Backfill `8f69a4c` (`stage-2.3.2`) in `docs/PROGRESS.md`, initialize the Stage 3 subphase queue, and append Subphase `3.1.1` entries to `TODO.md` and `CHANGELOG.md`.


---

## [2026-11-28] Entry 041 — Stage 3 Subphase 3.1.2 Bidirectional Structured Text, Markup & Tabular Engines (`FR-DOC-001`)

### Completed in Subphase 3.1.2
- [x] `[3.1.2]` Implement pure-Rust bidirectional structured markup parser & renderer (`crates/conversion-core/src/markup.rs`) supporting `Markdown` (CommonMark/GFM), HTML5, and `PlainText` with XSS sanitization (`<script>`/`<iframe>` removal, `on*=` stripping, `javascript:` URL blocking, `NFR-SEC-001`).
- [x] `[3.1.2]` Implement pure-Rust bidirectional structured data & tabular converter (`crates/conversion-core/src/tabular.rs`) covering RFC 4180 `CSV`/`TSV`, `JSON`, `YAML` 1.2 subset, and XXE/entity-bomb-hardened `XML` (`<!ENTITY`/`<!DOCTYPE` rejection, depth `<= 64`, `NFR-SEC-001`), plus direct tabular export routes into `Markdown`/`HTML`/`PlainText`.
- [x] `[3.1.2]` Wire `FID-MARKUP-PLAINTEXT` and `FID-CSV-FLATTEN` pre-flight warnings (`preflight.rs`) and quantitative pre/post `StructuralInventory` reporting into `UniversalConversionEngine` (`lib.rs`), backfill `0160de1` (`stage-3.1.1`) in `docs/PROGRESS.md`, and append Subphase `3.1.2` entries to `TODO.md` and `CHANGELOG.md`.


---

## [2026-11-28] Entry 042 — Stage 3 Subphase 3.2.1 Native Office, ODF & EPUB Conversions + Tier 2 Adapters (`FR-DOC-002`..`003`)

### Completed in Subphase 3.2.1
- [x] `[3.2.1]` Reuse L3 `compression-office` Zip-Slip/bomb-guarded container I/O (`read_validated_zip_entries`, `write_office_container_entries` with `mimetype`-first `Stored` packaging) cleanly in L4 `conversion-core` without duplicating ZIP crates (`MOD-RULE-02`).
- [x] `[3.2.1]` Implement pure-Rust WASM-native Office/ODF/EPUB conversion engine (`crates/conversion-core/src/office_epub.rs`) across `XLSX`/`ODS` <-> tabular/markup and `DOCX`/`ODT`/`EPUB` <-> `Markdown`/`HTML`/`PlainText` (preserving TOC `nav.xhtml`, spine ordering, hyperlinks, headings, tables, privacy scrubbing, and XML entity-bomb blocking).
- [x] `[3.2.1]` Implement sanitized `ExternalToolAdapter` implementations (`LibreOfficeAdapter`, `PandocAdapter`, `CalibreAdapter`, `crates/conversion-core/src/tier2_adapters.rs`), wire Tier 1 & Tier 2 routes through `ConversionGraph` and `UniversalConversionEngine`, backfill `6ea5132` (`stage-3.1.2`) in `docs/PROGRESS.md`, and append Subphase `3.2.1` entries to `TODO.md` and `CHANGELOG.md`.


---

## [2026-11-28] Entry 043 — Stage 3 Subphase 3.2.2 Stage 3 CLI (`convert`), `job-manager` Wiring & Fidelity Corpus

### Completed in Subphase 3.2.2
- [x] `[3.2.2]` Wire L5 `crates/job-manager` to L4 `crates/conversion-core` (`FF-MOD-001` §4 DAG `L5_JOB -> L0 & L1_REG & L4_OPT & L4_CNV`), exposing `JobManager::preflight_conversion`, `JobManager::convert_file`, and `ConvertFileReport` across both native and `wasm` feature builds.
- [x] `[3.2.2]` Implement `filefusion convert` CLI subcommand (`--to`, `-o/--output`, `--preflight-only`, `--force-loss`, `--no-preserve-structure`, `--strip-metadata`, `--overwrite`, `--json`) in `apps/cli/src/main.rs` (`MOD-RULE-03`) with expanded `CliTargetFormat` coverage across markup, tabular, OOXML/ODF/EPUB, audio, video, and archive targets.
- [x] `[3.2.2]` Add E2E Stage 3 conversion & fidelity corpus tests (`apps/cli/tests/cli_smoke.rs`) and `JobManager::convert_file` unit tests, backfill `1c90d81` (`stage-3.2.1`) in `docs/PROGRESS.md`, and append Subphase `3.2.2` entries to `TODO.md` and `CHANGELOG.md`.


---

## [2026-11-28] Entry 044 — Stage 4 Subphase 4.1.1 PDF Studio Structural Manipulation (`merge`/`split`/`reorder`/`rotate`/`crop`/`watermark`)

### Completed in Subphase 4.1.1
- [x] `[4.1.1]` Implement pure-Rust WASM-native PDF Studio operations in `crates/compression-pdf/src/studio.rs` (`#![forbid(unsafe_code)]`, `FR-PDF-001`, `FR-PDF-004`): `parse_page_range_spec`, `merge_pdfs`, `extract_or_reorder_pdf_pages`, `split_pdf_by_ranges`, `split_pdf_every_n_pages`, `rotate_pdf_pages`, `crop_pdf_pages`, and `apply_pdf_text_watermark` with encrypted PDF `FmtDrm` gating and zero external crate additions.
- [x] `[4.1.1]` Implement `inspect_pdf_scan_status` (`PdfScanInspection`) detecting image-only scanned OCR candidates and `/Sig`/`/ByteRange` signature dictionaries (`FR-PDF-003`..`004`), and export PDF Studio APIs in `crates/compression-pdf/src/lib.rs`.
- [x] `[4.1.1]` Backfill `186ecde` (`stage-3.2.2`) and initialize the Stage 4 subphase queue in `docs/PROGRESS.md`, and append Subphase `4.1.1` entries to `TODO.md` and `CHANGELOG.md`.


---

## [2026-11-28] Entry 045 — Stage 4 Subphase 4.1.2 Local OCR, PDF Encryption & Signature Verification

### Completed in Subphase 4.1.2
- [x] `[4.1.2]` Implement `crates/compression-pdf/src/ocr.rs`: isolated Tier 2 `TesseractAdapter` (`ExternalToolAdapter`, `MOD-RULE-04`, `Apache-2.0`), TSV/`hOCR` parsers (`parse_tesseract_tsv`, `parse_tesseract_hocr`), raster `/Image` `XObject` extractor (`extract_pdf_page_images`), and invisible `3 Tr` searchable PDF text overlay builder (`embed_searchable_ocr_text_layer`, `build_searchable_pdf_from_image_and_ocr`) (`FR-PDF-003`).
- [x] `[4.1.2]` Implement `crates/compression-pdf/src/security.rs`: pure-Rust `AES-256` (`V5`/`R6`) and `AES-128` (`V4`/`R4`) PDF password encryption/decryption (`encrypt_pdf_bytes`, `decrypt_pdf_bytes`, `inspect_pdf_security`), granular ISO-32000 `/P` permissions (`PdfPermissions`), sanitized `qpdf` crypto builders, and `/Sig` `/ByteRange` + DER `/Contents` audit (`verify_pdf_signatures`) (`FR-PDF-004`).
- [x] `[4.1.2]` Re-export PDF OCR & security APIs cleanly from `crates/compression-pdf/src/lib.rs`, backfill `e5dbda3` onto `4.1.1` in `docs/PROGRESS.md`, add unit/integration tests across native (113 passing) and WASM (58 passing) configurations, and append `TODO.md` / `CHANGELOG.md` with untouched `Cargo.lock`.


---

## [2026-11-28] Entry 046 — Stage 4 Subphase 4.2.1 Unified File Repair, Metadata Sanitizer & `.ffenc` Orchestration

### Completed in Subphase 4.2.1
- [x] `[4.2.1]` Add L3 Stage 4 recovery & metadata primitives: `compression-archive` (`inspect_zip_health`, `inspect_ffenc_header`), `compression-pdf` (`repair_pdf_bytes` with pure-Rust `xref` synthesis, `repair_pdf_to_staged_with_runner`, `update_pdf_metadata_bytes`), `compression-office` (`OfficeContainerMetadata`, `extract_office_metadata_from_entries`, `edit_metadata_xml`, `sanitize_and_edit_office_metadata`), and `compression-video` (`build_ffmpeg_container_repair_invocation`).
- [x] `[4.2.1]` Implement L4 Stage 4 orchestrators in `optimization-engine/src/{repair.rs,metadata.rs,security.rs}` (`FR-REP-001`..`002`, `FR-MET-001`..`002`, `FR-SEC-002`) covering honest recoverability classification, `PDF`/`ZIP`/`MP4`/`MOV`/`MKV`/`WebM` staged recovery, cross-domain privacy scrubbing/editing, and `.ffenc` / PDF AES staged encryption.
- [x] `[4.2.1]` Wire L5 `JobManager` orchestration methods in `crates/job-manager/src/stage4.rs`, backfill `84a9eeb` onto `4.1.2` in `docs/PROGRESS.md`, verify `116` native / `61` WASM tests passing with `Cargo.toml` and `Cargo.lock` untouched, and append Entry 046 to `TODO.md` / `CHANGELOG.md`.



---

## [2026-11-28] Entry 047 — Stage 4 Subphase 4.2.2 Stage 4 CLI & Security/Recovery Corpus Tests (Stage 4 Completion)

### Completed in Subphase 4.2.2
- [x] `[4.2.2]` Re-export PDF Studio & OCR APIs from L4 `crates/optimization-engine/src/lib.rs` and add L5 coordinator methods (`JobManager::{pdf_merge, pdf_split, pdf_reorder, pdf_rotate, pdf_crop, pdf_watermark, pdf_scan_check, pdf_verify_signatures, pdf_inspect_security, pdf_ocr}`) in `crates/job-manager/src/stage4.rs` (`MOD-RULE-03`, `tier2-tesseract` `EngMissing` gating).
- [x] `[4.2.2]` Implement Stage 4 CLI commands in `apps/cli/src/{stage4_cmds.rs,main.rs}`: `filefusion pdf <merge|split|reorder|rotate|crop|watermark|scan-check|verify-sig|security|ocr|protect|unlock>`, `filefusion repair`, `filefusion metadata <inspect|scrub|edit>`, `filefusion encrypt`, and `filefusion decrypt` with `--passphrase` / `--passphrase-env`, JSON/human output, boxed heavy CLI subcommand enums, and an `8 MiB` `std::thread::Builder` runner stack.
- [x] `[4.2.2]` Add 4 Stage 4 CLI & security/recovery corpus E2E tests in `apps/cli/tests/cli_smoke.rs` (`cli_pdf_studio_merge_split_rotate_crop_and_watermark_e2e`, `cli_pdf_security_scan_verify_protect_and_unlock_e2e`, `cli_repair_corpus_recovers_corrupt_pdf_and_zip_and_rejects_noise`, `cli_metadata_encrypt_decrypt_and_ffenc_tamper_security_corpus`), backfill `4f3f737` (`stage-4.2.1`) in `docs/PROGRESS.md`, verify `120` native / `61` WASM tests passing with `Cargo.toml` and `Cargo.lock` untouched, and append Entry 047 to `TODO.md` / `CHANGELOG.md`.
