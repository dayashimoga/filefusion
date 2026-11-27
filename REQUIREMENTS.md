# FileFusion — Product Requirements Document (PRD)

**Document ID:** `FF-REQ-001`  
**Version:** `1.0.0`  
**Status:** `APPROVED FOR STAGE 0 BASELINE`  
**Subphase Origin:** `0.1.1 — Finalize Product Requirements` (linked with [docs/USE_CASES.md](docs/USE_CASES.md) in `0.1.2` and [docs/FORMATS_AND_CAPABILITIES.md](docs/FORMATS_AND_CAPABILITIES.md) in `0.1.3`)  
**Owner:** Principal Architect (Agent 1) & Documentation Engineer (Agent 11)

---

## 1. Executive Summary & Product Vision

**FileFusion** is a universal file intelligence, conversion, compression, optimization, repair, automation, and management platform built on a **local-first, Rust-powered processing foundation**. It provides deterministic, high-throughput, privacy-preserving file operations across Desktop (Windows, Linux, macOS), Mobile (Android, iOS/iPadOS), Web (SvelteKit 5), CLI, and REST API / SDK targets.

Every capability in FileFusion is governed by a strict **no-fake-functionality engineering contract**: every button, CLI flag, and API route executes real algorithmic work with measurable quality, integrity verification, cancellation support, and actionable diagnostics.

---

## 2. Core Product Principles

1. **Production Implementation Only:** No mock processing, simulated progress bars, decorative UI controls, or hardcoded outputs.
2. **Local-First & Offline by Default:** All core inspection, conversion, compression, repair, deduplication, and workflow execution run locally on the user's device without mandatory network connectivity or cloud accounts.
3. **Rust-First Core Architecture:** All core file intelligence, format orchestration, codec pipelines, job scheduling, resource limiting, and verification engines are implemented in memory-safe Rust workspaces and exposed to Flutter (`flutter_rust_bridge`), CLI, API, and Web/WASM (where feasible).
4. **Zero Silent Fidelity or Data Loss:** Every transformation estimates or measures output fidelity before/after execution, reports unsupported format features explicitly, and never overwrites source files destructively without explicit user opt-in and atomic write verification.
5. **Hardware-Aware, Low-Resource Friendly:** Runs smoothly on CPU-only machines (including our 16 GB RAM Windows 11 reference baseline and mobile devices) with bounded memory pools, streaming I/O, and optional GPU/hardware encoder acceleration.
6. **License & Redistribution Compliance:** Core libraries prefer permissive licenses (MIT, Apache-2.0, BSD, Zlib,MPL-2.0). Copyleft or external powerhouse engines (FFmpeg, Ghostscript, LibreOffice, MuPDF, 7-Zip) are isolated behind well-defined plugin/adapter traits with explicit SBOM and license compliance tracking (`LICENSES.md`).

---

## 3. Target Platforms & Runtime Matrix

| Target ID | Platform | Technology Stack | Execution Model | Minimum Resource Target |
| :--- | :--- | :--- | :--- | :--- |
| `PLAT-WIN` | Windows 10/11 (x64) | Flutter Desktop + Rust FFI (`flutter_rust_bridge`) | Native local execution + optional external tool discovery | 4 GB RAM minimum (optimized for 16 GB dev/user systems), CPU-only baseline |
| `PLAT-LIN` | Linux (x64, Ubuntu 22.04+ / glibc) | Flutter Desktop + Rust FFI | Standalone binary / AppImage + system codec integration | 4 GB RAM, CPU-only baseline |
| `PLAT-MAC` | macOS 13+ (ARM64 / Apple Silicon & x64) | Flutter Desktop + Rust FFI | `.app` bundle + VideoToolbox hardware option | 4 GB RAM |
| `PLAT-AND` | Android 10+ (API 29+, ARM64/x64) | Flutter Mobile + Rust JNI/NDK FFI | Scoped storage compliant, foreground service for batch jobs | 3 GB RAM device, battery-aware throttling |
| `PLAT-IOS` | iOS / iPadOS 16+ (ARM64) | Flutter Mobile + Rust Static Lib FFI | App sandbox compliant, background task completion hooks | 3 GB RAM device |
| `PLAT-WEB` | Responsive Web (Modern Browsers) | SvelteKit + Svelte 5 + TypeScript | Browser-local WebAssembly / Web Workers where feasible + optional API backend | 2 GB browser heap budget |
| `PLAT-CLI` | Cross-Platform CLI (`filefusion`) | Native Rust binary (`clap`) | Direct library linkage to `filefusion` core crates | < 50 MB idle RSS, streaming I/O |
| `PLAT-API` | Self-Hosted / Cloud API Server | Native Rust HTTP service (`axum` / `tokio`) + Cloudflare Workers/R2 adapters | Stateless or SQLite/PostgreSQL-backed job service | < 128 MB idle container RSS |

---

## 4. Functional Requirements Catalog (20 Capability Domains)

Every requirement below carries a unique identifier (`FR-<DOMAIN>-<NNN>`), priority (`P0` = Core Foundation/MVP, `P1` = Full Production Release, `P2` = Advanced/Optional Extension), and verification method.

### 4.1 Universal File Detection & Inspection (`FR-DET`)
- **`FR-DET-001` [P0] Magic-Byte & Structural Signature Detection:** Identify file formats using multi-offset magic bytes, container structure probing (e.g., ZIP-based OOXML/EPUB/APK vs raw ZIP, RIFF sub-formats, ISO-BMFF/MP4 brands), and text/encoding heuristics rather than file extensions alone.
- **`FR-DET-002` [P0] Extension Mismatch & Spoofing Alert:** Detect and flag files whose extension contradicts their true binary signature (e.g., an executable or HTML file renamed to `.pdf` or `.jpg`).
- **`FR-DET-003` [P0] Deep Container & Stream Inventory:** Inspect internal streams and sub-components (video/audio/subtitle tracks, PDF object counts & embedded fonts/images, archive member trees, image color spaces/bit depth/ICC profiles).
- **`FR-DET-004` [P0] Cryptographic & Perceptual Digests:** Compute BLAKE3, SHA-256, and xxHash64 digests via streaming I/O, plus perceptual hashes (pHash/dHash) for visual media.

### 4.2 Document, PDF, Office & Ebook Conversion (`FR-DOC`)
- **`FR-DOC-001` [P0] Structured Text & Markup Conversion:** Convert bidirectionally across Markdown, HTML, Plain Text, CSV/TSV, JSON, and YAML with deterministic schema/encoding preservation.
- **`FR-DOC-002` [P0] Office & OpenDocument Conversion:** Convert between DOCX, XLSX, PPTX, ODT, ODS, ODP, CSV, and PDF via native Rust OOXML/ODF parsers where high-fidelity direct mapping exists, and headless LibreOffice/Pandoc adapters for complex layout rendering.
- **`FR-DOC-003` [P1] Ebook Format Conversion:** Convert between EPUB (EPUB 2/3), Markdown/HTML, PDF, and MOBI/AZW3 (via native Rust EPUB packaging and optional Calibre `ebook-convert` integration), preserving TOC, spine order, and cover art.

### 4.3 Image Conversion, Resizing & Optimization (`FR-IMG`)
- **`FR-IMG-001` [P0] Core Raster Format Support:** Decode, transcode, and encode JPEG, PNG, WebP, AVIF, GIF, BMP, TIFF, and ICO using memory-safe Rust crates (`image`, `ravif`, `oxipng`, `webp`) and optional native turbo backends (`libjpeg-turbo`, `libvips`).
- **`FR-IMG-002` [P0] High-Speed SIMD Resizing & Color Management:** Perform Lanczos3, Catmull-Rom, Mitchell, and Bilinear resizing via `fast_image_resize`, preserving or converting ICC color profiles, orientation (EXIF auto-orient), and alpha channels.
- **`FR-IMG-003` [P0] Lossless & Perceptually Lossy Optimization:** Strip non-essential metadata on demand, optimize PNG deflate/filter streams (`oxipng`), recompress JPEG/WebP/AVIF to target SSIM/PSNR thresholds or byte-size budgets.

### 4.4 Audio & Video Conversion & Compression (`FR-AV`)
- **`FR-AV-001` [P0] Video Transcoding & Container Remuxing:** Transcode and remux MP4, MKV, WebM, MOV, and AVI across H.264, HEVC (H.265), VP9, and AV1 (`rav1e` / `SVT-AV1` / FFmpeg pipelines) with lossless fast-path remuxing when codec change is unnecessary.
- **`FR-AV-002` [P0] Audio Transcoding & Normalization:** Convert between FLAC, WAV, ALAC (lossless) and Opus, AAC, MP3, Ogg Vorbis (lossy) with sample-rate conversion, channel mapping, and EBU R128 loudness normalization.
- **`FR-AV-003` [P1] Subtitle & Track Extraction:** Extract, mux, or convert embedded subtitle tracks (SRT, WebVTT, ASS/SSA) and extract audio streams from video containers without re-encoding when compatible.

### 4.5 Archive Operations (`FR-ARC`)
- **`FR-ARC-001` [P0] Multi-Format Archive Creation & Extraction:** Create, inspect, test, and extract ZIP, TAR, TAR.GZ, TAR.ZST, TAR.XZ, TAR.BZ2, and 7Z archives.
- **`FR-ARC-002` [P0] Modern High-Ratio Stream Compressors:** Support standalone and tar-wrapped Zstandard (`zstd`), LZ4 (`lz4_flex`), Brotli (`brotli`), Deflate/Gzip (`flate2`), and XZ/LZMA.
- **`FR-ARC-003` [P0] Archive Re-packing & Optimization:** Transcode legacy inefficient archives (e.g., store/deflate ZIP or `.tar.gz`) into high-density Zstandard or 7z archives with bit-exact payload verification.

### 4.6 Intelligent Lossless & Lossy Compression (`FR-CMP`)
- **`FR-CMP-001` [P0] Standard Compression Profiles:** Provide well-defined, reproducible presets (`Lossless Maximum`, `Balanced Optimal`, `Web/Sharing Ready`, `Extreme Size Target`, `Archival Cold Storage`) across all media, document, and archive formats.
- **`FR-CMP-002` [P0] Automatic No-Op & Inflation Guard:** Never emit an "optimized" file that is larger than the input (unless format conversion was explicitly requested); automatically detect already-compressed payloads and return a verified no-op recommendation.
- **`FR-CMP-003` [P1] Quality-Constrained Target Solver:** Binary-search or model encoder parameters (CRF/quantizer) to meet a minimum quality floor (e.g., SSIM >= 0.98 or VMAF >= 93) or maximum target file size.

### 4.7 Adaptive Codec & Algorithm Selection (`FR-ADP`)
- **`FR-ADP-001` [P0] Content-Aware Codec Recommender:** Analyze file entropy, resolution, transparency, frame rate, color depth, and target platform constraints to recommend the optimal codec and preset before processing.
- **`FR-ADP-002` [P1] Hardware vs. Software Backend Routing:** Probe available CPU SIMD features (AVX2, AVX-512, NEON) and optional GPU/hardware encoders (NVENC, QSV, AMF, VideoToolbox, MediaCodec), falling back transparently to deterministic CPU encoders when hardware acceleration is absent or lower quality per bit.

### 4.8 PDF Manipulation & OCR (`FR-PDF`)
- **`FR-PDF-001` [P0] Structural PDF Manipulation:** Merge, split by page ranges/bookmarks, reorder, rotate, crop, extract pages, and linearize (fast web view) PDFs.
- **`FR-PDF-002` [P1] PDF Stream & Asset Compression:** Downsample/recompress oversized embedded images inside PDFs, subset/deduplicate fonts, and compress unencoded PDF object streams.
- **`FR-PDF-003` [P1] Local OCR & Searchable PDF Generation:** Detect scanned/image-only PDFs and images, execute OCR via Tesseract / native OCR engines, embed invisible searchable text layers (`hOCR`/PDF text overlay), and export extracted text/tables.
- **`FR-PDF-004` [P1] Watermarking, Encryption & Signing:** Apply text/image watermarks, configure AES-256 PDF password protection and permission flags, and verify digital signatures.

### 4.9 Document Structure & Formatting Preservation (`FR-FID`)
- **`FR-FID-001` [P0] Structural Fidelity Analyzer:** Inspect headings, tables, lists, hyperlinks, embedded images, and page count before and after conversion to generate a quantitative **Conversion Fidelity Report**.
- **`FR-FID-002` [P0] Unsupported Feature Pre-Flight Warnings:** Warn the user prior to conversion when the target format cannot represent source features (e.g., animations in PPTX -> PDF, formulas in XLSX -> CSV, macros in XLSM -> XLSX).

### 4.10 Duplicate Detection & Storage Optimization (`FR-DUP`)
- **`FR-DUP-001` [P0] Multi-Stage Exact Duplicate Scanner:** Scan directory trees using size-bucketing -> partial head/tail hashing (4 KB) -> full BLAKE3 cryptographic hash to identify exact duplicates with minimal disk I/O.
- **`FR-DUP-002` [P1] Content-Defined Chunking (CDC) & Near-Duplicate Detection:** Use FastCDC rolling hashes to estimate sub-file deduplication savings and perceptual hashing (pHash/dHash/aHash) to find resized or re-encoded duplicate images.
- **`FR-DUP-003` [P0] Safe Cleanup & Quarantine Workflow:** Generate actionable space-reclamation plans with hardlink/reflink replacement or staged quarantine before permanent deletion.

### 4.11 File Repair & Recovery (`FR-REP`)
- **`FR-REP-001` [P1] Container & Cross-Reference Repair:** Diagnose and repair truncated ZIP central directories, broken PDF `xref` tables/trailers (via native parser & `qpdf` recovery), and unfinalized MP4/MKV container indexes where media payloads remain intact.
- **`FR-REP-002` [P0] Honest Recoverability Assessment:** Provide an accurate diagnostic report classifying damaged files as `Fully Repairable`, `Partially Salvageable (with loss summary)`, or `Unrecoverable (missing payload entropy)`.

### 4.12 Metadata Inspection & Editing (`FR-MET`)
- **`FR-MET-001` [P0] Unified Metadata Inspector:** Read and display EXIF, IPTC, XMP, ID3v2/Vorbis comments, MP4 atoms, PDF Info/XMP dictionaries, and Office core/app properties in a normalized schema.
- **`FR-MET-002` [P0] Privacy Metadata Sanitizer & Editor:** Edit mutable metadata fields or perform one-click **Privacy Scrubbing** (stripping GPS coordinates, camera serial numbers, author names, revision histories, and hidden comments) while keeping essential color profiles and display orientation intact.

### 4.13 Batch Conversion & Processing (`FR-BAT`)
- **`FR-BAT-001` [P0] Bounded-Concurrency Batch Queue:** Process hundreds or thousands of heterogeneous files using a resource-aware Tokio/Rayon job scheduler with configurable worker limits, priority levels, pause/resume, and per-item cancellation.
- **`FR-BAT-002` [P0] Output Naming Templates & Directory Mirroring:** Support deterministic output path templates (`{stem}`, `{ext}`, `{date}`, `{hash8}`, `{profile}`), relative folder structure preservation, and collision-handling policies (`Skip`, `Rename`, `Overwrite with Backup`).

### 4.14 Visual Workflow Automation (`FR-WFL`)
- **`FR-WFL-001` [P1] Declarative DAG Pipeline Engine:** Execute multi-step directed acyclic graphs (DAGs) serialized as portable JSON/YAML workflow definitions (e.g., *Watch Folder -> Filter by MIME/Size -> Strip EXIF -> Compress Image to WebP -> Pack into `.tar.zst`*).
- **`FR-WFL-002` [P1] Visual Workflow Builder UI:** Provide an interactive node/step editor in the Desktop, Mobile, and Web applications connected directly to the Rust workflow validation and execution engine.

### 4.15 AI-Powered Document & File Intelligence (`FR-AI`)
- **`FR-AI-001` [P2] Local-First & Strictly Opt-In AI Processing:** Support local rule/heuristic and local ONNX/LLM model inference for document summarization, smart filename suggestion, table extraction, and workflow generation.
- **`FR-AI-002` [P2] Explicit External Provider Consent Gate:** If a user configures an external AI API key, require explicit per-action or per-policy opt-in with token/cost estimation and zero background telemetry or silent uploads. Core FileFusion features must function 100% with AI disabled.

### 4.16 Local-First / Offline Processing (`FR-LOC`)
- **`FR-LOC-001` [P0] Zero-Network Core Guarantee:** Verify via automated tests that all Stage 1–7 core libraries, CLI commands, and desktop/mobile studio operations succeed with network interfaces completely disabled.
- **`FR-LOC-002` [P0] Local SQLite Persistence:** Store job history, presets, audit logs, benchmark results, and workflow templates in an encrypted-capable local SQLite database with automatic schema migrations.

### 4.17 Optional Cloud Processing & Synchronization (`FR-CLD`)
- **`FR-CLD-001` [P2] Self-Hostable & Cloudflare-Compatible Sync/Offload:** Support optional user-configured cloud storage (Cloudflare R2 / S3-compatible) and remote job execution for heavy workloads from low-power mobile/web clients.

### 4.18 CLI, SDK & Developer API (`FR-DEV`)
- **`FR-DEV-001` [P0] Production CLI (`filefusion`):** Provide scriptable subcommands (`inspect`, `convert`, `compress`, `pdf`, `archive`, `dedup`, `repair`, `metadata`, `workflow`, `bench`, `doctor`) with human-readable TUI progress and `--json` machine-readable output.
- **`FR-DEV-002` [P1] Rust SDK & REST API:** Expose clean Rust library crates (`file-core`, `conversion-core`, `compression-core`, etc.) and an OpenAPI-documented HTTP REST API with API-key authentication and rate limiting.

### 4.19 Secure File Handling & Encryption (`FR-SEC`)
- **`FR-SEC-001` [P0] Untrusted Input Sandboxing & Bomb Protection:** Enforce configurable maximum decompression ratios, maximum archive nesting depth, path traversal rejection (`..`, absolute paths, symlink escapes), memory allocation ceilings, and execution timeouts on every parser.
- **`FR-SEC-002` [P1] File & Archive Encryption:** Support authenticated encryption (AES-256-GCM / ChaCha20-Poly1305 with Argon2id key derivation) for standalone encrypted containers and password-protected archives/PDFs.

### 4.20 File Comparison, Validation & Quality Reporting (`FR-VAL`)
- **`FR-VAL-001` [P0] Post-Processing Integrity & Quality Gate:** Automatically validate that every generated output file is syntactically well-formed, readable by its target decoder, and accompanied by a structured report containing input/output sizes, compression ratio, elapsed time, cryptographic hashes, and quality metrics (SSIM, PSNR, VMAF, or lossless bit-exact confirmation).
- **`FR-VAL-002` [P1] Visual & Structural Diff Engine:** Compare two files (before vs. after, or version A vs. version B) across binary hashes, metadata diffs, document text/structure diffs, and image pixel difference maps.

---

## 5. Non-Functional Requirements (NFRs)

| NFR ID | Category | Requirement Specification | Verification Method |
| :--- | :--- | :--- | :--- |
| **`NFR-PERF-001`** | Memory Ceiling | Core CLI and library operations on a single file must default to streaming or bounded memory (< 512 MB RSS for typical workloads; configurable up to 25% of system RAM, max 4 GB on a 16 GB host). | Automated memory profiling & resource-limit unit tests |
| **`NFR-PERF-002`** | Startup Latency | CLI cold start (`filefusion --version` / `inspect`) < 50 ms; Desktop/Mobile app interactive shell launch < 800 ms on reference hardware. | Criterion / startup benchmark harness |
| **`NFR-PERF-003`** | Concurrency Control | Bounded worker pools (defaulting to `min(num_cpus, memory_budget / per_job_estimate)`) so batch jobs never freeze the host OS or exhaust 16 GB RAM. | Stress test with 1,000+ queued jobs |
| **`NFR-REL-001`** | Atomic File Safety | Source files must never be corrupted or truncated if a job fails, is cancelled, or power is lost. All outputs write to safe temporary files in the target filesystem before atomic rename. | Cancellation & fault-injection integration tests |
| **`NFR-REL-002`** | Cancellation Latency | User cancellation of an active job must abort running threads/subprocesses and clean up temporary files within 500 ms. | Cancellation responsiveness tests |
| **`NFR-SEC-001`** | Malware & Bomb Resilience | 100% rejection of zip-slip path traversal, symlink escapes, billion-laughs XML/SVG expansion, and zip bombs exceeding configured compression ratio/size thresholds. | Dedicated `tests/security/` corpus |
| **`NFR-QA-001`** | Test Coverage | >= 90% meaningful line/branch coverage across core Rust processing crates; >= 95% coverage for critical business logic (scheduler, safety guards, no-op guards, fidelity calculators); 100% CI pass rate. | `cargo tarpaulin` / `cargo-llvm-cov` in CI |
| **`NFR-UX-001`** | Accessibility & Themes | WCAG 2.1 AA contrast compliance, full keyboard navigation & command palette (`Ctrl+K` / `Cmd+K`), screen-reader labels, and synchronized Light/Dark futuristic themes across Flutter and SvelteKit. | Automated a11y checks (`svelte-check`, Flutter semantics tests) |
| **`NFR-OPS-001`** | Container Storage Budget | Local Podman development/testing containers and caches must never exceed 30 GB total disk footprint. | Environment audit script verification |

---

## 6. Traceability Matrix (Roadmap Stages to Requirements)

| Roadmap Stage | Primary Requirements Satisfied | Verification Gate |
| :--- | :--- | :--- |
| **Stage 0: Project Foundation** | Governance, Architecture, Monorepo, CI/CD (`NFR-QA-001`, `NFR-OPS-001`) | All Stage 0 docs, rules, skills, and selectable CI workflows verified |
| **Stage 1: Rust Core Foundation** | `FR-DET-001..004`, `FR-BAT-001`, `FR-LOC-001..002`, `FR-SEC-001`, `NFR-PERF-001..003`, `NFR-REL-001..002` | Core Rust unit & integration tests >= 90% coverage |
| **Stage 2: Universal Compression Engine** | `FR-IMG-001..003`, `FR-AV-001..002`, `FR-ARC-001..003`, `FR-CMP-001..003`, `FR-ADP-001..002`, `FR-VAL-001` | Benchmark suite + SSIM/PSNR/bit-exact verification tests |
| **Stage 3: Universal Conversion Engine** | `FR-DOC-001..003`, `FR-AV-003`, `FR-FID-001..002` | Round-trip and cross-format fidelity test corpus |
| **Stage 4: PDF & Document Studio** | `FR-PDF-001..004`, `FR-MET-001..002`, `FR-REP-001..002`, `FR-VAL-002` | PDF manipulation, OCR accuracy, and repair test fixtures |
| **Stage 5: File Intelligence & Storage** | `FR-DUP-001..003`, `FR-DET-004` | Deduplication accuracy, CDC chunking, and safe quarantine tests |
| **Stage 6: Workflow Automation** | `FR-WFL-001`, `FR-BAT-002`, `FR-DEV-001` | Multi-step DAG workflow execution & CLI automation tests |
| **Stage 7: Flutter Applications** | `PLAT-WIN`, `PLAT-LIN`, `PLAT-MAC`, `PLAT-AND`, `PLAT-IOS`, `FR-WFL-002`, `NFR-UX-001` | Native builds + widget/integration tests per platform |
| **Stage 8: Web Application** | `PLAT-WEB`, `NFR-UX-001` | SvelteKit build + Playwright/Vitest E2E tests |
| **Stage 9: AI Intelligence** | `FR-AI-001..002` | Offline AI fallback + consent-gate security verification |
| **Stage 10: Cloud & Developer Platform** | `PLAT-API`, `FR-CLD-001`, `FR-DEV-002` | REST API integration tests, OpenAPI validation, SDK examples |
| **Stage 11: Advanced Security & Quality** | `FR-SEC-002`, `NFR-SEC-001`, `NFR-QA-001` | Fuzzing, malformed corpus, full license & security audit |
| **Stage 12: Release & Distribution** | All `FR-*` and `NFR-*` | Independent platform artifacts, checksums, SBOM, release certification |

---

## 7. Key Assumptions, Dependencies & Risk Mitigations

1. **External Native Engines vs. Pure-Rust Core:**
   - *Constraint:* Not all world-class codecs (e.g., full FFmpeg video codecs, LibreOffice complex DOCX/PPTX layout, Ghostscript/qpdf, Tesseract OCR) are currently installed on the development machine (`h:\ufi`), nor can GPL binaries be statically linked into commercial/App-Store bundles without license contamination.
   - *Architectural Mitigation:* FileFusion uses a **Two-Tier Engine Architecture**:
     - **Tier 1 (Built-in Pure-Rust / Permissively Licensed Core):** Always compiled in and guaranteed available offline across all platforms (magic-byte inspection, BLAKE3/SHA-256, image processing via `image`/`fast_image_resize`/`oxipng`/`ravif`/`webp`, archives via `zip`/`tar`/`zstd`/`lz4_flex`/`brotli`/`flate2`, PDF manipulation via pure-Rust PDF crates, OOXML/ODF/EPUB/Markdown/HTML/CSV/JSON native parsers, metadata scrubbing, deduplication, and encryption).
     - **Tier 2 (Pluggable Native Engine Adapters):** Runtime-probed adapters for `ffmpeg`, `qpdf`, `tesseract`, `libreoffice`/`soffice`, `pandoc`, `calibre`, and `7z` with clear capability reporting (`filefusion doctor`) and automated provisioning in CI/container builds.
2. **Host Memory & Disk Protection (16 GB RAM Windows 11 Host):**
   - Cargo parallel jobs, Rayon thread pools, and test runners are capped to prevent swapping or OOM. Container usage (Podman) is strictly budgeted under 30 GB.
3. **GitHub CLI (`gh`) Availability:**
   - Because `gh` is not currently installed on the host PATH, Subphase `0.3.1` will either use Git + GitHub REST API / credential manager or install `gh` portably after confirming repository visibility (`public` vs `private`) with the user.
