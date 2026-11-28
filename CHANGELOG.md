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

---

## [0.0.15-stage0.4.2] — 2026-11-27

### Subphase 0.4.2 — Implement Selectable Application Builds
- **Changed:** Expanded `.github/workflows/build-apps.yml` with dynamic target matrix resolution across all 11 selectable `app` inputs, native-runner jobs (`build-cli`, `build-flutter`, `build-web`, `build-api`), `wasm32-unknown-unknown` compilation, SHA-pinned Flutter/pnpm/Java setup actions, and truthful Stage-aware build paths.
- **Changed:** Updated `docs/PROGRESS.md` (including verified `8a4a67e` commit reference for `0.4.1`) and appended Entry 015 to `TODO.md`.

---

## [0.0.16-stage0.4.3] — 2026-11-27

### Subphase 0.4.3 — Implement Independent Artifact Uploads
- **Changed:** Updated `.github/workflows/build-apps.yml` so every platform/application target (`filefusion-windows-x64`, `filefusion-linux-x64`, `filefusion-macos-arm64`, `filefusion-android-apk`, `filefusion-ios-build`, `filefusion-web`, `filefusion-cli-*`, `filefusion-api-linux-x64`) stages `LICENSE`, `LICENSES.md`, `build-metadata.json`, and `SHA256SUMS.txt` without nested ZIPs and uploads independently via SHA-pinned `actions/upload-artifact` (`v7.0.2`).
- **Changed:** Updated `docs/PROGRESS.md` (including verified `a108774` commit reference for `0.4.2`) and appended Entry 016 to `TODO.md`.

---

## [0.0.17-stage0.4.4] — 2026-11-27

### Subphase 0.4.4 — Validate Runner and Platform Matrix
- **Fixed:** Corrected `EmbarkStudios/cargo-deny-action` inputs (`command`, `arguments`, `command-arguments`) in `.github/workflows/build-apps.yml` and `.github/workflows/release.yml` after live CI matrix diagnostics on run `46014125464`.
- **Changed:** Validated native GitHub-hosted runner matrix (`ubuntu-24.04`, `windows-latest`, `macos-latest`), updated `docs/PROGRESS.md` (including verified `73c22a7` commit reference for `0.4.3`), and appended Entry 017 to `TODO.md`.

---

## [0.0.18-stage0.4.5] — 2026-11-27

### Subphase 0.4.5 — Validate Local Build Scripts (Stage 0 Complete)
- **Added:** Cross-platform local build scripts `scripts/build.ps1` and `scripts/build.sh` matching `.github/workflows/build-apps.yml` target selection, sequential 16 GB RAM execution, `dist/local/` staging + `SHA256SUMS.txt`, and cross-OS target documentation.
- **Changed:** Documented local vs GitHub-hosted runner targets in `DEVELOPER_GUIDE.md`; updated `docs/PROGRESS.md` (including verified `9f3c441` commit reference for `0.4.4`) and appended Entry 018 to `TODO.md`.

---

## [0.1.0-stage1.1.1] — 2026-11-27

### Subphase 1.1.1 — Cargo Workspace (Stage 1 Begin)
- **Added:** Multi-crate Cargo workspace (`crates/file-core`, `crates/codec-registry`, `crates/validation-engine`, `crates/job-manager`, `apps/cli` producing `filefusion` binary) with `ResourceLimits::for_host`, `DoctorReport`, path-traversal & bomb-guard validators, `CancellationToken`, `filefusion doctor [--json]`, and 12 passing unit & integration tests.
- **Changed:** Backfilled `d297150` (`stage-0.4.5`) in `docs/PROGRESS.md`, added Stage 1 subphase queue, and appended Entry 019 to `TODO.md`.

---

## [0.1.1-stage1.1.2] — 2026-11-27

### Subphase 1.1.2 — Shared Error and Logging System
- **Added:** Canonical typed error system (`FileFusionError`, `ErrorDiagnostic`, `ErrorCategory`, 11 `ErrorCode` `FF-ERR-*` variants) and privacy-aware structured `tracing` subsystem (`LoggingConfig`, `init_logging` to `stderr`, `redact_path_for_log`) in `crates/file-core`, wired through `validation-engine`, `job-manager`, and `apps/cli` (`--log-level`, `--log-json`).
- **Changed:** Backfilled `33bec50` (`stage-1.1.1`) in `docs/PROGRESS.md` and appended Entry 020 to `TODO.md`.

---

## [0.1.2-stage1.1.3] — 2026-11-27

### Subphase 1.1.3 — Configuration Management (Phase 1.1 Complete)
- **Added:** Schema-versioned configuration system (`FileFusionConfig`, `PrivacyConfig`, `BatchDefaults`, `CollisionPolicy`, `PathConfig`, `LoadedConfig`, `CliConfigOverrides`) in `crates/file-core/src/config.rs` with TOML support, deterministic multi-layer resolution, traversal/privacy validation, and atomic writes; CLI `--config`, `--workers`, `--max-memory-mb`, `--quiet`, `--no-color`, and `filefusion config show|path|init|validate` subcommands plus `DoctorReport.effective_limits`.
- **Changed:** Extended `FileFusionError` with `.with_exit_code(...)` for `CliExitCode::InvalidUsage`, wired `ansi_color` / `NO_COLOR` in `LoggingConfig`, backfilled `e27daed` (`stage-1.1.2`) in `docs/PROGRESS.md`, and appended Entry 021 to `TODO.md`.

---

## [0.1.3-stage1.2.1] — 2026-11-27

### Subphase 1.2.1 — File Signature Detection (Phase 1.2 Begin)
- **Added:** Multi-offset magic-byte and container signature detector (`crates/file-core/src/signature.rs`: `SignatureMatch`, `DetectionConfidence`, `detect_signature_from_bytes`, `detect_signature_from_reader`, `detect_signature_from_path`, `enforce_spoofing_policy`), expanded `FormatId` / `MediaKind` MIME & extension mappings, `job_manager::probe_file_signature`, and CLI `filefusion inspect [--strict] [--json]`.
- **Changed:** Backfilled `24e9bc1` (`stage-1.1.3`) in `docs/PROGRESS.md` and appended Entry 022 to `TODO.md`.

---

## [0.1.4-stage1.2.2] — 2026-11-27

### Subphase 1.2.2 — Metadata Inspection
- **Added:** Single-pass `64 KB` streaming `BLAKE3` / `SHA-256` / `xxHash64` digest engine (`DigestSet`) and normalized structural/privacy metadata inspector (`FileInspectionReport`, `StreamDescriptor`, `MetadataSummary`, `inspect_reader`, `inspect_path`, `inspect_bytes`) in `crates/file-core/src/metadata.rs` plus `job_manager::inspect_file`.
- **Changed:** Upgraded `filefusion inspect` to return full `FileInspectionReport` payloads by default (preserving fast `--signature-only` and `--strict` flags), backfilled `9b99e2c` (`stage-1.2.1`) in `docs/PROGRESS.md`, and appended Entry 023 to `TODO.md`.

---

## [0.1.5-stage1.2.3] — 2026-11-27

### Subphase 1.2.3 — File Classification
- **Added:** Shannon entropy estimator (`compute_entropy_milli_bits`), `ProcessingDomain`, `CompressibilityTier`, `FileClassification`, and rule-based action recommender (`classify_inspection_report`) in `crates/file-core/src/classification.rs`, attached to `FileInspectionReport` and surfaced in `job-manager` and `filefusion inspect`.
- **Changed:** Populated `FileInspectionReport.recommended_actions` and `classification` during inspection, backfilled `b9f9ea5` (`stage-1.2.2`) in `docs/PROGRESS.md`, and appended Entry 024 to `TODO.md`.

---

## [0.1.6-stage1.2.4] — 2026-11-27

### Subphase 1.2.4 — Capability Registry (Phase 1.2 Complete)
- **Added:** Two-Tier `CapabilityRegistry` and `ToolAvailability` in `crates/codec-registry/src/lib.rs` discovering Tier 1 built-in modules alongside all 7 Tier 2 external binaries (`ffmpeg`, `qpdf`, `tesseract`, `libreoffice`, `pandoc`, `calibre`, `7z`) with SPDX expressions, `LicenseClass`, `capabilities_for_format`, and `require_available` (`FF-ERR-ENG-MISSING`).
- **Changed:** Enabled `native-probe` vs zero-warning `wasm` feature gating across `codec-registry` and `job-manager`, exposed `DoctorReport.tool_availability`, backfilled `9ef27ca` (`stage-1.2.3`) in `docs/PROGRESS.md`, and appended Entry 025 to `TODO.md`.

---

## [0.1.7-stage1.3.1] — 2026-11-27

### Subphase 1.3.1 — Processing API (Phase 1.3 Begin)
- **Added:** Shared `L0` Processing API contracts (`crates/file-core/src/pipeline.rs`: `JobContext`, `ProgressSink`, `VecProgressSink`, `CompressionProfile`, `CompressionStatus`, `ProcessingOperation`, `ProcessingRequest`, `ValidatedSource`, atomic `StagedPath`, `resolve_output_destination`, `ItemOutcome`, `JobReceipt`, `ProcessingHandler`) and `JobManager::execute_batch` in `crates/job-manager/src/lib.rs`.
- **Changed:** Moved `CancellationToken` into `file-core` (re-exported identically from `job-manager` for `MOD-RULE-01` compatibility), backfilled `a185199` (`stage-1.2.4`) in `docs/PROGRESS.md`, and appended Entry 026 to `TODO.md`.

---

## [0.1.8-stage1.3.2] — 2026-11-28

### Subphase 1.3.2 — Plugin/Backend Contracts
- **Added:** Canonical engine/backend traits & DTOs (`crates/file-core/src/contracts.rs`: `CompressionEngine`, `ConversionEngine`, `ExternalToolAdapter`, `IsolatedProcessRunner`, `QualityEvaluator`, `OutputValidator`, `ProcessInvocation`, `ProcessOutput`), `MagicByteOutputValidator` (`crates/validation-engine/src/lib.rs`), and `NativeIsolatedProcessRunner` + `BackendCatalog` (`crates/job-manager/src/backends.rs`).
- **Changed:** Wired `IsolatedProcessRunner` into `JobContext` and `JobManager::create_context`, backfilled `8ec6dd2` (`stage-1.3.1`) in `docs/PROGRESS.md`, and appended Entry 027 to `TODO.md`.


---

## [0.1.9-stage1.3.3] — 2026-11-28

### Subphase 1.3.3 — Job Scheduler
- **Added:** Bounded dual-pool `tokio` + `rayon` job scheduler (`crates/job-manager/src/scheduler.rs`: `JobScheduler`, `SchedulerController`, `JobPriority`, `ScheduledItemSpec`, `SchedulerStats`) with priority ordering, FIFO ties, memory-weighted permits, pause/resume, per-item cancellation, `< 500 ms` abort, and zero-warning WASM sequential fallback.
- **Changed:** Wired `JobManager` batch execution through `JobScheduler` (`crates/job-manager/src/lib.rs`), fixed `wasm` test feature imports in `crates/job-manager/src/backends.rs`, backfilled `b26ae47` (`stage-1.3.2`) in `docs/PROGRESS.md`, and appended Entry 028 to `TODO.md`.

---

## [0.1.10-stage1.3.4] — 2026-11-28

### Subphase 1.3.4 — Cancellation and Progress
- **Added:** Hierarchical `CancellationToken::child()` & `subscribe_on_cancel` hooks, `CallbackProgressSink`, `MonotonicProgressSink`, `ThrottledProgressSink`, and `JobContext::emit_progress_estimate` (`crates/file-core/src/pipeline.rs`); multi-subscriber `ProgressBus` with empirical ETA estimation and NDJSON formatting (`crates/job-manager/src/progress.rs`); `JobManager::cancel_job` / `active_job_ids` registry and live subprocess cancellation latency test (`crates/job-manager/src/{lib.rs,backends.rs}`).
- **Changed:** Wired `JobScheduler` reactive `subscribe_on_cancel` hooks and `run_validated_item` monotonic lifecycle stages (`Processing` / `Validating`), added `serde_json` to `crates/job-manager/Cargo.toml`, backfilled `02e86d7` (`stage-1.3.3`) in `docs/PROGRESS.md`, and appended Entry 029 to `TODO.md`.

---

## [0.1.11-stage1.3.5] — 2026-11-28

### Subphase 1.3.5 — Resource Limits (Stage 1 Phase 1.3 Complete)
- **Added:** L0 `DeviceProfile` (`Workstation`, `Mobile`, `WebWasm`), thread-safe `MemoryGovernor` / RAII `MemoryReservation`, `MemoryBudgetSnapshot`, `recommended_chunk_bytes` (`64 KiB`–`1 MiB`), `clamp_workers_for_job`, `validate_file_budget`, Linux/Android `/proc/meminfo` probe, and cancellation-aware `copy_bounded_streaming` (`crates/file-core/src/limits.rs`).
- **Changed:** Wired `MemoryGovernor` into `JobContext`, `JobScheduler` dispatch/release lifecycle, and `JobManager::memory_stats()` (`crates/file-core/src/{lib.rs,pipeline.rs}`, `crates/job-manager/src/{lib.rs,scheduler.rs}`), backfilled `15651ec` (`stage-1.3.4` + `4aadd0a`) in `docs/PROGRESS.md`, and appended Entry 030 to `TODO.md`.

---

## [0.2.0-stage2.1.1] — 2026-11-28

### Subphase 2.1.1 — Compression Core & Quality Engine (Stage 2 Begin)
- **Added:** L1 `quality-engine` crate (`crates/quality-engine/src/lib.rs`: windowed SSIM, PSNR dB, `diff_pixel_buffers`, bounded `files_are_bit_exact`, `BitExactQualityEvaluator`) and L2 `compression-core` crate (`crates/compression-core/src/lib.rs`: `CompressionProfileConfig` across all 5 `FR-CMP-001` presets, `prescreen_incompressible_noop`, `finalize_staged_compression` No-Op Guard, atomic `commit_compression_stage`).
- **Changed:** Registered `BitExactQualityEvaluator` in `BackendCatalog::with_defaults()` (`crates/job-manager/{Cargo.toml,src/backends.rs}`), added Stage 2 queue & backfilled `9db02eb` (`stage-1.3.5`) in `docs/PROGRESS.md`, and appended Entry 031 to `TODO.md`.

---

## [0.2.1-stage2.1.2] — 2026-11-28

### Subphase 2.1.2 — Hardware Acceleration Probe (`L1`)
- **Added:** L1 `hardware-acceleration` crate (`crates/hardware-acceleration/src/lib.rs`: safe `SimdFeatures` ISA detection, GPU vendor probes for `NVENC`/`QSV`/`AMF`/`VideoToolbox`/`MediaCodec`/`VAAPI`, `IsolatedProcessRunner`-backed `probe_ffmpeg_hwaccels`, `select_video_backend` deterministic CPU fallback router, and WASM no-op stub).
- **Changed:** Wired `probe_hardware_acceleration` / `JobManager::probe_hardware` in `crates/job-manager/{Cargo.toml,src/lib.rs}`, backfilled `333a66f` (`stage-2.1.1`) in `docs/PROGRESS.md`, and appended Entry 032 to `TODO.md`.


---

## [0.2.2-stage2.2.1] — 2026-11-28

### Subphase 2.2.1 — Image Compression Crate (`compression-image`, `L3`)
- **Added:** L3 `compression-image` crate (`crates/compression-image/Cargo.toml`, `crates/compression-image/src/lib.rs`) supporting pure-Rust decode (`JPEG`, `PNG`, `WebP`, `GIF`, `BMP`, `TIFF`, `ICO`) and encode (`JPEG`, `oxipng` PNG, lossless `WebP`, `ravif` AVIF, `GIF`, `BMP`, `TIFF`, `ICO`), EXIF `0x0112` orientation detection/normalization (`detect_exif_orientation`, `apply_exif_orientation`), SIMD resizing via `fast_image_resize` (`ResizeFilter`, `ImageResizeSpec`), `RasterQualityEvaluator`, and `RasterCompressionEngine` with `FR-CMP-002` No-Op guard and memory reservations.
- **Changed:** Added `compression-image` and permissive raster codec dependencies (`fast_image_resize`, `image`, `oxipng`, `ravif`, `rgb`) to workspace `Cargo.toml` / `Cargo.lock`, backfilled `a702338` (`stage-2.1.2`) in `docs/PROGRESS.md`, and appended Entry 033 to `TODO.md`.

---

## [0.2.3-stage2.2.2] — 2026-11-28

### Subphase 2.2.2 — Archive Compression Crate (`compression-archive`, `L3`)
- **Added:** L3 `compression-archive` crate (`crates/compression-archive/{Cargo.toml,src/{lib.rs,streams.rs,container.rs,ffenc.rs,repair.rs}}`) implementing `StreamCodec` (`Zstd`, `Lz4`, `Brotli`, `Gzip`, `Xz`, `Bzip2`), `ArchiveContainerKind` create/list/extract (`Zip`, `Tar`, `TarGz`, `TarZst`, `TarXz`, `TarBz2`, `TarLz4`, `TarBrotli`, `SevenZ`) with bomb & Zip-Slip rollback protection, `.ffenc` (`AES-256-GCM`/`ChaCha20-Poly1305` + `Argon2id` + AAD), `repair_truncated_zip`, and `ArchiveCompressionEngine` / `ArchiveQualityEvaluator`.
- **Changed:** Added `compression-archive` and permissive archive/crypto crates (`aes-gcm`, `argon2`, `brotli`, `chacha20poly1305`, `crc32fast`, `flate2`, `getrandom`, `lz4_flex`, `lzma-rust2`, `oxiarc-bzip2`, `sevenz-rust`, `tar`, `zip`, `zstd`) to workspace `Cargo.toml` / `Cargo.lock`, backfilled `25d97e5` (`stage-2.2.1`) in `docs/PROGRESS.md`, and appended Entry 034 to `TODO.md`.

---

## [0.2.4-stage2.2.3] — 2026-11-28

### Subphase 2.2.3 — Audio Compression Crate (`compression-audio`, `L3`)
- **Added:** L3 `compression-audio` crate (`crates/compression-audio/{Cargo.toml,src/{lib.rs,dsp.rs,tags.rs,ffmpeg_adapter.rs}}`) providing pure-Rust decode (`WAV`/`FLAC`/`MP3`/`Ogg/Vorbis`/`AAC`/`M4A`/`ALAC`/`MKV`), lossless `WAV` & `FLAC` (`flacenc`) encode, `resample_pcm`, `remap_channels`, ITU-R BS.1770 / EBU R128 loudness measurement/normalization (`measure_integrated_lufs`, `normalize_loudness_lufs`), `ID3v2` read/write/strip, `AudioQualityEvaluator`, `AudioCompressionEngine` (`FR-CMP-002` No-Op guard), and `native-process` Tier 2 `FFmpeg` transcoding/stream-copy extraction via `IsolatedProcessRunner`.
- **Changed:** Added `compression-audio` and permissive audio dependencies (`claxon`, `flacenc`, `hound`, `id3`, `symphonia`) to workspace `Cargo.toml` / `Cargo.lock`, backfilled `ba2658d` (`stage-2.2.2`) in `docs/PROGRESS.md`, and appended Entry 035 to `TODO.md`.


---

## [0.2.5-stage2.2.4] — 2026-11-28

### Subphase 2.2.4 — Video Compression Crate (`compression-video`, `L3`)
- **Added:** L3 `compression-video` crate (`crates/compression-video/{Cargo.toml,src/{lib.rs,probe.rs,subtitles.rs,pipeline.rs}}`) providing `ffprobe` JSON stream inventory parsing (`parse_ffprobe_json_probe`), container fast-path remuxing (`can_fast_path_remux`), unfinalized atom diagnostics (`diagnose_unfinalized_video_container`), pure-Rust SRT <-> WebVTT subtitle conversion (`parse_srt_cues`, `parse_webvtt_cues`, `convert_subtitle_text`) + `FFmpeg` subtitle extraction args, hardware-vs-CPU routing (`resolve_video_route`), explicit `-f <muxer>` staged `FFmpeg` transcode invocation builder, `libvmaf` parser, and `VideoCompressionEngine` (`FR-CMP-002` No-Op guard & `VideoQualityEvaluator`).
- **Changed:** Added `compression-video` to workspace `Cargo.toml` / `Cargo.lock` with zero new third-party dependencies, backfilled `ee5957b` (`stage-2.2.3`) in `docs/PROGRESS.md`, and appended Entry 036 to `TODO.md`.


---

## [0.2.6-stage2.2.5] — 2026-11-28

### Subphase 2.2.5 — PDF & Office Compression Crates (`compression-pdf`, `compression-office`, `L3`)
- **Added:** L3 `compression-pdf` crate (`crates/compression-pdf/{Cargo.toml,src/{lib.rs,optimizer.rs,repair.rs}}`) and L3 `compression-office` crate (`crates/compression-office/{Cargo.toml,src/{lib.rs,container.rs,xml.rs}}`), plus `compression_image::optimize_raster_in_memory` for zero-temp-file embedded raster recompression inside PDF, OOXML, ODF, and EPUB containers.
- **Changed:** Added `compression-pdf`, `compression-office`, and MIT-licensed `lopdf` (`0.42`, `default-features = false`) to workspace `Cargo.toml` / `Cargo.lock`, backfilled `2660aed` (`stage-2.2.4`) in `docs/PROGRESS.md`, and appended Entry 037 to `TODO.md`.


---

## [0.2.7-stage2.3.1] — 2026-11-28

### Subphase 2.3.1 — Adaptive Optimization Engine & Quality Solver (`optimization-engine`, `L4`)
- **Added:** L4 `optimization-engine` crate (`crates/optimization-engine/{Cargo.toml,src/{lib.rs,recommender.rs,solver.rs,dedup.rs,dispatcher.rs}}`) delivering `recommend_codec` (`FR-ADP-001`..`002`), bounded binary quality solver (`solve_quantizer_binary_search`, `solve_jpeg_quality_target`, `solve_and_stage_jpeg`, `FR-CMP-003`), FastCDC sub-file chunk deduplication & `dHash`/2D-DCT `pHash` perceptual hashing (`FR-DUP-002`, `FR-DET-004`), and unified L3 engine `OptimizationDispatcher`.
- **Changed:** Added `optimization-engine` to workspace `Cargo.toml` / `Cargo.lock` with zero new external dependencies, backfilled `19c5a04` (`stage-2.2.5`) in `docs/PROGRESS.md`, and appended Entry 038 to `TODO.md`.


---

## [0.2.8-stage2.3.2] — 2026-11-28

### Subphase 2.3.2 — Stage 2 CLI (`compress`/`archive`) & Criterion Benchmarks (Stage 2 Completion)
- **Added:** Real `filefusion compress`, `filefusion archive {create,list,extract}`, and `filefusion bench` CLI subcommands (`apps/cli/src/main.rs`, E2E tests in `apps/cli/tests/cli_smoke.rs`), L5 `JobManager` compression/archive/benchmark orchestration (`crates/job-manager/src/lib.rs`), L4 archive path collection helpers (`crates/optimization-engine/src/dispatcher.rs`), and permanent Criterion benchmark suite (`tests/benchmarks/stage2_compression.rs`, `Criterion 0.6.0`).
- **Changed:** Recorded host CPU-only Stage 2 Criterion baselines in `PERFORMANCE.md`, wired `job-manager -> optimization-engine`, backfilled `c0bfe41` (`stage-2.3.1`) in `docs/PROGRESS.md`, and appended Entry 039 to `TODO.md`.


---

## [0.3.1-stage3.1.1] — 2026-11-28

### Subphase 3.1.1 — Conversion Core Foundation, Multi-Hop Router & Cross-Media Conversion (`conversion-core`, `L4`)
- **Added:** L4 `conversion-core` crate (`crates/conversion-core/{Cargo.toml,src/{lib.rs,graph.rs,preflight.rs}}`) delivering weighted Dijkstra multi-hop conversion route planning (`ConversionGraph`), pre-flight loss/encryption warnings (`evaluate_preflight`, `FR-FID-002`), markup structure inventory & fidelity scoring (`analyze_markup_structure`, `build_fidelity_report`, `FR-FID-001`), and `UniversalConversionEngine` (`ConversionEngine`) for pure-Rust cross-raster (`JPEG`/`PNG`/`WebP`/`AVIF`/`GIF`/`BMP`/`TIFF`/`ICO`), cross-archive (`Zip`/`Tar`/`Gzip`/`Zstd`/`Xz`/`Bzip2`/`Lz4`/`SevenZ`), cross-audio (`-> WAV`/`FLAC`), plus `native-process`-gated Tier 2 `FFmpeg` media conversion dispatch (`convert_with_optional_runner`).
- **Changed:** Registered `crates/conversion-core` in workspace `Cargo.toml` / `Cargo.lock` with zero new external dependencies, backfilled `8f69a4c` (`stage-2.3.2`) and queued Stage 3 subphases in `docs/PROGRESS.md`, and appended Entry 040 to `TODO.md`.


---

## [0.3.2-stage3.1.2] — 2026-11-28

### Subphase 3.1.2 — Bidirectional Structured Text, Markup & Tabular Engines (`FR-DOC-001`)
- **Added:** Pure-Rust structured markup engine (`crates/conversion-core/src/markup.rs`) and structured data/tabular engine (`crates/conversion-core/src/tabular.rs`) delivering bidirectional conversions across `Markdown`, `HTML`, `PlainText`, `CSV`/`TSV`, `JSON`, `YAML`, and `XML`, tabular-to-markup exports (`graph.rs`), and pre-flight warnings (`FID-MARKUP-PLAINTEXT`, `FID-CSV-FLATTEN`, `preflight.rs`) with active XSS/XXE security defenses and quantitative `ConversionFidelityReport` metrics.
- **Changed:** Backfilled `0160de1` (`stage-3.1.1`) in `docs/PROGRESS.md`, extended `UniversalConversionEngine` in `crates/conversion-core/src/lib.rs` with zero new external dependencies in `Cargo.lock`, and appended Entry 041 to `TODO.md`.


---

## [0.3.3-stage3.2.1] — 2026-11-28

### Subphase 3.2.1 — Native Office, ODF & EPUB Conversions + Tier 2 `LibreOffice`/`Pandoc`/`Calibre` Adapters (`FR-DOC-002`..`003`)
- **Added:** Pure-Rust Office/ODF/EPUB container converter (`crates/conversion-core/src/office_epub.rs`) supporting `XLSX`/`ODS` spreadsheets, `DOCX`/`ODT` documents, and `EPUB` 3 ebooks (`nav.xhtml` TOC + spine ordering + XML entity-bomb defense), sanitized `ExternalToolAdapter` subprocess wrappers for `LibreOffice`, `Pandoc`, and `Calibre` (`crates/conversion-core/src/tier2_adapters.rs`), and public container helpers (`read_validated_zip_entries`, `write_office_container_entries`) in `crates/compression-office`.
- **Changed:** Expanded `ConversionGraph` (`graph.rs`), formula/stripping warnings (`preflight.rs`), and `UniversalConversionEngine` (`lib.rs`) for Tier 1 Office/EPUB and Tier 2 `LibreOffice`/`Pandoc`/`Calibre` dispatch, backfilled `6ea5132` (`stage-3.1.2`) in `docs/PROGRESS.md`, and appended Entry 042 to `TODO.md`.


---

## [0.3.4-stage3.2.2] — 2026-11-28

### Subphase 3.2.2 — Stage 3 CLI (`filefusion convert`), `job-manager` Wiring & Conversion Fidelity Corpus (Stage 3 Completion)
- **Added:** Real `filefusion convert` CLI subcommand (`--to`, `--preflight-only`, `--force-loss`, `--strip-metadata`, `--overwrite`, `--json`) with full target-format coverage in `apps/cli/src/main.rs`, L5 `JobManager::preflight_conversion`, `JobManager::convert_file`, and `ConvertFileReport` in `crates/job-manager/src/lib.rs`, plus E2E Stage 3 conversion & fidelity corpus coverage in `apps/cli/tests/cli_smoke.rs`.
- **Changed:** Wired `job-manager -> conversion-core` across `native-process`, `native-logging`, and `wasm` features (`crates/job-manager/Cargo.toml`, `Cargo.lock`) with zero new external dependencies, backfilled `1c90d81` (`stage-3.2.1`) in `docs/PROGRESS.md`, and appended Entry 043 to `TODO.md`.


---

## [0.4.1-stage4.1.1] — 2026-11-28

### Subphase 4.1.1 — PDF Studio Structural Manipulation & Scan Detection (`compression-pdf`, `L3`)
- **Added:** Pure-Rust WASM-compatible PDF Studio suite (`crates/compression-pdf/src/studio.rs`) delivering multi-PDF `merge_pdfs`, `split_pdf_by_ranges`, `split_pdf_every_n_pages`, `extract_or_reorder_pdf_pages`, `rotate_pdf_pages`, `crop_pdf_pages`, rotated transparent vector text watermarking (`apply_pdf_text_watermark`), and `inspect_pdf_scan_status` (`FR-PDF-001`, `FR-PDF-003`..`004`), re-exported from `crates/compression-pdf/src/lib.rs`.
- **Changed:** Backfilled `186ecde` (`stage-3.2.2`) and queued Stage 4 subphases in `docs/PROGRESS.md`, and appended Entry 044 to `TODO.md` with zero changes to `Cargo.toml` / `Cargo.lock`.


---

## [0.4.1-stage4.1.2] — 2026-11-28

### Subphase 4.1.2 — Local OCR (`TesseractAdapter`), PDF Encryption & `/Sig` Verification (`compression-pdf`, `L3`)
- **Added:** Local OCR subsystem (`crates/compression-pdf/src/ocr.rs`) with `TesseractAdapter` (`ExternalToolAdapter` via `IsolatedProcessRunner`), `parse_tesseract_tsv`, `parse_tesseract_hocr`, `extract_pdf_page_images`, and invisible `3 Tr` searchable PDF overlay generation (`embed_searchable_ocr_text_layer`, `build_searchable_pdf_from_image_and_ocr`) (`FR-PDF-003`).
- **Added:** PDF cryptography & signature verification subsystem (`crates/compression-pdf/src/security.rs`) delivering pure-Rust `AES-256` (`V5`/`R6`) and `AES-128` (`V4`/`R4`) PDF protection, ISO-32000 `/P` permissions (`PdfPermissions`), decryption, security inspection, sanitized `qpdf` encrypt/decrypt command builders, and offline `/Sig` `/ByteRange` + ASN.1 DER verification (`verify_pdf_signatures`) (`FR-PDF-004`).
- **Changed:** Re-exported shared internal helpers cleanly in `crates/compression-pdf/src/studio.rs`, exported Subphase `4.1.2` APIs and tests in `crates/compression-pdf/src/lib.rs` with `Cargo.lock` untouched, backfilled `e5dbda3` onto `4.1.1` in `docs/PROGRESS.md`, and appended Entry 045 to `TODO.md`.


---

## [0.4.1-stage4.2.1] — 2026-11-28

### Subphase 4.2.1 — Unified File Repair, Metadata Sanitizer & `.ffenc` Orchestration (`L3`–`L5`)
- **Added:** L4 unified repair, metadata inspection/sanitization/editing, and `.ffenc`/PDF security orchestrators (`optimization-engine/src/{repair.rs,metadata.rs,security.rs}`) plus L5 `JobManager` Stage 4 orchestration (`job-manager/src/stage4.rs`: `diagnose_repair`, `repair_file`, `inspect_metadata`, `sanitize_and_edit_metadata`, `inspect_ffenc`, `encrypt_ffenc`, `decrypt_ffenc`, `encrypt_pdf`, `decrypt_pdf`) (`FR-REP-001`..`002`, `FR-MET-001`..`002`, `FR-SEC-002`, `FR-PDF-004`).
- **Added:** L3 primitives for pure-Rust `xref` PDF recovery & `/Info` updates (`compression-pdf`), ZIP & `.ffenc` header diagnostics (`compression-archive`), Office/ODF/EPUB metadata extraction & XML editing (`compression-office`), and `FFmpeg` container repair invocations (`compression-video`).
- **Changed:** Backfilled `84a9eeb` (`stage-4.1.2`) in `docs/PROGRESS.md`, expanded `optimization-engine` (6 tests) and `job-manager` (18 tests) suites (`116` native / `61` WASM passing), and appended Entry 046 to `TODO.md` with `Cargo.toml` and `Cargo.lock` untouched.



---

## [0.4.2-stage4.2.2] — 2026-11-28

### Subphase 4.2.2 — Stage 4 CLI (`filefusion pdf`, `repair`, `metadata`, `encrypt`/`decrypt`) & Stage 4 Security/Recovery Corpus (Stage 4 Completion)
- **Added:** Stage 4 CLI surface (`apps/cli/src/stage4_cmds.rs` wired via `apps/cli/src/main.rs`) delivering `filefusion pdf <merge|split|reorder|rotate|crop|watermark|scan-check|verify-sig|security|ocr|protect|unlock>`, `filefusion repair`, `filefusion metadata <inspect|scrub|edit>`, `filefusion encrypt`, and `filefusion decrypt` (`--passphrase` / `--passphrase-env`, `--json`, `--overwrite`).
- **Added:** L5 `JobManager` PDF Studio & OCR coordinator methods (`crates/job-manager/src/stage4.rs`), clean L4 PDF Studio/OCR re-exports (`crates/optimization-engine/src/lib.rs`), and 4 end-to-end Stage 4 CLI security & recovery corpus integration tests in `apps/cli/tests/cli_smoke.rs` (13 total `cli_smoke` tests, `120` total native workspace tests, `61` WASM-featured tests).
- **Changed:** Boxed heavy `Commands::{Repair, Encrypt, Decrypt}` variants and wrapped `apps/cli/src/main.rs` execution in an `8 MiB` `std::thread::Builder` stack to prevent Windows debug main-thread stack overflows; backfilled `4f3f737` (`stage-4.2.1`) in `docs/PROGRESS.md`, and appended Entry 047 to `TODO.md` with `Cargo.toml` and `Cargo.lock` untouched.



---

## [0.5.1-stage5.1.1] — 2026-11-28

### Subphase 5.1.1 — Multi-Stage Exact Duplicate Scanner, Streaming `FastCDC`, Perceptual Clustering & Storage Treemap (`optimization-engine`, `L4`)
- **Added:** Stage 5 storage intelligence module (`crates/optimization-engine/src/storage.rs`) delivering 3-stage exact duplicate detection (`scan_exact_duplicates`: size -> `4 KB` head/tail partial hash -> full streaming `BLAKE3` with I/O savings metrics, `FR-DUP-001`), perceptual near-duplicate image clustering (`scan_near_duplicate_images` across `aHash`/`dHash`/`pHash` with keeper selection, `FR-DUP-002`), and directory treemap / domain / format disk analytics (`analyze_storage_treemap`), re-exported from `crates/optimization-engine/src/lib.rs`.
- **Changed:** Upgraded `analyze_files_fastcdc` in `crates/optimization-engine/src/dedup.rs` to bounded sliding-window streaming via `chunk_stream_fastcdc` (`NFR-PERF-001`) alongside `compute_image_ahash` and `compute_perceptual_hash_set` (`FR-DET-004`); initialized Stage 5 queue (`5.1.1`..`5.2.2`) and backfilled `a4f9b9f` (`stage-4.2.2`) in `docs/PROGRESS.md`, expanded `optimization-engine` to 8 tests (`122` native / `63` WASM passing), and appended Entry 048 to `TODO.md` with `Cargo.toml` and `Cargo.lock` untouched.



---

## [0.5.1-stage5.1.2] — 2026-11-28

### Subphase 5.1.2 — Unified File Comparison/Diff & Safe Space-Reclamation Planner (`optimization-engine`, `L4`)
- **Added:** Unified file comparison/diff module (`crates/optimization-engine/src/diff.rs`: `compare_files`) combining streaming byte/`BLAKE3` equality, streaming `FastCDC` overlap, `SSIM`/`PSNR`/pixel summary & `aHash`/`dHash`/`pHash` distances via `quality-engine`, bounded LCS UTF-8 text line hunks, and metadata diffs (`FR-VAL-002`, `FR-DET-004`).
- **Added:** Safe space-reclamation planner & executor (`crates/optimization-engine/src/reclaim.rs`: `build_reclamation_plan`, `execute_reclamation_plan`, `restore_quarantine_batch`, `purge_quarantine_batch`) with TOCTOU `BLAKE3` verification, atomic same-volume hardlink swapping, reversible `.filefusion-quarantine/<batch>/manifest.json` quarantine/restore, and explicit consent-gated purge (`FR-DUP-003`, `DEF-009`), re-exported from `crates/optimization-engine/src/lib.rs`.
- **Changed:** Backfilled `8c034ee` (`stage-5.1.1`) in `docs/PROGRESS.md`, expanded `optimization-engine` to 10 tests (`124` native / `65` WASM passing), and appended Entry 049 to `TODO.md` with `Cargo.toml` and `Cargo.lock` untouched.



---

## [0.5.2-stage5.2.1] — 2026-11-28

### Subphase 5.2.1 — L5 Storage Intelligence & Safe Cleanup Orchestration (`job-manager`, `L5`)
- **Added:** L5 Stage 5 storage intelligence & cleanup orchestrator (`crates/job-manager/src/stage5.rs`: `JobCleanupPlanResult`, `JobCleanupExecutionResult`, and `JobManager` methods for exact/near duplicate scanning, streaming `FastCDC`, storage treemaps, unified file diff, dry-run/live reclamation, hardlink consolidation, and reversible quarantine restore/purge) (`FR-DUP-001`..`003`, `FR-VAL-002`).
- **Changed:** Re-exported Stage 5 storage/diff/reclamation types in `crates/job-manager/src/lib.rs`, added `job_manager_orchestrates_stage5_dedup_diff_treemap_and_quarantine` (`125` native / `66` WASM passing), backfilled `d573a48` (`stage-5.1.2`) in `docs/PROGRESS.md`, and appended Entry 050 to `TODO.md` with `Cargo.toml` and `Cargo.lock` untouched.



---

## [0.5.2-stage5.2.2] — 2026-11-28

### Subphase 5.2.2 — Stage 5 CLI (`filefusion dedup`, `diff`, `storage`) & Recovery/Quarantine/Hardlink Corpus (Stage 5 Completion)
- **Added:** Stage 5 CLI subcommands (`apps/cli/src/stage5_cmds.rs` wired via `apps/cli/src/main.rs`) delivering `filefusion dedup <exact|near|cdc|cleanup|restore|purge>`, `filefusion diff`, and `filefusion storage` with dry-run default cleanup safety, `--apply` / `--yes-permanent-delete` / `--confirm` gates, `--collision <skip|rename|overwrite>`, and JSON/human output (`FR-DUP-001`..`003`, `FR-DET-004`, `FR-VAL-002`).
- **Added:** 4 end-to-end Stage 5 CLI corpus tests in `apps/cli/tests/cli_smoke.rs` (bringing `cli_smoke` to 17 tests, `129` total native workspace tests, and `66` WASM-featured tests) covering 3-stage I/O pruning, near-duplicate image clustering, `FastCDC`, storage treemaps, quarantine restore collisions, purge consent gates, same-volume atomic hardlinks, and visual/LCS diffing.
- **Changed:** Backfilled `251711a` (`stage-5.2.1`) in `docs/PROGRESS.md` and appended Entry 051 to `TODO.md` with `Cargo.toml` and `Cargo.lock` untouched.



---

## [0.6.1-stage6.1.1] — 2026-11-28

### Subphase 6.1.1 — Batch Naming Templates, Directory Mirroring, Preset Store & Schedule Evaluator (`file-core`, `job-manager`)
- **Added:** Real `YYYY-MM-DD` UTC `{date}` formatting (`utc_date_from_unix_secs`) and relative directory mirroring (`resolve_mirrored_output_destination`) with out-of-root `SecTraversal` enforcement in `crates/file-core/src/{pipeline.rs,lib.rs}` (`FR-BAT-002`).
- **Added:** Reusable preset registry & local schedule evaluator (`crates/job-manager/src/presets.rs`) featuring 5 built-in `OptimizationPreset` presets, JSON/YAML round-trip helpers via `conversion-core::tabular`, atomic `PresetStore` / `ScheduleStore` persistence, and deterministic 5-field UTC cron + fixed-interval evaluation (`FR-LOC-002`).
- **Changed:** Re-exported Stage `6.1.1` primitives in `crates/job-manager/src/lib.rs`, added tests in `file-core` (29 passing) and `job-manager` (20 passing; `131` native / `67` WASM total), backfilled `cb73285` (`stage-5.2.2`) & initialized the Stage 6 queue in `docs/PROGRESS.md`, and appended Entry 052 to `TODO.md` with `Cargo.toml` and `Cargo.lock` untouched.
