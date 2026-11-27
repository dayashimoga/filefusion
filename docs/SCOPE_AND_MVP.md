# FileFusion — MVP Boundary, Release Milestones & Deferred-Features Register

**Document ID:** `FF-SCP-001`  
**Version:** `1.0.0`  
**Status:** `APPROVED FOR STAGE 0 BASELINE`  
**Subphase Origin:** `0.1.4 — Define MVP and deferred features`  
**Parent Specifications:** [REQUIREMENTS.md](../REQUIREMENTS.md) (`FF-REQ-001`), [docs/USE_CASES.md](USE_CASES.md) (`FF-UC-001`), [docs/FORMATS_AND_CAPABILITIES.md](FORMATS_AND_CAPABILITIES.md) (`FF-FMT-001`)  
**Owners:** Agent 1 (Principal Architect), Agent 11 (Documentation Engineer)

---

## 1. Purpose & Scope Control Policy

This document closes **Phase 0.1 (Product Definition)** by establishing:
1. Exact deliverable boundaries for **Milestone A (Core MVP)**, **Milestone B (v1.0 GA Cross-Platform Studio)**, and **Milestone C (Optional AI & Cloud Extensions)**.
2. An explicit, auditable **Deferred-Features & Non-Goals Register (`DEF-*`)** to prevent scope creep, premature optimization, or unapproved architectural expansion.
3. The mandatory **Scope Admission Gate** enforced by the Principal Architect (Agent 1) and Independent Reviewer (Agent 12) on every subphase.

> **No-Fake-Functionality Rule:** Deferring a feature to Milestone B or C means it is **not exposed as a decorative stub or non-functional UI button** in earlier milestones. Every exposed command, API, or UI control in any milestone must be 100% genuine, integrated, and tested.

---

## 2. Release Milestone Boundaries

### 2.1 Milestone A — Core Engine & CLI MVP (`MVP-v0.1`, Stages 1–3)

Delivers a genuinely working, benchmarked, offline-first Rust processing core and production `filefusion` CLI binary before GUI/cloud layers are built on top.

| Capability Area | Included in `MVP-v0.1` (Strict Scope) | Verification Target |
| :--- | :--- | :--- |
| **Stage 1: Rust Core Foundation** | Workspace crates, structured error/tracing diagnostics, local SQLite job/config store, magic-byte & container detection (`FR-DET-001..004`), capability registry (`doctor`), bounded Tokio/Rayon scheduler, 500 ms cancellation, atomic temp-write safety, archive bomb & path-traversal guards (`FR-SEC-001`). | `>= 90%` core / `>= 95%` critical logic test coverage (`NFR-QA-001`) |
| **Stage 2: Universal Compression Engine** | Compression trait contracts, 5 reproducible profiles (`FR-CMP-001`), automatic No-Op / inflation guard (`FR-CMP-002`), Criterion benchmark harness, Image optimization (`JPEG`, `PNG`, `WebP`, `AVIF` + `fast_image_resize` + EXIF/ICC handling + SSIM/PSNR verification), Archive engine (`ZIP`, `TAR`, `Zstd`, `LZ4`, `Brotli`, `Gzip`, `XZ`, `7z`), Tier 2 FFmpeg video (`H.264`, `HEVC`, `VP9`, `AV1` + fast-path remux) & audio (`FLAC`, `WAV`, `Opus`, `AAC`, `MP3`), adaptive codec recommender (`FR-ADP-001`), quality-constrained solver (`FR-CMP-003`). | Empirical Criterion benchmarks + SSIM/PSNR/bit-exact tests |
| **Stage 3: Universal Conversion Engine** | Bidirectional Markup/Data (`Markdown`, `HTML`, `CSV`, `TSV`, `JSON`, `YAML`), native `EPUB 2/3` & basic `DOCX`/`XLSX`/`ODF`, Tier 2 `LibreOffice`/`Pandoc`/`Calibre` adapters, multi-hop conversion graph (`direct-first` + lossless intermediate hub), Pre-Flight Unsupported Feature warnings (`FR-FID-002`), post-conversion Fidelity Report (`FR-FID-001`). | Round-trip & conversion fidelity test corpus |
| **CLI Surface (`MVP-v0.1`)** | `filefusion inspect`, `compress`, `convert`, `archive`, `bench`, `doctor` with real-time progress and `--json` output (`FR-DEV-001`). | End-to-end CLI integration tests on real fixtures |

---

### 2.2 Milestone B — Full Cross-Platform Studio (`v1.0-GA`, Stages 4–8, 11–12)

Builds the remaining local studios, Flutter Desktop/Mobile apps, SvelteKit Web app, security hardening, and full multi-platform release automation atop the proven Rust core.

| Stage | Included in `v1.0-GA` | Key Deliverables |
| :--- | :--- | :--- |
| **Stage 4: PDF & Document Studio** | Merge, split, page management, rotate, crop, watermarks, AES-256 encryption, signature verification, local Tesseract OCR -> searchable PDF & table extraction, metadata inspection & one-click privacy scrubbing, structural file repair (`PDF`, `ZIP`, media containers) with honest recoverability reporting, file comparison/diff (`FR-VAL-002`). |
| **Stage 5: File Intelligence & Storage** | Multi-stage exact duplicate scanner (size -> 4 KB head/tail -> BLAKE3), FastCDC sub-file chunking analysis, perceptual image similarity (`pHash`/`dHash`), storage treemap analytics, safe quarantine & same-volume hardlink cleanup. |
| **Stage 6: Workflow Automation** | Declarative JSON/YAML DAG pipeline engine, conditional routing, reusable presets, scheduled local tasks, batch naming templates & directory mirroring (`FR-BAT-002`), workflow CLI runner. |
| **Stage 7: Flutter Applications** | `flutter_rust_bridge` bindings, futuristic Light/Dark Design System (`NFR-UX-001`), all 14 navigation views, drag-and-drop intake (`UXJ-01`), `Ctrl+K` Command Palette (`UXJ-02`), progressive disclosure (`UXJ-03`), Visual Workflow Builder (`FR-WFL-002`), verified separately on **Windows, Linux, macOS, Android, and iOS**. |
| **Stage 8: Web Application** | SvelteKit 5 + TypeScript responsive accessible web app (`WCAG 2.1 AA`), browser-local WASM processing for supported Tier 1 operations, batch queue & visual diff viewer. |
| **Stage 11–12: Security, QA & Release** | Fuzzing (`cargo-fuzz`/proptest), malformed/bomb corpus, dependency & license audit (`LICENSES.md`), SBOM generation, cross-platform regression & fidelity certification, GitHub Release automation with per-target artifacts and SHA-256 checksums. |

---

### 2.3 Milestone C — Optional AI & Cloud Developer Platform (`v1.x+`, Stages 9–10)

Strictly optional extensions layered cleanly without adding mandatory cloud dependencies, telemetry, or paid API requirements to `v1.0-GA`:

| Stage | Included in `v1.x+` | Strict Guardrails |
| :--- | :--- | :--- |
| **Stage 9: AI Intelligence** | Opt-in local ONNX/LLM runtime + explicit external AI provider adapters for document summarization, complex table extraction, translation, smart naming, and prompt-to-workflow generation (`FR-AI-001..002`). | Disabled by default; mandatory per-action Privacy & Cost Consent Gate before any external payload leaves the device. |
| **Stage 10: Cloud & Developer Platform** | Optional user-configured self-hosted Rust REST API (`axum` + OpenAPI + API keys + rate limiting), PostgreSQL server metadata, Cloudflare Workers/R2 optional sync & heavy-job offload for low-power mobile/web devices (`FR-CLD-001`, `FR-DEV-002`). | Core local apps must continue working 100% when cloud endpoints are unconfigured or offline. |

---

## 3. Requirement Priority Alignment (`FR-*` -> Milestones)

| Priority Tier in `REQUIREMENTS.md` | Target Milestone | Count & Scope Summary |
| :--- | :--- | :--- |
| **`P0` (Foundation & Core)** | `MVP-v0.1` (Core/CLI) + `v1.0-GA` (PDF/Dup/GUI) | Mandatory baseline requirements (`FR-DET`, `FR-DOC-001..002`, `FR-IMG`, `FR-AV-001..002`, `FR-ARC`, `FR-CMP-001..002`, `FR-ADP-001`, `FR-PDF-001`, `FR-FID`, `FR-DUP-001/003`, `FR-REP-002`, `FR-MET`, `FR-BAT`, `FR-LOC`, `FR-DEV-001`, `FR-SEC-001`, `FR-VAL-001`). |
| **`P1` (Full Production Studio)** | `MVP-v0.1` (Quality solver/HW/Rust crates) + `v1.0-GA` (Stages 4–8) + `v1.x+` (Stage 10 REST API) | Advanced optimization, Ebook, Subtitles, OCR, Signing, Repair, FastCDC, Visual Workflows, `.ffenc` encryption, Visual Diff (`FR-CMP-003`, `FR-ADP-002`, `FR-DOC-003`, `FR-AV-003`, `FR-PDF-002..004`, `FR-DUP-002`, `FR-REP-001`, `FR-WFL`, `FR-SEC-002`, `FR-VAL-002`, and `FR-DEV-002` Rust SDK in Stages 1–3 + REST API in Stage 10). |
| **`P2` (Optional Extensions)** | `v1.x+` (Stages 9–10) | Opt-in AI Intelligence (`FR-AI-001..002`) and Optional Cloud Sync/Offload (`FR-CLD-001`). |

---

## 4. Deferred-Features & Explicit Non-Goals Register (`DEF-*`)

Features below are **explicitly deferred or excluded** to protect architecture clarity, resource budgets (16 GB RAM / 30 GB container limit), and delivery velocity:

| ID | Candidate / Deferred Feature | Classification | Reason & Re-Evaluation Gate |
| :--- | :--- | :--- | :--- |
| **`DEF-001`** | OS Shell Context-Menu Extensions (Windows Explorer / Finder / Nautilus) & Always-On Background Folder Watcher Daemon | **Deferred (Post-`v1.0`)** | Native shell extensions require per-OS COM/FinderSync plugins and background memory residency. Stage 6 delivers scheduled/on-demand DAG workflows first; shell integration is evaluated after `v1.0-GA`. |
| **`DEF-002`** | DRM Removal / Circumvention (Adobe ADEPT, Kindle KFX DRM, FairPlay, Widevine) | **Permanently Out of Scope** | Violates legal compliance and security principles. DRM-encrypted inputs are cleanly detected and rejected with an actionable message (`FF-FMT-001` §6.1). |
| **`DEF-003`** | Mandatory Cloud Accounts, Subscriptions, License Servers, or Telemetry | **Permanently Out of Scope** | Prohibited by Global Engineering Principles ("Local-first", "No unnecessary subscriptions"). |
| **`DEF-004`** | Camera RAW Debayering (`CR2`, `NEF`, `ARW`, `RAF`, `DNG`) & Color-Grading LUT Studio | **Deferred (Post-`v1.0`)** | Complex sensor-specific demosaicing (`libraw`/darktable scope) adds heavy binary footprint and diverges from core raster/document/archive mission. Re-evaluate for `v1.2+` as an optional Tier 2 adapter. |
| **`DEF-005`** | Proprietary Legacy Reverse-Engineering (CorelDRAW `.cdr`, MS Publisher `.pub`, Apple Pages/Keynote `.pages`/`.key`) | **Deferred / Adapter-Only** | Closed undocumented formats lack reliable open specs. Only supported if Tier 2 LibreOffice import filters handle them cleanly. |
| **`DEF-006`** | Multi-Track Nonlinear Video/Audio Timeline Editor (NLE) or Real-Time Collaborative Word Processor | **Permanently Out of Scope** | FileFusion is a file intelligence, transformation, compression, repair, and automation platform—not a creative NLE or Google Docs clone. |
| **`DEF-007`** | Training or Fine-Tuning AI Models on User Documents | **Permanently Out of Scope** | Violates local-first privacy guarantees. Stage 9 covers strictly opt-in inference only. |
| **`DEF-008`** | Peer-to-Peer / Distributed Swarm Transcoding Cluster | **Deferred (Post-Stage 10)** | Single-node bounded local processing + optional single API offload (`FR-CLD-001`) solves practical user needs without distributed consensus complexity. |
| **`DEF-009`** | Kernel-Level Filesystem Deduplication Driver / Custom Block Storage | **Permanently Out of Scope** | High OS instability/security risk; Stage 5 uses safe user-space hardlinks (`std::fs::hard_link`) and reversible quarantine directories. |
| **`DEF-010`** | Legacy 32-bit OS Targets (`x86` Windows/Linux, Windows 7/8, Android < API 29, iOS < 16) | **Excluded** | Toolchain (`rustc`, `flutter 3.38+`), memory alignment, and security sandboxing baselines target modern 64-bit platforms (`FF-REQ-001` §3). |
| **`DEF-011`** | Full Local Multi-GB LLM Weight Bundling inside Base Installer | **Excluded from Base Bundle** | Violates lightweight installer and 16 GB RAM / mobile device constraints. Optional AI models in Stage 9 are user-downloaded or user-configured separately. |
| **`DEF-012`** | Real-Time Antivirus Signature Scanning / Malware Disinfection | **Permanently Out of Scope** | FileFusion performs magic-byte spoofing detection and parser sandboxing (`FR-SEC-001`), not OS antivirus signature scanning. |

---

## 5. Mandatory Scope Admission & Course-Correction Gate

Before any agent introduces a new feature, crate, or dependency not already specified in `FF-REQ-001` / `FF-FMT-001` / `FF-SCP-001`, it **MUST** pass all 5 checks (Section 12 of Engineering Contract):
1. **Product Vision Fit:** Does it directly serve file intelligence, transformation, compression, repair, or automation?
2. **Active Subphase Alignment:** Does it belong to the *currently active subphase*, or must it be logged in `TODO.md` / `DEF-*` for its designated stage?
3. **Architecture & Dependency Impact:** Does it respect Tier 1 vs. Tier 2 license isolation and bounded memory limits?
4. **Measurable Benefit vs. Cost:** Does a benchmark or user need justify the binary size, maintenance burden, and test surface?
5. **ADR & User Approval:** Material architectural changes require an Architecture Decision Record (`docs/DECISION_LOG.md`) and explicit user approval.
