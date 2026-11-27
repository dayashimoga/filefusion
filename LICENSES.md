# FileFusion — Dependency License Register & Compliance Policy

**Document ID:** `FF-LIC-001`  
**Version:** `1.0.0`  
**Architecture Reference:** [docs/DECISION_LOG.md](docs/DECISION_LOG.md) (`ADR-002`) | [deny.toml](deny.toml)

---

## 1. Two-Tier License Isolation Policy (`MOD-RULE-05`)

FileFusion distributes its core Rust workspace (`crates/*`) under the dual **`Apache-2.0 OR MIT`** license ([LICENSE](LICENSE)) and enforces strict separation between permissive core dependencies and copyleft external powerhouse engines:

### Tier 1 — Statically Linked Core Crates (`deny.toml` Enforced)
Allowed licenses for built-in Rust dependencies:
- `MIT`, `Apache-2.0`, `Apache-2.0 WITH LLVM-exception`
- `BSD-2-Clause`, `BSD-3-Clause`, `ISC`, `Zlib`, `BSL-1.0`, `CC0-1.0`
- `MPL-2.0` (weak file-level copyleft, compatible when unmodified or published per MPL-2.0 terms)
- `Unicode-3.0`, `Unicode-DFS-2016`

**Forbidden in Tier 1:** `GPL-2.0`, `GPL-3.0`, `AGPL-3.0`, and unverified proprietary dependencies (`cargo deny check licenses` fails CI on violation).

### Tier 2 — Out-of-Process External Engine Adapters
Mature external tools are discovered at runtime by `codec-registry` and invoked out-of-process via `IsolatedProcessRunner` (`FF-CTR-001` §3.3). They are **not** statically linked into Tier 1 crates or mobile App Store binaries:

| External Engine | Typical Upstream License | Capabilities Provided | Distribution & Attribution Rule |
| :--- | :--- | :--- | :--- |
| **FFmpeg** (`ffmpeg`/`ffprobe`) | `LGPL-2.1+` or `GPL-2.0+/3.0` (build dependent) | Video/audio transcoding, fast-path remuxing, subtitle muxing | Runtime-discovered or independently packaged with full upstream license & source-offer notice |
| **LibreOffice** (`soffice`) | `MPL-2.0` | Complex print-layout Office/ODF -> PDF rendering | Runtime-discovered local install or isolated server container |
| **Pandoc** | `GPL-2.0+` | Extended academic/document format transformations | Runtime-discovered optional adapter |
| **Calibre** (`ebook-convert`) | `GPL-3.0` | Legacy `MOBI` / `AZW3` conversions | Runtime-discovered optional adapter |
| **qpdf** | `Apache-2.0` (or `Artistic-2.0` legacy) | Advanced PDF linearization, AES-256 encryption, `xref` recovery | Runtime-discovered or permissive-compatible bundle |
| **Ghostscript** | `AGPL-3.0` (or commercial) | Optional deep PDF raster/stream re-distillation | Strictly optional runtime-discovered user binary (never bundled in Tier 1) |
| **MuPDF** | `AGPL-3.0` (or commercial) | Optional PDF inspection/rendering backend | Strictly optional runtime-discovered or avoided in favor of permissive parsers |
| **Tesseract OCR** | `Apache-2.0` | Local multi-language OCR & searchable PDF text layers | Runtime-discovered or permissive bundle with `tessdata` attribution |
| **7-Zip** (`7z`) | `LGPL-2.1+` (+ unRAR notice if full) | Extended `.7z` archive fallback | Runtime-discovered optional adapter complementing pure-Rust `sevenz-rust` |

---

## 2. SBOM & Release Compliance Gate

- Before any tagged release (Stage 12), CI generates a machine-readable SBOM alongside third-party license notices for every platform artifact (`release-certification`).
- Every new crate added to `Cargo.toml` or package added to `pubspec.yaml` / `package.json` must pass `deny.toml` / license verification.
