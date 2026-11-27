# FileFusion — Architecture Decision Log (ADRs)

**Document ID:** `FF-ADR-001`  
**Version:** `1.0.0`  
**Status:** `APPROVED FOR STAGE 0 BASELINE`  
**Subphase Origin:** `0.2.4 — Create architecture decision records`  
**Parent Specifications:** [ARCHITECTURE.md](../ARCHITECTURE.md) (`FF-ARCH-001`), [docs/MODULE_BOUNDARIES.md](MODULE_BOUNDARIES.md) (`FF-MOD-001`), [docs/CONTRACTS.md](CONTRACTS.md) (`FF-CTR-001`)  
**Owners:** Agent 1 (Principal Architect), Agent 11 (Documentation Engineer)

---

## ADR Governance Policy

Material architectural choices, new core dependencies, and cross-plane changes must be recorded here as numbered Architecture Decision Records (`ADR-NNN`) before implementation (`FF-SCP-001` §5). Accepted ADRs remain immutable in history; future revisions supersede them via a new ADR referencing the prior ID.

---

### `ADR-001` — Rust-First Shared Core Across 16 Modular Crates (7 Acyclic Tiers `L0`–`L6`)
- **Status:** `ACCEPTED` (2026-11-27)
- **Context:** FileFusion targets 8 runtime surfaces (Windows, Linux, macOS, Android, iOS, Web, CLI, API) across 20 capability domains while guaranteeing high throughput, memory safety, and identical behavior on CPU-only machines.
- **Decision:** Implement all parsing, detection, compression, conversion orchestration, quality metrics, validation, and job scheduling in a Cargo workspace of **16 crates** arranged in **7 strict acyclic tiers (`L0`–`L6`)** (`FF-MOD-001`).
- **Alternatives Considered:**
  1. *Duplicating processing logic in Dart (Flutter) and TypeScript (Web):* Rejected due to 3x maintenance cost, behavioral drift, and poor CPU/memory performance.
  2. *Single monolithic Rust crate:* Rejected because it breaks `wasm32` compilation isolation, slows incremental compiles on 16 GB RAM hosts, and encourages circular coupling.
- **Consequences:** Enables fast incremental builds, `wasm32` subset compilation via feature flags (`native-process`, `sqlite-store`, `rayon-parallel`), and enforceable `MOD-RULE-01`..`06` boundaries.

---

### `ADR-002` — Two-Tier Engine & License Isolation Model (Pure-Rust Tier 1 vs. Out-of-Process Tier 2)
- **Status:** `ACCEPTED` (2026-11-27)
- **Context:** Mature open-source powerhouse engines (`FFmpeg`, `Ghostscript`, `MuPDF`, `LibreOffice`, `Tesseract`, `Calibre`, `7-Zip`) use diverse licenses (`GPL`, `LGPL`, `AGPL`, `MPL`, `Apache`) and heavy native toolchains that cannot always be statically linked into mobile App-Store or permissive Rust packages.
- **Decision:** Separate engines into **Tier 1 (`Y1`)** (permissively licensed pure-Rust crates statically compiled in, guaranteed 100% offline everywhere) and **Tier 2 (`Y2`)** (runtime-discovered external binaries executed out-of-process via `ExternalToolAdapter` + `IsolatedProcessRunner` with SPDX license tracking in `codec-registry`).
- **Alternatives Considered:**
  1. *Rewriting FFmpeg/LibreOffice/Tesseract from scratch in Rust:* Rejected per Global Engineering Principle #4 (unrealistic timeline and lower codec maturity).
  2. *Static FFI linking of GPL/AGPL C/C++ libraries into core crates:* Rejected due to license contamination across distribution channels and mobile binary bloat.
- **Consequences:** Clean license hygiene (`MOD-RULE-05`), transparent capability reporting (`filefusion doctor`), and graceful degradation or optional API offload when Tier 2 binaries are absent.

---

### `ADR-003` — Dual-Pool Concurrency (`tokio` + Bounded `rayon`) & Memory-Permit Accounting
- **Status:** `ACCEPTED` (2026-11-27)
- **Context:** Batch media/archive workloads can easily exhaust 16 GB RAM on Windows 11 development/user systems or freeze 3–6 GB mobile devices if unbounded thread pools run heavy codecs concurrently.
- **Decision:** Use `tokio` for async I/O, queues, SQLite writes, and subprocess orchestration, paired with a **bounded `rayon` CPU compute pool** (`max(1, min(logical_cpus - 1, available_ram_mb / per_worker_budget_mb))`) and memory-weighted job permits + `64 KB`–`1 MB` streaming ring buffers (`FF-ARCH-001` §4).
- **Alternatives Considered:** Unbounded `rayon::spawn` or whole-file `fs::read()` buffers; rejected because large video/archive batches would trigger OOM swaps.
- **Consequences:** Predictable memory ceilings (`NFR-PERF-001`), responsive UI during heavy batches, and deterministic `< 500 ms` cooperative cancellation.

---

### `ADR-004` — Flutter + `flutter_rust_bridge` v2 for Desktop & Mobile (`packages/filefusion_ui`)
- **Status:** `ACCEPTED` (2026-11-27)
- **Context:** FileFusion requires native desktop and mobile applications across Windows, Linux, macOS, Android, and iOS with a unified modern futuristic Light/Dark design system (`NFR-UX-001`).
- **Decision:** Build `apps/desktop` and `apps/mobile` with **Flutter/Dart** sharing a common UI/design-system package (`packages/filefusion_ui`) connected to Rust `L5`/`L6` via **`flutter_rust_bridge` v2** (`crates/flutter-bridge`), passing file paths/URIs rather than large raw byte buffers across FFI (`FF-CTR-001` §5.1).
- **Alternatives Considered:**
  1. *Electron / Tauri:* Electron bloats RAM/installer size; Tauri lacks mature Android/iOS parity compared to Flutter's single 5-platform renderer.
  2. *Five separate native UIs (WinUI, SwiftUI, Jetpack Compose, GTK):* Rejected due to 5x UI engineering and design-drift overhead.
- **Consequences:** Single shared Dart studio suite across 5 platforms with zero duplicated business logic (`MOD-RULE-03`, `MOD-RULE-06`).

---

### `ADR-005` — SvelteKit 5 + Svelte 5 + TypeScript + Web Workers (`wasm32`) for Web
- **Status:** `ACCEPTED` (2026-11-27)
- **Context:** Web users require zero-install browser access with fast load times, strict `WCAG 2.1 AA` DOM semantics/screen-reader accessibility, and local browser execution without locking the UI thread.
- **Decision:** Build `apps/web` using **SvelteKit + Svelte 5 + TypeScript**, executing Tier 1 Rust WASM modules inside a dedicated **Web Worker** (`filefusion.worker.ts`) via `Transferable` `ArrayBuffer` handoff (`<= 2 GB` tab heap guard), with optional self-hosted API offload for Tier 2 jobs.
- **Alternatives Considered:** Shipping Flutter Web CanvasKit as the primary web frontend; rejected due to multi-MB initial WASM canvas payload, weaker native DOM accessibility/SEO semantics, and lack of fine-grained SvelteKit SSR/Cloudflare Pages integration.
- **Consequences:** Sub-second initial web load, native HTML accessibility landmarks, and non-blocking client-side WASM processing.

---

### `ADR-006` — Local SQLite (WAL Mode) Persistence & Optional Stage 10 PostgreSQL / Cloudflare R2
- **Status:** `ACCEPTED` (2026-11-27)
- **Context:** FileFusion must work 100% offline locally (`FR-LOC-001`) while supporting optional multi-user hosted API execution and cloud sync in Stage 10 (`FR-CLD-001`).
- **Decision:** Embed **SQLite (WAL mode)** via `rusqlite` in `job-manager` for all local job history, presets, workflows, and benchmark telemetry (`FR-LOC-002`). Defer **PostgreSQL**, **Cloudflare Workers**, and **Cloudflare R2** adapters strictly to optional Stage 10 deployment (`apps/api` / `infrastructure/`).
- **Alternatives Considered:** Requiring a local PostgreSQL container or cloud database for desktop operation; rejected as violating local-first simplicity and resource budgets.
- **Consequences:** Zero external daemon dependencies for offline users; clean forward-only schema migrations (`schema_migrations`).

---

### `ADR-007` — Atomic Same-Filesystem Staging, Mandatory `OutputValidator` & No-Op Guard
- **Status:** `ACCEPTED` (2026-11-27)
- **Context:** Interruptions, encoder bugs, or already-compressed inputs can otherwise corrupt source files, leave half-written outputs, or produce larger "optimized" files.
- **Decision:** All write pipelines target an isolated staging file (`.filefusion.tmp.<uuid>`), pass `OutputValidator` syntactic/decoder verification (`FR-VAL-001`), enforce the **No-Op Guard** (`CompressionStatus::NoOpRetainedOriginal` if same-format `output >= input`), and atomically rename only on full validation (`NFR-REL-001`).
- **Alternatives Considered:** In-place file modification or trusting encoder exit codes without re-probing output; rejected as unsafe.
- **Consequences:** Zero source corruption on crash/cancel (`<= 500 ms` cleanup) and verifiable size/quality guarantees on every output.

---

### `ADR-008` — Untrusted-Input Security Gate & Zero-Shell-Injection `IsolatedProcessRunner`
- **Status:** `ACCEPTED` (2026-11-27)
- **Context:** File processors parse untrusted third-party binaries exposed to path traversal (`../`), zip bombs, billion-laughs SVG/XML expansion, extension spoofing, and shell metacharacter injection.
- **Decision:** Enforce magic-byte probing (`FR-DET-001`), configurable `ResourceLimits` (`100:1` ratio, max depth `3`), path/symlink canonicalization guards in `validation-engine`, and an `IsolatedProcessRunner` trait (`file-core` -> `job-manager`) invoking Tier 2 tools strictly via `Vec<OsString>` argument arrays with private temp dirs and process-group termination (`FR-SEC-001`, `MOD-RULE-04`).
- **Alternatives Considered:** Relying on shell wrapper commands (`sh -c` / `cmd /c`) or extension-based dispatch; rejected as critical security vulnerabilities.
- **Consequences:** Defense-in-depth across every intake path, verifiable via the Stage 11 malformed/bomb corpus.

---

### `ADR-009` — Strictly Opt-In, Privacy-Gated AI Intelligence (Stage 9)
- **Status:** `ACCEPTED` (2026-11-27)
- **Context:** Users handle confidential legal, medical, and personal files (`USR-01`), and core conversion/compression must never rely on paid external APIs.
- **Decision:** Core Stages 1–8 function 100% with AI completely disabled. Stage 9 AI features support local inference first and require an explicit per-action/policy **Privacy & Cost Consent Gate** (`FR-AI-002`) before transmitting any snippet to a user-configured external provider.
- **Alternatives Considered:** Background telemetry or automatic cloud document uploading for "smart" naming; rejected as a severe privacy violation.
- **Consequences:** Air-gapped and privacy-regulated users retain full trust while power users can opt into AI workflows transparently.

---

### `ADR-010` — Selectable GitHub Actions Native Matrix & Local Podman (< 30 GB) Strategy
- **Status:** `ACCEPTED` (2026-11-27)
- **Context:** Building all 8 targets (including Linux AppImage, macOS `.app`, Android APK/AAB, and iOS archives) concurrently on a 16 GB RAM Windows 11 host is impossible for Apple targets and risks local disk/RAM exhaustion.
- **Decision:** Implement a centralized selectable `.github/workflows/build-apps.yml` using native GitHub-hosted runners per OS with per-target independent artifact uploads, paired with `scripts/build.ps1` / `scripts/build.sh` for local host-supported builds and **Podman** disposable service containers strictly capped under a **30 GB total storage budget** (`NFR-OPS-001`).
- **Alternatives Considered:** Heavy local multi-VM emulators or bundling all platforms into a single monolithic release archive; rejected per Sections 9 and 10 of the Engineering Contract.
- **Consequences:** Fast targeted CI builds, clean independent artifacts, and preserved host responsiveness on Windows 11.
