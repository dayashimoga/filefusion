# FileFusion — Master Development Roadmap (Stages 0–12)

**Document ID:** `FF-ROAD-001`  
**Version:** `1.0.0`  
**Subphase Origin:** `0.3.5 — Create documentation foundation`  
**Tracking Ledger:** [docs/PROGRESS.md](PROGRESS.md) | **Scope Boundaries:** [docs/SCOPE_AND_MVP.md](SCOPE_AND_MVP.md)

---

## Overview

FileFusion executes sequentially across **13 stages (Stage 0 through Stage 12)** partitioned into phases and independently verifiable subphases.

| Stage | Name | Milestone Target | Primary Exit Criteria |
| :---: | :--- | :---: | :--- |
| **0** | **Project Foundation** | Foundation | Requirements, architecture, contracts, ADRs, GitHub repo, monorepo roots, Antigravity rules/skills, docs, and selectable CI/local builds operational |
| **1** | **Rust Core Foundation** | `MVP-v0.1` | Cargo workspace (`file-core`, `codec-registry`, `validation-engine`, `job-manager`), magic-byte inspection, Tokio/Rayon scheduler, `<= 500 ms` cancel, atomic staging, SQLite WAL, `>= 90%` core coverage |
| **2** | **Universal Compression Engine** | `MVP-v0.1` | Compression traits/profiles, No-Op Guard, Criterion benchmarks, Image (`JPEG`/`PNG`/`WebP`/`AVIF` + resize), Video (`FFmpeg` H.264/HEVC/VP9/AV1 + HW probe), Audio (`FLAC`/`WAV`/`Opus`/`AAC`/`MP3`), PDF/Office/EPUB/Archive (`ZIP`/`TAR`/`Zstd`/`Brotli`/`7z`), adaptive selector |
| **3** | **Universal Conversion Engine** | `MVP-v0.1` | Office/ODF/Markup/Data & Ebook (`EPUB`/`MOBI`/`AZW3`) conversion, media conversion, multi-hop graph, Pre-Flight warnings, and `ConversionFidelityReport` + production CLI (`filefusion`) |
| **4** | **PDF & Document Studio** | `v1.0-GA` | PDF merge/split/rotate/crop/watermark/encrypt/sign, local Tesseract OCR searchable PDF + table extraction, metadata scrub, structural repair & file diff |
| **5** | **File Intelligence & Storage** | `v1.0-GA` | Multi-stage BLAKE3 exact dedup, FastCDC chunking, perceptual image similarity (`pHash`/`dHash`), storage analytics, safe quarantine & hardlink cleanup |
| **6** | **Workflow Automation** | `v1.0-GA` | Visual/JSON/YAML DAG pipeline engine, presets, conditional batch rules, naming templates, scheduled local tasks, CLI workflow runner |
| **7** | **Flutter Applications** | `v1.0-GA` | `flutter_rust_bridge` v2, shared `packages/filefusion_ui` Light/Dark design system, 14 studios, separately verified on Windows, Linux, macOS, Android, iOS |
| **8** | **Web Application** | `v1.0-GA` | SvelteKit 5 + Svelte 5 + TypeScript (`WCAG 2.1 AA`), WASM Web Worker (`filefusion.worker.ts`), batch/diff UI, optional API offload |
| **9** | **AI Intelligence** | `v1.x+` | Strictly opt-in local ONNX/LLM + external AI adapters with mandatory Privacy & Cost Consent Gate |
| **10** | **Cloud & Developer Platform** | `v1.x+` | Optional self-hosted `axum` REST API (`/api/v1`), OpenAPI, API keys, rate limiting, PostgreSQL, Cloudflare Workers/R2 sync/offload |
| **11** | **Advanced Security & Quality** | `v1.0-GA` | Fuzz testing, malformed/bomb corpus, dependency & license audit (`cargo-deny`), memory/CPU benchmarks, cross-platform regression & a11y certification |
| **12** | **Release & Distribution** | `v1.0-GA` | Versioned platform builds, independent GitHub Release artifacts, SHA-256 checksums, SBOM, license notices, update mechanism, final certification |

---

## Stage 0 Subphase Breakdown (Active Stage)

- **Phase 0.1 Product Definition:** `0.1.1` Requirements -> `0.1.2` Users & Use Cases -> `0.1.3` Formats & Capabilities -> `0.1.4` MVP & Deferred Scope (`COMPLETE`)
- **Phase 0.2 Architecture:** `0.2.1` System Architecture -> `0.2.2` Module Boundaries -> `0.2.3` Rust & App Contracts -> `0.2.4` ADRs (`COMPLETE`)
- **Phase 0.3 Engineering Environment:** `0.3.1` GitHub Repo -> `0.3.2` Monorepo -> `0.3.3` Rules & Skills -> `0.3.4` Specialized Agents -> `0.3.5` Documentation Foundation (`ACTIVE`)
- **Phase 0.4 CI/CD:** `0.4.1` GitHub Actions -> `0.4.2` Selectable App Builds -> `0.4.3` Independent Artifact Uploads -> `0.4.4` Runner & Platform Matrix -> `0.4.5` Local Build Scripts (`UPCOMING`)
