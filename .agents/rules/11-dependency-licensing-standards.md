# Rule 11 — Dependency and Licensing Standards

- **Reuse Mature Open-Source Libraries:** Prefer audited Rust crates and mature native engines (`ADR-002`) over reinventing codecs.
- **Two-Tier License Isolation (`MOD-RULE-05`):** Tier 1 Rust crates (`L0`–`L5`) must use permissive/weak licenses listed in `deny.toml` (`MIT`, `Apache-2.0`, `BSD`, `Zlib`, `ISC`, `MPL-2.0`). Never statically link GPL/AGPL libraries (`FFmpeg` GPL, `Ghostscript`, `MuPDF`) into Tier 1 crates.
- **Out-of-Process Tier 2 Adapters:** Isolate GPL/LGPL/external tools (`ffmpeg`, `qpdf`, `tesseract`, `libreoffice`, `pandoc`, `calibre`, `7z`) behind `ExternalToolAdapter` with SPDX tracking in `codec-registry`.
- **SBOM & Audit Gate:** Keep `LICENSES.md` current, forbid wildcard versions (`wildcards = "deny"`) and unknown git sources (`deny.toml`), and run `cargo-deny` / dependency vulnerability audits in CI.
- **Minimal Dependency Bloat:** Justify every new crate or npm/pub dependency against binary size, compile time, and maintenance health.
