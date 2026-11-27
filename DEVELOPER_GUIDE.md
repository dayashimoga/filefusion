# FileFusion — Developer Onboarding & Environment Guide

**Document ID:** `FF-DEV-001`  
**Version:** `1.0.0`  
**Subphase Origin:** `0.3.5 — Create documentation foundation`

---

## 1. Host Prerequisites & Verified Baseline

FileFusion is developed and tested on **CPU-only workstations** (reference host: Windows 11, 12 logical cores, **16 GB RAM**):

- **Rust (Required for Core/CLI/Bridge):** Stable Rust (`rustc 1.99.1+`) with `rustfmt` and `clippy`.
  - *Windows Note:* On hosts without MSVC Build Tools (`link.exe`), `stable-x86_64-pc-windows-gnullvm` (paired with `LLVM-MinGW`) links natively without forcing extra host installations. Do not commit a `rust-toolchain.toml` that overrides a developer's local host target triple.
- **Flutter & Dart (Stage 7+):** Flutter `3.38.4+` / Dart `3.10.3+`.
- **Node.js & pnpm (Stage 8+):** Node `v25+` and `pnpm`.
- **Podman (Optional Disposable Services/Testing):** Podman `6.1+` kept strictly under a **30 GB total storage budget** (`podman system df`).
- **Optional Tier 2 External Engines (Discovered at Runtime via `filefusion doctor`):** `ffmpeg`, `qpdf`, `tesseract`, `libreoffice`, `pandoc`, `calibre`, `7z`.

---

## 2. Daily Developer Commands

```powershell
# Verify formatting, strict workspace lints (unsafe_code = forbid, unwrap_used = deny), and tests
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# Inspect crate dependency DAG (verify MOD-RULE-01..05)
cargo metadata --no-deps --format-version 1
```

---

## 3. Local Build Scripts & Cross-Compilation Matrix

FileFusion provides `scripts/build.ps1` (Windows) and `scripts/build.sh` (Linux/macOS/Git Bash) mirroring `.github/workflows/build-apps.yml`:

```powershell
# Windows: build and verify individual targets or all locally supported targets sequentially
powershell -ExecutionPolicy Bypass -File scripts/build.ps1 -App cli -BuildMode debug
powershell -ExecutionPolicy Bypass -File scripts/build.ps1 -App all -BuildMode release
```

```bash
# Linux / macOS: build and verify selected targets
./scripts/build.sh --app cli --build-mode release
```

| Host OS | Supported Local Targets | Unavailable Locally (Delegated to GitHub Actions) |
| :--- | :--- | :--- |
| **Windows 11** | `cli`, `windows`, `web`, `api`, `android` (with SDK) | `linux` desktop, `macos` desktop, `ios` |
| **Linux** | `cli`, `linux`, `web`, `api`, `android` (with SDK) | `windows` desktop, `macos` desktop, `ios` |
| **macOS** | `cli`, `macos`, `ios`, `web`, `api`, `android` (with SDK) | `windows` desktop, `linux` desktop |

---

## 4. Resource-Aware Workflow on 16 GB RAM Hosts

1. **Sequential Heavy Builds:** Never run full Flutter Android/Desktop, SvelteKit production, and Rust release LTO builds concurrently on a 16 GB RAM host.
2. **Delegate Non-Host Targets to CI:** Cross-platform macOS, iOS, and Linux AppImage/binary builds run on native GitHub-hosted runners (`.github/workflows/build-apps.yml`, added in Phase `0.4`).
3. **Subphase Discipline:** Always inspect [docs/PROGRESS.md](docs/PROGRESS.md) before writing code and follow `.agents/skills/subphase-completion-verification/SKILL.md`.
