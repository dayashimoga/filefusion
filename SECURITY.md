# FileFusion — Security Policy & Threat Model

**Document ID:** `FF-SEC-001`  
**Version:** `1.0.0`  
**Architecture Reference:** [ARCHITECTURE.md](ARCHITECTURE.md) §5 (`FF-ARCH-001`) | [docs/CONTRACTS.md](docs/CONTRACTS.md) §2 (`FF-CTR-001`)

---

## 1. Threat Model & Core Security Posture

Every input file, filename, path, archive member, metadata dictionary, and XML/SVG payload entering FileFusion is **untrusted input**.

| Threat Vector | Mitigation Control | Verification Target |
| :--- | :--- | :--- |
| **Archive Path Traversal (Zip-Slip)** | Canonicalize output roots; reject `..`, leading `/` or `\`, Windows drive prefixes (`C:`), null bytes, and symlinks/hardlinks escaping extraction root (`FF-ERR-SEC-TRAVERSAL`) | `tests/security/` traversal corpus (`NFR-SEC-001`) |
| **Decompression Bombs / Billion Laughs** | Enforce `ResourceLimits`: default `100:1` max expansion ratio, configurable max uncompressed bytes, `max_archive_depth = 3`, and SVG/XML entity limits (`FF-ERR-SEC-BOMB`) | `tests/security/` zip/zstd/svg bomb fixtures |
| **Extension Spoofing / Masquerading** | Magic-byte and structural container probing (`FR-DET-001..002`) flags mismatches between claimed extension and binary signature (`FF-ERR-SEC-SPOOFED`) | `file-core` signature unit & spoofing tests |
| **Command / Shell Injection** | Tier 2 binaries run strictly via `IsolatedProcessRunner` using `Vec<OsString>` argument arrays (`MOD-RULE-04`); never `sh -c` or `cmd /c` | Static review + argument escaping tests |
| **Unsafe Memory & Panics** | Root workspace enforces `#![forbid(unsafe_code)]`, `unwrap_used = "deny"`, `panic = "deny"` across Tier 1 Rust crates | `cargo clippy --workspace --all-targets -- -D warnings` |
| **Metadata Privacy Leaks** | One-click Privacy Scrub (`FR-MET-002`) strips GPS coordinates, serial numbers, author names, and edit histories while preserving safe visual orientation/ICC | Post-scrub re-parse assertion (`0` flagged keys) |
| **Silent Cloud / AI Exfiltration** | Core tools execute 100% offline (`FR-LOC-001`); Stage 9 External AI & Stage 10 Cloud Offload require explicit user opt-in Consent Gates (`FR-AI-002`) | Offline network-disabled integration suite |
| **Weak Encryption** | `.ffenc` encrypted containers use authenticated `AES-256-GCM` or `ChaCha20-Poly1305` with `Argon2id` KDF (`FR-SEC-002`) | Authenticated encryption round-trip & tamper tests |

---

## 2. Subprocess Isolation & Temp Hygiene

- Tier 2 adapters (`ffmpeg`, `qpdf`, `tesseract`, `libreoffice`, `pandoc`, `calibre`, `7z`) execute with:
  1. Sanitized environment variables (no inherited secret tokens).
  2. Private isolated temporary working directories (`0700` on Unix / restricted ACL on Windows).
  3. Wall-clock timeout enforcement (`ResourceLimits::subprocess_timeout_secs`).
  4. OS process-group / job-object termination on `CancellationToken` within `<= 500 ms`.
  5. Immediate cleanup of staging files (`.filefusion.tmp.<uuid>`) on completion, error, or cancellation.

---

## 3. Supply-Chain & Secret Hygiene

- `deny.toml` enforces `cargo-deny` advisory, ban, source, and permissive license checks.
- `.gitignore` blocks `.env*`, `*.pem`, `*.key`, `*.jks`, `*.keystore`, and local SQLite databases.
- GitHub Actions workflows pin third-party actions to immutable commit SHAs and request minimal `permissions:`.

---

## 4. Reporting a Vulnerability

If you discover a security vulnerability in FileFusion:
1. **Do not** open a public GitHub issue with exploit details for unpatched vulnerabilities.
2. Use GitHub **Private Vulnerability Reporting** (Security Advisories) on `https://github.com/dayashimoga/filefusion/security/advisories/new` (once enabled) or contact the maintainers privately with reproduction steps, affected version/commit, and impact assessment.
3. We commit to acknowledging reports promptly, reproducing fixes with regression fixtures in `tests/security/`, and publishing transparent advisories upon remediation.
