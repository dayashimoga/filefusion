# FileFusion — Codebase Guide

**Document ID:** `FF-CBG-001`  
**Version:** `1.0.0`  
**Related Specs:** [ARCHITECTURE.md](ARCHITECTURE.md) | [docs/MODULE_BOUNDARIES.md](docs/MODULE_BOUNDARIES.md) | [docs/MONOREPO_SETUP.md](docs/MONOREPO_SETUP.md)

---

## 1. Repository Layout

- **`.agents/`**: Persistent project rules (`.agents/rules/*.md`), 14 reusable workflow skills, and 12 specialist agent definitions (`.agents/skills/*/SKILL.md`).
- **`.cargo/` & Root Configs**: `.gitignore`, `.gitattributes` (LF enforcement + binary fixture protection), `.editorconfig`, `deny.toml` (`cargo-deny` license/security allowlist), and root `Cargo.toml` workspace.
- **`crates/`**: Rust processing workspace organized across 7 acyclic dependency tiers (`L0`–`L6`). Currently active: [`crates/file-core/`](crates/file-core/src/lib.rs) (Stage 0 workspace anchor; full 16-crate rollout tracked in `docs/MONOREPO_SETUP.md` §3).
- **`docs/` & Root Governance Specs**: Authoritative product requirements, use cases, format matrices, scope boundaries, architecture, contracts, ADRs, roadmap, and progress ledger.
- **Upcoming Directories (Activated Per Stage):** `.github/` & `scripts/` (`0.4.x`), `apps/cli`, `tests/`, `fixtures/` (`Stage 1+`), `apps/desktop`, `apps/mobile`, `packages/filefusion_ui` (`Stage 7`), `apps/web`, `packages/filefusion_wasm` (`Stage 8`), `apps/api`, `infrastructure/` (`Stage 10`).

---

## 2. Where New Code Belongs

| Capability / Change | Target Crate or Package | Tier | Forbidden Placement |
| :--- | :--- | :---: | :--- |
| Errors, signatures, digests, `ResourceLimits`, `IsolatedProcessRunner` trait | `crates/file-core` | `L0` | Never import `tokio` runtime or format codecs |
| Tier 2 probing & license metadata | `crates/codec-registry` | `L1` | Never statically link GPL libraries |
| Quality metrics (SSIM/PSNR/VMAF) & post-output validation | `crates/quality-engine`, `crates/validation-engine` | `L1` | Never depend on `job-manager` or apps |
| Format compression (`image`, `video`, `audio`, `pdf`, `office`, `archive`) | `crates/compression-*` | `L3` | No peer cross-dependencies (`MOD-RULE-02`) |
| Adaptive codec selection, dedup/CDC, conversion graph & fidelity | `crates/optimization-engine`, `crates/conversion-core` | `L4` | Never spawn unmanaged `Command`s |
| Bounded scheduler, cancellation, atomic staging, SQLite WAL, DAG workflows | `crates/job-manager` | `L5` | Never depend on Flutter/Web UI |
| Thin FFI, CLI, Web Worker WASM, REST API, Shared UI | `flutter-bridge`, `apps/*`, `packages/*` | `L6` / `P1` | Zero business logic or format parsing |
