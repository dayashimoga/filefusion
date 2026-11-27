# FileFusion — Monorepo Setup & Activation Schedule

**Document ID:** `FF-MONO-001`  
**Version:** `1.0.0`  
**Subphase Origin:** `0.3.2 — Establish monorepo`  
**Parent Specification:** [docs/MODULE_BOUNDARIES.md](MODULE_BOUNDARIES.md) (`FF-MOD-001`)

---

## 1. Non-Decorative Monorepo Policy

Per the Master Engineering Contract (§4) and `FF-MOD-001`, FileFusion **does not create empty placeholder directories or stub crates**. Top-level configuration roots are active immediately, while application and domain crate directories are materialized in the exact stage where they receive tested, production code.

---

## 2. Active Monorepo Configuration Roots (`0.3.2`)

| File / Directory | Purpose |
| :--- | :--- |
| [`.gitignore`](../.gitignore) | Cross-ecosystem ignore rules (Rust, Flutter/Dart, SvelteKit/Node, SQLite WAL, temp staging, secrets) |
| [`.gitattributes`](../.gitattributes) | Cross-OS LF enforcement (`CRLF` preserved for `.ps1`/`.bat`/`.cmd`; binary protection for media/doc fixtures) |
| [`.editorconfig`](../.editorconfig) | Consistent formatting across Rust, Dart, TypeScript/Svelte, TOML, YAML, Markdown, and PowerShell |
| [`rustfmt` / `clippy` via active `stable` toolchain](../Cargo.toml) | Uses host's default `stable` Rust toolchain (`gnullvm` on local Win11, `msvc`/`gnu` on CI) without forcing an uninstalled MSVC linker |
| [`.cargo/config.toml`](../.cargo/config.toml) | Incremental build and 16 GB RAM host-friendly Cargo defaults |
| [`deny.toml`](../deny.toml) | `cargo-deny` permissive license allowlist (`ADR-002`, `MOD-RULE-05`) and advisory gate |
| [`Cargo.toml`](../Cargo.toml) | Workspace root (`resolver = "2"`, `edition = "2024"`, strict `unsafe_code = "forbid"` & clippy lints) |
| [`crates/file-core/`](../crates/file-core/Cargo.toml) | L0 workspace anchor crate compiled and unit-tested cleanly (`cargo test`, `cargo clippy`) |

---

## 3. Planned Directory Activation Schedule

| Target Path | Activated In | Purpose |
| :--- | :--- | :--- |
| `.agents/rules/`, `.agents/skills/` | `0.3.3` – `0.3.4` | Antigravity persistent rules, reusable workflows, and 12 specialist agent skills |
| Root docs & `docs/*` governance suite | `0.3.5` | `ENGINEERING_CONTRACT.md`, `CODEBASE_GUIDE.md`, `USER_GUIDE.md`, `DEVELOPER_GUIDE.md`, `SECURITY.md`, `PERFORMANCE.md`, `LICENSES.md`, `ROADMAP.md`, `README.md`, `CONTRIBUTING.md`, `LICENSE` |
| `.github/`, `scripts/` | `0.4.1` – `0.4.5` | Selectable `build-apps.yml`, release workflow, issue/PR templates, `build.ps1`, `build.sh` |
| `crates/*`, `apps/cli/`, `tests/`, `fixtures/` | `Stage 1` – `Stage 6` | Full 16-crate Rust workspace, CLI binary, real/malformed fixtures, Criterion benchmarks |
| `apps/desktop/`, `apps/mobile/`, `packages/filefusion_ui/` | `Stage 7` | Flutter Windows/Linux/macOS/Android/iOS applications + shared UI kit |
| `apps/web/`, `packages/filefusion_wasm/` | `Stage 8` | SvelteKit 5 + TypeScript + WASM Web Worker application |
| `apps/api/`, `infrastructure/` | `Stage 10` | Self-hosted `axum` REST API, Podman manifests (`< 30 GB`), Cloudflare Workers/R2 configs |
