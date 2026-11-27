# Contributing to FileFusion

Thank you for contributing to **FileFusion**. All contributions are governed by [docs/ENGINEERING_CONTRACT.md](docs/ENGINEERING_CONTRACT.md) and dual-licensed under `Apache-2.0 OR MIT` ([LICENSE](LICENSE)).

---

## Core Contribution Rules

1. **Active Subphase Discipline:** Check [docs/PROGRESS.md](docs/PROGRESS.md) before starting work. Do not introduce features outside the active subphase or deferred in [docs/SCOPE_AND_MVP.md](docs/SCOPE_AND_MVP.md).
2. **No Fake Functionality:** Never submit mock handlers, hardcoded sample results, or non-functional UI buttons.
3. **Architecture & Boundaries:** Respect the 5-plane architecture ([ARCHITECTURE.md](ARCHITECTURE.md)), `MOD-RULE-01`..`06` ([docs/MODULE_BOUNDARIES.md](docs/MODULE_BOUNDARIES.md)), and contracts ([docs/CONTRACTS.md](docs/CONTRACTS.md)).
4. **Security & Licensing:** Never bypass bomb/traversal checks, never use shell strings (`sh -c`/`cmd /c`) for Tier 2 tools, and keep Tier 1 dependencies compliant with [deny.toml](deny.toml) and [LICENSES.md](LICENSES.md).
5. **Append-Only Logs:** Never edit or delete historical entries in [TODO.md](TODO.md) or [CHANGELOG.md](CHANGELOG.md)—always append chronologically.

---

## Mandatory Pre-PR Verification

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Use Conventional Commits (`feat(...)`, `fix(...)`, `docs(...)`, `chore(...)`, `ci(...)`, `sec(...)`, `test(...)`, `perf(...)`) and include test/benchmark evidence in pull requests.
