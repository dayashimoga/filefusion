# FileFusion

**Universal File Intelligence, Transformation, Optimization & Automation Platform**

FileFusion is a **local-first, Rust-powered** cross-platform platform for inspecting, converting, compressing, optimizing, repairing, deduplicating, encrypting, and automating files across Desktop (Windows, Linux, macOS), Mobile (Android, iOS/iPadOS), Responsive Web (SvelteKit 5 + WASM), CLI, and REST API targets.

> **Current Project Status:** **Stage 0 — Project Foundation (`Phase 0.3` active)**  
> Authoritative specifications, architecture, contracts, ADRs, monorepo roots, and Antigravity agent governance are established. Core processing crates and applications are tracked stage-by-stage in [docs/PROGRESS.md](docs/PROGRESS.md).

---

## Core Design Principles

- **Production Implementation Only:** Zero simulated processing, hardcoded outputs, or decorative controls ([docs/ENGINEERING_CONTRACT.md](docs/ENGINEERING_CONTRACT.md)).
- **Local-First & Offline by Default:** Core processing executes locally without mandatory cloud accounts, subscriptions, or hidden uploads.
- **Rust-First Shared Core:** 16 modular crates organized across 7 acyclic tiers (`L0`–`L6`) with `#![forbid(unsafe_code)]`, bounded memory/concurrency for CPU-only 16 GB hosts and mobile devices, `<= 500 ms` cancellation, and atomic temp-file staging.
- **Two-Tier Engine & License Isolation:** Permissive pure-Rust Tier 1 core always compiled in, paired with runtime-discovered, out-of-process Tier 2 powerhouse adapters (`FFmpeg`, `qpdf`, `Tesseract`, `LibreOffice`, `Pandoc`, `Calibre`, `7-Zip`).
- **Zero Silent Fidelity Loss:** Pre-flight unsupported-feature warnings, automatic No-Op Guard (never inflates same-format optimized files), and post-processing output validation.

---

## Documentation Map

| Document | Purpose |
| :--- | :--- |
| [REQUIREMENTS.md](REQUIREMENTS.md) | Product requirements (`FR-*`, `NFR-*`, 20 capability domains, Stage 0–12 traceability) |
| [docs/USE_CASES.md](docs/USE_CASES.md) | 6 target personas, 14 core navigation studios, 16 end-to-end use cases, UX journeys |
| [docs/FORMATS_AND_CAPABILITIES.md](docs/FORMATS_AND_CAPABILITIES.md) | Magic-byte signatures, codec/format matrices, Tier 1 vs Tier 2 routing, security bounds |
| [docs/SCOPE_AND_MVP.md](docs/SCOPE_AND_MVP.md) | Milestone A (`MVP-v0.1`), Milestone B (`v1.0-GA`), Milestone C (`v1.x+`), `DEF-*` register |
| [ARCHITECTURE.md](ARCHITECTURE.md) | 5-plane system architecture, 8-step lifecycle, trust boundaries, resource governance |
| [docs/MODULE_BOUNDARIES.md](docs/MODULE_BOUNDARIES.md) | 16-crate acyclic DAG (`L0`–`L6`), `MOD-RULE-01`..`06`, WASM/mobile feature flags |
| [docs/CONTRACTS.md](docs/CONTRACTS.md) | `FileFusionError` taxonomy, Rust traits, SQLite WAL schema, FFI/WASM/CLI/REST contracts |
| [docs/DECISION_LOG.md](docs/DECISION_LOG.md) | Architecture Decision Records (`ADR-001` through `ADR-010`) |
| [docs/ENGINEERING_CONTRACT.md](docs/ENGINEERING_CONTRACT.md) | Binding implementation, testing (`>= 90%`/`95%`), performance, and 7-gate completion rules |
| [docs/ROADMAP.md](docs/ROADMAP.md) & [docs/PROGRESS.md](docs/PROGRESS.md) | Stage 0–12 roadmap and live subphase execution ledger |
| [CODEBASE_GUIDE.md](CODEBASE_GUIDE.md) & [DEVELOPER_GUIDE.md](DEVELOPER_GUIDE.md) | Repository navigation, toolchain setup, and build/test workflows |
| [USER_GUIDE.md](USER_GUIDE.md) | End-user guide (tracking verified capabilities per milestone) |
| [SECURITY.md](SECURITY.md), [PERFORMANCE.md](PERFORMANCE.md), [LICENSES.md](LICENSES.md) | Threat model, benchmark ledger, and license/SBOM compliance register |
| [CONTRIBUTING.md](CONTRIBUTING.md), [TODO.md](TODO.md), [CHANGELOG.md](CHANGELOG.md) | Contributor rules and append-only project ledgers |

---

## Quick Verification (Current Baseline)

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Licensed under dual **`Apache-2.0 OR MIT`** ([LICENSE](LICENSE)). See [LICENSES.md](LICENSES.md) for Tier 1 and Tier 2 third-party notices.
