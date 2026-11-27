# FileFusion — Supported Formats, Codecs & Capability Matrix

**Document ID:** `FF-FMT-001`  
**Version:** `1.0.0`  
**Status:** `APPROVED FOR STAGE 0 BASELINE`  
**Subphase Origin:** `0.1.3 — Define supported formats and capabilities`  
**Parent Specifications:** [REQUIREMENTS.md](../REQUIREMENTS.md) (`FF-REQ-001`), [docs/USE_CASES.md](USE_CASES.md) (`FF-UC-001`)  
**Owners:** Agent 1 (Principal Architect), Agent 3 (Compression & Codec Engineer), Agent 4 (Document & Conversion Engineer), Agent 11 (Documentation Engineer)

---

## 1. Engine Tier & License Isolation Model

To satisfy cross-platform portability (Windows, Linux, macOS, Android, iOS, Web/WASM, CLI, API) and strict open-source license hygiene, every format capability belongs to one of two execution tiers:

| Tier | Integration Mechanism | Availability | Typical License Class | Redistribution & Sandboxing Policy |
| :--- | :--- | :--- | :--- | :--- |
| **Tier 1 (Built-in Core)** | Compiled directly into `filefusion` Rust crates | **100% Guaranteed Offline** across Desktop, Mobile, CLI, API (and subset in Web/WASM) | Permissive (`MIT`, `Apache-2.0`, `BSD-2/3-Clause`, `Zlib`, `ISC`, `MPL-2.0`) | Statically linked memory-safe Rust (or audited permissive C libraries where safe). Zero external runtime downloads needed. |
| **Tier 2 (Pluggable Adapter)** | Runtime-discovered external CLI/shared adapter via `codec-registry` | Desktop, CLI, Self-Hosted API (and remote offload for Mobile/Web) | Mixed / Copyleft (`GPL-2.0/3.0`, `LGPL-2.1/3.0`, `AGPL-3.0`, `MPL-2.0`) | Executed out-of-process under resource/timeout guards (`FR-SEC-001`). Never statically linked into proprietary or mobile app-store binaries without explicit license compliance approval. |

Every capability status uses the following codes:
- `Y1` = Supported natively via **Tier 1** (always available offline).
- `Y2` = Supported via **Tier 2** external adapter (discovered via `filefusion doctor`).
- `Y1/Y2` = Tier 1 handles standard/structural cases; Tier 2 enhances speed, complex layouts, or specialized codecs.
- `PARTIAL` = Inspection/extraction or subset conversion supported; gaps reported in Pre-Flight Warnings (`FR-FID-002`).
- `NO` = Not supported (explicitly rejected rather than faked).

---

## 2. Universal File Detection & Signature Matrix (`FR-DET`)

All intake paths identify files via magic bytes and container structure probing (`FR-DET-001`), never trusting file extensions alone (`FR-DET-002`):

| Format Family | Primary Magic Bytes / Signatures | Container Disambiguation Rule | Spoofing Risk Flagged |
| :--- | :--- | :--- | :--- |
| **PNG** | `89 50 4E 47 0D 0A 1A 0A` | Verify `IHDR` chunk offset; detect `acTL` for APNG | Renamed HTML/EXE with `.png` extension |
| **JPEG** | `FF D8 FF` (`E0`/`E1`/`DB`/`EE`) | Parse SOF markers for subsampling & progressive mode | Truncated EOI (`FF D9`) flagged for repair/warn |
| **WebP** | `52 49 46 46` (`RIFF`) + `57 45 42 50` (`WEBP` @ +8) | Distinguish `VP8 ` (lossy), `VP8L` (lossless), `VP8X` (alpha/anim) | Disambiguated from RIFF WAV and AVI |
| **AVIF / HEIC** | `.... 66 74 79 70` (`ftyp` @ +4) | Check major/compatible brands (`avif`, `avis`, `heic`, `mif1`) | Disambiguated from MP4/MOV ISO-BMFF |
| **MP4 / MOV** | `.... 66 74 79 70` (`ftyp` @ +4) | Check `isom`, `mp42`, `M4A `, `qt  `; probe `moov`/`mdat` atoms | Unfinalized `mdat` without `moov` flagged for `FR-REP` |
| **MKV / WebM** | `1A 45 DF A3` (EBML header) | Parse `DocType` string (`matroska` vs `webm`) | Disambiguated cleanly at EBML header |
| **PDF** | `%PDF-1.x` / `%PDF-2.0` (within first 1024 B) | Scan trailer `%%EOF`, `startxref`, `/Encrypt`, text vs image-only pages | Missing `%%EOF`/`xref` routed to `FR-REP-001` |
| **ZIP / OOXML / EPUB / ODF** | `50 4B 03 04` (`PK..`) or `50 4B 05 06` (empty) | Probe central directory & entry paths (`word/document.xml` -> DOCX, `xl/` -> XLSX, `ppt/` -> PPTX, `mimetype` -> EPUB/ODT/ODS/ODP) | Zip-slip (`../`) or extreme compression ratio flagged (`FR-SEC-001`) |
| **7-Zip (`7z`)** | `37 7A BC AF 27 1C` | Parse start header & encoded header CRC | Corrupt end-header flagged for recovery assessment |
| **TAR / Zstd / Gzip / XZ / Brotli / LZ4** | `ustar` @ +257; `28 B5 2F FD`; `1F 8B`; `FD 37 7A 58 5A 00`; `04 22 4D 18` | Probe inner TAR header inside compressed stream when feasible | Decompression bomb byte limit enforced |
| **FLAC / WAV / MP3 / OGG** | `fLaC`; `RIFF`+`WAVE`; `ID3` or frame sync `FF FB`; `OggS` | Inspect Vorbis/Opus head packet inside `OggS` | Misnamed raw PCM/MP3 streams caught |
| **Text / Markup / Data** | BOM (`EF BB BF`, `FF FE`) + UTF-8 validity + structural tokens (`{`, `---`, `<!DOCTYPE`, `<svg`, `,`/`;` tabular consistency) | Reject high-entropy binary payloads disguised as `.txt`/`.csv` | Active `<script>` HTML disguised as `.txt`/`.pdf` flagged |

---

## 3. Image Formats & Optimization Matrix (`FR-IMG`, `FR-CMP`, `FR-VAL`)

| Format | Decode | Encode | Lossless Mode | Lossy Mode | Alpha | Animation | Metadata (EXIF/XMP/ICC) | Engine Backend (Tier) | Quality / Verification Metric |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :--- | :--- |
| **JPEG** (`.jpg`, `.jpeg`) | `Y1` | `Y1/Y2` | `Y1` (lossless metadata strip / progressive repack) | `Y1` (default) | `NO` | `NO` | `Y1` (read/write/scrub, auto-orient) | Tier 1: `image` (`zune-jpeg` / MozJPEG-compatible Rust); Tier 2: `libjpeg-turbo` | SSIM, PSNR, byte delta, no-op guard |
| **PNG** (`.png`) | `Y1` | `Y1` | `Y1` (default) | `PARTIAL` (palette quantization opt-in) | `Y1` | `Y1` (APNG inspect) | `Y1` (tEXt/iTXt/eXIf/iCCP) | Tier 1: `image` + `oxipng` (Zopfli/libdeflater stream optimization) | Bit-exact pixel verification (RGBA lossless) |
| **WebP** (`.webp`) | `Y1` | `Y1` | `Y1` (`VP8L`) | `Y1` (`VP8`) | `Y1` | `Y1` (Tier 1/2) | `Y1` (EXIF/XMP/ICC) | Tier 1: `image-webp` / `webp`; Tier 2: `libwebp` | SSIM/PSNR (lossy) or pixel-exact (lossless) |
| **AVIF** (`.avif`) | `Y1` | `Y1` | `Y1` (lossless YUV/identity) | `Y1` (default) | `Y1` | `PARTIAL` | `Y1` (ICC/EXIF) | Tier 1: `ravif` + `rav1e` / `dav1d` or pure-Rust decoder; Tier 2: `libavif` | SSIM, PSNR, speed/size Pareto curve |
| **TIFF** (`.tif`, `.tiff`) | `Y1` | `Y1` | `Y1` (Deflate/LZW/PackBits) | `PARTIAL` | `Y1` | `NO` | `Y1` | Tier 1: `tiff` (`image`) | Pixel-exact lossless verification |
| **GIF** (`.gif`) | `Y1` | `Y1` | `Y1` (256-color palette) | `NO` | `Y1` (1-bit) | `Y1` | `Y1` (comment) | Tier 1: `gif` (`image`); conversion to WebP/MP4 supported | Frame count & duration preservation |
| **BMP / ICO** | `Y1` | `Y1` | `Y1` | `NO` | `Y1` (ICO/32-bit BMP) | `NO` | `NO` | Tier 1: `image` | Exact dimension & pixel match |
| **SVG** (`.svg`) | `Y1` | `PARTIAL` | `Y1` (XML/path minification) | `NO` | `Y1` | `PARTIAL` | `Y1` | Tier 1: `resvg` / `usvg` (safe XML entity guard against billion-laughs); rasterize to PNG/WebP/PDF | Vector-to-raster bounds & anti-XXE check |

**Image Resampling & Color Operations (`FR-IMG-002`):**
- SIMD-accelerated resizing via `fast_image_resize` (Lanczos3, CatmullRom, Mitchell, Bilinear, Nearest).
- Automatic EXIF orientation normalization before stripping metadata (`UC-010`) so rotated photos never flip sideways.
- ICC profile preservation or sRGB conversion before web delivery.

---

## 4. Video Formats, Codecs & Hardware Matrix (`FR-AV`, `FR-ADP`)

| Container | Read/Probe | Write/Mux | Fast-Path Remux (`FR-AV-001`) | Supported Video Codecs | Supported Subtitle Tracks | Engine Backend |
| :--- | :---: | :---: | :---: | :--- | :--- | :--- |
| **MP4** (`.mp4`, `.m4v`) | `Y1/Y2` | `Y2` | `Y2` (H.264/HEVC/AV1 + AAC/Opus) | H.264, H.265/HEVC, AV1 | `mov_text`, external SRT/VTT | Tier 1 ISO-BMFF prober + Tier 2 `ffmpeg` |
| **MKV** (`.mkv`) | `Y1/Y2` | `Y2` | `Y2` (nearly all streams) | H.264, H.265/HEVC, VP9, AV1 | SRT, ASS/SSA, WebVTT, PGS | Tier 1 EBML prober + Tier 2 `ffmpeg` |
| **WebM** (`.webm`) | `Y1/Y2` | `Y2` | `Y2` (VP9/AV1 + Opus/Vorbis) | VP9, AV1 | WebVTT | Tier 1 EBML prober + Tier 2 `ffmpeg` |
| **MOV** (`.mov`) | `Y1/Y2` | `Y2` | `Y2` (to MP4/MKV when compatible) | H.264, HEVC, ProRes (input) | `mov_text` | Tier 1 ISO-BMFF prober + Tier 2 `ffmpeg` |
| **AVI** (`.avi`) | `Y1/Y2` | `PARTIAL` | `Y2` (legacy input -> MP4/MKV) | Legacy MPEG-4 / H.264 | External SRT | Tier 1 RIFF prober + Tier 2 `ffmpeg` |

### Video Codec & Hardware Acceleration Matrix (`FR-ADP-002`)

| Codec | Software Encoder (CPU-Only Baseline) | Hardware Acceleration Backends (Optional) | Recommended Use Case | Quality Verification |
| :--- | :--- | :--- | :--- | :--- |
| **H.264 (AVC)** | `libx264` (Tier 2) | `h264_nvenc` (NVIDIA), `h264_qsv` (Intel), `h264_amf` (AMD), `h264_videotoolbox` (macOS), `MediaCodec` (Android) | Maximum device compatibility, fast sharing | VMAF / SSIM / PSNR + duration/frame check |
| **H.265 (HEVC)** | `libx265` (Tier 2) | `hevc_nvenc`, `hevc_qsv`, `hevc_amf`, `hevc_videotoolbox`, `MediaCodec` | 4K archival & mobile size reduction (~35–45% smaller than H.264) | VMAF / SSIM + 10-bit color tag check |
| **VP9** | `libvpx-vp9` (Tier 2) | `vp9_qsv` (Intel) | WebM web video compatibility | VMAF / SSIM |
| **AV1** | `SVT-AV1` (`libsvtav1`), `rav1e` (Tier 1/2 Rust crate) | `av1_nvenc` (RTX 40+), `av1_qsv` (Arc), `av1_amf` | Best compression efficiency (~30% smaller than HEVC at equal VMAF) | VMAF / SSIM + CRF binary-search solver |

---

## 5. Audio Formats & Codecs Matrix (`FR-AV-002`, `FR-MET-001`)

| Format | Lossless / Lossy | Decode | Encode | Tagging Metadata | Normalization (`EBU R128`) | Engine Backend |
| :--- | :--- | :---: | :---: | :--- | :---: | :--- |
| **FLAC** (`.flac`) | Lossless | `Y1/Y2` | `Y1/Y2` | Vorbis Comments + Cover Art | `Y2` (Gain tags or PCM resample) | Tier 1 (`claxon` / `symphonia` + `flacenc`) + Tier 2 `ffmpeg` |
| **WAV** (`.wav`) | Lossless (PCM) | `Y1` | `Y1` | RIFF INFO / BWF | `Y1/Y2` | Tier 1 (`hound`) + Tier 2 `ffmpeg` |
| **ALAC** (`.m4a`) | Lossless | `Y1/Y2` | `Y2` | MP4 iTunes atoms | `Y2` | Tier 1 `symphonia` decode + Tier 2 `ffmpeg` |
| **Opus** (`.opus`, `.ogg`) | Lossy (Best efficiency) | `Y1/Y2` | `Y2` | Vorbis Comments | `Y2` | Tier 1 `symphonia` decode + Tier 2 `libopus` / `ffmpeg` |
| **AAC** (`.m4a`, `.aac`) | Lossy (Universal) | `Y1/Y2` | `Y2` | MP4 iTunes atoms + Cover Art | `Y2` | Tier 1 `symphonia` decode + Tier 2 `ffmpeg` / OS native |
| **MP3** (`.mp3`) | Lossy (Legacy) | `Y1/Y2` | `Y2` | ID3v2.3 / ID3v2.4 (`id3` crate) | `Y2` | Tier 1 `symphonia`/`id3` + Tier 2 `libmp3lame`/`ffmpeg` |
| **Vorbis** (`.ogg`) | Lossy | `Y1/Y2` | `Y2` | Vorbis Comments | `Y2` | Tier 1 `symphonia` + Tier 2 `ffmpeg` |

---

## 6. Document, PDF, Office & Ebook Matrix (`FR-DOC`, `FR-PDF`, `FR-FID`)

### 6.1 Documents, Spreadsheets, Presentations & Ebooks

| Format | Inspect / Extract Text | Structured Convert (To/From) | Fidelity Pre-Flight Warnings (`FR-FID-002`) | Engine Backend |
| :--- | :---: | :--- | :--- | :--- |
| **Markdown** (`.md`) | `Y1` | `Y1` -> HTML, PDF (basic), EPUB, DOCX (basic), TXT; `Y2` -> full Pandoc targets | Warns if custom raw HTML blocks are stripped on plain-text export | Tier 1 (`pulldown-cmark`) + Tier 2 `pandoc` |
| **HTML** (`.html`, `.htm`) | `Y1` | `Y1` -> Markdown, TXT, EPUB; `Y1/Y2` -> PDF, DOCX | Warns if external CSS/JS/iframes are stripped for security/offline safety | Tier 1 (`scraper`/`html5ever`) + Tier 2 `pandoc` |
| **CSV / TSV / JSON / YAML** | `Y1` | `Y1` <-> CSV, TSV, JSON, YAML, XLSX (tabular data) | Warns on nested JSON flattening to CSV | Tier 1 (`csv`, `serde_json`, `serde_yaml`, `calamine`, `rust_xlsxwriter`) |
| **DOCX** (`.docx`) | `Y1` | `Y1` -> TXT, Markdown, HTML, Metadata scrub; `Y2` -> high-fidelity PDF, ODT, EPUB | Warns if macros (`.docm`), Track Changes, or complex WordArt require LibreOffice layout | Tier 1 OOXML ZIP/XML parser + Tier 2 `libreoffice`/`pandoc` |
| **XLSX** (`.xlsx`) | `Y1` | `Y1` <-> CSV, JSON, ODS (data); `Y2` -> PDF, full-style ODS | Warns that CSV export drops formulas, charts, formatting, and secondary sheets | Tier 1 (`calamine` + `rust_xlsxwriter`) + Tier 2 `libreoffice` |
| **PPTX** (`.pptx`) | `Y1` | `Y1` -> speaker notes/text/embedded media extract; `Y2` -> PDF, ODP | Warns that slide animations/transitions do not translate to static PDF | Tier 1 OOXML XML parser + Tier 2 `libreoffice` |
| **ODT / ODS / ODP** | `Y1` | `Y1` -> text/metadata/CSV (`ODS`); `Y2` <-> DOCX/XLSX/PPTX/PDF | Same layout fidelity checks as OOXML | Tier 1 ODF ZIP/XML parser + Tier 2 `libreoffice` |
| **EPUB 2 / 3** (`.epub`) | `Y1` | `Y1` <-> Markdown, HTML, TXT; `Y2` -> PDF, MOBI, AZW3 | Warns on DRM-encrypted EPUBs (explicitly rejected; no DRM circumvention) | Tier 1 native EPUB/OPF builder + Tier 2 `calibre` (`ebook-convert`) |
| **MOBI / AZW3** | `PARTIAL` | `Y2` <-> EPUB, PDF, HTML (DRM-free files only) | Rejects DRM-protected files clearly; requires Calibre adapter | Tier 2 `calibre` (`ebook-convert`) |

### 6.2 PDF Studio & OCR Capabilities (`FR-PDF`)

| Capability | Engine Tier | Notes & Constraints |
| :--- | :---: | :--- |
| Merge, Split, Extract Pages, Reorder, Rotate, Crop | `Y1` / `Y1+Y2` | Tier 1 pure-Rust (`lopdf` / `pdf-writer`) handles structural tree & crop-box operations; Tier 2 `qpdf` handles linearized/complex recovery |
| Metadata Inspection & Privacy Scrubbing | `Y1` | Strips Info dict + XMP XML packets atomically |
| PDF Stream & Embedded Image Compression | `Y1/Y2` | Recompresses embedded raster images (JPEG/PNG -> optimized JPEG/Flate) + object stream compression (`qpdf`/`ghostscript`) |
| Watermarking & Page Numbering | `Y1` | Vector/text overlay stream injection with opacity & rotation |
| AES-256 Password Protection, Permissions & Digital Signature Verification | `Y1/Y2` | Tier 2 `qpdf` (standard R=6 AES-256) or validated Tier 1 encryption writer; cryptographic CMS/PKCS#7 signature verification |
| OCR & Searchable PDF Generation | `Y2` (or `Y1` wrapper) | Tesseract OCR (`tesseract`) renders/reads page images and builds aligned invisible text layer + extracts TXT/CSV tables |
| Broken `xref` / Trailer Repair | `Y1/Y2` | Tier 1 object scanner + Tier 2 `qpdf` reconstruction with explicit salvageability status (`FR-REP-002`) |

---

## 7. Archive & Stream Compression Matrix (`FR-ARC`, `FR-SEC`)

| Format | Create | Extract | Inspect / Test Integrity | Re-Pack Target | Encryption | Engine Backend |
| :--- | :---: | :---: | :---: | :---: | :--- | :--- |
| **ZIP** (`.zip`) | `Y1` | `Y1` | `Y1` | `Y1` (to `.tar.zst`, `.7z`, `.ffenc`) | `Y1` (AES-256 / ZipCrypto warn) | Tier 1 (`zip` crate) |
| **TAR** (`.tar`) | `Y1` | `Y1` | `Y1` | `Y1` | Wrap in `.ffenc` or encrypted `.7z` | Tier 1 (`tar` crate) |
| **TAR.ZST / Zstandard** (`.zst`, `.tzst`) | `Y1` | `Y1` | `Y1` | `Y1` (Preferred high-speed/high-ratio archival) | Wrap in `.ffenc` | Tier 1 (`zstd` crate) |
| **TAR.GZ / Gzip** (`.gz`, `.tgz`) | `Y1` | `Y1` | `Y1` | `Y1` | Wrap in `.ffenc` | Tier 1 (`flate2` + `zlib-rs`) |
| **TAR.XZ / LZMA** (`.xz`, `.txz`) | `Y1` | `Y1` | `Y1` | `Y1` | Wrap in `.ffenc` | Tier 1 (`xz2` / `lzma-rs`) |
| **TAR.BZ2 / Bzip2** (`.bz2`) | `Y1` | `Y1` | `Y1` | `Y1` | Wrap in `.ffenc` | Tier 1 (`bzip2`) |
| **Brotli / LZ4** (`.br`, `.lz4`) | `Y1` | `Y1` | `Y1` | `Y1` | Wrap in `.ffenc` | Tier 1 (`brotli`, `lz4_flex`) |
| **7-Zip** (`.7z`) | `Y1/Y2` | `Y1/Y2` | `Y1/Y2` | `Y1/Y2` | `Y1/Y2` (AES-256) | Tier 1 (`sevenz-rust`) + Tier 2 `7z` CLI |
| **FileFusion Encrypted Container** (`.ffenc`) | `Y1` | `Y1` | `Y1` | `NO` | `Y1` (**AES-256-GCM / ChaCha20-Poly1305 + Argon2id**) | Tier 1 (`aes-gcm`, `chacha20poly1305`, `argon2`) |

**Mandatory Security Guards for Every Archive Reader (`FR-SEC-001`, `NFR-SEC-001`):**
1. Reject any member containing `..`, leading `/` or `\`, Windows drive letters (`C:`), or null bytes.
2. Reject or refuse extraction of symlinks/hardlinks pointing outside the canonical extraction root.
3. Enforce configurable ceilings: default max expansion ratio `100:1` (configurable), max uncompressed extraction size, and max nested archive recursion depth (`default = 3`).

---

## 8. Platform Availability Matrix (`PLAT-*`)

| Capability Domain | Windows / Linux / macOS (`Desktop` & `CLI`) | Android / iOS (`Mobile`) | Responsive Web (`WASM Local`) | Web + Self-Hosted API (`Cloud/Offload`) |
| :--- | :---: | :---: | :---: | :---: |
| File Detection, Hashing & Spoofing Check | `Y1` | `Y1` | `Y1` (WASM) | `Y1` |
| Image Decode/Encode/Resize/Optimize | `Y1` | `Y1` (memory-capped) | `Y1` (WASM, up to tab heap limit) | `Y1` |
| Archive Create/Extract/Repack (`ZIP`, `TAR`, `Zstd`, `7z`) | `Y1` | `Y1` | `Y1` (WASM streaming) | `Y1` |
| Metadata Inspect & Privacy Scrub | `Y1` | `Y1` | `Y1` (WASM) | `Y1` |
| PDF Merge/Split/Watermark/Protect | `Y1` (+ `Y2` `qpdf`) | `Y1` | `Y1` (WASM) | `Y1` + `Y2` |
| Structured Doc/Ebook (`MD`, `HTML`, `CSV`, `EPUB`, basic `DOCX`) | `Y1` | `Y1` | `Y1` (WASM) | `Y1` |
| Complex Office Print Layout (`DOCX`/`PPTX`/`XLSX` -> PDF) | `Y2` (`LibreOffice`/`Pandoc`) | `PARTIAL` (remote API offload if configured) | `PARTIAL` (API offload) | `Y2` |
| Video/Audio Heavy Transcoding (`AV1`/`HEVC`/`Opus`) | `Y2` (`FFmpeg` + HW accel) | `Y1/Y2` (OS MediaCodec/AVFoundation or API offload) | `PARTIAL` (WebCodecs / ffmpeg.wasm small clips or API) | `Y2` |
| Local OCR (`Tesseract`) | `Y2` (`tesseract`) | `PARTIAL` (on-device OCR bridge or API) | `PARTIAL` (API offload) | `Y2` |
| Deduplication, FastCDC & Safe Quarantine | `Y1` | `Y1` (scoped media folders) | `PARTIAL` (selected directory handle) | `Y1` (server storage) |
| Encrypted `.ffenc` Containers | `Y1` | `Y1` | `Y1` (WASM) | `Y1` |

---

## 9. Multi-Hop Conversion Graph & Fast-Path Policy (`FR-DOC`, `FR-AV`, Stage 3.4)

1. **Direct Route First:** Always prefer a direct Tier 1 or Tier 2 encoder (e.g., `PNG -> AVIF`, `DOCX -> Markdown`, `MKV -> MP4` remux) to minimize generational loss.
2. **Lossless Intermediate Hub:** When a multi-hop route is required (e.g., `PPTX -> PDF -> PNG` page previews, or `FLAC -> WAV -> Opus`), intermediate data passes through a lossless representation (`PNG` / `PCM` / `PDF`) inside bounded temporary buffers and is immediately purged on completion or cancellation.
3. **Explicit No-Go Boundaries:** FileFusion refuses nonsensical or destructive conversions (e.g., `Audio MP3 -> XLSX`, `Encrypted DRM EPUB -> PDF`, `Lossy JPEG -> "Lossless" PNG` without explicit user override warning that file size will balloon without adding detail).
