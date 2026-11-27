# FileFusion — Target Users, Personas & Use Cases

**Document ID:** `FF-UC-001`  
**Version:** `1.0.0`  
**Status:** `APPROVED FOR STAGE 0 BASELINE`  
**Subphase Origin:** `0.1.2 — Define target users and use cases`  
**Parent Specification:** [REQUIREMENTS.md](../REQUIREMENTS.md) (`FF-REQ-001`)  
**Owners:** Agent 1 (Principal Architect), Agent 7 (UI/UX Designer), Agent 11 (Documentation Engineer)

---

## 1. Purpose & Scope

This document defines **who uses FileFusion**, **why they use it**, **how they interact with its 14 core workspace views, CLI, and API**, and **how every user journey maps deterministically to functional (`FR-*`) and non-functional (`NFR-*`) requirements** in [REQUIREMENTS.md](../REQUIREMENTS.md).

In accordance with FileFusion's **Global Engineering Contract**, no use case relies on simulated results, silent quality loss, or mandatory cloud uploads. Every UI interaction uses **progressive disclosure** (fast, sensible defaults upfront; deep codec/pipeline parameters available on demand) and ends in verifiable output integrity.

---

## 2. Target User Personas

| Persona ID | Persona Name & Role | Primary Platforms | Resource & Environment Profile | Core Pain Points | Primary FileFusion Value |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **`USR-01`** | **Privacy-First Knowledge Worker** (Legal, Healthcare, Finance, Research) | Windows, macOS, Linux Desktop | Corporate or air-gapped laptop (8–16 GB RAM), strict NDA/HIPAA/GDPR data policies, no cloud uploads allowed | Online PDF/DOCX converters upload confidential contracts to third-party servers; hidden EXIF/author metadata leaks sensitive details; scanned PDFs aren't searchable | 100% offline processing (`FR-LOC-001`), PDF Studio + local OCR (`FR-PDF-001..003`), one-click Privacy Scrubbing (`FR-MET-002`), Conversion Fidelity Reports (`FR-FID-001`) |
| **`USR-02`** | **Creative Media Producer & Web Publisher** (Designer, Content Creator, Frontend Dev) | Desktop (Win/macOS/Linux) + Responsive Web | 16 GB RAM workstation, CPU-only or optional GPU, large batches of RAW/PNG/JPEG/MP4/WAV assets | Blindly compressing images/video ruins visual fidelity or inflates file sizes; juggling separate apps for WebP/AVIF, FFmpeg, and audio normalization | Adaptive codec recommendation (`FR-ADP-001`), SSIM/PSNR/VMAF quality-constrained optimization (`FR-CMP-003`, `FR-VAL-001`), automatic no-op guard (`FR-CMP-002`), Image/Video/Audio Studios |
| **`USR-03`** | **Software Developer & DevOps / Automation Engineer** | CLI (`filefusion`), REST API, Rust SDK, Linux/Windows | Headless CI runners, build scripts, Podman/Docker containers, SSH sessions | Inconsistent OS tools (`zip`, `ffmpeg`, `magick`), lack of structured JSON inspection output, fragile shell scripts for asset pipelines | Single scriptable `filefusion` CLI with `--json` (`FR-DEV-001`), portable DAG workflow specs (`FR-WFL-001`), Zstd/Brotli/7Z archive repacking (`FR-ARC-001..003`), Rust/HTTP API (`FR-DEV-002`) |
| **`USR-04`** | **Storage & Archive Custodian / IT Administrator** | Windows/Linux Desktop + CLI | Multi-TB local NAS/external drives on 16 GB RAM workstation; legacy mixed archives & damaged backups | Duplicate files and near-duplicate photos waste disk space; truncated ZIPs or broken PDFs from failed transfers appear lost; suspicious renamed files pose security risks | Multi-stage BLAKE3 duplicate scanner (`FR-DUP-001`), FastCDC & pHash analysis (`FR-DUP-002`), Safe Quarantine (`FR-DUP-003`), File Repair (`FR-REP-001..002`), Spoofing Alert (`FR-DET-002`) |
| **`USR-05`** | **Mobile & Field Professional** (Consultant, Engineer, Student) | Android, iOS / iPadOS | Phone/tablet (3–6 GB RAM), intermittent connectivity, battery sensitivity, scoped mobile storage | Mobile share-sheets reject large PDFs/videos; scanning receipts creates bloated unsearchable PDFs; desktop-only tools aren't available on site | Battery/memory-aware Rust FFI mobile app (`PLAT-AND`, `PLAT-IOS`), quick batch photo/video shrink (`FR-BAT-001`), local PDF merge/sign (`FR-PDF-001..004`), optional R2 offload (`FR-CLD-001`) |
| **`USR-06`** | **Browser-First & Accessibility User** | Responsive Web (SvelteKit 5) | Chromebook, shared kiosk, or locked-down browser (2 GB tab heap); keyboard-only or screen-reader navigation | Cannot install native binaries; complex media tools lack keyboard shortcuts, contrast, or clear error messages | Zero-install SvelteKit web app with browser-local WASM processing (`PLAT-WEB`), WCAG 2.1 AA compliance (`NFR-UX-001`), `Ctrl+K` Command Palette, clear guided presets |

---

## 3. Core Navigation & Studio Mapping

FileFusion organizes its 20 capability domains into **14 intuitive navigation destinations** in the GUI (Desktop, Mobile, Web), mirrored by top-level subcommands in the CLI:

| Nav # | Navigation Destination | Primary Purpose | Primary Personas | Linked Use Cases |
| :---: | :--- | :--- | :--- | :--- |
| **1** | **Home / Workspace** | Universal drop-zone, magic-byte auto-inspection, smart recommended actions, quick stats | All (`USR-01`..`USR-06`) | `UC-001`, `UXJ-01`, `UXJ-02` |
| **2** | **Convert** | Cross-format Document, Office, Ebook, Markup, and Media conversion with Fidelity Pre-Flight | `USR-01`, `USR-02`, `USR-05`, `USR-06` | `UC-002`, `UC-003` |
| **3** | **Compress & Optimize** | Adaptive codec selection, quality-constrained optimization, no-op protection, batch shrink | `USR-02`, `USR-05`, `USR-06` | `UC-004`, `UC-005` |
| **4** | **PDF Studio** | Merge, split, reorder, rotate, compress, watermark, password-protect, sign PDFs | `USR-01`, `USR-05` | `UC-006` |
| **5** | **Image Studio** | Raster transcoding (AVIF/WebP/PNG/JPEG), SIMD resize, ICC/alpha handling, visual pixel diff | `USR-02`, `USR-06` | `UC-004`, `UC-015` |
| **6** | **Video Studio** | H.264/HEVC/VP9/AV1 transcoding, fast-path remux, bitrate/VMAF targeting, HW/CPU routing | `USR-02`, `USR-05` | `UC-005` |
| **7** | **Audio Studio** | FLAC/Opus/AAC/MP3 transcoding, stream extraction from video, EBU R128 loudness normalization | `USR-02`, `USR-03` | `UC-007` |
| **8** | **Archive Manager** | Browse, create, extract, test, and re-pack ZIP, TAR, Zstd, Brotli, LZ4, 7Z with bomb protection | `USR-03`, `USR-04` | `UC-008` |
| **9** | **OCR & Extraction** | Local Tesseract OCR to searchable PDF, plain text, and structured table extraction | `USR-01`, `USR-05` | `UC-009` |
| **10** | **File Intelligence** | Deep hex/container inspection, spoofing detection, metadata privacy scrub, duplicates & storage map | `USR-01`, `USR-04` | `UC-001`, `UC-010`, `UC-011` |
| **11** | **File Repair** | Recoverability assessment and structural repair for damaged ZIP, PDF, and media containers | `USR-04`, `USR-01` | `UC-012` |
| **12** | **Automation Workflows** | Visual DAG builder, reusable presets, conditional batch rules, JSON/YAML export, optional AI assist | `USR-03`, `USR-02`, `USR-04` | `UC-013`, `UC-014` |
| **13** | **History & Jobs** | Real-time bounded batch queue, cancellation, SQLite job history, output verification & diff reports | All (`USR-01`..`USR-06`) | `UC-015`, `UXJ-04` |
| **14** | **Settings** | Theme (Light/Dark), CPU/RAM limits, Tier 2 engine diagnostics (`doctor`), privacy/AI consent, cloud sync | All (`USR-01`..`USR-06`) | `UC-016` |

---

## 4. Detailed End-to-End Use Cases

### `UC-001` — Universal File Inspection & Spoofing Detection
- **Primary Persona:** `USR-04` (Custodian), `USR-01` (Knowledge Worker)
- **Navigation / Interface:** `Home / Workspace`, `File Intelligence`, or `filefusion inspect <path> [--json]`
- **Requirements Covered:** `FR-DET-001`, `FR-DET-002`, `FR-DET-003`, `FR-DET-004`, `FR-ADP-001`, `FR-SEC-001`
- **Preconditions:** User has one or more local files (including suspicious, unknown-extension, or complex container files).
- **Main Flow:**
  1. User drops files onto **Home / Workspace** (or runs `filefusion inspect`).
  2. Tier 1 Rust engine reads bounded header/footer signatures, probes ZIP/RIFF/ISO-BMFF/PDF containers, and computes streaming BLAKE3/SHA-256 hashes without executing embedded scripts.
  3. UI displays true MIME/format, container stream inventory (e.g., codecs, color depth, embedded fonts, archive members), and format-aware optimization recommendations.
  4. If the filename extension disagrees with the true binary signature (e.g., an HTML script named `invoice.pdf`), FileFusion raises a prominent **Extension Mismatch / Spoofing Warning**.
- **Alternative / Error Flows:**
  - *Unreadable / zero-byte / locked file:* Actionable OS permission or empty-file diagnostic shown without crashing.
  - *Oversized / recursive container:* Bomb protection aborts deep scan at configured recursion/entry limits (`NFR-SEC-001`).
- **Postconditions:** Normalized inspection report rendered in UI/CLI and optionally cached in local SQLite (`FR-LOC-002`).

---

### `UC-002` — High-Fidelity Office & Document Conversion with Pre-Flight Warnings
- **Primary Persona:** `USR-01` (Knowledge Worker)
- **Navigation / Interface:** `Convert`, or `filefusion convert <input> --to pdf|docx|md|html`
- **Requirements Covered:** `FR-DOC-001`, `FR-DOC-002`, `FR-FID-001`, `FR-FID-002`, `FR-VAL-001`, `NFR-REL-001`
- **Preconditions:** Source document exists (e.g., `.docx`, `.xlsx`, `.pptx`, `.odt`, `.md`, `.html`); target format selected.
- **Main Flow:**
  1. User selects source documents and chooses target output format (e.g., `DOCX -> PDF` or `XLSX -> CSV`).
  2. FileFusion runs **Unsupported Feature Pre-Flight Analysis**: e.g., warns that `XLSX -> CSV` drops multi-sheet tabs and formulas, preserving values only.
  3. User confirms conversion; FileFusion routes via Tier 1 native parser (for Markdown/HTML/CSV/JSON/basic OOXML) or Tier 2 LibreOffice/Pandoc adapter (for complex print layouts).
  4. Output is written to a staging temp file, validated by target format parser, compared structurally against source (headings, tables, links, page count), and atomically moved to final path.
  5. User receives the converted file alongside a **Conversion Fidelity Report** (`FR-FID-001`).
- **Alternative / Error Flows:**
  - *Tier 2 engine missing for complex layout:* Clear prompt explaining that native Tier 1 handles structured text while LibreOffice is needed for full visual pagination; links to `Settings -> Engine Diagnostics`.
  - *User cancels mid-batch:* Active child process terminates cleanly within 500 ms (`NFR-REL-002`); staging files removed; original files untouched.
- **Postconditions:** Verified output file and recorded Fidelity Report in `History & Jobs`.

---

### `UC-003` — Ebook Conversion & Metadata Preservation
- **Primary Persona:** `USR-01`, `USR-06`
- **Navigation / Interface:** `Convert`, or `filefusion convert book.md --to epub`
- **Requirements Covered:** `FR-DOC-003`, `FR-FID-001`, `FR-MET-001`, `FR-VAL-001`
- **Preconditions:** Source Markdown, HTML, DOCX, PDF, EPUB, or MOBI/AZW3 file.
- **Main Flow:**
  1. User selects ebook source and target (`EPUB`, `PDF`, `HTML`, or `MOBI/AZW3` where Calibre adapter is available).
  2. FileFusion displays/edits title, author, language, ISBN, and cover image before conversion.
  3. Conversion engine builds valid EPUB 3 OPF spine/manifest and navigation TOC (or delegates legacy `.mobi`/`.azw3` to Tier 2 Calibre).
  4. Post-conversion validator verifies zip container structure, XHTML well-formedness, and TOC anchor integrity.
- **Alternative / Error Flows:**
  - *Broken internal links or missing cover:* Reported in Fidelity Report with exact resource path warnings rather than silently failing.
- **Postconditions:** Validated ebook ready for e-readers.

---

### `UC-004` — Quality-Constrained Batch Image Optimization & Resizing
- **Primary Persona:** `USR-02` (Creative Media Producer), `USR-05` (Mobile User)
- **Navigation / Interface:** `Compress & Optimize` or `Image Studio`, or `filefusion compress ./photos --profile web-ready`
- **Requirements Covered:** `FR-IMG-001`, `FR-IMG-002`, `FR-IMG-003`, `FR-CMP-001..003`, `FR-BAT-001..002`, `FR-VAL-001`
- **Preconditions:** Directory or batch of JPEG, PNG, WebP, AVIF, TIFF, or GIF images.
- **Main Flow:**
  1. **Basic Mode (Progressive Disclosure):** User picks preset (`Web/Sharing Ready` or `Lossless Maximum`) and optional max dimension (e.g., `1920px`).
  2. **Advanced Mode (Optional):** User sets target `SSIM >= 0.98`, output format (`AVIF` / `WebP` / keep original), Lanczos3 resampling, and EXIF retention policy.
  3. Adaptive selector recommends optimal codec (e.g., `oxipng`/lossless WebP for flat UI graphics with alpha; `AVIF`/`WebP`/`JPEG` for photography).
  4. Bounded Rayon worker pool processes images in parallel, auto-applying EXIF orientation and preserving ICC profiles.
  5. Quality solver verifies SSIM/PSNR; **No-Op Guard** automatically skips re-encoding if output would exceed original size on same-format compression.
- **Alternative / Error Flows:**
  - *Corrupt image header:* Single item marked failed with exact decode error; remaining batch items continue unaffected.
- **Postconditions:** Optimized images saved per naming template (`{stem}_opt.{ext}`), batch savings & SSIM summary logged.

---

### `UC-005` — Adaptive Video Compression & Fast-Path Remuxing
- **Primary Persona:** `USR-02`, `USR-05`
- **Navigation / Interface:** `Video Studio` or `Compress & Optimize`, or `filefusion compress clip.mov --to mp4`
- **Requirements Covered:** `FR-AV-001`, `FR-ADP-001`, `FR-ADP-002`, `FR-CMP-001..003`, `FR-VAL-001`, `NFR-PERF-001`
- **Preconditions:** Video file (MP4, MKV, MOV, WebM, AVI) and Tier 2 FFmpeg/codec backend available.
- **Main Flow:**
  1. FileFusion inspects video/audio streams (codec, resolution, bitrate, HDR/SDR color, FPS).
  2. If container change is requested with compatible streams (e.g., `MKV (H.264+AAC) -> MP4`), Adaptive Engine recommends **Lossless Fast-Path Remux** (zero re-encoding, near-instant).
  3. For size reduction, engine benchmarks/selects CPU (`SVT-AV1`, `x265`, `x264`) or hardware encoder (`NVENC`/`QSV`/`VideoToolbox`) based on user preset (`Balanced Optimal` vs `Quality-Constrained VMAF >= 93`).
  4. Real-time progress (frame %, FPS, ETA, current size) streams to UI/CLI; user can pause or cancel safely.
  5. Output stream integrity and duration/sync are validated post-encode.
- **Alternative / Error Flows:**
  - *Hardware encoder unsupported for 10-bit pixel format:* Transparent fallback to deterministic CPU encoder with notification in job log.
  - *Output larger than input:* No-Op Guard warns user and preserves original file.
- **Postconditions:** Verified compressed or remuxed video with bitrate and quality metrics.

---

### `UC-006` — Secure Offline PDF Studio (Merge, Split, Watermark, Encrypt)
- **Primary Persona:** `USR-01` (Knowledge Worker), `USR-05` (Mobile User)
- **Navigation / Interface:** `PDF Studio`, or `filefusion pdf merge|split|protect ...`
- **Requirements Covered:** `FR-PDF-001`, `FR-PDF-002`, `FR-PDF-004`, `FR-LOC-001`, `FR-SEC-002`, `FR-VAL-001`, `NFR-REL-001`
- **Preconditions:** One or more local PDF files.
- **Main Flow:**
  1. User opens multiple PDFs in **PDF Studio**; page thumbnails and metadata render locally with zero network calls.
  2. User reorders pages, deletes blank pages, rotates page 3 by 90°, adds diagonal `"CONFIDENTIAL"` watermark, and enables AES-256 password encryption with print-only permissions.
  3. User optionally enables PDF image downsampling/stream compression (`FR-PDF-002`).
  4. Engine writes linearized output PDF atomically and verifies page count, encryption flags, and xref table readability.
- **Alternative / Error Flows:**
  - *Source PDF is password-locked:* UI prompts for decryption password before structural editing; fails securely on wrong password without corrupting source.
- **Postconditions:** Encrypted, watermarked, structurally verified PDF saved locally.

---

### `UC-007` — Audio Extraction, Transcoding & Loudness Normalization
- **Primary Persona:** `USR-02`, `USR-03`
- **Navigation / Interface:** `Audio Studio`, or `filefusion convert lecture.mp4 --to opus --normalize`
- **Requirements Covered:** `FR-AV-002`, `FR-AV-003`, `FR-MET-001`, `FR-VAL-001`
- **Preconditions:** Audio files (WAV, FLAC, MP3, AAC, Opus, OGG) or video files containing audio tracks.
- **Main Flow:**
  1. User loads mixed audio/video files into **Audio Studio**.
  2. User selects target format (`FLAC` for archival lossless, `Opus` at 96 kbps for speech/web, or `AAC`) and toggles **EBU R128 Loudness Normalization (-16 LUFS)**.
  3. Engine extracts/transcodes audio streams, preserves or updates ID3/Vorbis tags and cover art, and verifies decoded sample count/duration.
- **Alternative / Error Flows:**
  - *Video contains no audio track:* Pre-flight check disables audio export for that item with clear status `"No audio stream detected"`.
- **Postconditions:** Normalized audio files with verified stream integrity and metadata.

---

### `UC-008` — Bomb-Safe Archive Inspection, Creation & High-Ratio Repacking
- **Primary Persona:** `USR-03` (DevOps), `USR-04` (Custodian)
- **Navigation / Interface:** `Archive Manager`, or `filefusion archive pack|extract|repack`
- **Requirements Covered:** `FR-ARC-001`, `FR-ARC-002`, `FR-ARC-003`, `FR-SEC-001`, `FR-SEC-002`, `FR-VAL-001`, `NFR-SEC-001`
- **Preconditions:** Untrusted incoming archive (`.zip`, `.tar.gz`, `.7z`, `.tar.zst`) or local folder to archive.
- **Main Flow:**
  1. **Inspection/Extraction:** User opens an archive. Security gate validates every member path against path traversal (`../`, absolute drive paths), blocks unsafe symlinks escaping root, and monitors uncompressed byte count + compression ratio in real time.
  2. **Repacking:** User converts legacy uncompressed/deflate `.zip` archives into `.tar.zst` (level 19) or encrypted `.7z` / AES-256-GCM `.ffenc` container.
  3. FileFusion verifies bit-exact member hashes (BLAKE3) between source archive entries and repacked archive entries before marking complete.
- **Alternative / Error Flows:**
  - *Zip bomb or `../` path traversal detected:* Extraction immediately aborts (`FR-SEC-001`), deletes partial temp directory, and logs a critical security warning with offending entry path.
- **Postconditions:** Safely extracted files or bit-exact verified repacked archive.

---

### `UC-009` — Local OCR & Structured Table Extraction from Scanned PDFs
- **Primary Persona:** `USR-01` (Knowledge Worker), `USR-05` (Mobile User)
- **Navigation / Interface:** `OCR & Extraction`, or `filefusion pdf ocr scan.pdf --lang eng`
- **Requirements Covered:** `FR-PDF-003`, `FR-LOC-001`, `FR-FID-001`, `FR-VAL-001`
- **Preconditions:** Image-only scanned PDF or photographed document (`PNG`/`JPEG`); local Tesseract OCR language pack installed.
- **Main Flow:**
  1. FileFusion detects that `scan.pdf` has 0 selectable text characters and recommends **Make Searchable PDF (OCR)**.
  2. User selects OCR languages (e.g., `eng`), deskew/contrast pre-processing, and optional table-to-CSV extraction.
  3. Engine renders/extracts page images, executes local Tesseract OCR in bounded worker threads, embeds an invisible aligned text layer over original page scans, and exports detected tables to CSV/Markdown.
  4. User verifies selectable text preview side-by-side with original page scan.
- **Alternative / Error Flows:**
  - *Missing language training data:* Pre-flight check detects missing `.traineddata` and prevents silent English-only fallback.
- **Postconditions:** Searchable PDF + optional CSV/Markdown extracted text, processed 100% offline.

---

### `UC-010` — Privacy Metadata Inspection & One-Click Scrubbing
- **Primary Persona:** `USR-01` (Knowledge Worker), `USR-02` (Publisher)
- **Navigation / Interface:** `File Intelligence` -> Metadata Inspector, or `filefusion metadata scrub`
- **Requirements Covered:** `FR-MET-001`, `FR-MET-002`, `FR-VAL-001`
- **Preconditions:** Photos (EXIF/GPS/XMP), PDFs (Author/Producer/History), Office docs (revisions/comments/author), or audio files.
- **Main Flow:**
  1. User loads files into Metadata Inspector; FileFusion categorizes fields into **Privacy-Sensitive** (GPS lat/long, device serial, author email, edit history), **Technical** (dimensions, ICC profile, orientation), and **Descriptive** (title, copyright).
  2. User selects **Privacy Scrub (Safe Preset)** which strips sensitive tags while baking EXIF orientation into pixel order/preserving ICC profiles so visual rendering never breaks.
  3. Post-scrub verification re-parses the output file to confirm 0 privacy-flagged keys remain.
- **Alternative / Error Flows:**
  - *User wants custom selective removal:* Editor allows toggling individual keys before writing new file copy.
- **Postconditions:** Sanitized copy verified free of flagged metadata leaks.

---

### `UC-011` — Multi-Stage Duplicate Detection, CDC Analysis & Safe Cleanup
- **Primary Persona:** `USR-04` (Storage Custodian)
- **Navigation / Interface:** `File Intelligence` -> Storage & Duplicates, or `filefusion dedup <dir>`
- **Requirements Covered:** `FR-DUP-001`, `FR-DUP-002`, `FR-DUP-003`, `FR-DET-004`, `FR-VAL-001`, `NFR-PERF-001`
- **Preconditions:** Target folder(s) containing potentially duplicate or near-duplicate files.
- **Main Flow:**
  1. Engine runs 3-stage pipeline: (1) File size grouping -> (2) 4 KB head+tail partial hash -> (3) full streaming BLAKE3 hash for exact duplicates.
  2. Optionally runs FastCDC chunking analysis (sub-file similarity) and perceptual hashing (`pHash`/`dHash`) for visually similar resized/recompressed images.
  3. UI presents storage treemap and grouped duplicate sets, highlighting newest/oldest/path priority rules.
  4. User chooses **Move Duplicates to FileFusion Quarantine** (reversible staging folder) or **Replace with Hardlinks** rather than irreversible immediate deletion.
- **Alternative / Error Flows:**
  - *Cross-filesystem hardlink requested:* UI automatically disables hardlink option for cross-volume selections and explains why, offering quarantine instead.
- **Postconditions:** Reclaimed disk space with reversible audit log in SQLite.

---

### `UC-012` — Damaged File Assessment & Structural Repair
- **Primary Persona:** `USR-04` (Custodian), `USR-01` (Knowledge Worker)
- **Navigation / Interface:** `File Repair`, or `filefusion repair damaged.zip`
- **Requirements Covered:** `FR-REP-001`, `FR-REP-002`, `FR-VAL-001`, `NFR-REL-001`
- **Preconditions:** Corrupt or truncated ZIP, PDF, or media container file.
- **Main Flow:**
  1. User loads damaged file into **File Repair**.
  2. Diagnostic engine scans headers, trailers, central directories, `xref` tables, and payload streams to output an honest status: `Fully Repairable`, `Partially Salvageable`, or `Unrecoverable`.
  3. User runs repair; engine rebuilds missing PDF `xref` tables (Tier 1 / `qpdf`), recovers intact ZIP local-file-header entries into a clean archive, or re-indexes truncated media streams where payload frames survive.
  4. Repaired copy is written to a new path (`*_repaired.*`) and validated; recovery manifest lists exact salvaged vs. lost items.
- **Alternative / Error Flows:**
  - *File consists of zero-filled blocks or random ciphertext:* Engine truthfully reports `Unrecoverable (missing payload entropy)` and refuses to emit a fake "repaired" file.
- **Postconditions:** Verified salvaged file + transparent loss/recovery report.

---

### `UC-013` — Visual DAG Workflow Automation & Portable CLI Execution
- **Primary Persona:** `USR-03` (DevOps), `USR-02` (Media Producer)
- **Navigation / Interface:** `Automation Workflows`, or `filefusion workflow run pipeline.yaml`
- **Requirements Covered:** `FR-WFL-001`, `FR-WFL-002`, `FR-BAT-001..002`, `FR-DEV-001`
- **Preconditions:** User wants repeatable multi-step processing across folders/batches.
- **Main Flow:**
  1. User builds a visual node pipeline in **Automation Workflows**:
     `[Input Folder] -> [Filter: Image > 2 MB] -> [Metadata Privacy Scrub] -> [Resize <= 2048px] -> [Encode WebP SSIM>=0.98] -> [Pack .tar.zst]`.
  2. Workflow validator checks type compatibility between connected nodes before saving.
  3. User runs workflow interactively or exports `pipeline.yaml` to run headlessly on a schedule or in CI via `filefusion workflow run pipeline.yaml --json`.
- **Alternative / Error Flows:**
  - *Cycle detected or incompatible node types (e.g., Audio Normalize connected to PDF output):* Graph editor blocks invalid connection with clear schema message.
- **Postconditions:** Deterministic execution across batches with per-node audit metrics.

---

### `UC-014` — Strictly Opt-In Local / External AI File Intelligence
- **Primary Persona:** `USR-01`, `USR-03`
- **Navigation / Interface:** `OCR & Extraction` / `Automation Workflows` / `Settings -> AI Privacy`
- **Requirements Covered:** `FR-AI-001`, `FR-AI-002`, `FR-LOC-001`
- **Preconditions:** User explicitly enables an optional local ONNX/LLM model or configures an external API provider in Settings.
- **Main Flow:**
  1. By default, AI features are **OFF** and 100% of core tools work without them.
  2. When user explicitly invokes **Summarize Document** or **Generate Workflow from Prompt**:
     - If **Local AI** is active: runs strictly offline on local runtime.
     - If **External AI** is configured: displays a mandatory **Privacy & Cost Consent Modal** showing exact payload preview (text/metadata snippet), estimated token cost, and destination provider before any bytes leave the device.
  3. AI outputs (e.g., draft workflow JSON, document summary, smart filename) are presented for user review before applying file modifications.
- **Alternative / Error Flows:**
  - *Network offline or user declines consent:* Core non-AI extraction/conversion remains available; zero silent background transmission occurs.
- **Postconditions:** User-approved AI assistance with transparent token/privacy audit log.

---

### `UC-015` — Batch Monitoring, Cancellation & Visual/Structural Comparison
- **Primary Persona:** All (`USR-01`..`USR-06`)
- **Navigation / Interface:** `History & Jobs`, or `filefusion diff a.png b.png`
- **Requirements Covered:** `FR-BAT-001`, `FR-VAL-001`, `FR-VAL-002`, `FR-LOC-002`, `NFR-REL-001`, `NFR-REL-002`
- **Preconditions:** Active or completed jobs recorded in local SQLite.
- **Main Flow:**
  1. User opens **History & Jobs** during a 500-file batch job to monitor per-item throughput, memory usage, and remaining ETA.
  2. User clicks **Cancel Remaining** or **Pause**; active jobs abort cleanly within 500 ms without partial output clutter.
  3. On completed jobs, user opens **Compare Before/After**:
     - Images: Split-slider visual comparison + pixel difference heatmap + SSIM/PSNR/size delta.
     - Documents: Text/structural diff + Fidelity Report.
     - Archives/Binary: BLAKE3 hash & member tree diff.
- **Alternative / Error Flows:**
  - *Source file moved/deleted since job ran:* History view still displays recorded metrics and output link, noting source is no longer on disk for live re-diff.
- **Postconditions:** Full auditability and empirical quality proof for every operation.

---

### `UC-016` — Cross-Platform Diagnostics, Resource Tuning & Optional Cloud Offload
- **Primary Persona:** `USR-03`, `USR-05`, `USR-06`
- **Navigation / Interface:** `Settings`, `filefusion doctor`, or `REST API`
- **Requirements Covered:** `FR-DEV-001`, `FR-DEV-002`, `FR-CLD-001`, `NFR-PERF-001..003`, `NFR-UX-001`
- **Preconditions:** App running on Desktop, Mobile, Web, CLI, or API server.
- **Main Flow:**
  1. User opens **Settings** (or runs `filefusion doctor`) to inspect Tier 1 built-in capabilities, Tier 2 external tool availability (`ffmpeg`, `qpdf`, `tesseract`, `libreoffice`), CPU SIMD flags, and hardware encoders.
  2. User configures max concurrent workers and memory cap (e.g., 2 GB cap on a 16 GB Windows 11 host or 512 MB on Android), toggles Light/Dark futuristic theme, and configures optional self-hosted/Cloudflare R2 sync endpoint (`FR-CLD-001`).
  3. Mobile/Web user optionally offloads a 4K AV1 video transcode to their configured self-hosted Rust API server while keeping local inspection offline.
- **Alternative / Error Flows:**
  - *Configured cloud endpoint unreachable:* App continues local processing seamlessly and queues sync only when user retries.
- **Postconditions:** Predictable, resource-bounded runtime tuned to host hardware.

---

## 5. Cross-Cutting UX & Accessibility Journeys (`UXJ-*`)

| Journey ID | Name | Experience Contract | Requirements |
| :--- | :--- | :--- | :--- |
| **`UXJ-01`** | **Universal Drop-Zone & Smart Intent Routing** | Dropping any heterogeneous mix of files onto **Home / Workspace** inspects magic bytes in `< 100 ms` and groups files by detected type with 1-click contextual actions (e.g., *"12 PNGs -> Optimize to WebP (-68% est.)"*, *"3 PDFs -> Merge & Scrub Metadata"*). | `FR-DET-001`, `FR-ADP-001`, `NFR-PERF-002` |
| **`UXJ-02`** | **Command Palette & Keyboard-First Navigation** | Pressing `Ctrl+K` / `Cmd+K` anywhere opens a fuzzy-searchable command palette to jump to any of the 14 studios, trigger presets, toggle Light/Dark mode, or inspect recent jobs without touching a mouse. | `NFR-UX-001` |
| **`UXJ-03`** | **Progressive Disclosure (Calm Power UI)** | Every studio surfaces a clean **Quick Action Bar** (Preset + Destination + Run) by default, keeping advanced sliders (CRF, quantization tables, CDC chunk sizes, FFmpeg flags) inside an expandable **Advanced Parameters** drawer so casual users aren't overwhelmed while experts retain full control. | `FR-CMP-001`, `NFR-UX-001` |
| **`UXJ-04`** | **Honest Failure, Cancellation & Atomic Recovery** | No spinner ever stalls indefinitely. Every error surfaces: (1) What failed, (2) Why (exact format/OS/dependency reason), and (3) Actionable next step (e.g., open File Repair or check `filefusion doctor`). Cancelling cleans temp files atomically. | `NFR-REL-001`, `NFR-REL-002` |

---

## 6. Requirements-to-Use-Case Traceability Matrix

| Requirement Domain | Requirement IDs | Covered By Use Cases |
| :--- | :--- | :--- |
| **4.1 File Detection & Inspection** | `FR-DET-001`..`004` | `UC-001`, `UC-011`, `UXJ-01` |
| **4.2 Document, Office & Ebook Conversion** | `FR-DOC-001`..`003` | `UC-002`, `UC-003` |
| **4.3 Image Conversion & Optimization** | `FR-IMG-001`..`003` | `UC-004`, `UC-015` |
| **4.4 Audio & Video Conversion** | `FR-AV-001`..`003` | `UC-005`, `UC-007` |
| **4.5 Archive Operations** | `FR-ARC-001`..`003` | `UC-008` |
| **4.6 Intelligent Compression** | `FR-CMP-001`..`003` | `UC-004`, `UC-005`, `UXJ-03` |
| **4.7 Adaptive Codec Selection** | `FR-ADP-001`..`002` | `UC-001`, `UC-005`, `UXJ-01` |
| **4.8 PDF Manipulation & OCR** | `FR-PDF-001`..`004` | `UC-006`, `UC-009` |
| **4.9 Formatting & Structure Preservation** | `FR-FID-001`..`002` | `UC-002`, `UC-003`, `UC-009` |
| **4.10 Duplicate Detection & Storage** | `FR-DUP-001`..`003` | `UC-011` |
| **4.11 File Repair & Recovery** | `FR-REP-001`..`002` | `UC-012` |
| **4.12 Metadata Inspection & Editing** | `FR-MET-001`..`002` | `UC-003`, `UC-007`, `UC-010` |
| **4.13 Batch Conversion & Processing** | `FR-BAT-001`..`002` | `UC-004`, `UC-013`, `UC-015` |
| **4.14 Visual Workflow Automation** | `FR-WFL-001`..`002` | `UC-013` |
| **4.15 AI File Intelligence (Opt-In)** | `FR-AI-001`..`002` | `UC-014` |
| **4.16 Local-First / Offline** | `FR-LOC-001`..`002` | `UC-001`, `UC-006`, `UC-009`, `UC-014`, `UC-015` |
| **4.17 Optional Cloud Sync/Offload** | `FR-CLD-001` | `UC-016` |
| **4.18 CLI, SDK & Developer API** | `FR-DEV-001`..`002` | `UC-001`..`UC-013`, `UC-016` |
| **4.19 Secure File Handling & Encryption** | `FR-SEC-001`..`002` | `UC-001`, `UC-006`, `UC-008` |
| **4.20 Validation, Comparison & Quality** | `FR-VAL-001`..`002` | `UC-002`..`UC-012`, `UC-015` |
