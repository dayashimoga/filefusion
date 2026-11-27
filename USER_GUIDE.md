# FileFusion — User Guide

**Document ID:** `FF-UG-001`  
**Version:** `1.0.0` (Stage 0 Foundation Edition)  
**Product Scope:** [REQUIREMENTS.md](REQUIREMENTS.md) | [docs/USE_CASES.md](docs/USE_CASES.md)

---

## 1. Current Availability Notice

FileFusion is currently completing **Stage 0 (Project Foundation)** ahead of **Milestone A (`MVP-v0.1`: Stages 1–3 Rust Core & CLI)** and **Milestone B (`v1.0-GA`: Stages 4–8 Desktop, Mobile & Web Studios)**.

> In accordance with our [Global Engineering Contract](docs/ENGINEERING_CONTRACT.md), this guide documents only verified capabilities as they ship. Today, only the `file-core` Rust workspace anchor is compiled; interactive CLI and GUI studios activate incrementally in Stages 1–8.

---

## 2. Planned Studio & CLI Experience

When Milestone A (`MVP-v0.1`) and Milestone B (`v1.0-GA`) ship, users interact with FileFusion through **14 unified studios** and the `filefusion` CLI:

1. **Home / Workspace:** Drag-and-drop universal intake with instant magic-byte inspection and spoofing alerts (`UC-001`).
2. **Convert & Compress & Optimize:** Format conversion with Pre-Flight feature warnings, Conversion Fidelity Reports, adaptive codec selection, and the automatic No-Op Guard (never inflates already-optimized files).
3. **Media & Document Studios:** Dedicated **PDF Studio** (merge, split, rotate, crop, watermark, AES-256 encrypt), **Image Studio**, **Video Studio** (fast-path remux + H.264/HEVC/VP9/AV1), **Audio Studio** (FLAC/Opus/AAC + EBU R128), **Archive Manager** (ZIP/TAR/Zstd/Brotli/7z + `.ffenc`), and **OCR & Extraction** (local Tesseract).
4. **Intelligence, Repair & Automation:** **File Intelligence** (metadata privacy scrub, BLAKE3/FastCDC/pHash duplicate cleanup), **File Repair**, **Automation Workflows** (visual & YAML DAGs), **History & Jobs** (`<= 500 ms` cancel + visual diff), and **Settings** (Light/Dark theme, CPU/RAM caps, `doctor` diagnostics, opt-in AI/cloud controls).

---

## 3. Privacy & Offline Guarantee

- **100% Local-First:** Core inspection, conversion, compression, PDF editing, OCR, and deduplication never upload your files.
- **No Silent AI Uploads:** Stage 9 AI assistance is disabled by default and requires explicit confirmation before any external API request.
