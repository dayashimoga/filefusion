# FileFusion — Master Execution Progress Tracker

**Project:** FileFusion — Universal File Intelligence, Transformation & Optimization Platform  
**Governance Policy:** Strict One-Subphase-At-A-Time Execution. Every subphase must be implemented, tested, independently reviewed, documented, and committed before requesting user confirmation to proceed.

---

## Current Active Subphase

- **Stage:** Stage 5 — File Intelligence & Storage
- **Phase:** Phase 5.1 — Duplicate, Perceptual Similarity & Storage Analytics Engines
- **Subphase:** `5.1.1` — Multi-stage exact duplicate scanner (`size -> 4 KB head/tail -> BLAKE3`), streaming `FastCDC`, `aHash`/`dHash`/`pHash` clustering & storage treemap analytics (`optimization-engine`)
- **Status:** `COMPLETE`

---

## Subphase Execution Log

### Subphase 5.1.1 — Multi-stage exact duplicate scanner (`size -> 4 KB head/tail -> BLAKE3`), streaming `FastCDC`, `aHash`/`dHash`/`pHash` clustering & storage treemap analytics (`optimization-engine`)

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 5 — File Intelligence & Storage |
| **Phase** | Phase 5.1 — Duplicate, Perceptual Similarity & Storage Analytics Engines |
| **Subphase** | `5.1.1` — Multi-stage exact duplicate scanner (`size -> 4 KB head/tail -> BLAKE3`), streaming `FastCDC`, `aHash`/`dHash`/`pHash` clustering & storage treemap analytics (`optimization-engine`) |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 3 (Compression & Codec Engineer), Agent 2 (Rust Core Engineer), Agent 8 (QA & Automation Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 4 hours |
| **Actual Duration** | ~15 minutes |
| **Acceptance Criteria** | 1. Upgrade `crates/optimization-engine/src/dedup.rs` with bounded-memory streaming `FastCDC` (`chunk_stream_fastcdc` matching `chunk_bytes_fastcdc` cuts identically without full-file buffering, `NFR-PERF-001`) plus `compute_image_ahash` and `compute_perceptual_hash_set` (`aHash`, `dHash`, `pHash`, `FR-DET-004`).<br>2. Implement `crates/optimization-engine/src/storage.rs` delivering 3-stage exact duplicate scanning (`scan_exact_duplicates`: size bucket -> `4 KB` head + `4 KB` tail `BLAKE3` -> full `64 KB` streaming `BLAKE3`, `.filefusion-quarantine` & symlink-loop protection, `FR-DUP-001`), perceptual near-duplicate image clustering (`scan_near_duplicate_images`: union-find similarity groups with highest-resolution keeper selection, `FR-DUP-002`), and hierarchical directory treemap + domain/format analytics (`analyze_storage_treemap`).<br>3. Re-export Stage 5 APIs cleanly in `crates/optimization-engine/src/lib.rs`, backfill `a4f9b9f` (`stage-4.2.2`) onto `4.2.2` in `docs/PROGRESS.md`, add unit/integration tests covering streaming `FastCDC`, 3-stage I/O pruning, near-duplicate clustering, and treemap rollups, pass `cargo fmt --all -- --check`, native & WASM Clippy (`-D warnings`), `cargo test --workspace --locked` (`122` passed), and WASM-featured tests (`63` passed across 12 crates) with `Cargo.toml`/`Cargo.lock` untouched, append `TODO.md`/`CHANGELOG.md`, pass Agent 12 audit, commit, tag `stage-5.1.1`, and push. |
| **Test Results** | `cargo fmt --all -- --check`: PASS; `cargo clippy --workspace --all-targets -- -D warnings`: PASS (0 warnings); `cargo clippy --no-default-features --features wasm --locked -- -D warnings` (15 crates): PASS (0 warnings); `cargo test --workspace --locked`: 122 passed, 0 failed (`codec-registry`: 3, `compression-archive`: 5, `compression-audio`: 4, `compression-core`: 3, `compression-image`: 3, `compression-office`: 3, `compression-pdf`: 11, `compression-video`: 4, `conversion-core`: 8, `file-core`: 28, `filefusion` unit: 1, `cli_smoke`: 13, `hardware-acceleration`: 3, `job-manager`: 18, `optimization-engine`: 8, `quality-engine`: 2, `validation-engine`: 5); WASM-featured unit tests (12 crates): 63 passed, 0 failed; Agent 12 independent audit: **PASS**; `Cargo.lock`: untouched. |
| **Files Changed** | `crates/optimization-engine/src/{dedup.rs,lib.rs,storage.rs}`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | Tagged `stage-5.1.1` on `main` (pushed to `origin`; exact SHA backfilled in `5.1.2`) |
| **Outstanding Issues** | None |

### Subphase 4.2.2 — Stage 4 CLI (`filefusion pdf`, `repair`, `metadata`, `encrypt`/`decrypt`) & Stage 4 security/recovery corpus tests (Stage 4 Completion)

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 4 — PDF & Document Studio, Repair & Security Engines |
| **Phase** | Phase 4.2 — Unified Repair, Encryption & Metadata Orchestration |
| **Subphase** | `4.2.2` — Stage 4 CLI (`filefusion pdf`, `repair`, `metadata`, `encrypt`/`decrypt`) & Stage 4 security/recovery corpus tests |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 2 (Rust Core Engineer), Agent 4 (Document & Conversion Engineer), Agent 8 (QA & Automation Engineer), Agent 9 (Security Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 3 hours |
| **Actual Duration** | ~35 minutes |
| **Acceptance Criteria** | 1. Re-export PDF Studio & OCR APIs cleanly from L4 `crates/optimization-engine/src/lib.rs` and expose L5 coordinator methods (`JobManager::{pdf_merge, pdf_split, pdf_reorder, pdf_rotate, pdf_crop, pdf_watermark, pdf_scan_check, pdf_verify_signatures, pdf_inspect_security, pdf_ocr}`) in `crates/job-manager/src/stage4.rs` (`MOD-RULE-03` compliant, zero L5->L3 bypasses, `tier2-tesseract` `EngMissing` gating).<br>2. Implement real Stage 4 CLI commands in `apps/cli/src/{stage4_cmds.rs,main.rs}`: `filefusion pdf <merge|split|reorder|rotate|crop|watermark|scan-check|verify-sig|security|ocr|protect|unlock>`, `filefusion repair`, `filefusion metadata <inspect|scrub|edit>`, `filefusion encrypt`, and `filefusion decrypt` (`--passphrase`, `--passphrase-env`, `--json`, `--overwrite`), boxing heavy `Commands` variants and executing `run()` on an `8 MiB` thread stack for Windows debug CLI reliability.<br>3. Add 4 Stage 4 CLI & security/recovery corpus E2E tests in `apps/cli/tests/cli_smoke.rs`, backfill `4f3f737` (`stage-4.2.1`) onto `4.2.1`, pass `cargo fmt --all -- --check`, native & WASM Clippy (`-D warnings`), `cargo test --workspace --locked` (120 passed across workspace + all 13 `cli_smoke` E2E tests), and WASM-featured unit tests (61 passed across 12 crates), keep `Cargo.toml`/`Cargo.lock` untouched, append `TODO.md`/`CHANGELOG.md`, pass Agent 12 audit, commit, tag `stage-4.2.2`, and push. |
| **Test Results** | `cargo fmt --all -- --check`: PASS; `cargo clippy --workspace --all-targets -- -D warnings`: PASS (0 warnings); `cargo clippy --no-default-features --features wasm --locked -- -D warnings` (15 crates): PASS (0 warnings); `cargo test --workspace --locked`: 120 passed, 0 failed (`codec-registry`: 3, `compression-archive`: 5, `compression-audio`: 4, `compression-core`: 3, `compression-image`: 3, `compression-office`: 3, `compression-pdf`: 11, `compression-video`: 4, `conversion-core`: 8, `file-core`: 28, `filefusion` unit: 1, `cli_smoke`: 13, `hardware-acceleration`: 3, `job-manager`: 18, `optimization-engine`: 6, `quality-engine`: 2, `validation-engine`: 5); WASM-featured unit tests (12 crates): 61 passed, 0 failed; Agent 12 independent audit: **PASS**; `Cargo.lock`: untouched. |
| **Files Changed** | `crates/optimization-engine/src/lib.rs`, `crates/job-manager/src/{lib.rs,stage4.rs}`, `apps/cli/src/{main.rs,stage4_cmds.rs}`, `apps/cli/tests/cli_smoke.rs`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `a4f9b9f` (`tag stage-4.2.2` on `main`, pushed to `origin`) |
| **Outstanding Issues** | None |

### Subphase 4.2.1 — Unified file repair (`PDF`/`ZIP`/media index), `.ffenc` container & privacy metadata sanitizer/editor orchestration

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 4 — PDF & Document Studio, Repair & Security Engines |
| **Phase** | Phase 4.2 — Unified Repair, Encryption & Metadata Orchestration |
| **Subphase** | `4.2.1` — Unified file repair (`PDF`/`ZIP`/media index), `.ffenc` container & privacy metadata sanitizer/editor orchestration |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 3 (Compression & Codec Engineer), Agent 9 (Security Engineer), Agent 2 (Rust Core Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 4 hours |
| **Actual Duration** | ~20 minutes |
| **Acceptance Criteria** | 1. Extend L3 engines cleanly for cross-format Stage 4 orchestration (`FR-REP-001`..`002`, `FR-MET-001`..`002`, `FR-SEC-002`): `compression-archive` (`inspect_zip_health`, `inspect_ffenc_header`), `compression-pdf` (`repair_pdf_bytes` with pure-Rust `xref` synthesis, `repair_pdf_to_staged_with_runner`, `update_pdf_metadata_bytes`), `compression-office` (`OfficeContainerMetadata`, `extract_office_metadata_from_entries`, `edit_metadata_xml`, `sanitize_and_edit_office_metadata`), and `compression-video` (`build_ffmpeg_container_repair_invocation`).<br>2. Implement L4 unified orchestrators (`optimization-engine/src/{repair.rs,metadata.rs,security.rs}`) classifying `Healthy`/`FullyRepairable`/`PartiallySalvageable`/`Unrecoverable`, staging repairs across `PDF`/`ZIP`/`MP4`/`MOV`/`MKV`/`WebM`, inspecting & scrubbing/editing metadata while preserving essential orientation/color profiles, and staging `.ffenc` / PDF AES encryption.<br>3. Wire L5 control-plane orchestration onto `JobManager` (`job-manager/src/stage4.rs`), backfill `84a9eeb` (`stage-4.1.2`) onto `4.1.2`, pass `cargo fmt --all -- --check`, native & WASM Clippy (`-D warnings`), `cargo test --workspace --locked` (116 tests), and WASM-featured tests (61 tests across 12 crates), keep `Cargo.toml`/`Cargo.lock` untouched, append `TODO.md`/`CHANGELOG.md`, pass Agent 12 audit, commit, tag `stage-4.2.1`, and push. |
| **Test Results** | `cargo fmt --all -- --check`: PASS; `cargo clippy --workspace --all-targets -- -D warnings`: PASS (0 warnings); `cargo clippy --no-default-features --features wasm --locked -- -D warnings` (15 crates): PASS (0 warnings); `cargo test --workspace --locked`: 116 passed, 0 failed (`codec-registry`: 3, `compression-archive`: 5, `compression-audio`: 4, `compression-core`: 3, `compression-image`: 3, `compression-office`: 3, `compression-pdf`: 11, `compression-video`: 4, `conversion-core`: 8, `file-core`: 28, `filefusion` unit: 1, `cli_smoke`: 9, `hardware-acceleration`: 3, `job-manager`: 18, `optimization-engine`: 6, `quality-engine`: 2, `validation-engine`: 5); WASM-featured unit tests (`compression-archive`: 5, `compression-audio`: 3, `compression-core`: 3, `compression-image`: 3, `compression-office`: 3, `compression-pdf`: 9, `compression-video`: 3, `conversion-core`: 7, `hardware-acceleration`: 3, `job-manager`: 14, `optimization-engine`: 6, `quality-engine`: 2): 61 passed, 0 failed; Agent 12 independent audit: **PASS**; `Cargo.lock`: untouched. |
| **Files Changed** | `crates/compression-archive/src/{ffenc.rs,lib.rs,repair.rs}`, `crates/compression-office/src/{container.rs,lib.rs,xml.rs}`, `crates/compression-pdf/src/{lib.rs,optimizer.rs,repair.rs}`, `crates/compression-video/src/{lib.rs,pipeline.rs}`, `crates/optimization-engine/src/{lib.rs,metadata.rs,repair.rs,security.rs}`, `crates/job-manager/src/{lib.rs,stage4.rs}`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `4f3f737` (`tag stage-4.2.1` on `main`, pushed to `origin`) |
| **Outstanding Issues** | None |

### Subphase 4.1.2 — Local OCR (`TesseractAdapter`, searchable PDF text overlay) + PDF encryption, permissions & `/Sig` verification

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 4 — PDF & Document Studio, Repair & Security Engines |
| **Phase** | Phase 4.1 — PDF Studio & OCR / Security Engines |
| **Subphase** | `4.1.2` — Local OCR (`TesseractAdapter`, searchable PDF text overlay) + PDF encryption, permissions & `/Sig` verification |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 4 (Document & Conversion Engineer), Agent 9 (Security Engineer), Agent 8 (QA & Automation Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 4 hours |
| **Actual Duration** | ~20 minutes |
| **Acceptance Criteria** | 1. Implement isolated Tier 2 `TesseractAdapter` (`ExternalToolAdapter`, `MOD-RULE-04`, `Apache-2.0`, `FR-PDF-003`) in `crates/compression-pdf/src/ocr.rs` plus pure-Rust TSV/`hOCR` parsers (`parse_tesseract_tsv`, `parse_tesseract_hocr`), raster `/Image` `XObject` extractor (`extract_pdf_page_images`), and invisible `3 Tr` searchable PDF text overlay embedder (`embed_searchable_ocr_text_layer`, `build_searchable_pdf_from_image_and_ocr`).<br>2. Implement pure-Rust `AES-256` (`V5`/`R6`) and `AES-128` (`V4`/`R4`) PDF encryption, granular ISO-32000 `/P` permissions (`PdfPermissions`), decryption, security inspection, `qpdf` crypto invocation builders, and offline `/Sig` `/ByteRange` + ASN.1 DER `/Contents` verification (`verify_pdf_signatures`, `FR-PDF-004`) in `crates/compression-pdf/src/security.rs` with zero changes to `Cargo.lock`.<br>3. Pass `cargo fmt --all -- --check`, native & WASM Clippy (`-D warnings`), `cargo test --workspace --locked` (113 tests), and WASM-featured tests (58 tests across 12 crates), update `docs/PROGRESS.md` (backfilling `e5dbda3` onto `4.1.1`), append `TODO.md`/`CHANGELOG.md`, obtain Agent 12 `PASS`, commit, tag `stage-4.1.2`, and push. |
| **Test Results** | `cargo fmt --all -- --check`: PASS; `cargo clippy --workspace --all-targets -- -D warnings`: PASS (0 warnings); `cargo clippy --no-default-features --features wasm --locked -- -D warnings` (15 crates): PASS (0 warnings); `cargo test --workspace --locked`: 113 passed, 0 failed (`codec-registry`: 3, `compression-archive`: 5, `compression-audio`: 4, `compression-core`: 3, `compression-image`: 3, `compression-office`: 3, `compression-pdf`: 11, `compression-video`: 4, `conversion-core`: 8, `file-core`: 28, `filefusion` unit: 1, `cli_smoke`: 9, `hardware-acceleration`: 3, `job-manager`: 17, `optimization-engine`: 4, `quality-engine`: 2, `validation-engine`: 5); WASM-featured unit tests (`compression-archive`: 5, `compression-audio`: 3, `compression-core`: 3, `compression-image`: 3, `compression-office`: 3, `compression-pdf`: 9, `compression-video`: 3, `conversion-core`: 7, `hardware-acceleration`: 3, `job-manager`: 13, `optimization-engine`: 4, `quality-engine`: 2): 58 passed, 0 failed; Agent 12 independent audit: **PASS**; `Cargo.lock`: untouched. |
| **Files Changed** | `crates/compression-pdf/src/{lib.rs,ocr.rs,security.rs,studio.rs}`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `84a9eeb` (`tag stage-4.1.2` on `main`, pushed to `origin`) |
| **Outstanding Issues** | None |

### Subphase 4.1.1 — Structural PDF manipulation (`merge`, `split`, `reorder`, `rotate`, `crop`, `watermark`, scan detection)

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 4 — PDF & Document Studio, Repair & Security Engines |
| **Phase** | Phase 4.1 — PDF Studio & OCR / Security Engines |
| **Subphase** | `4.1.1` — Structural PDF manipulation (`merge`, `split`, `reorder`, `rotate`, `crop`, `watermark`, scan detection) |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 4 (Document & Conversion Engineer), Agent 3 (Compression & Codec Engineer), Agent 8 (QA & Automation Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 3 hours |
| **Actual Duration** | ~25 minutes |
| **Acceptance Criteria** | 1. Implement pure-Rust WASM-native PDF Studio structural operations in `crates/compression-pdf/src/studio.rs` (`#![forbid(unsafe_code)]`, `FR-PDF-001`, `FR-PDF-004`): `parse_page_range_spec`, `merge_pdfs`, `extract_or_reorder_pdf_pages`, `split_pdf_by_ranges`, `split_pdf_every_n_pages`, `rotate_pdf_pages` (`90°` increments mod 360), `crop_pdf_pages` (`PdfCropMargins`), and centered rotated vector text overlay watermarking (`apply_pdf_text_watermark`, `PdfWatermarkOptions`), plus encrypted-PDF rejection (`ErrorCode::FmtDrm`) and inherited attribute materialization.<br>2. Implement pure-Rust scanned/image-only page & `/Sig` digital signature pre-screening (`inspect_pdf_scan_status`, `PdfScanInspection`) and export all PDF Studio APIs cleanly from `crates/compression-pdf/src/lib.rs` with zero new external dependencies.<br>3. Pass `cargo fmt --all -- --check`, native & WASM Clippy (`-D warnings`), `cargo test --workspace --locked` (109 tests), and WASM-featured tests (55 tests across 12 crates), backfill `186ecde` (`stage-3.2.2`), initialize the Stage 4 queue in `docs/PROGRESS.md`, append `TODO.md`/`CHANGELOG.md`, obtain Agent 12 `PASS` audit, commit, tag `stage-4.1.1`, and push. |
| **Test Results** | `cargo fmt --all -- --check`: PASS; `cargo clippy --workspace --all-targets -- -D warnings`: PASS (0 warnings); `cargo clippy --no-default-features --features wasm --locked -- -D warnings` (15 crates): PASS (0 warnings); `cargo test --workspace --locked`: 109 passed, 0 failed (`codec-registry`: 3, `compression-archive`: 5, `compression-audio`: 4, `compression-core`: 3, `compression-image`: 3, `compression-office`: 3, `compression-pdf`: 7, `compression-video`: 4, `conversion-core`: 8, `file-core`: 28, `filefusion` unit: 1, `cli_smoke`: 9, `hardware-acceleration`: 3, `job-manager`: 17, `optimization-engine`: 4, `quality-engine`: 2, `validation-engine`: 5); WASM-featured unit tests (`compression-archive`: 5, `compression-audio`: 3, `compression-core`: 3, `compression-image`: 3, `compression-office`: 3, `compression-pdf`: 6, `compression-video`: 3, `conversion-core`: 7, `hardware-acceleration`: 3, `job-manager`: 13, `optimization-engine`: 4, `quality-engine`: 2): 55 passed, 0 failed; Agent 12 independent audit: **PASS**; `Cargo.lock`: untouched. |
| **Files Changed** | `crates/compression-pdf/src/{lib.rs,studio.rs}`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `e5dbda3` (`tag stage-4.1.1` on `main`, pushed to `origin`) |
| **Outstanding Issues** | None |

### Subphase 3.2.2 — Stage 3 CLI (`filefusion convert`), `job-manager` wiring & conversion fidelity corpus tests

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 3 — Universal Conversion Engine |
| **Phase** | Phase 3.2 — Office, ODF, Ebook & CLI Integration |
| **Subphase** | `3.2.2` — Stage 3 CLI (`filefusion convert`), `job-manager` wiring & conversion fidelity corpus tests |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 2 (Rust Core Engineer), Agent 4 (Document & Conversion Engineer), Agent 8 (QA & Automation Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 3 hours |
| **Actual Duration** | ~20 minutes |
| **Acceptance Criteria** | 1. Wire L5 `crates/job-manager` to L4 `crates/conversion-core` strictly matching `FF-MOD-001` §4 (`L5_JOB -> L0 & L1_REG & L4_OPT & L4_CNV`) with `native-process`, `native-logging`, and `wasm` feature parity; expose `JobManager::preflight_conversion`, `JobManager::convert_file`, and `ConvertFileReport` (`FR-DOC-001`..`003`, `FR-FID-001`..`002`).<br>2. Expand `CliTargetFormat` and implement thin Plane 1 `filefusion convert` subcommand (`--to`, `-o/--output`, `--preflight-only`, `--force-loss`, `--no-preserve-structure`, `--strip-metadata`, `--overwrite`, `--json`) in `apps/cli/src/main.rs` (`MOD-RULE-03`) with pure `--json` stdout and stderr tracing.<br>3. Add real E2E Stage 3 fidelity corpus tests (`cli_convert_stage3_markup_and_epub_fidelity_corpus`, `cli_convert_stage3_spreadsheet_preflight_and_force_loss_gate`) in `apps/cli/tests/cli_smoke.rs` plus `JobManager::convert_file` unit tests (`crates/job-manager/src/lib.rs`), pass formatting, native & WASM Clippy (`-D warnings`), `cargo test --workspace --locked` (106 tests), and WASM-featured tests (52 tests across 12 crates), backfill `1c90d81` (`stage-3.2.1`), obtain Agent 12 `PASS` audit, commit, tag `stage-3.2.2`, and push. |
| **Test Results** | `cargo fmt --all -- --check`: PASS; `cargo clippy --workspace --all-targets -- -D warnings`: PASS (0 warnings); `cargo clippy --no-default-features --features wasm --locked -- -D warnings` (15 crates): PASS (0 warnings); `cargo test --workspace --locked`: 106 passed, 0 failed (`codec-registry`: 3, `compression-archive`: 5, `compression-audio`: 4, `compression-core`: 3, `compression-image`: 3, `compression-office`: 3, `compression-pdf`: 4, `compression-video`: 4, `conversion-core`: 8, `file-core`: 28, `filefusion` unit: 1, `cli_smoke`: 9, `hardware-acceleration`: 3, `job-manager`: 17, `optimization-engine`: 4, `quality-engine`: 2, `validation-engine`: 5); WASM-featured unit tests (`compression-archive`: 5, `compression-audio`: 3, `compression-core`: 3, `compression-image`: 3, `compression-office`: 3, `compression-pdf`: 3, `compression-video`: 3, `conversion-core`: 7, `hardware-acceleration`: 3, `job-manager`: 13, `optimization-engine`: 4, `quality-engine`: 2): 52 passed, 0 failed; `Cargo.lock` diff: zero external crate additions; Agent 12 independent audit: **PASS**. |
| **Files Changed** | `Cargo.lock`, `crates/job-manager/{Cargo.toml,src/lib.rs}`, `apps/cli/src/main.rs`, `apps/cli/tests/cli_smoke.rs`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `186ecde` (tag `stage-3.2.2`, pushed to `origin/main`) |
| **Outstanding Issues** | None |

### Subphase 3.2.1 — Native Office, ODF & EPUB conversions + Tier 2 `LibreOffice`/`Pandoc`/`Calibre` adapters

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 3 — Universal Conversion Engine |
| **Phase** | Phase 3.2 — Office, ODF, Ebook & CLI Integration |
| **Subphase** | `3.2.1` — Native Office, ODF & EPUB conversions + Tier 2 `LibreOffice`/`Pandoc`/`Calibre` adapters |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 4 (Document & Conversion Engineer), Agent 9 (Security Engineer), Agent 8 (QA & Automation Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 4 hours |
| **Actual Duration** | ~20 minutes |
| **Acceptance Criteria** | 1. Expose validated Zip-Slip/bomb-guarded container packing helpers (`read_validated_zip_entries`, `write_office_container_entries`) in L3 `crates/compression-office` with `mimetype`-first `Stored` packaging (`FR-DOC-002`..`003`, `FR-SEC-001`).<br>2. Implement pure-Rust WASM-native container converters in `crates/conversion-core/src/office_epub.rs` for spreadsheets (`Xlsx`/`Ods` <-> `Csv`/`Json`/`Yaml`/`Xml`/`Markdown`/`Html`/`PlainText`) and prose/ebook documents (`Docx`/`Odt`/`Epub` <-> `Markdown`/`Html`/`PlainText` & `Docx`/`Odt` <-> `Epub` preserving TOC `nav.xhtml`, spine ordering, hyperlinks, tables, headings, XML DTD entity-bomb blocking `NFR-SEC-001`, and `strip_metadata` privacy scrubbing).<br>3. Implement isolated `#[cfg(feature = "native-process")]` `ExternalToolAdapter` structs (`LibreOfficeAdapter`, `PandocAdapter`, `CalibreAdapter`) and `execute_tier2_document_tool` in `crates/conversion-core/src/tier2_adapters.rs` (`FF-CTR-001` §3.3), wire all Tier 1 & Tier 2 routes through `ConversionGraph` (`graph.rs`) and `UniversalConversionEngine` (`lib.rs`), pass native & WASM formatting/Clippy/tests, backfill `6ea5132` (`stage-3.1.2`), obtain Agent 12 `PASS` audit, commit, tag `stage-3.2.1`, and push. |
| **Test Results** | `cargo fmt --all -- --check`: PASS; `cargo clippy --workspace --all-targets -- -D warnings`: PASS (0 warnings); `cargo clippy --no-default-features --features wasm --locked -- -D warnings` (15 crates): PASS (0 warnings); `cargo test --workspace --locked`: 103 passed, 0 failed (`codec-registry`: 3, `compression-archive`: 5, `compression-audio`: 4, `compression-core`: 3, `compression-image`: 3, `compression-office`: 3, `compression-pdf`: 4, `compression-video`: 4, `conversion-core`: 8, `file-core`: 28, `filefusion` unit: 1, `cli_smoke`: 7, `hardware-acceleration`: 3, `job-manager`: 16, `optimization-engine`: 4, `quality-engine`: 2, `validation-engine`: 5); WASM-featured unit tests (`compression-archive`: 5, `compression-audio`: 3, `compression-core`: 3, `compression-image`: 3, `compression-office`: 3, `compression-pdf`: 3, `compression-video`: 3, `conversion-core`: 7, `hardware-acceleration`: 3, `job-manager`: 12, `optimization-engine`: 4, `quality-engine`: 2): 51 passed, 0 failed; `Cargo.lock`: zero external dependency additions; Agent 12 independent audit: **PASS**. |
| **Files Changed** | `crates/compression-office/src/{container.rs,lib.rs}`, `crates/conversion-core/src/{lib.rs,graph.rs,preflight.rs,markup.rs,office_epub.rs,tier2_adapters.rs}`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `1c90d81` (tag `stage-3.2.1`, pushed to `origin/main`) |
| **Outstanding Issues** | None |

### Subphase 3.1.2 — Bidirectional structured text, markup & tabular engines (`Markdown`/`HTML`/`PlainText`/`CSV`/`JSON`/`YAML`/`XML`)

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 3 — Universal Conversion Engine |
| **Phase** | Phase 3.1 — Conversion Graph & Structured Text / Markup Engines |
| **Subphase** | `3.1.2` — Bidirectional structured text, markup & tabular engines (`Markdown`/`HTML`/`PlainText`/`CSV`/`JSON`/`YAML`/`XML`) |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 4 (Document & Conversion Engineer), Agent 2 (Rust Core Engineer), Agent 8 (QA & Automation Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 4 hours |
| **Actual Duration** | ~20 minutes |
| **Acceptance Criteria** | 1. Implement pure-Rust bidirectional structured markup parser/renderer (`crates/conversion-core/src/markup.rs`, `FR-DOC-001`, `#![forbid(unsafe_code)]`, WASM-native) across CommonMark/GFM `Markdown`, HTML5, and `PlainText` preserving headings, GFM pipe/`<table>` tables, links, images, lists, code blocks, blockquotes, and quotes, while actively sanitizing `<script>`/`<style>`/`<iframe>`/`<object>`/`<embed>`, `on*=` attributes, and `javascript:`/`vbscript:`/`data:text/html` URLs (`NFR-SEC-001`).<br>2. Implement pure-Rust bidirectional structured data & tabular converter (`crates/conversion-core/src/tabular.rs`, `FR-DOC-001`) across RFC 4180 `CSV`/auto-detected `TSV`, pretty `JSON` (deterministic key order), indented `YAML` 1.2 subset, and security-hardened `XML` (`<!ENTITY`/`<!DOCTYPE` rejection & depth ceiling `<= 64`, `NFR-SEC-001`), plus direct tabular export routes into `Markdown`/`HTML`/`PlainText`.<br>3. Wire `FID-MARKUP-PLAINTEXT` and `FID-CSV-FLATTEN` pre-flight warnings (`preflight.rs`) and quantitative pre/post `StructuralInventory` fidelity reporting into `UniversalConversionEngine` (`lib.rs`, `FR-FID-001`..`002`), pass `cargo fmt --all -- --check`, native & WASM `clippy -- -D warnings`, `cargo test --workspace --locked` (101 tests), and WASM-featured tests (50 tests across 12 crates), backfill `0160de1` (`stage-3.1.1`), obtain Agent 12 `PASS` audit, commit, tag `stage-3.1.2`, and push. |
| **Test Results** | `cargo fmt --all -- --check`: PASS; `cargo clippy --workspace --all-targets -- -D warnings`: PASS (0 warnings); `cargo clippy --no-default-features --features wasm --locked -- -D warnings` (15 crates): PASS (0 warnings); `cargo test --workspace --locked`: 101 passed, 0 failed (`codec-registry`: 3, `compression-archive`: 5, `compression-audio`: 4, `compression-core`: 3, `compression-image`: 3, `compression-office`: 3, `compression-pdf`: 4, `compression-video`: 4, `conversion-core`: 6, `file-core`: 28, `filefusion` unit: 1, `cli_smoke`: 7, `hardware-acceleration`: 3, `job-manager`: 16, `optimization-engine`: 4, `quality-engine`: 2, `validation-engine`: 5); WASM-featured unit tests (`compression-archive`: 5, `compression-audio`: 3, `compression-core`: 3, `compression-image`: 3, `compression-office`: 3, `compression-pdf`: 3, `compression-video`: 3, `conversion-core`: 6, `hardware-acceleration`: 3, `job-manager`: 12, `optimization-engine`: 4, `quality-engine`: 2): 50 passed, 0 failed; `Cargo.lock` diff: zero external crate additions; Agent 12 independent audit: **PASS**. |
| **Files Changed** | `crates/conversion-core/src/{lib.rs,graph.rs,preflight.rs,markup.rs,tabular.rs}`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `6ea5132` (tag `stage-3.1.2`, pushed to `origin/main`) |
| **Outstanding Issues** | None |

### Subphase 3.1.1 — Conversion Core foundation, multi-hop graph router & cross-media conversion (`conversion-core`)

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 3 — Universal Conversion Engine |
| **Phase** | Phase 3.1 — Conversion Graph & Structured Text / Markup Engines |
| **Subphase** | `3.1.1` — Conversion Core foundation, multi-hop graph router & cross-media conversion (`conversion-core`) |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 4 (Document & Conversion Engineer), Agent 3 (Compression & Codec Engineer), Agent 2 (Rust Core Engineer), Agent 8 (QA & Automation Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 3 hours |
| **Actual Duration** | ~20 minutes |
| **Acceptance Criteria** | 1. Materialize L4 `crates/conversion-core` (`#![forbid(unsafe_code)]`, `FF-MOD-001` §4 DAG `L4_CNV -> L1_REG & L1_VAL & L3_*` with optional `compression-video` gated under `native-process` for WASM purity) with zero new external crates in `Cargo.lock`.<br>2. Implement `ConversionGraph` weighted Dijkstra direct-first / lossless-hub multi-hop router (`graph.rs`), pre-flight compatibility & feature-loss warning evaluator + structural markup inventory & `ConversionFidelityReport` builder (`preflight.rs`, `FR-FID-001`..`002`), and `UniversalConversionEngine` implementing `ConversionEngine` (`lib.rs`, `FF-CTR-001` §3.2) for pure-Rust cross-raster (`JPEG`/`PNG`/`WebP`/`AVIF`/`GIF`/`BMP`/`TIFF`/`ICO`), cross-archive (`Zip`/`Tar`/`Gzip`/`Zstd`/`Xz`/`Bzip2`/`Lz4`/`SevenZ`), cross-audio (`-> WAV`/`FLAC`), plus `native-process`-gated `convert_with_optional_runner` for Tier 2 `FFmpeg` video/audio routes.<br>3. Pass `cargo fmt --all -- --check`, native & WASM `clippy -- -D warnings`, `cargo test --workspace --locked` (99 tests), WASM-featured unit tests (48 tests across 12 crates including `conversion-core`), backfill `8f69a4c` (`stage-2.3.2`), append `TODO.md` / `CHANGELOG.md`, obtain Agent 12 `PASS` audit, commit, tag `stage-3.1.1`, and push. |
| **Test Results** | `cargo fmt --all -- --check`: PASS; `cargo clippy --workspace --all-targets -- -D warnings`: PASS (0 warnings); `cargo clippy --no-default-features --features wasm --locked -- -D warnings` (15 crates including `conversion-core`): PASS (0 warnings); `cargo test --workspace --locked`: 99 passed, 0 failed (`codec-registry`: 3, `compression-archive`: 5, `compression-audio`: 4, `compression-core`: 3, `compression-image`: 3, `compression-office`: 3, `compression-pdf`: 4, `compression-video`: 4, `conversion-core`: 4, `file-core`: 28, `filefusion` unit: 1, `cli_smoke`: 7, `hardware-acceleration`: 3, `job-manager`: 16, `optimization-engine`: 4, `quality-engine`: 2, `validation-engine`: 5); WASM-featured unit tests (`compression-archive`: 5, `compression-audio`: 3, `compression-core`: 3, `compression-image`: 3, `compression-office`: 3, `compression-pdf`: 3, `compression-video`: 3, `conversion-core`: 4, `hardware-acceleration`: 3, `job-manager`: 12, `optimization-engine`: 4, `quality-engine`: 2): 48 passed, 0 failed; `Cargo.lock` new crate diff: only `+name = "conversion-core"`; Agent 12 independent audit: **PASS**. |
| **Files Changed** | `Cargo.toml`, `Cargo.lock`, `crates/conversion-core/{Cargo.toml,src/{lib.rs,graph.rs,preflight.rs}}`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `0160de1` (tag `stage-3.1.1`, pushed to `origin/main`) |
| **Outstanding Issues** | None |

### Subphase 2.3.2 — Stage 2 CLI (`compress`/`archive`) & Criterion benchmarks

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 2 — Universal Compression Engine |
| **Phase** | Phase 2.3 — Adaptive Optimization & Benchmarks |
| **Subphase** | `2.3.2` — Stage 2 CLI (`compress`/`archive`) & Criterion benchmarks |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 2 (Rust Core Engineer), Agent 3 (Compression & Codec Engineer), Agent 8 (QA & Automation Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 3 hours |
| **Actual Duration** | ~20 minutes |
| **Acceptance Criteria** | 1. Wire L5 `crates/job-manager` to L4 `crates/optimization-engine` strictly matching `FF-MOD-001` §4 (`L5_JOB -> L0 & L1_REG & L4_OPT`) with `native-process` and `wasm` feature parity; expose `JobManager::compress_file`, `create_archive`, `list_archive`, `extract_archive`, and `run_quick_benchmark`.<br>2. Implement thin Plane 1 CLI subcommands `filefusion compress`, `filefusion archive {create,list,extract}`, and `filefusion bench` in `apps/cli/src/main.rs` (`MOD-RULE-03` compliant, structured `--json` stdout + stderr tracing) backed by real E2E tests in `apps/cli/tests/cli_smoke.rs`.<br>3. Materialize permanent Criterion benchmark suite (`Criterion 0.6.0`, permissive `Apache-2.0 OR MIT`) in `tests/benchmarks/stage2_compression.rs` measuring FastCDC, Zstd, oxipng, and the JPEG SSIM solver on host CPU, recording empirical results in `PERFORMANCE.md`.<br>4. Pass `cargo fmt --all -- --check`, native & WASM `clippy -- -D warnings`, `cargo test --workspace --locked` (95 tests), WASM-featured unit tests (44 tests), `cargo bench --bench stage2_compression`, verify `Cargo.lock` SPDX licenses, backfill `c0bfe41` (`stage-2.3.1`), append `TODO.md` / `CHANGELOG.md`, obtain Agent 12 `PASS` audit, commit, tag `stage-2.3.2`, and push. |
| **Test Results** | `cargo fmt --all -- --check`: PASS; `cargo clippy --workspace --all-targets -- -D warnings`: PASS (0 warnings); `cargo clippy --no-default-features --features wasm --locked -- -D warnings` (14 crates): PASS (0 warnings); `cargo test --workspace --locked`: 95 passed, 0 failed (`codec-registry`: 3, `compression-archive`: 5, `compression-audio`: 4, `compression-core`: 3, `compression-image`: 3, `compression-office`: 3, `compression-pdf`: 4, `compression-video`: 4, `file-core`: 28, `filefusion` unit: 1, `cli_smoke`: 7, `hardware-acceleration`: 3, `job-manager`: 16, `optimization-engine`: 4, `quality-engine`: 2, `validation-engine`: 5); WASM-featured unit tests (`compression-archive`: 5, `compression-audio`: 3, `compression-core`: 3, `compression-image`: 3, `compression-office`: 3, `compression-pdf`: 3, `compression-video`: 3, `hardware-acceleration`: 3, `job-manager`: 12, `optimization-engine`: 4, `quality-engine`: 2): 44 passed, 0 failed; `cargo bench -p optimization-engine --bench stage2_compression`: PASS (4/4 workloads); `Cargo.lock` SPDX audit: PASS (0 disallowed licenses); Agent 12 independent audit: **PASS**. |
| **Files Changed** | `Cargo.toml`, `Cargo.lock`, `crates/optimization-engine/{Cargo.toml,src/{lib.rs,dispatcher.rs}}`, `crates/job-manager/{Cargo.toml,src/lib.rs}`, `apps/cli/src/main.rs`, `apps/cli/tests/cli_smoke.rs`, `tests/benchmarks/stage2_compression.rs`, `PERFORMANCE.md`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `8f69a4c` (tag `stage-2.3.2`, pushed to `origin/main`) |
| **Outstanding Issues** | None |

### Subphase 2.3.1 — Adaptive optimization engine & quality solver (`optimization-engine`)

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 2 — Universal Compression Engine |
| **Phase** | Phase 2.3 — Adaptive Optimization & Benchmarks |
| **Subphase** | `2.3.1` — Adaptive optimization engine & quality solver (`optimization-engine`) |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 3 (Compression & Codec Engineer), Agent 2 (Rust Core Engineer), Agent 8 (QA & Automation Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 3 hours |
| **Actual Duration** | ~20 minutes |
| **Acceptance Criteria** | 1. Materialize L4 `crates/optimization-engine` (`#![forbid(unsafe_code)]`, `FF-MOD-001` §4 DAG `L4_OPT -> L1_HW & L3_IMG & L3_VID & L3_AUD & L3_PDF & L3_OFF & L3_ARC`, `FR-ADP-001`..`002`, `FR-CMP-003`, `FR-DUP-002`, `FR-DET-004`) with zero new external dependencies in `Cargo.lock`.<br>2. Implement `recommend_codec` (`recommender.rs`) analyzing inspected format, entropy, alpha/metadata privacy risk, and `HardwareProfile` (`select_video_backend` HW-vs-deterministic-CPU fallback); implement bounded quality-constrained binary solver `solve_quantizer_binary_search` / `solve_jpeg_quality_target` / `solve_and_stage_jpeg` (`solver.rs`) minimizing output bytes subject to `min_ssim`, `min_psnr_db`, and `target_max_bytes` with cancellation checks.<br>3. Implement FastCDC Gear-hash sub-file deduplication analyzer (`chunk_bytes_fastcdc`, `analyze_files_fastcdc`) and perceptual `dHash` / 2D-DCT `pHash` hashing (`dedup.rs`), plus unified `OptimizationDispatcher` (`dispatcher.rs`) coordinating all 6 L3 format engines cleanly across native (`native-process`) and `wasm` feature builds.<br>4. Pass `cargo fmt --all -- --check`, native & WASM `clippy -- -D warnings`, `cargo test --workspace --locked` (94 tests), WASM-featured unit tests (44 tests), backfill `19c5a04` (`stage-2.2.5`), append `TODO.md` / `CHANGELOG.md`, obtain Agent 12 `PASS` audit, commit, tag `stage-2.3.1`, and push. |
| **Test Results** | `cargo fmt --all -- --check`: PASS; `cargo clippy --workspace --all-targets -- -D warnings`: PASS (0 warnings); `cargo clippy --no-default-features --features wasm --locked -- -D warnings` (14 crates including `optimization-engine`): PASS (0 warnings); `cargo clippy -p optimization-engine --all-targets --no-default-features --features wasm --locked -- -D warnings`: PASS (0 warnings); `cargo test --workspace --locked`: 94 passed, 0 failed (`codec-registry`: 3, `compression-archive`: 5, `compression-audio`: 4, `compression-core`: 3, `compression-image`: 3, `compression-office`: 3, `compression-pdf`: 4, `compression-video`: 4, `file-core`: 28, `filefusion` unit: 1, `cli_smoke`: 6, `hardware-acceleration`: 3, `job-manager`: 16, `optimization-engine`: 4, `quality-engine`: 2, `validation-engine`: 5); WASM-featured unit tests (`compression-archive`: 5, `compression-audio`: 3, `compression-core`: 3, `compression-image`: 3, `compression-office`: 3, `compression-pdf`: 3, `compression-video`: 3, `hardware-acceleration`: 3, `job-manager`: 12, `optimization-engine`: 4, `quality-engine`: 2): 44 passed, 0 failed; Agent 12 independent audit: **PASS**. |
| **Files Changed** | `Cargo.toml`, `Cargo.lock`, `crates/optimization-engine/Cargo.toml`, `crates/optimization-engine/src/{lib.rs,recommender.rs,solver.rs,dedup.rs,dispatcher.rs}`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `c0bfe41` (tag `stage-2.3.1`, pushed to `origin/main`) |
| **Outstanding Issues** | None |

### Subphase 2.2.5 — PDF & Office compression crates (`compression-pdf`, `compression-office`)

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 2 — Universal Compression Engine |
| **Phase** | Phase 2.2 — Format Compression Engines |
| **Subphase** | `2.2.5` — PDF & Office compression crates (`compression-pdf`, `compression-office`) |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 3 (Compression & Codec Engineer), Agent 4 (Document & Conversion Engineer), Agent 8 (QA & Automation Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 4 hours |
| **Actual Duration** | ~20 minutes |
| **Acceptance Criteria** | 1. Materialize L3 `crates/compression-pdf` (`#![forbid(unsafe_code)]`, `FF-MOD-001` §4 DAG `L3_PDF -> L2_CMP & L1_REG & L3_IMG`, `FR-PDF-001`..`002`, `FR-REP-001`..`002`, `FR-MET-002`, `FR-CMP-001`..`002`) using MIT-licensed `lopdf` (`0.42`, `default-features = false`, MSRV `1.85`), `flate2`, and shared `compression-image::optimize_raster_in_memory` for embedded PDF image XObjects, Flate/zlib stream compaction, unused object pruning, `/Info`/`XMP` privacy scrubbing, honest `PdfRepairAssessment` (`Healthy`/`FullyRepairable`/`PartiallySalvageable`/`Unrecoverable`), and `native-process`-gated Tier 2 `qpdf` `IsolatedProcessRunner` support.<br>2. Materialize L3 `crates/compression-office` (`#![forbid(unsafe_code)]`, `FF-MOD-001` §4 DAG `L3_OFF -> L2_CMP & L1_REG & L3_IMG`, `FR-DOC-002`..`003`, `FR-SEC-001`, `FR-MET-002`) supporting `DOCX`, `XLSX`, `PPTX`, `ODT`, `ODS`, `ODP`, and `EPUB` with `validation-engine` Zip-Slip & decompression-bomb enforcement, embedded raster optimization via `compression-image`, lossless XML whitespace/comment minification preserving `<w:t xml:space="preserve">`, privacy scrubbing (`docProps/*`, `meta.xml`, `.opf`), spec-compliant `mimetype`-first `Stored` packaging, and `FR-CMP-002` No-Op Guard.<br>3. Pass `cargo fmt --all -- --check`, native & WASM `clippy -- -D warnings`, `cargo test --workspace --locked` (90 tests), WASM-featured unit tests (40 tests), verify permissive `Cargo.lock` SPDX licenses, backfill `2660aed` (`stage-2.2.4`), append `TODO.md` / `CHANGELOG.md`, obtain Agent 12 `PASS` audit, commit, tag `stage-2.2.5`, and push. |
| **Test Results** | `cargo fmt --all -- --check`: PASS; `cargo clippy --workspace --all-targets -- -D warnings`: PASS (0 warnings); `cargo clippy --no-default-features --features wasm --locked -- -D warnings` (13 crates including `compression-pdf`, `compression-office`): PASS (0 warnings); `cargo clippy -p compression-pdf -p compression-office --all-targets --no-default-features --features wasm --locked -- -D warnings`: PASS (0 warnings); `cargo test --workspace --locked`: 90 passed, 0 failed (`codec-registry`: 3, `compression-archive`: 5, `compression-audio`: 4, `compression-core`: 3, `compression-image`: 3, `compression-office`: 3, `compression-pdf`: 4, `compression-video`: 4, `file-core`: 28, `filefusion` unit: 1, `cli_smoke`: 6, `hardware-acceleration`: 3, `job-manager`: 16, `quality-engine`: 2, `validation-engine`: 5); WASM-featured unit tests (`compression-archive`: 5, `compression-audio`: 3, `compression-core`: 3, `compression-image`: 3, `compression-office`: 3, `compression-pdf`: 3, `compression-video`: 3, `hardware-acceleration`: 3, `job-manager`: 12, `quality-engine`: 2): 40 passed, 0 failed; `Cargo.lock` SPDX check: PASS (0 disallowed licenses); Agent 12 independent audit: **PASS**. |
| **Files Changed** | `Cargo.toml`, `Cargo.lock`, `crates/compression-image/src/lib.rs`, `crates/compression-pdf/Cargo.toml`, `crates/compression-pdf/src/{lib.rs,optimizer.rs,repair.rs}`, `crates/compression-office/Cargo.toml`, `crates/compression-office/src/{lib.rs,container.rs,xml.rs}`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `19c5a04` (tag `stage-2.2.5`, pushed to `origin/main`) |
| **Outstanding Issues** | None |

### Subphase 2.2.4 — Video compression crate (`compression-video`)

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 2 — Universal Compression Engine |
| **Phase** | Phase 2.2 — Format Compression Engines |
| **Subphase** | `2.2.4` — Video compression crate (`compression-video`) |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 3 (Compression & Codec Engineer), Agent 9 (Security Engineer), Agent 8 (QA & Automation Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 3 hours |
| **Actual Duration** | ~20 minutes |
| **Acceptance Criteria** | 1. Materialize L3 `crates/compression-video` (`#![forbid(unsafe_code)]`, `FF-MOD-001` §4 DAG `L3_VID -> L2_CMP & L1_REG & L1_HW`, `FR-AV-001`, `FR-AV-003`, `FR-ADP-002`, `FR-CMP-001`..`002`), depending only on `L0`–`L2` (`file-core`, `codec-registry`, `hardware-acceleration`, `quality-engine`, `validation-engine`, `compression-core`, `serde`, `serde_json` — zero new third-party dependencies in `Cargo.lock`).<br>2. Implement `ffprobe` JSON inventory parsing (`parse_ffprobe_json_probe`, `VideoContainerInventory`), lossless container fast-path remux compatibility (`can_fast_path_remux`), unfinalized container atom health check (`diagnose_unfinalized_video_container`), pure-Rust SRT <-> WebVTT subtitle parser & converter (`parse_srt_cues`, `parse_webvtt_cues`, `convert_subtitle_text`, `build_ffmpeg_extract_subtitle_invocation`), hardware-vs-CPU routing (`select_video_backend` for `H264`/`Hevc`/`Vp9`/`Av1`), explicit `-f <muxer>` staged `FFmpeg` invocations via `IsolatedProcessRunner` (`build_ffmpeg_video_transcode_invocation`), `libvmaf` parser (`build_ffmpeg_vmaf_invocation`, `parse_ffmpeg_vmaf_score`), and `VideoCompressionEngine` (`CompressionEngine` + `compress_with_runner`) enforcing `FR-CMP-002` No-Op Guard.<br>3. Pass `cargo fmt --all -- --check`, native & WASM `clippy -- -D warnings`, `cargo test --workspace --locked` (83 tests), WASM-featured unit tests (34 tests), verify zero unapproved additions in `Cargo.lock`, backfill `ee5957b` (`stage-2.2.3`), append `TODO.md` / `CHANGELOG.md`, obtain Agent 12 `PASS` audit, commit, tag `stage-2.2.4`, and push. |
| **Test Results** | `cargo fmt --all -- --check`: PASS; `cargo clippy --workspace --all-targets -- -D warnings`: PASS (0 warnings); `cargo clippy --no-default-features --features wasm --locked -- -D warnings` (`file-core`, `codec-registry`, `hardware-acceleration`, `quality-engine`, `validation-engine`, `compression-core`, `compression-image`, `compression-archive`, `compression-audio`, `compression-video`, `job-manager`): PASS (0 warnings); `cargo clippy -p compression-video --all-targets --no-default-features --features wasm --locked -- -D warnings`: PASS (0 warnings); `cargo test --workspace --locked`: 83 passed, 0 failed (`codec-registry`: 3, `compression-archive`: 5, `compression-audio`: 4, `compression-core`: 3, `compression-image`: 3, `compression-video`: 4, `file-core`: 28, `filefusion` unit: 1, `cli_smoke`: 6, `hardware-acceleration`: 3, `job-manager`: 16, `quality-engine`: 2, `validation-engine`: 5); WASM-featured unit tests (`compression-archive`: 5, `compression-audio`: 3, `compression-core`: 3, `compression-image`: 3, `compression-video`: 3, `hardware-acceleration`: 3, `job-manager`: 12, `quality-engine`: 2): 34 passed, 0 failed; Agent 12 independent audit: **PASS**. |
| **Files Changed** | `Cargo.toml`, `Cargo.lock`, `crates/compression-video/Cargo.toml`, `crates/compression-video/src/{lib.rs,probe.rs,subtitles.rs,pipeline.rs}`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `2660aed` (tag `stage-2.2.4`, pushed to `origin/main`) |
| **Outstanding Issues** | None |

### Subphase 2.2.3 — Audio compression crate (`compression-audio`)

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 2 — Universal Compression Engine |
| **Phase** | Phase 2.2 — Format Compression Engines |
| **Subphase** | `2.2.3` — Audio compression crate (`compression-audio`) |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 3 (Compression & Codec Engineer), Agent 8 (QA & Automation Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 3 hours |
| **Actual Duration** | ~15 minutes |
| **Acceptance Criteria** | 1. Materialize L3 `crates/compression-audio` (`#![forbid(unsafe_code)]`, `FF-MOD-001` §4 DAG `L2 -> L3`, `FR-AV-002`, `FR-AV-003`, `FR-CMP-001`..`002`, `FR-MET-001`..`002`) implementing pure-Rust audio decoding (`WAV` via `hound`, `FLAC` via `claxon`, `MP3`/`Ogg/Vorbis`/`AAC`/`M4A`/`ALAC`/`MKV` via `symphonia`) and lossless `WAV` / `FLAC` (`flacenc`) encoding tuned to `CompressionProfile`.<br>2. Implement DSP sample-rate conversion (`resample_pcm`), channel layout remapping (`remap_channels`), and ITU-R BS.1770 / EBU R128 K-weighted gated loudness measurement & true-peak-limited normalization (`measure_integrated_lufs`, `normalize_loudness_lufs`).<br>3. Implement ID3v1/v2 tag inspection, writing, and privacy stripping (`read_id3_metadata`, `write_id3_metadata`, `strip_id3_metadata`, `has_non_essential_metadata`), `AudioQualityEvaluator` (PCM bit-exactness & time-domain waveform PSNR), `AudioCompressionEngine` (`FR-CMP-002` No-Op Guard), and `native-process`-gated Tier 2 `FFmpeg` `IsolatedProcessRunner` helpers (`build_ffmpeg_audio_transcode_invocation`, `build_ffmpeg_extract_audio_invocation`, `execute_ffmpeg_invocation`).<br>4. Pass `cargo fmt --all -- --check`, native & WASM `clippy -- -D warnings`, `cargo test --workspace --locked` (79 tests), WASM-featured unit tests (31 tests), verify permissive `Cargo.lock` licenses, backfill `ba2658d` (`stage-2.2.2`), append `TODO.md` / `CHANGELOG.md`, pass Agent 12 audit, commit, tag `stage-2.2.3`, and push. |
| **Test Results** | `cargo fmt --all -- --check`: PASS (`FMT_EXIT:0`); `cargo clippy --workspace --all-targets -- -D warnings`: PASS (0 warnings); `cargo clippy --no-default-features --features wasm --locked -- -D warnings` (`file-core`, `codec-registry`, `hardware-acceleration`, `quality-engine`, `validation-engine`, `compression-core`, `compression-image`, `compression-archive`, `compression-audio`, `job-manager`): PASS (0 warnings); `cargo test --workspace --locked`: 79 passed, 0 failed (`codec-registry`: 3, `compression-archive`: 5, `compression-audio`: 4, `compression-core`: 3, `compression-image`: 3, `file-core`: 28, `filefusion` unit: 1, `cli_smoke`: 6, `hardware-acceleration`: 3, `job-manager`: 16, `quality-engine`: 2, `validation-engine`: 5); WASM-featured unit tests (`compression-archive`: 5, `compression-audio`: 3, `compression-core`: 3, `compression-image`: 3, `hardware-acceleration`: 3, `job-manager`: 12, `quality-engine`: 2): 31 passed, 0 failed; Agent 12 independent audit: **PASS**. |
| **Files Changed** | `Cargo.toml`, `Cargo.lock`, `crates/compression-audio/Cargo.toml`, `crates/compression-audio/src/{lib.rs,dsp.rs,tags.rs,ffmpeg_adapter.rs}`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `ee5957b` (tag `stage-2.2.3`, pushed to `origin/main`) |
| **Outstanding Issues** | None |

### Subphase 2.2.2 — Archive compression crate (`compression-archive`)

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 2 — Universal Compression Engine |
| **Phase** | Phase 2.2 — Format Compression Engines |
| **Subphase** | `2.2.2` — Archive compression crate (`compression-archive`) |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 3 (Compression & Codec Engineer), Agent 9 (Security Engineer), Agent 8 (QA & Automation Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 4 hours |
| **Actual Duration** | ~25 minutes |
| **Acceptance Criteria** | 1. Materialize L3 `crates/compression-archive` (`#![forbid(unsafe_code)]`, `FF-MOD-001` §4 DAG `L2 -> L3`, `FR-ARC-001`..`003`, `FR-SEC-001`..`002`, `FR-REP-001`) supporting multi-format archive creation, listing, and bomb/traversal-safe extraction across `Zip`, `Tar`, `TarGz`, `TarZst`, `TarXz`, `TarBz2`, `TarLz4`, `TarBrotli`, and `SevenZ` (`sevenz-rust` + optional `native-process` Tier 2 `7z` via `IsolatedProcessRunner`).<br>2. Implement high-ratio standalone stream codecs (`StreamCodec`: `Zstd`, `Lz4`, `Brotli`, `Gzip`, `Xz` via `lzma-rust2`, `Bzip2` via `oxiarc-bzip2`) with mid-stream decompression-bomb enforcement (`validation_engine`).<br>3. Implement `.ffenc` authenticated encryption containers (`Aes256Gcm` & `ChaCha20Poly1305` + `Argon2id` + CSPRNG `getrandom` salt/nonce + header AAD binding) and ZIP central-directory recovery (`repair_truncated_zip`), plus `ArchiveCompressionEngine` (`FR-CMP-002` No-Op guard) and `ArchiveQualityEvaluator` (bit-exact BLAKE3 entry/stream equivalence).<br>4. Pass `cargo fmt --all -- --check`, native & WASM `clippy -- -D warnings`, `cargo test --workspace --locked` (75 tests), WASM-featured unit tests (28 tests), verify permissive `Cargo.lock` licenses, backfill `25d97e5` (`stage-2.2.1`), append `TODO.md` / `CHANGELOG.md`, pass Agent 12 audit, commit, tag `stage-2.2.2`, and push. |
| **Test Results** | `cargo fmt --all -- --check`: PASS (`FMT_EXIT:0`); `cargo clippy --workspace --all-targets -- -D warnings`: PASS (0 warnings); `cargo clippy --no-default-features --features wasm --locked -- -D warnings` (`file-core`, `codec-registry`, `hardware-acceleration`, `quality-engine`, `validation-engine`, `compression-core`, `compression-image`, `compression-archive`, `job-manager`): PASS (0 warnings); `cargo test --workspace --locked`: 75 passed, 0 failed (`codec-registry`: 3, `compression-archive`: 5, `compression-core`: 3, `compression-image`: 3, `file-core`: 28, `filefusion` unit: 1, `cli_smoke`: 6, `hardware-acceleration`: 3, `job-manager`: 16, `quality-engine`: 2, `validation-engine`: 5); WASM-featured unit tests (`compression-archive`: 5, `compression-core`: 3, `compression-image`: 3, `hardware-acceleration`: 3, `job-manager`: 12, `quality-engine`: 2): 28 passed, 0 failed; Agent 12 independent audit: **PASS**. |
| **Files Changed** | `Cargo.toml`, `Cargo.lock`, `crates/compression-archive/Cargo.toml`, `crates/compression-archive/src/{lib.rs,streams.rs,container.rs,ffenc.rs,repair.rs}`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `ba2658d` (tag `stage-2.2.2`, pushed to `origin/main`) |
| **Outstanding Issues** | None |

### Subphase 2.2.1 — Image compression crate (`compression-image`)

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 2 — Universal Compression Engine |
| **Phase** | Phase 2.2 — Format Compression Engines |
| **Subphase** | `2.2.1` — Image compression crate (`compression-image`) |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 3 (Compression & Codec Engineer), Agent 8 (QA & Automation Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 4 hours |
| **Actual Duration** | ~20 minutes |
| **Acceptance Criteria** | 1. Materialize L3 `crates/compression-image` (`#![forbid(unsafe_code)]`, `FF-MOD-001` §4 DAG `L2 -> L3`, `FR-IMG-001`..`003`) supporting pure-Rust decode (`JPEG`, `PNG`, `WebP`, `GIF`, `BMP`, `TIFF`, `ICO`) and encode (`JPEG` via `image::codecs::jpeg::JpegEncoder`, `PNG` via `oxipng`, lossless `WebP` via `image::codecs::webp::WebPEncoder`, `AVIF` via `ravif`, plus `GIF`/`BMP`/`TIFF`/`ICO`).<br>2. Implement EXIF TIFF orientation (`0x0112`, values `1..=8`) detection and normalization (`detect_exif_orientation`, `apply_exif_orientation`) and SIMD-accelerated resizing via `fast_image_resize` (`ResizeFilter`, `ImageResizeSpec`, aspect-ratio preservation).<br>3. Implement `RasterQualityEvaluator` (`file_core::QualityEvaluator`) using `quality_engine::evaluate_pixel_buffers`, and `RasterCompressionEngine` (`file_core::CompressionEngine`) + `compress_with_options` with `JobContext::reserve_working_set_mb`, `compression_core::finalize_staged_compression` (`FR-CMP-002` No-Op guard & `MagicByteOutputValidator`), and atomic cleanup.<br>4. Pass `cargo fmt --all -- --check`, native & WASM `clippy -- -D warnings`, `cargo test --workspace --locked` (70 tests), WASM unit tests (23 tests), verify permissive `Cargo.lock` licenses, backfill `a702338` (`stage-2.1.2`), append `TODO.md` / `CHANGELOG.md`, pass Agent 12 audit, commit, tag `stage-2.2.1`, and push. |
| **Test Results** | `cargo fmt --all -- --check`: PASS (`FMT_EXIT:0`); `cargo clippy --workspace --all-targets -- -D warnings`: PASS (0 warnings); `cargo clippy --no-default-features --features wasm --locked -- -D warnings` (`file-core`, `codec-registry`, `hardware-acceleration`, `quality-engine`, `validation-engine`, `compression-core`, `compression-image`, `job-manager`): PASS (0 warnings); `cargo test --workspace --locked`: 70 passed, 0 failed (`codec-registry`: 3, `compression-core`: 3, `compression-image`: 3, `file-core`: 28, `filefusion` unit: 1, `cli_smoke`: 6, `hardware-acceleration`: 3, `job-manager`: 16, `quality-engine`: 2, `validation-engine`: 5); WASM-featured unit tests (`compression-core`: 3, `compression-image`: 3, `hardware-acceleration`: 3, `job-manager`: 12, `quality-engine`: 2): 23 passed, 0 failed; Agent 12 independent audit: **PASS**. |
| **Files Changed** | `Cargo.toml`, `Cargo.lock`, `crates/compression-image/Cargo.toml`, `crates/compression-image/src/lib.rs`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `25d97e5` (tag `stage-2.2.1`, pushed to `origin/main`) |
| **Outstanding Issues** | None |

### Subphase 2.1.2 — Hardware acceleration probe

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 2 — Universal Compression Engine |
| **Phase** | Phase 2.1 — Compression Foundation & Quality Metrics |
| **Subphase** | `2.1.2` — Hardware acceleration probe |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 3 (Compression & Codec Engineer), Agent 2 (Rust Core Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 2 hours |
| **Actual Duration** | ~10 minutes |
| **Acceptance Criteria** | 1. Materialize L1 `crates/hardware-acceleration` (`#![forbid(unsafe_code)]`, `FF-MOD-001` §3/§6 `hw-accel` vs zero-warning `wasm` no-op stub) implementing `SimdFeatures::detect()` (`SSE4.2`, `AVX2`, `AVX-512F`, `NEON`, `wasm_simd128`), non-blocking GPU driver discovery (`NVENC`, `QSV`, `AMF`, `VideoToolbox`, `MediaCodec`, `VAAPI`), `parse_ffmpeg_hwaccels`, `IsolatedProcessRunner`-backed `probe_ffmpeg_hwaccels`, and `select_video_backend` CPU fallback router (`FR-ADP-002`).<br>2. Expose `probe_hardware_acceleration`, `select_video_backend`, and `JobManager::probe_hardware` in `crates/job-manager/{Cargo.toml,src/lib.rs}`.<br>3. Pass `cargo fmt --all -- --check`, native & WASM `clippy -- -D warnings`, `cargo test --workspace --locked` (67 tests), WASM unit tests (20 tests), backfill `333a66f` (`stage-2.1.1`), append `TODO.md` / `CHANGELOG.md`, pass Agent 12 audit, commit, tag `stage-2.1.2`, and push. |
| **Test Results** | `cargo fmt --all -- --check`: PASS (`FMT_EXIT:0`); `cargo clippy --workspace --all-targets -- -D warnings`: PASS (0 warnings); `cargo clippy --no-default-features --features wasm --locked -- -D warnings` (`file-core`, `codec-registry`, `hardware-acceleration`, `quality-engine`, `validation-engine`, `compression-core`, `job-manager`): PASS (0 warnings); `cargo test --workspace --locked`: 67 passed, 0 failed (`codec-registry`: 3, `compression-core`: 3, `file-core`: 28, `filefusion` unit: 1, `cli_smoke`: 6, `hardware-acceleration`: 3, `job-manager`: 16, `quality-engine`: 2, `validation-engine`: 5); WASM unit tests (`compression-core`: 3, `hardware-acceleration`: 3, `job-manager`: 12, `quality-engine`: 2): 20 passed, 0 failed; Agent 12 independent audit: **PASS**. |
| **Files Changed** | `Cargo.toml`, `Cargo.lock`, `crates/hardware-acceleration/Cargo.toml`, `crates/hardware-acceleration/src/lib.rs`, `crates/job-manager/Cargo.toml`, `crates/job-manager/src/lib.rs`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `a702338` (tag `stage-2.1.2`, pushed to `origin/main`) |
| **Outstanding Issues** | None |

### Subphase 2.1.1 — Compression Core & Quality Engine

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 2 — Universal Compression Engine |
| **Phase** | Phase 2.1 — Compression Foundation & Quality Metrics |
| **Subphase** | `2.1.1` — Compression Core & Quality Engine |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 3 (Compression & Codec Engineer), Agent 2 (Rust Core Engineer), Agent 8 (QA & Automation Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 3 hours |
| **Actual Duration** | ~15 minutes |
| **Acceptance Criteria** | 1. Materialize L1 `crates/quality-engine` (`FF-MOD-001` §4 DAG, `#![forbid(unsafe_code)]`, WASM-ready) implementing windowed SSIM (`compute_ssim`), PSNR dB (`compute_psnr_db` / `LOSSLESS_PSNR_CEILING_DB`), `diff_pixel_buffers`, bounded `files_are_bit_exact`, and `BitExactQualityEvaluator` (`QualityEvaluator`), and register in `BackendCatalog::with_defaults()`.<br>2. Materialize L2 `crates/compression-core` (`FR-CMP-001`..`002`, `FF-CTR-001` §3.1, `ADR-007`) implementing `CompressionProfileConfig` across all 5 standard presets, `prescreen_incompressible_noop`, `finalize_staged_compression` (mandatory No-Op inflation guard + `OutputValidator` + optional `min_ssim`/`target_max_bytes` verification), and atomic `commit_compression_stage`.<br>3. Pass `cargo fmt --all -- --check`, native & WASM `clippy -- -D warnings`, `cargo test --workspace --locked` (64 tests), WASM unit tests (17 tests), backfill `9db02eb` (`stage-1.3.5`), add Stage 2 queue in `docs/PROGRESS.md`, append `TODO.md` / `CHANGELOG.md`, pass Agent 12 audit, commit, tag `stage-2.1.1`, and push. |
| **Test Results** | `cargo fmt --all -- --check`: PASS (`FMT_EXIT:0`); `cargo clippy --workspace --all-targets -- -D warnings`: PASS (0 warnings); `cargo clippy --no-default-features --features wasm --locked -- -D warnings` (`file-core`, `codec-registry`, `quality-engine`, `validation-engine`, `compression-core`, `job-manager`): PASS (0 warnings); `cargo test --workspace --locked`: 64 passed, 0 failed (`codec-registry`: 3, `compression-core`: 3, `file-core`: 28, `filefusion`: 1, `cli_smoke`: 6, `job-manager`: 16, `quality-engine`: 2, `validation-engine`: 5); WASM unit tests (`compression-core`: 3, `job-manager`: 12, `quality-engine`: 2): 17 passed, 0 failed; Agent 12 independent audit: **PASS**. |
| **Files Changed** | `Cargo.toml`, `Cargo.lock`, `crates/quality-engine/Cargo.toml`, `crates/quality-engine/src/lib.rs`, `crates/compression-core/Cargo.toml`, `crates/compression-core/src/lib.rs`, `crates/job-manager/Cargo.toml`, `crates/job-manager/src/backends.rs`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `333a66f` (tag `stage-2.1.1`, pushed to `origin/main`) |
| **Outstanding Issues** | None |

### Subphase 1.3.5 — Resource limits

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 1 — Rust Core Foundation |
| **Phase** | Phase 1.3 — Pipeline & Execution |
| **Subphase** | `1.3.5` — Resource limits |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 2 (Rust Core Engineer), Agent 9 (Security Engineer), Agent 8 (QA & Automation Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 2 hours |
| **Actual Duration** | ~15 minutes |
| **Acceptance Criteria** | 1. Implement L0 runtime resource governance in `crates/file-core/src/limits.rs`: `DeviceProfile` (`Workstation`, `Mobile`, `WebWasm`), `MemoryGovernor` & RAII `MemoryReservation`, `MemoryBudgetSnapshot`, adaptive `ResourceLimits::for_profile`, `recommended_chunk_bytes` (`64 KiB`–`1 MiB`), `clamp_workers_for_job`, `validate_file_budget`, `parse_proc_meminfo_mb`, and cancellation-aware `copy_bounded_streaming` (`NFR-PERF-001`, `NFR-PERF-003`, `FR-SEC-001`).<br>2. Wire `MemoryGovernor` into `JobContext` (`reserve_working_set_mb`), `JobScheduler` permit dispatch/release (`SchedulerController::memory_snapshot`), and `JobManager` (`memory_governor`, `memory_stats`).<br>3. Pass `cargo fmt --all -- --check`, native & WASM `clippy -- -D warnings`, `cargo test --workspace --locked` (59 tests), WASM unit tests (12 tests), backfill `15651ec` / `4aadd0a` (`stage-1.3.4`), append `TODO.md` / `CHANGELOG.md`, pass Agent 12 audit, commit, tag `stage-1.3.5`, and push. |
| **Test Results** | `cargo fmt --all -- --check`: PASS (`FMT_EXIT:0`); `cargo clippy --workspace --all-targets -- -D warnings`: PASS (0 warnings); `cargo clippy --no-default-features --features wasm --locked -- -D warnings` (`file-core`, `codec-registry`, `job-manager`): PASS (0 warnings); `cargo test --workspace --locked`: 59 passed, 0 failed (`codec-registry`: 3, `file-core`: 28, `validation-engine`: 5, `job-manager`: 16, `filefusion` unit: 1, `cli_smoke`: 6); `cargo test -p job-manager --no-default-features --features wasm --locked`: 12 passed, 0 failed; Agent 12 independent audit: **PASS**. |
| **Files Changed** | `crates/file-core/src/limits.rs`, `crates/file-core/src/lib.rs`, `crates/file-core/src/pipeline.rs`, `crates/job-manager/src/scheduler.rs`, `crates/job-manager/src/lib.rs`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `9db02eb` (tag `stage-1.3.5`, pushed to `origin/main`) |
| **Outstanding Issues** | None |

### Subphase 1.3.4 — Cancellation and progress

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 1 — Rust Core Foundation |
| **Phase** | Phase 1.3 — Pipeline & Execution |
| **Subphase** | `1.3.4` — Cancellation and progress |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 2 (Rust Core Engineer), Agent 8 (QA & Automation Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 2 hours |
| **Actual Duration** | ~15 minutes |
| **Acceptance Criteria** | 1. Upgrade L0 `CancellationToken` in `crates/file-core/src/pipeline.rs` with hierarchical parent-child linking (`child()`), reactive cancellation callbacks (`subscribe_on_cancel` / RAII `CancelSubscription`), `CallbackProgressSink`, `MonotonicProgressSink`, `ThrottledProgressSink`, and `JobContext::emit_progress_estimate` (`FF-CTR-001` §2.3, `NFR-REL-002`).<br>2. Implement multi-subscriber `ProgressBus`, `ProgressSubscription`, `ProgressBusStats`, empirical rolling `EtaTracker`, and compact NDJSON event serialization in `crates/job-manager/src/progress.rs`.<br>3. Wire `ActiveJobGuard` / `JobManager::cancel_job` / `JobManager::active_job_ids`, reactive scheduler cancellation hooks (`< 1 ms` wakeup), and monotonic lifecycle progress stages across `job-manager`, with tests verifying `< 500 ms` abort across batches and isolated subprocesses.<br>4. Pass `cargo fmt --all -- --check`, native & WASM `clippy -- -D warnings`, `cargo test --workspace --locked`, WASM unit tests, backfill `02e86d7` (`stage-1.3.3`), append `TODO.md` / `CHANGELOG.md`, pass Agent 12 audit, commit, tag `stage-1.3.4`, and push. |
| **Test Results** | `cargo fmt --all -- --check`: PASS (`FMT_EXIT:0`); `cargo clippy --workspace --all-targets -- -D warnings`: PASS (0 warnings); `cargo clippy --no-default-features --features wasm --locked -- -D warnings` (`file-core`, `codec-registry`, `job-manager`): PASS (0 warnings); `cargo test --workspace --locked`: 54 passed, 0 failed (`codec-registry`: 3, `file-core`: 24, `validation-engine`: 5, `job-manager`: 15, `filefusion` unit: 1, `cli_smoke`: 6); `cargo test -p job-manager --no-default-features --features wasm --locked`: 11 passed, 0 failed; Agent 12 independent audit: **PASS**. |
| **Files Changed** | `crates/file-core/src/pipeline.rs`, `crates/file-core/src/lib.rs`, `crates/job-manager/Cargo.toml`, `crates/job-manager/src/progress.rs`, `crates/job-manager/src/scheduler.rs`, `crates/job-manager/src/lib.rs`, `crates/job-manager/src/backends.rs`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `15651ec` (tag `stage-1.3.4`, pushed to `origin/main`; `4aadd0a` `Cargo.lock` sync) |
| **Outstanding Issues** | None |

### Subphase 1.3.3 — Job scheduler

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 1 — Rust Core Foundation |
| **Phase** | Phase 1.3 — Pipeline & Execution |
| **Subphase** | `1.3.3` — Job scheduler |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 2 (Rust Core Engineer), Agent 8 (QA & Automation Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 4 hours |
| **Actual Duration** | ~20 minutes |
| **Acceptance Criteria** | 1. Add feature-gated bounded `tokio` + `rayon` dependencies (`native-scheduler`, `rayon-parallel`, zero-warning `wasm` single-worker fallback) in `Cargo.toml` / `crates/job-manager/Cargo.toml` (`ADR-003`, `FF-MOD-001` §6).<br>2. Implement `JobScheduler`, `SchedulerController`, `JobPriority` (`High` > `Normal` > `Low` + FIFO tie-breaking), `ScheduledItemSpec`, and `SchedulerStats` in `crates/job-manager/src/scheduler.rs` enforcing `ResourceLimits::max_workers`, memory-weighted `max_memory_mb` permits (`FF-ERR-RES-MEMORY` rejection for oversized items), pause/resume, per-item cancellation, and `< 500 ms` global cancellation abort (`FR-BAT-001`, `NFR-PERF-003`, `NFR-REL-002`).<br>3. Wire `JobManager::execute_batch` / `execute_scheduled_batch` / `create_scheduler_controller` in `crates/job-manager/src/lib.rs` with 1,200+ item stress, memory-permit serialization, priority-order, pause/resume, and WASM-parity tests.<br>4. Pass `cargo fmt --all -- --check`, `clippy -- -D warnings` (native & `--features wasm`), `cargo test --workspace --locked`, WASM unit tests, backfill `b26ae47` (`stage-1.3.2`), append `TODO.md` / `CHANGELOG.md`, pass Agent 12 audit, commit, tag `stage-1.3.3`, and push. |
| **Test Results** | `cargo fmt --all -- --check`: PASS (`FMT_EXIT:0`); `cargo clippy --workspace --all-targets -- -D warnings`: PASS (0 warnings); `cargo clippy --no-default-features --features wasm --locked -- -D warnings` (`file-core`, `codec-registry`, `job-manager`): PASS (0 warnings); `cargo test --workspace --locked`: 49 passed, 0 failed (`codec-registry`: 3, `file-core`: 22, `validation-engine`: 5, `job-manager`: 12, `filefusion` unit: 1, `cli_smoke`: 6); `cargo test -p job-manager --no-default-features --features wasm --locked`: 9 passed, 0 failed; Agent 12 independent audit: **PASS**. |
| **Files Changed** | `Cargo.toml`, `Cargo.lock`, `crates/job-manager/Cargo.toml`, `crates/job-manager/src/scheduler.rs`, `crates/job-manager/src/lib.rs`, `crates/job-manager/src/backends.rs`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `02e86d7` (tag `stage-1.3.3`, pushed to `origin/main`) |
| **Outstanding Issues** | None |

### Subphase 1.3.2 — Plugin/backend contracts

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 1 — Rust Core Foundation |
| **Phase** | Phase 1.3 — Pipeline & Execution |
| **Subphase** | `1.3.2` — Plugin/backend contracts |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 2 (Rust Core Engineer), Agent 3 (Compression & Codec Engineer), Agent 4 (Document & Conversion Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 2 hours |
| **Actual Duration** | ~10 minutes |
| **Acceptance Criteria** | 1. Define canonical backend traits & DTOs in `crates/file-core/src/contracts.rs` (`FF-CTR-001` §3, `MOD-RULE-01`, `MOD-RULE-04`): `CompressionEngine`, `ConversionEngine`, `ExternalToolAdapter`, `IsolatedProcessRunner`, `QualityEvaluator`, `OutputValidator`, plus `CompressionRequest`, `CompressionOutcome`, `QualityMetrics`, `ConversionRoute`, `PreflightAssessment`, `ConversionRequest`, `ConversionFidelityReport`, `ValidationOutcome`, `ProcessInvocation`, and `ProcessOutput`.<br>2. Inject `Option<Arc<dyn IsolatedProcessRunner>>` into `JobContext` (`crates/file-core/src/pipeline.rs`) and implement L1 `MagicByteOutputValidator` (`OutputValidator`) in `crates/validation-engine/src/lib.rs`.<br>3. Implement `NativeIsolatedProcessRunner` (sanitized `env_clear()`, private RAII temp dir, argument arrays, bounded I/O capture, timeout + cancellation kill, with `wasm32` stub) and pluggable `BackendCatalog` in `crates/job-manager/src/backends.rs` + `JobManager::create_context`.<br>4. Pass `cargo fmt --all -- --check`, `clippy -- -D warnings` (native & `--features wasm`), `cargo test --workspace --locked`, backfill `8ec6dd2` (`stage-1.3.1`), append `TODO.md` / `CHANGELOG.md`, pass Agent 12 audit, commit, tag `stage-1.3.2`, and push. |
| **Test Results** | `cargo fmt --all -- --check`: PASS (`FMT_EXIT:0`); `cargo clippy --workspace --all-targets -- -D warnings`: PASS (0 warnings); `cargo clippy --no-default-features --features wasm --locked -- -D warnings` (`file-core`, `codec-registry`, `job-manager`): PASS (0 warnings); `cargo test --workspace --locked`: 43 passed, 0 failed (`codec-registry`: 3, `file-core`: 22, `validation-engine`: 5, `job-manager`: 6, `filefusion` unit: 1, `cli_smoke`: 6); Agent 12 independent audit: **PASS**. |
| **Files Changed** | `crates/file-core/src/contracts.rs`, `crates/file-core/src/pipeline.rs`, `crates/file-core/src/lib.rs`, `crates/validation-engine/src/lib.rs`, `crates/job-manager/src/backends.rs`, `crates/job-manager/src/lib.rs`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `b26ae47` (tag `stage-1.3.2`, pushed to `origin/main`) |
| **Outstanding Issues** | None |

### Subphase 1.3.1 — Processing API

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 1 — Rust Core Foundation |
| **Phase** | Phase 1.3 — Pipeline & Execution |
| **Subphase** | `1.3.1` — Processing API |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 2 (Rust Core Engineer), Agent 1 (Principal Architect), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 2 hours |
| **Actual Duration** | ~15 minutes |
| **Acceptance Criteria** | 1. Implement shared L0 Processing API contracts in `crates/file-core/src/pipeline.rs` (`MOD-RULE-01`, `FF-CTR-001` §3–§4): `CancellationToken`, `ProgressSink` / `NoOpProgressSink` / `VecProgressSink`, `JobContext`, `CompressionProfile` (`FR-CMP-001`), `CompressionStatus` (`FR-CMP-002`), `ProcessingOperation`, `ProcessingRequest`, `ValidatedSource`, `StagedPath` (atomic sibling `.filefusion.tmp.<id>` staging & cleanup-on-drop rollback), `resolve_output_destination` (`FR-BAT-002`), `ItemOutcome` / `JobReceipt`, and `ProcessingHandler`.<br>2. Lift `CancellationToken` ownership cleanly from `job-manager` to `file-core` (preserving re-exports) and implement `JobManager::execute_batch` in `crates/job-manager/src/lib.rs` with monotonic `ProgressEvent` emission, `ValidatedSource` checks, collision resolution, and cancellation propagation.<br>3. Pass `cargo fmt --all -- --check`, `clippy -- -D warnings` (native & `--features wasm`), `cargo test --workspace --locked`, backfill `a185199` (`stage-1.2.4`), append `TODO.md` / `CHANGELOG.md`, pass Agent 12 audit, commit, tag `stage-1.3.1`, and push. |
| **Test Results** | `cargo fmt --all -- --check`: PASS (`FMT_EXIT:0`); `cargo clippy --workspace --all-targets -- -D warnings`: PASS (0 warnings); `cargo clippy --no-default-features --features wasm --locked -- -D warnings` (`file-core`, `codec-registry`, `job-manager`): PASS (0 warnings); `cargo test --workspace --locked`: 40 passed, 0 failed (`codec-registry`: 3, `file-core`: 22, `validation-engine`: 4, `job-manager`: 4, `filefusion` unit: 1, `cli_smoke`: 6); Agent 12 independent audit: **PASS**. |
| **Files Changed** | `crates/file-core/src/pipeline.rs`, `crates/file-core/src/lib.rs`, `crates/job-manager/src/lib.rs`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `8ec6dd2` (tag `stage-1.3.1` on `main`, pushed to `origin`) |
| **Outstanding Issues** | None |

### Subphase 1.2.4 — Capability registry

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 1 — Rust Core Foundation |
| **Phase** | Phase 1.2 — Core Engines |
| **Subphase** | `1.2.4` — Capability registry |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 2 (Rust Core Engineer), Agent 3 (Compression & Codec Engineer), Agent 4 (Document & Conversion Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 2 hours |
| **Actual Duration** | ~12 minutes |
| **Acceptance Criteria** | 1. Implement `CapabilityRegistry` and `ToolAvailability` in `crates/codec-registry/src/lib.rs` separating Tier 1 built-in permissive engines (`file-core-workspace`, `file-core-inspection`, `validation-engine-security`) from Tier 2 out-of-process tools (`ffmpeg`, `qpdf`, `tesseract`, `libreoffice`, `pandoc`, `calibre`, `7z`) with SPDX metadata and `LicenseClass`.<br>2. Probe Tier 2 binaries cleanly under `native-probe` with `wasm` stub compatibility, implement `capabilities_for_format(FormatId)` and `require_available(id)` (`FF-ERR-ENG-MISSING` -> `CliExitCode::MissingEngine = 5`), and expose `tool_availability` on `DoctorReport`.<br>3. Wire feature gates (`codec-registry/wasm`, `codec-registry/native-probe`) and re-exports through `job-manager` and update CLI `doctor` integration assertions.<br>4. Pass `cargo fmt --all -- --check`, `clippy -- -D warnings` (native & `--features wasm`), `cargo test --workspace --locked`, backfill `9ef27ca` (`stage-1.2.3`), append `TODO.md` / `CHANGELOG.md`, pass Agent 12 audit, commit, tag `stage-1.2.4`, and push. |
| **Test Results** | `cargo fmt --all -- --check`: PASS (`FMT_EXIT:0`); `cargo clippy --workspace --all-targets -- -D warnings`: PASS (0 warnings); `cargo clippy --no-default-features --features wasm --locked -- -D warnings` (`file-core`, `codec-registry`, `job-manager`): PASS (0 warnings); `cargo test --workspace --locked`: 37 passed, 0 failed (`codec-registry`: 3, `file-core`: 20, `validation-engine`: 4, `job-manager`: 3, `filefusion` unit: 1, `cli_smoke`: 6); Agent 12 independent audit: **PASS**. |
| **Files Changed** | `crates/codec-registry/Cargo.toml`, `crates/codec-registry/src/lib.rs`, `crates/job-manager/Cargo.toml`, `crates/job-manager/src/lib.rs`, `apps/cli/tests/cli_smoke.rs`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `a185199` (tag `stage-1.2.4` on `main`, pushed to `origin`) |
| **Outstanding Issues** | None |

### Subphase 1.2.3 — File classification

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 1 — Rust Core Foundation |
| **Phase** | Phase 1.2 — Core Engines |
| **Subphase** | `1.2.3` — File classification |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 2 (Rust Core Engineer), Agent 3 (Compression & Codec Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 1 hour |
| **Actual Duration** | ~12 minutes |
| **Acceptance Criteria** | 1. Implement `ProcessingDomain`, `CompressibilityTier`, `FileClassification`, `compute_entropy_milli_bits`, `classify_processing_domain`, and `classify_inspection_report` in `crates/file-core/src/classification.rs`.<br>2. Attach classification (`domain`, `compressibility`, `entropy_milli_bits`, `privacy_sensitive`, `requires_security_review`, `recommended_actions`) to `FileInspectionReport` in `crates/file-core/src/metadata.rs` covering security triage, privacy metadata scrubbing (`FR-MET-002`), no-op guards (`FR-CMP-002`), and compression targets (`FR-ADP-001`).<br>3. Expose classification types through `job-manager` and display them in `filefusion inspect` human/JSON output + tests.<br>4. Pass `cargo fmt --all -- --check`, `clippy -- -D warnings`, `cargo test --workspace --locked`, WASM builds, backfill `b9f9ea5` (`stage-1.2.2`), append `TODO.md` / `CHANGELOG.md`, pass Agent 12 audit, commit, tag `stage-1.2.3`, and push. |
| **Test Results** | `cargo fmt --all -- --check`: PASS (`FMT_EXIT:0`); `cargo clippy --workspace --all-targets -- -D warnings`: PASS (0 warnings); `cargo test --workspace --locked`: 35 passed, 0 failed (`file-core`: 20, `codec-registry`: 1, `validation-engine`: 4, `job-manager`: 3, `filefusion` unit: 1, `cli_smoke`: 6); `cargo check --no-default-features --features wasm --locked` (`file-core`, `codec-registry`, `job-manager`): PASS; Agent 12 independent audit: **PASS**. |
| **Files Changed** | `crates/file-core/src/classification.rs`, `crates/file-core/src/metadata.rs`, `crates/file-core/src/lib.rs`, `crates/job-manager/src/lib.rs`, `apps/cli/src/main.rs`, `apps/cli/tests/cli_smoke.rs`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `9ef27ca` (tag `stage-1.2.3` on `main`, pushed to `origin`) |
| **Outstanding Issues** | None |

### Subphase 1.2.2 — Metadata inspection

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 1 — Rust Core Foundation |
| **Phase** | Phase 1.2 — Core Engines |
| **Subphase** | `1.2.2` — Metadata inspection |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 2 (Rust Core Engineer), Agent 4 (Document & Conversion Engineer), Agent 9 (Security Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 2 hours |
| **Actual Duration** | ~15 minutes |
| **Acceptance Criteria** | 1. Add streaming `blake3`, `sha2`, and `xxhash-rust` (`xxh64`) workspace dependencies with permissive licenses (`deny.toml`) and zero `unsafe` code.<br>2. Implement single-pass `64 KB` streaming digest calculation (`DigestSet`) and bounded structural stream/metadata inspection (`StreamDescriptor`, `MetadataSummary`, `FileInspectionReport`) in `crates/file-core/src/metadata.rs` covering PNG, JPEG, WebP, GIF, BMP, WAV, FLAC, MP3, PDF, ZIP/OOXML/EPUB/ODF, and UTF-8 text.<br>3. Expose `job_manager::inspect_file` with privacy-safe `tracing` spans (`redact_path_for_log`) and upgrade `filefusion inspect` (`--signature-only`, `--strict`, `--json`) + E2E smoke tests.<br>4. Pass `cargo fmt --all -- --check`, `clippy -- -D warnings`, `cargo test --workspace --locked`, WASM builds, backfill `9b99e2c` (`stage-1.2.1`), append `TODO.md` / `CHANGELOG.md`, pass Agent 12 audit, commit, tag `stage-1.2.2`, and push. |
| **Test Results** | `cargo fmt --all -- --check`: PASS; `cargo clippy --workspace --all-targets -- -D warnings`: PASS (0 warnings); `cargo test --workspace --locked`: 33 passed, 0 failed (`file-core`: 18, `codec-registry`: 1, `validation-engine`: 4, `job-manager`: 3, `filefusion` unit: 1, `cli_smoke`: 6); `cargo check --no-default-features --features wasm --locked` (`file-core`, `codec-registry`, `job-manager`): PASS; Agent 12 independent audit: **PASS**. |
| **Files Changed** | `Cargo.toml`, `Cargo.lock`, `crates/file-core/Cargo.toml`, `crates/file-core/src/config.rs`, `crates/file-core/src/metadata.rs`, `crates/file-core/src/signature.rs`, `crates/file-core/src/lib.rs`, `crates/job-manager/src/lib.rs`, `apps/cli/src/main.rs`, `apps/cli/tests/cli_smoke.rs`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `b9f9ea5` (tag `stage-1.2.2` on `main`, pushed to `origin`) |
| **Outstanding Issues** | None |

### Subphase 1.2.1 — File signature detection

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 1 — Rust Core Foundation |
| **Phase** | Phase 1.2 — Core Engines |
| **Subphase** | `1.2.1` — File signature detection |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 2 (Rust Core Engineer), Agent 9 (Security Engineer), Agent 8 (QA & Automation Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 2 hours |
| **Actual Duration** | ~12 minutes |
| **Acceptance Criteria** | 1. Expand `MediaKind` and `FormatId` in `crates/file-core/src/lib.rs` with canonical MIME types and extension lists covering images, audio/video, documents/ebooks, archives, and executables.<br>2. Implement `crates/file-core/src/signature.rs` (`FR-DET-001`, `FR-DET-002`): multi-offset magic bytes, `RIFF` / `ISO-BMFF` / `EBML` / `Ogg` / local-header+EOCD `ZIP` subformat probing (`DOCX`/`XLSX`/`PPTX`/`ODT`/`ODS`/`ODP`/`EPUB`), UTF-8/markup text heuristics, and `enforce_spoofing_policy` (`FF-ERR-SEC-SPOOFED`, exit code `3`).<br>3. Wire `probe_file_signature` into `job-manager` and `filefusion inspect [--strict] [--json]` into `apps/cli` + E2E integration tests.<br>4. Verify `cargo fmt --all -- --check`, `clippy -- -D warnings`, `cargo test --workspace --locked`, WASM builds, backfill `24e9bc1` (`stage-1.1.3`), append `TODO.md` / `CHANGELOG.md`, pass Agent 12 audit, commit, tag `stage-1.2.1`, and push. |
| **Test Results** | `cargo fmt --all -- --check`: PASS; `cargo clippy --workspace --all-targets -- -D warnings`: PASS (0 warnings); `cargo test --workspace --locked`: 28 passed, 0 failed (`file-core`: 14, `codec-registry`: 1, `validation-engine`: 4, `job-manager`: 3, `filefusion` unit: 1, `cli_smoke`: 5); `cargo check --no-default-features --features wasm --locked` (`file-core`, `codec-registry`, `job-manager`): PASS; Agent 12 independent audit: **PASS**. |
| **Files Changed** | `crates/file-core/src/lib.rs`, `crates/file-core/src/signature.rs`, `crates/job-manager/src/lib.rs`, `apps/cli/src/main.rs`, `apps/cli/tests/cli_smoke.rs`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `9b99e2c` (tag `stage-1.2.1` on `main`, pushed to `origin`) |
| **Outstanding Issues** | None |

### Subphase 1.1.3 — Configuration management

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 1 — Rust Core Foundation |
| **Phase** | Phase 1.1 — Workspace |
| **Subphase** | `1.1.3` — Configuration management |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 2 (Rust Core Engineer), Agent 9 (Security Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 2 hours |
| **Actual Duration** | ~12 minutes |
| **Acceptance Criteria** | 1. Implement schema-versioned `FileFusionConfig`, `PrivacyConfig` (`offline_only = true`, `telemetry_enabled = false`, `external_ai_opt_in = false`), `BatchDefaults`, `CollisionPolicy`, and `PathConfig` in `crates/file-core/src/config.rs`.<br>2. Enforce deterministic precedence (defaults -> OS/explicit TOML -> `FILEFUSION_*` env -> CLI overrides), strict validation (including `naming_template` traversal protection and offline/AI privacy gates), and atomic `save_config_atomic` TOML persistence.<br>3. Wire `--config`, `--workers`, `--max-memory-mb`, `--quiet`, `--no-color`, and `filefusion config show|path|init|validate` into `job-manager`, `codec-registry` (`DoctorReport.effective_limits`), and `apps/cli`.<br>4. Pass `cargo fmt`, `clippy -- -D warnings`, `cargo test --workspace --locked`, WASM builds, backfill `e27daed` (`stage-1.1.2`), update `TODO.md` / `CHANGELOG.md`, pass Agent 12 audit, commit, tag `stage-1.1.3`, and push to `origin`. |
| **Test Results** | `cargo fmt --all -- --check`: PASS (`FMT_CHECK_EXIT:0`); `cargo clippy --workspace --all-targets -- -D warnings`: PASS (0 warnings); `cargo test --workspace --locked`: 24 passed, 0 failed (`file-core`: 11, `codec-registry`: 1, `validation-engine`: 4, `job-manager`: 3, `filefusion` unit: 1, `cli_smoke`: 4); `cargo check --no-default-features --features wasm --locked` (`file-core`, `codec-registry`, `job-manager`): PASS; Agent 12 independent audit: **PASS**. |
| **Files Changed** | `Cargo.toml`, `Cargo.lock`, `crates/file-core/Cargo.toml`, `crates/file-core/src/config.rs`, `crates/file-core/src/error.rs`, `crates/file-core/src/logging.rs`, `crates/file-core/src/lib.rs`, `crates/codec-registry/src/lib.rs`, `crates/job-manager/src/lib.rs`, `apps/cli/Cargo.toml`, `apps/cli/src/main.rs`, `apps/cli/tests/cli_smoke.rs`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `24e9bc1` (tag `stage-1.1.3` on `main`, pushed to `origin`) |
| **Outstanding Issues** | None |

### Subphase 1.1.2 — Shared error and logging system

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 1 — Rust Core Foundation |
| **Phase** | Phase 1.1 — Workspace |
| **Subphase** | `1.1.2` — Shared error and logging system |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 2 (Rust Core Engineer), Agent 8 (QA & Automation Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 2 hours |
| **Actual Duration** | ~12 minutes |
| **Acceptance Criteria** | 1. Implement `ErrorCategory`, all 11 canonical `ErrorCode` (`FF-ERR-*`) variants, `ErrorDiagnostic`, and `FileFusionError` (`thiserror`, `From<std::io::Error>`, `exit_code()`) in `crates/file-core/src/error.rs`.<br>2. Implement structured `tracing` initialization (`LoggingConfig`, `LogLevel`, `LogFormat`, `init_logging` routed strictly to `stderr`, `redact_path_for_log`) in `crates/file-core/src/logging.rs` with `native-logging` / `wasm` feature gating.<br>3. Wire `From<PathSafetyError>` / `From<BombGuardError>` in `validation-engine`, add `tracing` spans & `CancellationToken::check()` in `job-manager`, and add `--log-level` / `--log-json` + structured error reporting in `apps/cli`.<br>4. Verify `cargo fmt`, `clippy -- -D warnings`, `cargo test --workspace`, `--features wasm` builds, backfill `33bec50` for `1.1.1`, update `TODO.md` / `CHANGELOG.md`, pass Agent 12 audit, commit, tag, and push to `origin`. |
| **Test Results** | `cargo fmt --all -- --check`: PASS; `cargo clippy --workspace --all-targets -- -D warnings`: PASS (0 warnings); `cargo test --workspace` & `--locked`: 17 passed, 0 failed (`file-core`: 7, `codec-registry`: 1, `validation-engine`: 4, `job-manager`: 2, `filefusion` unit: 1, `cli_smoke`: 2); `cargo check --no-default-features --features wasm --locked` (`file-core`, `job-manager`): PASS; CLI `--log-level info --log-json doctor --json`: stderr JSON logs + valid stdout JSON report. Agent 12 independent audit: **PASS**. |
| **Files Changed** | `Cargo.toml`, `Cargo.lock`, `crates/file-core/Cargo.toml`, `crates/file-core/src/error.rs`, `crates/file-core/src/logging.rs`, `crates/file-core/src/lib.rs`, `crates/validation-engine/src/lib.rs`, `crates/job-manager/Cargo.toml`, `crates/job-manager/src/lib.rs`, `apps/cli/Cargo.toml`, `apps/cli/src/main.rs`, `apps/cli/tests/cli_smoke.rs`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `e27daed` (tag `stage-1.1.2` on `main`, pushed to `origin`) |
| **Outstanding Issues** | None |

### Subphase 1.1.1 — Cargo workspace

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 1 — Rust Core Foundation |
| **Phase** | Phase 1.1 — Workspace |
| **Subphase** | `1.1.1` — Cargo workspace |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 2 (Rust Core Engineer), Agent 1 (Principal Architect), Agent 8 (QA & Automation Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 2 hours |
| **Actual Duration** | ~10 minutes |
| **Acceptance Criteria** | 1. Expand root `Cargo.toml` workspace with `[workspace.dependencies]` (`serde`, `serde_json`, `clap`, internal path crates) and Stage 1 foundation crates: `crates/file-core` (`L0`), `crates/codec-registry` (`L1`), `crates/validation-engine` (`L1`), `crates/job-manager` (`L5`), and `apps/cli` (`filefusion-cli` -> `filefusion`).<br>2. Implement real baseline types & safety functions (`ResourceLimits::for_host`, `ProgressEvent`, `CliExitCode`, `DoctorReport`, `validate_archive_member_path`, `validate_expansion_budget`, `CancellationToken`, and `filefusion doctor [--json]` / `--version`).<br>3. Enforce `#![forbid(unsafe_code)]`, `MOD-RULE-01`..`05`, and pass `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and `scripts/build.ps1 -App cli`.<br>4. Backfill `d297150` for `0.4.5`, append to `TODO.md` and `CHANGELOG.md`, pass Agent 12 audit, commit, tag, and push to `origin`. |
| **Test Results** | `cargo fmt --all -- --check`: PASS; `cargo clippy --workspace --all-targets -- -D warnings`: PASS (0 warnings); `cargo test --workspace --locked`: 12 passed, 0 failed (`file-core`: 3, `codec-registry`: 1, `validation-engine`: 3, `job-manager`: 2, `filefusion` unit: 1, `cli_smoke`: 2); `--no-default-features --features wasm` check: PASS across all 4 library crates; `scripts/build.ps1 -App cli -BuildMode debug`: PASS (`filefusion.exe` 12,484,096 B, `sha256sum -c` 5/5 OK); Agent 12 independent re-audit: **PASS**. |
| **Files Changed** | `Cargo.toml`, `Cargo.lock`, `crates/file-core/Cargo.toml`, `crates/file-core/src/lib.rs`, `crates/codec-registry/Cargo.toml`, `crates/codec-registry/src/lib.rs`, `crates/validation-engine/Cargo.toml`, `crates/validation-engine/src/lib.rs`, `crates/job-manager/Cargo.toml`, `crates/job-manager/src/lib.rs`, `apps/cli/Cargo.toml`, `apps/cli/src/main.rs`, `apps/cli/tests/cli_smoke.rs`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `33bec50` (tagged `stage-1.1.1` on `main`, pushed to `https://github.com/dayashimoga/filefusion`) |
| **Outstanding Issues** | None |

### Subphase 0.4.5 — Validate local build scripts

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 0 — Project Foundation |
| **Phase** | Phase 0.4 — CI/CD |
| **Subphase** | `0.4.5` — Validate local build scripts |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 10 (DevOps Engineer), Agent 8 (QA & Automation Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 2 hours |
| **Actual Duration** | ~12 minutes |
| **Acceptance Criteria** | 1. Author `scripts/build.ps1` (Windows) and `scripts/build.sh` (Linux/macOS) supporting all 11 selectable targets (`all`, `desktop`, `windows`, `linux`, `macos`, `mobile`, `android`, `ios`, `web`, `cli`, `api`), `build_mode`, `include_tests`, `enable_optional_codecs`, and `--dry-run` / `-DryRun`.<br>2. Execute host-supported targets sequentially (`<= 16 GB` RAM safe) and explicitly report/reject unavailable cross-OS targets (`linux`/`macos`/`ios` on Windows).<br>3. Document local vs CI-delegated target support in `DEVELOPER_GUIDE.md`.<br>4. Validate `scripts/build.ps1` and `scripts/build.sh` (`bash -n` + live execution + negative cross-OS target test), backfill `9f3c441` for `0.4.4`, update `TODO.md` and `CHANGELOG.md`, pass Agent 12 audit (closing Stage 0), commit, tag, and push to `origin`. |
| **Test Results** | `build.ps1` validated (`-App all -DryRun` exit `0`, `-App ios` negative exit `1`, `-App cli -BuildMode debug`, `-App web -BuildMode release`, `-App windows -BuildMode debug`); `build.sh` validated (`bash -n` exit `0`, `--app all --dry-run` exit `0`, `--app api --build-mode debug`, `--include-tests invalid` negative exit `2`); `sha256sum -c SHA256SUMS.txt` **4/4 `OK`** across all 4 `dist/local/*` staged directories (`bom=False`, `cr=False`); Cargo `fmt`/`clippy`/`test --locked` (`1 passed; 0 failed`); Agent 12 independent re-audit: **PASS** (Stage 0 Exit Criteria verified). |
| **Files Changed** | `scripts/build.ps1`, `scripts/build.sh`, `DEVELOPER_GUIDE.md`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `d297150` (tagged `stage-0.4.5` on `main`, pushed to `https://github.com/dayashimoga/filefusion`) |
| **Outstanding Issues** | None |

### Subphase 0.4.4 — Validate runner and platform matrix

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 0 — Project Foundation |
| **Phase** | Phase 0.4 — CI/CD |
| **Subphase** | `0.4.4` — Validate runner and platform matrix |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 10 (DevOps Engineer), Agent 8 (QA & Automation Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 1 hour |
| **Actual Duration** | ~10 minutes |
| **Acceptance Criteria** | 1. Inspect live GitHub Actions workflow runs (`46013319849`, `46014125464`) across `ubuntu-24.04`, `windows-latest`, and `macos-latest` runners.<br>2. Fix the live `EmbarkStudios/cargo-deny-action` argument placement (`command: check`, `arguments: --all-features`, `command-arguments: advisories licenses bans sources`) in `build-apps.yml` and `release.yml`.<br>3. Trigger and observe a live `workflow_dispatch` targeted run (`app=web`) plus push run verifying 100% green jobs and independent artifact uploads.<br>4. Backfill `73c22a7` for `0.4.3`, update `TODO.md` and `CHANGELOG.md`, pass Agent 12 audit, commit, tag, and push to `origin`. |
| **Test Results** | Agent 12 audit: **PASS** (25/25 & 6/6 SHA pins, 5/5 `bash -n` scripts, append-only logs & scope verified). Live GitHub Actions `push` run `46015089636` (`05ea464`): **13/13 jobs `success`** across `ubuntu-24.04`, `windows-latest`, `macos-latest` with **10/10 independent artifacts** uploaded. Live `workflow_dispatch` (`app=web`) run `46015090900`: **`success`** (`Build Web` succeeded, `Build CLI`/`Build Flutter`/`Build API` skipped, `filefusion-web` artifact uploaded). |
| **Files Changed** | `.github/workflows/build-apps.yml`, `.github/workflows/release.yml`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `9f3c441` (tagged `stage-0.4.4` on `main`; workflow fix at `05ea464`, pushed to `https://github.com/dayashimoga/filefusion`) |
| **Outstanding Issues** | None |

### Subphase 0.4.3 — Implement independent artifact uploads

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 0 — Project Foundation |
| **Phase** | Phase 0.4 — CI/CD |
| **Subphase** | `0.4.3` — Implement independent artifact uploads |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 10 (DevOps Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 1 hour |
| **Actual Duration** | ~5 minutes |
| **Acceptance Criteria** | 1. Package each application/platform target independently under `dist/<artifact_name>/` without nesting ZIPs inside ZIPs and preserving directory structures.<br>2. Bundle `LICENSE`, `LICENSES.md`, `build-metadata.json`, and deterministic `SHA256SUMS.txt` inside each independent artifact.<br>3. Upload each target separately via `actions/upload-artifact@cf430e030ddbb5b0abf93d22962f4752f3646cd9` (`v7.0.2`, compatible v4+ successor) with `if-no-files-found: error` and clear `$GITHUB_STEP_SUMMARY` / failure diagnostics.<br>4. Backfill `a108774` for `0.4.2`, update `TODO.md` and `CHANGELOG.md`, pass Agent 12 audit, commit, tag, and push to `origin`. |
| **Test Results** | Agent 12 independent audit: **PASS** (10 independent artifact paths, `bash -n` on all 5 extracted scripts `exit=0`, `ConvertFrom-Json` on all 4 metadata heredocs, `actions/upload-artifact` v7.0.2 SHA verified, Cargo `fmt`/`clippy`/`test --locked` `1 passed; 0 failed`, append-only logs & scope verified). |
| **Files Changed** | `.github/workflows/build-apps.yml`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `73c22a7` (tagged `stage-0.4.3` on `main`, pushed to `https://github.com/dayashimoga/filefusion`) |
| **Outstanding Issues** | None |

### Subphase 0.4.2 — Implement selectable application builds

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 0 — Project Foundation |
| **Phase** | Phase 0.4 — CI/CD |
| **Subphase** | `0.4.2` — Implement selectable application builds |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 10 (DevOps Engineer), Agent 2 (Rust Core Engineer), Agent 5 (Flutter Engineer), Agent 6 (Web Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 2 hours |
| **Actual Duration** | ~5 minutes |
| **Acceptance Criteria** | 1. Expand `.github/workflows/build-apps.yml` `validate-inputs` to resolve selective target matrices for all 11 choices (`all`, `desktop`, `windows`, `linux`, `macos`, `mobile`, `android`, `ios`, `web`, `cli`, `api`) so unrelated targets are skipped when a single target is chosen.<br>2. Implement native-runner target jobs (`build-cli`, `build-flutter`, `build-web`, `build-api`) with Cargo/Flutter/Node caching, `wasm32-unknown-unknown` build verification, and `build_mode` / `include_tests` support.<br>3. Pin newly added Flutter, pnpm, and Java actions to live-verified immutable 40-char commit SHAs.<br>4. Backfill `8a4a67e` for `0.4.1`, update `TODO.md` and `CHANGELOG.md`, pass Agent 12 review, commit, tag, and push to `origin`. |
| **Test Results** | Agent 12 independent audit: **PASS** (bash matrix simulation across all 11 `app` choices + fallback, 21/21 SHA-pinned `uses:` verified via `git ls-remote`, `cargo check -p file-core --no-default-features --features wasm`, `cargo fmt`/`clippy`/`test --locked` `1 passed; 0 failed`, append-only logs & scope verified). |
| **Files Changed** | `.github/workflows/build-apps.yml`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `a108774` (tagged `stage-0.4.2` on `main`, pushed to `https://github.com/dayashimoga/filefusion`) |
| **Outstanding Issues** | None |

### Subphase 0.4.1 — Configure GitHub Actions

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 0 — Project Foundation |
| **Phase** | Phase 0.4 — CI/CD |
| **Subphase** | `0.4.1` — Configure GitHub Actions |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 10 (DevOps Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 2 hours |
| **Actual Duration** | ~5 minutes |
| **Acceptance Criteria** | 1. Create `.github/ISSUE_TEMPLATE/` (`bug_report.yml`, `feature_request.yml`, `config.yml`), `.github/pull_request_template.md`, and `.github/dependabot.yml`.<br>2. Create baseline `.github/workflows/build-apps.yml` (push/PR/`workflow_dispatch` with `app`/`build_mode`/`include_tests`/`enable_optional_codecs` inputs, input validation, Rust core lint/test, and `cargo-deny` audit) and `.github/workflows/release.yml` (`v*.*.*` tagged release pipeline).<br>3. Pin every third-party GitHub Action to an immutable commit SHA verified from GitHub, enforce least-privilege `permissions:`, and validate YAML syntax.<br>4. Backfill `53dada6` for `0.3.5`, update `TODO.md` and `CHANGELOG.md`, pass Agent 12 audit, commit, tag, and push to `origin`. |
| **Test Results** | Agent 12 independent audit: **PASS** (all 5 third-party actions verified pinned to live 40-char GitHub commit SHAs, least-privilege permissions, `advisories`+`licenses`+`bans`+`sources` gate, Cargo `fmt`/`clippy`/`test --locked` `1 passed; 0 failed`, append-only logs & scope verified). |
| **Files Changed** | `.github/ISSUE_TEMPLATE/bug_report.yml`, `.github/ISSUE_TEMPLATE/feature_request.yml`, `.github/ISSUE_TEMPLATE/config.yml`, `.github/pull_request_template.md`, `.github/dependabot.yml`, `.github/workflows/build-apps.yml`, `.github/workflows/release.yml`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `8a4a67e` (tagged `stage-0.4.1` on `main`, pushed to `https://github.com/dayashimoga/filefusion`) |
| **Outstanding Issues** | None |

### Subphase 0.3.5 — Create documentation foundation

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 0 — Project Foundation |
| **Phase** | Phase 0.3 — Engineering environment |
| **Subphase** | `0.3.5` — Create documentation foundation |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 11 (Documentation Engineer), Agent 1 (Principal Architect), Agent 9 (Security Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 2 hours |
| **Actual Duration** | ~5 minutes |
| **Acceptance Criteria** | 1. Author `docs/ENGINEERING_CONTRACT.md` (Section 7 contract) and `docs/ROADMAP.md` (Section 8 Stages 0–12 map).<br>2. Author `README.md`, `CONTRIBUTING.md`, `LICENSE` (`Apache-2.0 OR MIT`), `SECURITY.md`, `PERFORMANCE.md`, `LICENSES.md`, `CODEBASE_GUIDE.md`, `USER_GUIDE.md`, and `DEVELOPER_GUIDE.md`.<br>3. Verify all cross-links, truthful Stage 0 status (no unverified claims), and append-only `TODO.md` / `CHANGELOG.md` integrity.<br>4. Backfill `e613fa1` for `0.3.4`, pass Agent 12 audit (closing Phase `0.3`), commit, tag, and push to `origin`. |
| **Test Results** | Agent 12 independent audit: **PASS** (17/17 required docs present, 51/51 relative links resolved, Cargo sanity check `1 passed; 0 failed`, license/truthfulness/append-only logs & Phase 0.3 closure verified). |
| **Files Changed** | `docs/ENGINEERING_CONTRACT.md`, `docs/ROADMAP.md`, `README.md`, `CONTRIBUTING.md`, `LICENSE`, `SECURITY.md`, `PERFORMANCE.md`, `LICENSES.md`, `CODEBASE_GUIDE.md`, `USER_GUIDE.md`, `DEVELOPER_GUIDE.md`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `53dada6` (tagged `stage-0.3.5` on `main`, pushed to `https://github.com/dayashimoga/filefusion`) |
| **Outstanding Issues** | None |

### Subphase 0.3.4 — Configure specialized agents

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 0 — Project Foundation |
| **Phase** | Phase 0.3 — Engineering environment |
| **Subphase** | `0.3.4` — Configure specialized agents |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 1 (Principal Architect), Agent 11 (Documentation Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 2 hours |
| **Actual Duration** | ~4 minutes |
| **Acceptance Criteria** | 1. Create dedicated `SKILL.md` files for all 12 specialist agents (`agent-01-principal-architect` through `agent-12-independent-reviewer`) in `.agents/skills/`.<br>2. Include explicit Responsibilities, Permitted Tools, Implementation Standards, Verification Commands, and Completion Criteria in every specialist skill.<br>3. Follow `agy-customizations` YAML frontmatter format (`name` lowercase-hyphenated matching directory, third-person `description`).<br>4. Backfill `4c9c283` for `0.3.3`, append to `TODO.md` and `CHANGELOG.md`, pass Agent 12 audit, commit, tag, and push to `origin`. |
| **Test Results** | Agent 12 independent audit: **PASS** (12/12 specialist skills, 12/12 frontmatter & 5 required sections, technical cross-check + Cargo sanity pass, append-only logs & scope verified). |
| **Files Changed** | `.agents/skills/agent-01-*/SKILL.md` through `.agents/skills/agent-12-*/SKILL.md` (12 skills), `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `e613fa1` (tagged `stage-0.3.4` on `main`, pushed to `https://github.com/dayashimoga/filefusion`) |
| **Outstanding Issues** | None |

### Subphase 0.3.3 — Configure Antigravity rules and skills

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 0 — Project Foundation |
| **Phase** | Phase 0.3 — Engineering environment |
| **Subphase** | `0.3.3` — Configure Antigravity rules and skills |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 1 (Principal Architect), Agent 11 (Documentation Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 2 hours |
| **Actual Duration** | ~5 minutes |
| **Acceptance Criteria** | 1. Inspect supported Antigravity customization format (`agy-customizations` docs) without depending on deprecated workflow formats.<br>2. Author root `AGENTS.md` (no frontmatter) and all 12 project rules in `.agents/rules/` within the 24 KB/file and 20k aggregate token budget.<br>3. Author all 14 reusable workflow skills in `.agents/skills/*/SKILL.md` with valid YAML frontmatter (`name` lowercase-hyphenated, third-person `description`) and validation steps.<br>4. Backfill `99794ae` for `0.3.2`, append to `TODO.md` and `CHANGELOG.md`, pass Agent 12 audit, commit, tag, and push to `origin`. |
| **Test Results** | Agent 12 independent audit: **PASS** (12/12 rule domains, 14/14 reusable skills, 0 deprecated frontmatter/workflow dependencies, `13,102 B` / `~3,276` est. tokens = 16.4% of 20k budget, append-only logs & scope verified). |
| **Files Changed** | `AGENTS.md`, `.agents/rules/*.md` (12 rules), `.agents/skills/*/SKILL.md` (14 skills), `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `4c9c283` (tagged `stage-0.3.3` on `main`, pushed to `https://github.com/dayashimoga/filefusion`) |
| **Outstanding Issues** | None |

### Subphase 0.3.2 — Establish monorepo

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 0 — Project Foundation |
| **Phase** | Phase 0.3 — Engineering environment |
| **Subphase** | `0.3.2` — Establish monorepo |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 1 (Principal Architect), Agent 2 (Rust Core Engineer), Agent 10 (DevOps Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 1 hour |
| **Actual Duration** | ~10 minutes |
| **Acceptance Criteria** | 1. Configure cross-OS monorepo governance files (`.gitignore`, `.gitattributes`, `.editorconfig`, `.cargo/config.toml`, `deny.toml`).<br>2. Establish root `Cargo.toml` workspace (`edition = "2024"`, `unsafe_code = "forbid"`, strict clippy lints) with non-decorative `crates/file-core` anchor crate.<br>3. Author `docs/MONOREPO_SETUP.md` (`FF-MONO-001`) documenting active roots and stage activation schedule.<br>4. Pass `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and Agent 12 independent audit.<br>5. Backfill `2450a3e` for `0.3.1`, update `TODO.md` and `CHANGELOG.md`, commit, tag, and push to `origin`. |
| **Test Results** | `cargo fmt --all -- --check`: PASS; `cargo clippy --workspace --all-targets -- -D warnings`: PASS (0 warnings); `cargo test --workspace`: 1 passed, 0 failed (`workspace_identity_is_consistent`); `cargo metadata --no-deps`: PASS; Agent 12 independent audit: **PASS** (0 blocking defects). |
| **Files Changed** | `.gitignore`, `.gitattributes`, `.editorconfig`, `.cargo/config.toml`, `deny.toml`, `Cargo.toml`, `Cargo.lock`, `crates/file-core/Cargo.toml`, `crates/file-core/src/lib.rs`, `docs/MONOREPO_SETUP.md`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `99794ae` (tagged `stage-0.3.2` on `main`, pushed to `https://github.com/dayashimoga/filefusion`) |
| **Outstanding Issues** | None |

### Subphase 0.3.1 — Create GitHub repository

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 0 — Project Foundation |
| **Phase** | Phase 0.3 — Engineering environment |
| **Subphase** | `0.3.1` — Create GitHub repository |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 10 (DevOps Engineer), Agent 9 (Security Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 1 hour |
| **Actual Duration** | ~15 minutes |
| **Acceptance Criteria** | 1. Confirm repository visibility preference (`Public`) with user.<br>2. Create public GitHub repository `filefusion` (`https://github.com/dayashimoga/filefusion`) using authenticated Git Credential Manager + GitHub REST API without exposing credentials.<br>3. Point `origin` to `https://github.com/dayashimoga/filefusion.git`, push `main` and all `stage-0.*` tags, and verify remote refs (`git ls-remote origin`).<br>4. Pass Agent 12 independent security & governance review, update `TODO.md` and `CHANGELOG.md`, and commit to `main`. |
| **Test Results** | Agent 12 & Agent 9 audit: **PASS** (0 secrets in `.git/config` or workspace; append-only logs & scope verified); `git push -u origin main --tags` & `git ls-remote origin` verified. |
| **Files Changed** | `.git/config` (`origin` -> `https://github.com/dayashimoga/filefusion.git`), `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `2450a3e` (tagged `stage-0.3.1` on `main`, pushed to `https://github.com/dayashimoga/filefusion`) |
| **Outstanding Issues** | None (previous `dayatin/ufi` collaborator permission blocker resolved per user direction to create `https://github.com/dayashimoga/filefusion`). |

### Subphase 0.2.4 — Create architecture decision records

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 0 — Project Foundation |
| **Phase** | Phase 0.2 — Architecture |
| **Subphase** | `0.2.4` — Create architecture decision records |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 1 (Principal Architect), Agent 11 (Documentation Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 1 hour |
| **Actual Duration** | ~3 minutes |
| **Acceptance Criteria** | 1. Author `docs/DECISION_LOG.md` (`FF-ADR-001` v1.0.0) documenting `ADR-001` through `ADR-010` covering Rust workspace layering, Two-Tier engine/license isolation, dual-pool concurrency, Flutter/FRB, SvelteKit 5/WASM, SQLite/Cloud storage, atomic staging/No-Op Guard, security gate/IsolatedProcessRunner, opt-in AI privacy, and CI/Podman strategy.<br>2. Cross-link from `ARCHITECTURE.md`, backfill `fff6c24` for `0.2.3` in `docs/PROGRESS.md`, append to `TODO.md` and `CHANGELOG.md`.<br>3. Pass Agent 12 independent review (including Phase `0.2` closure) and commit to `main`. |
| **Test Results** | Agent 12 independent audit: **PASS** (0 blocking defects, 0 contradictions, 0 scope violations; all 10 ADRs, cross-spec consistency, append-only logs, and Phase 0.2 closure verified). |
| **Files Changed** | `docs/DECISION_LOG.md`, `ARCHITECTURE.md`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `2a65c5f` (tagged `stage-0.2.4` on `main`) |
| **Outstanding Issues** | Repository visibility preference (`public` vs `private`) still awaiting user decision ahead of Subphase `0.3.1` (next subphase). |

### Subphase 0.2.3 — Define Rust and application contracts

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 0 — Project Foundation |
| **Phase** | Phase 0.2 — Architecture |
| **Subphase** | `0.2.3` — Define Rust and application contracts |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 1 (Principal Architect), Agent 2 (Rust Core Engineer), Agent 5 (Flutter Engineer), Agent 6 (Web Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 2 hours |
| **Actual Duration** | ~3 minutes |
| **Acceptance Criteria** | 1. Author `docs/CONTRACTS.md` (`FF-CTR-001` v1.0.0) specifying the canonical `FileFusionError` / `ErrorCode` taxonomy, `ResourceLimits`, `FileInspectionReport`, and `ProgressEvent` schemas.<br>2. Define Rust core trait contracts (`CompressionEngine`, `ConversionEngine`, `ExternalToolAdapter`, `QualityEvaluator`, `OutputValidator`) and SQLite WAL persistence tables.<br>3. Specify Plane 2 transport contracts for `flutter-bridge` (`flutter_rust_bridge` v2), SvelteKit WASM Web Workers, CLI subcommands/exit codes/`--json`, and `/api/v1` REST OpenAPI envelope.<br>4. Cross-link from `ARCHITECTURE.md` and `docs/MODULE_BOUNDARIES.md`, backfill `606553b` for `0.2.2`, append to `TODO.md` and `CHANGELOG.md`, pass Agent 12 review, and commit to `main`. |
| **Test Results** | Agent 12 independent audit: **PASS** (0 blocking defects, 0 scope violations; error taxonomy, core traits, SQLite WAL schema, Plane 2 transport contracts, and append-only logs verified). |
| **Files Changed** | `docs/CONTRACTS.md`, `ARCHITECTURE.md`, `docs/MODULE_BOUNDARIES.md`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `fff6c24` (tagged `stage-0.2.3` on `main`) |
| **Outstanding Issues** | Repository visibility preference (`public` vs `private`) still awaiting user decision ahead of Subphase `0.3.1`. |

### Subphase 0.2.2 — Define module boundaries

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 0 — Project Foundation |
| **Phase** | Phase 0.2 — Architecture |
| **Subphase** | `0.2.2` — Define module boundaries |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 1 (Principal Architect), Agent 2 (Rust Core Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 1 hour |
| **Actual Duration** | ~3 minutes |
| **Acceptance Criteria** | 1. Author `docs/MODULE_BOUNDARIES.md` (`FF-MOD-001` v1.0.0) mapping all monorepo directories (`apps/*`, `crates/*`, `packages/*`, `infrastructure/`, `scripts/`, `tests/*`, `fixtures/`, `docs/`, `.github/`, `.agents/`) and ownership.<br>2. Specify all 16 Rust workspace crates across 7 acyclic dependency tiers (`L0`–`L6`) with a valid Mermaid DAG.<br>3. Codify forbidden dependency/coupling rules (`MOD-RULE-01`..`MOD-RULE-06`) and Desktop/Mobile/WASM Cargo feature-flag boundaries.<br>4. Cross-link from `ARCHITECTURE.md`, backfill `d6fb2b5` for `0.2.1`, append to `TODO.md` and `CHANGELOG.md`, pass Agent 12 review, and commit to `main`. |
| **Test Results** | Agent 12 independent audit: **PASS** (0 blocking defects, 0 scope violations; 16-crate DAG acyclicity, Mermaid syntax, `MOD-RULE-01..06`, WASM feature gating, and append-only logs verified). |
| **Files Changed** | `docs/MODULE_BOUNDARIES.md`, `ARCHITECTURE.md`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `606553b` (tagged `stage-0.2.2` on `main`) |
| **Outstanding Issues** | Repository visibility preference (`public` vs `private`) still awaiting user decision ahead of Subphase `0.3.1`. |

### Subphase 0.2.1 — Define system architecture

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 0 — Project Foundation |
| **Phase** | Phase 0.2 — Architecture |
| **Subphase** | `0.2.1` — Define system architecture |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 1 (Principal Architect), Agent 2 (Rust Core Engineer), Agent 11 (Documentation Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 2 hours |
| **Actual Duration** | ~3 minutes |
| **Acceptance Criteria** | 1. Author `ARCHITECTURE.md` (`FF-ARCH-001` v1.0.0) establishing the 5-plane layered architecture, Mermaid architecture diagram, and forbidden-coupling rules.<br>2. Specify the 8-step end-to-end processing lifecycle, dual Tokio/Rayon resource governance (16 GB RAM & 3–6 GB mobile guardrails), and 500 ms cancellation propagation.<br>3. Specify the 4 trust boundaries, zero-shell-injection Tier 2 isolation, cross-platform topologies, and observability model.<br>4. Pass Agent 12 independent review, backfill `df8c41b` for `0.1.4`, append to `TODO.md` and `CHANGELOG.md`, and commit to `main`. |
| **Test Results** | Agent 12 independent audit: **PASS** (0 blocking defects, 0 scope violations; 5-plane isolation, 8-step lifecycle, Mermaid syntax, security trust boundaries, and append-only logs verified). |
| **Files Changed** | `ARCHITECTURE.md`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `d6fb2b5` (tagged `stage-0.2.1` on `main`) |
| **Outstanding Issues** | Repository visibility preference (`public` vs `private`) still awaiting user decision ahead of Subphase `0.3.1`. |

### Subphase 0.1.4 — Define MVP and deferred features

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 0 — Project Foundation |
| **Phase** | Phase 0.1 — Product definition |
| **Subphase** | `0.1.4` — Define MVP and deferred features |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 1 (Principal Architect), Agent 11 (Documentation Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 1 hour |
| **Actual Duration** | ~4 minutes |
| **Acceptance Criteria** | 1. Author `docs/SCOPE_AND_MVP.md` (`FF-SCP-001` v1.0.0) establishing clear boundaries for Milestone A (`MVP-v0.1`), Milestone B (`v1.0-GA`), and Milestone C (`v1.x+`).<br>2. Establish formal Deferred-Features & Explicit Non-Goals Register (`DEF-001`..`DEF-012`) and 5-point Scope Admission Gate.<br>3. Verify complete alignment of `P0`/`P1`/`P2` requirements across `REQUIREMENTS.md`, `docs/USE_CASES.md`, and `docs/FORMATS_AND_CAPABILITIES.md`.<br>4. Pass Agent 12 independent review, update append-only `TODO.md` and `CHANGELOG.md`, and commit to `main`. |
| **Test Results** | Agent 12 independent audit: **PASS** (0 blocking defects; milestone partitioning, `DEF-001`..`DEF-012`, 5-point Scope Admission Gate, no-fake-functionality policy, append-only logs, and Phase 0.1 closure verified). |
| **Files Changed** | `docs/SCOPE_AND_MVP.md`, `REQUIREMENTS.md`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `df8c41b` (tagged `stage-0.1.4` on `main`) |
| **Outstanding Issues** | Repository visibility preference (`public` vs `private`) still awaiting user decision ahead of Subphase `0.3.1`. |

### Subphase 0.1.3 — Define supported formats and capabilities

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 0 — Project Foundation |
| **Phase** | Phase 0.1 — Product definition |
| **Subphase** | `0.1.3` — Define supported formats and capabilities |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 1 (Principal Architect), Agent 3 (Compression & Codec Engineer), Agent 4 (Document & Conversion Engineer), Agent 11 (Documentation Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 1 hour |
| **Actual Duration** | ~5 minutes |
| **Acceptance Criteria** | 1. Author `docs/FORMATS_AND_CAPABILITIES.md` (`FF-FMT-001` v1.0.0) covering magic-byte detection, image, video/HW, audio, document/office/ebook, PDF/OCR, archive/encryption, platform matrix, and conversion graph policies.<br>2. Clearly separate Tier 1 (pure-Rust permissive core) vs Tier 2 (license-isolated external adapters) across all formats.<br>3. Cross-link `docs/FORMATS_AND_CAPABILITIES.md` from `REQUIREMENTS.md`.<br>4. Pass Agent 12 independent review, update append-only `TODO.md` and `CHANGELOG.md`, and commit to `main`. |
| **Test Results** | Agent 12 independent audit: **PASS** (0 blocking defects; Tier 1/Tier 2 license isolation, security bomb/XXE/DRM guards, 9 format/platform sections, and append-only governance verified). |
| **Files Changed** | `docs/FORMATS_AND_CAPABILITIES.md`, `REQUIREMENTS.md`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `4e6d610` (tagged `stage-0.1.3` on `main`) |
| **Outstanding Issues** | Repository visibility preference (`public` vs `private`) still awaiting user decision ahead of Subphase `0.3.1`. |

### Subphase 0.1.2 — Define target users and use cases

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 0 — Project Foundation |
| **Phase** | Phase 0.1 — Product definition |
| **Subphase** | `0.1.2` — Define target users and use cases |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 1 (Principal Architect), Agent 7 (UI/UX Designer), Agent 11 (Documentation Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 1 hour |
| **Actual Duration** | ~5 minutes |
| **Acceptance Criteria** | 1. Define comprehensive target user personas (`USR-01`..`USR-06`) covering Desktop, Mobile, Web, CLI/API, low-resource, and accessibility needs.<br>2. Map all 14 core navigation studios to concrete workflows.<br>3. Author end-to-end use cases (`UC-001`..`UC-016`) and cross-cutting UX journeys (`UXJ-01`..`UXJ-04`) with main flows, error/cancellation flows, and progressive disclosure.<br>4. Verify 100% traceability across all 20 `FR-*` domains and `NFR-*` constraints in `REQUIREMENTS.md`.<br>5. Pass Agent 12 independent review, update append-only `TODO.md` and `CHANGELOG.md`, and commit to `main`. |
| **Test Results** | Agent 12 independent audit: **PASS** (0 defects; 6/6 personas, 14/14 navigation views, 20/20 capability domains & 47/47 `FR-*` IDs verified traced; append-only logs intact). |
| **Files Changed** | `docs/USE_CASES.md`, `REQUIREMENTS.md`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `84207b0` (tagged `stage-0.1.2` on `main`) |
| **Outstanding Issues** | Repository visibility preference (`public` vs `private`) still awaiting user decision ahead of Subphase `0.3.1`. |

### Subphase 0.1.1 — Finalize product requirements

| Field | Detail |
| :--- | :--- |
| **Stage** | Stage 0 — Project Foundation |
| **Phase** | Phase 0.1 — Product definition |
| **Subphase** | `0.1.1` — Finalize product requirements |
| **Status** | `COMPLETE` |
| **Assigned Agents** | Agent 1 (Principal Architect), Agent 11 (Documentation Engineer), Agent 12 (Independent Reviewer) |
| **Planned Maximum Duration** | 1 hour (Simple/Moderate planning window) |
| **Actual Duration** | ~15 minutes |
| **Acceptance Criteria** | 1. Conduct full environment, hardware, toolchain, and GitHub auth audit.<br>2. Author authoritative `REQUIREMENTS.md` covering all 20 capability domains, 8 target platforms, functional requirements (`FR-*`), non-functional requirements (`NFR-*`), traceability matrix, and Two-Tier Engine Architecture.<br>3. Initialize `docs/PROGRESS.md`, `TODO.md` (append-only), and `CHANGELOG.md` (append-only).<br>4. Pass independent review by Agent 12 checking completeness against Master Engineering Contract.<br>5. Initialize local Git repository on `main` and commit baseline artifacts. |
| **Test Results** | Environment audit executed (`rustc 1.99.1`, `cargo 1.99.1`, `flutter 3.38.4`, `node v25.8.2`, `podman 6.1.4`, `git 2.56.0`). Requirements completeness verification: 20/20 capability domains mapped (`FR-DET` through `FR-VAL`), 9/9 NFR baselines specified, 0 placeholders. |
| **Files Changed** | `REQUIREMENTS.md`, `docs/PROGRESS.md`, `TODO.md`, `CHANGELOG.md` |
| **Commit Reference** | `3e2650d` (initial baseline commit on `main`) |
| **Outstanding Issues** | 1. GitHub CLI (`gh`) is not installed on host PATH; repository visibility preference (`public` vs `private`) requested from user ahead of Subphase `0.3.1`.<br>2. Optional external CLI tools (`ffmpeg`, `qpdf`, `tesseract`, `7z`, `libreoffice`, `pandoc`, `calibre`) are not installed on host PATH; documented in Two-Tier Engine Architecture for capability probing and CI/container integration. |

---

## Upcoming Subphases Queue (Stage 0)

| Subphase | Title | Status | Assigned Agent(s) | Planned Max Budget |
| :--- | :--- | :--- | :--- | :--- |
| `0.1.2` | Define target users and use cases | `COMPLETE` | Agent 1, Agent 7, Agent 11, Agent 12 | 1 hour |
| `0.1.3` | Define supported formats and capabilities | `COMPLETE` | Agent 1, Agent 3, Agent 4, Agent 11, Agent 12 | 1 hour |
| `0.1.4` | Define MVP and deferred features | `COMPLETE` | Agent 1, Agent 11, Agent 12 | 1 hour |
| `0.2.1` | Define system architecture | `COMPLETE` | Agent 1, Agent 2, Agent 11, Agent 12 | 2 hours |
| `0.2.2` | Define module boundaries | `COMPLETE` | Agent 1, Agent 2, Agent 12 | 1 hour |
| `0.2.3` | Define Rust and application contracts | `COMPLETE` | Agent 1, Agent 2, Agent 5, Agent 6, Agent 12 | 2 hours |
| `0.2.4` | Create architecture decision records | `COMPLETE` | Agent 1, Agent 11, Agent 12 | 1 hour |
| `0.3.1` | Create GitHub repository | `COMPLETE` | Agent 10, Agent 9, Agent 12 | 1 hour |
| `0.3.2` | Establish monorepo | `COMPLETE` | Agent 1, Agent 2, Agent 10, Agent 12 | 1 hour |
| `0.3.3` | Configure Antigravity rules and skills | `COMPLETE` | Agent 1, Agent 11, Agent 12 | 2 hours |
| `0.3.4` | Configure specialized agents | `COMPLETE` | Agent 1, Agent 11, Agent 12 | 2 hours |
| `0.3.5` | Create documentation foundation | `COMPLETE` | Agent 11, Agent 1, Agent 9, Agent 12 | 2 hours |
| `0.4.1` | Configure GitHub Actions | `COMPLETE` | Agent 10, Agent 12 | 2 hours |
| `0.4.2` | Implement selectable application builds | `COMPLETE` | Agent 10, Agent 2, Agent 5, Agent 6, Agent 12 | 2 hours |
| `0.4.3` | Implement independent artifact uploads | `COMPLETE` | Agent 10, Agent 12 | 1 hour |
| `0.4.4` | Validate runner and platform matrix | `COMPLETE` | Agent 10, Agent 8, Agent 12 | 1 hour |
| `0.4.5` | Validate local build scripts | `COMPLETE` | Agent 10, Agent 8, Agent 12 | 2 hours |

---

## Upcoming Subphases Queue (Stage 1 — Rust Core Foundation)

| Subphase | Title | Status | Assigned Agent(s) | Planned Max Budget |
| :--- | :--- | :--- | :--- | :--- |
| `1.1.1` | Cargo workspace | `COMPLETE` | Agent 2, Agent 1, Agent 8, Agent 12 | 2 hours |
| `1.1.2` | Shared error and logging system | `COMPLETE` | Agent 2, Agent 8, Agent 12 | 2 hours |
| `1.1.3` | Configuration management | `COMPLETE` | Agent 2, Agent 9, Agent 12 | 2 hours |
| `1.2.1` | File signature detection | `COMPLETE` | Agent 2, Agent 9, Agent 8, Agent 12 | 2 hours |
| `1.2.2` | Metadata inspection | `COMPLETE` | Agent 2, Agent 4, Agent 9, Agent 12 | 2 hours |
| `1.2.3` | File classification | `COMPLETE` | Agent 2, Agent 3, Agent 12 | 1 hour |
| `1.2.4` | Capability registry | `COMPLETE` | Agent 2, Agent 3, Agent 4, Agent 12 | 2 hours |
| `1.3.1` | Processing API | `COMPLETE` | Agent 2, Agent 1, Agent 12 | 2 hours |
| `1.3.2` | Plugin/backend contracts | `COMPLETE` | Agent 2, Agent 3, Agent 4, Agent 12 | 2 hours |
| `1.3.3` | Job scheduler | `COMPLETE` | Agent 2, Agent 8, Agent 12 | 4 hours |
| `1.3.4` | Cancellation and progress | `COMPLETE` | Agent 2, Agent 8, Agent 12 | 2 hours |
| `1.3.5` | Resource limits | `COMPLETE` | Agent 2, Agent 9, Agent 8, Agent 12 | 2 hours |

---

## Upcoming Subphases Queue (Stage 2 — Universal Compression Engine)

| Subphase | Title | Status | Assigned Agent(s) | Planned Max Budget |
| :--- | :--- | :--- | :--- | :--- |
| `2.1.1` | Compression Core & Quality Engine (`L1`/`L2`) | `COMPLETE` | Agent 3, Agent 2, Agent 8, Agent 12 | 3 hours |
| `2.1.2` | Hardware acceleration probe (`L1`) | `COMPLETE` | Agent 3, Agent 2, Agent 12 | 2 hours |
| `2.2.1` | Image compression crate (`compression-image`) | `COMPLETE` | Agent 3, Agent 8, Agent 12 | 4 hours |
| `2.2.2` | Archive compression crate (`compression-archive`) | `COMPLETE` | Agent 3, Agent 9, Agent 8, Agent 12 | 4 hours |
| `2.2.3` | Audio compression crate (`compression-audio`) | `COMPLETE` | Agent 3, Agent 8, Agent 12 | 3 hours |
| `2.2.4` | Video compression crate (`compression-video`) | `COMPLETE` | Agent 3, Agent 9, Agent 8, Agent 12 | 3 hours |
| `2.2.5` | PDF & Office compression crates (`compression-pdf`, `compression-office`) | `COMPLETE` | Agent 3, Agent 4, Agent 8, Agent 12 | 4 hours |
| `2.3.1` | Adaptive optimization engine & quality solver (`optimization-engine`) | `COMPLETE` | Agent 3, Agent 2, Agent 8, Agent 12 | 3 hours |
| `2.3.2` | Stage 2 CLI (`compress`/`archive`) & Criterion benchmarks | `COMPLETE` | Agent 2, Agent 3, Agent 8, Agent 12 | 3 hours |

---

## Upcoming Subphases Queue (Stage 3 — Universal Conversion Engine)

| Subphase | Title | Status | Assigned Agent(s) | Planned Max Budget |
| :--- | :--- | :--- | :--- | :--- |
| `3.1.1` | Conversion Core foundation, multi-hop graph router & cross-media conversion (`conversion-core`) | `COMPLETE` | Agent 4, Agent 3, Agent 2, Agent 8, Agent 12 | 3 hours |
| `3.1.2` | Bidirectional structured text, markup & tabular engines (`Markdown`/`HTML`/`CSV`/`JSON`/`YAML`/`XML`) | `COMPLETE` | Agent 4, Agent 2, Agent 8, Agent 12 | 4 hours |
| `3.2.1` | Native Office, ODF & EPUB conversions + Tier 2 `LibreOffice`/`Pandoc`/`Calibre` adapters | `COMPLETE` | Agent 4, Agent 9, Agent 8, Agent 12 | 4 hours |
| `3.2.2` | Stage 3 CLI (`filefusion convert`), `job-manager` wiring & conversion fidelity corpus tests | `COMPLETE` | Agent 2, Agent 4, Agent 8, Agent 12 | 3 hours |

---

## Upcoming Subphases Queue (Stage 4 — PDF & Document Studio, Repair & Security Engines)

| Subphase | Title | Status | Assigned Agent(s) | Planned Max Budget |
| :--- | :--- | :--- | :--- | :--- |
| `4.1.1` | Structural PDF manipulation (`merge`, `split`, `reorder`, `rotate`, `crop`, `watermark`, scan detection) | `COMPLETE` | Agent 4, Agent 3, Agent 8, Agent 12 | 3 hours |
| `4.1.2` | Local OCR (`TesseractAdapter`, searchable PDF text overlay) + PDF encryption, permissions & `/Sig` verification | `COMPLETE` | Agent 4, Agent 9, Agent 8, Agent 12 | 4 hours |
| `4.2.1` | Unified file repair (`PDF`/`ZIP`/media index), `.ffenc` container & privacy metadata sanitizer/editor orchestration | `COMPLETE` | Agent 3, Agent 9, Agent 2, Agent 12 | 4 hours |
| `4.2.2` | Stage 4 CLI (`filefusion pdf`, `repair`, `metadata`, `encrypt`/`decrypt`) & Stage 4 security/recovery corpus tests | `COMPLETE` | Agent 2, Agent 4, Agent 8, Agent 9, Agent 12 | 3 hours |

---

## Upcoming Subphases Queue (Stage 5 — File Intelligence & Storage)

| Subphase | Title | Status | Assigned Agent(s) | Planned Max Budget |
| :--- | :--- | :--- | :--- | :--- |
| `5.1.1` | Multi-stage exact duplicate scanner (`size -> 4 KB head/tail -> BLAKE3`), streaming `FastCDC`, `aHash`/`dHash`/`pHash` clustering & storage treemap analytics (`optimization-engine`) | `COMPLETE` | Agent 3, Agent 2, Agent 8, Agent 12 | 4 hours |
| `5.1.2` | Unified file comparison/diff (`compare_files` / visual SSIM/PSNR / CDC delta) & safe space-reclamation planner (hardlinks + reversible quarantine) (`optimization-engine`) | `QUEUED` | Agent 3, Agent 9, Agent 8, Agent 12 | 4 hours |
| `5.2.1` | L5 storage intelligence & safe cleanup orchestration (`JobManager` Stage 5 coordinator, batch dedup/treemap/diff/quarantine/restore) | `QUEUED` | Agent 2, Agent 9, Agent 8, Agent 12 | 3 hours |
| `5.2.2` | Stage 5 CLI (`filefusion dedup`, `diff`, `storage`) & Stage 5 recovery/quarantine/hardlink corpus tests | `QUEUED` | Agent 2, Agent 8, Agent 9, Agent 12 | 3 hours |
