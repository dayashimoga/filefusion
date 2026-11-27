---
name: feature-implementation
description: >-
  Guides end-to-end implementation of a FileFusion subphase feature from contract
  verification through Rust core, bridge/UI wiring, testing, and documentation.
  Use when implementing any new capability in an active subphase.
---

# Feature Implementation

## Steps

1. Confirm the feature belongs strictly to the active subphase in `docs/PROGRESS.md` and trace its `FR-*` / `NFR-*` IDs in `REQUIREMENTS.md`.
2. Verify or define typed structs, error codes (`FF-ERR-*`), and trait methods in `docs/CONTRACTS.md`.
3. Implement real, production logic inside the designated `crates/*` module with:
   - Streaming I/O and bounded memory (`ResourceLimits`).
   - Cooperative `CancellationToken` checks (`<= 500 ms` abort).
   - Atomic `.filefusion.tmp.<uuid>` staging and `OutputValidator` verification.
4. Wire the feature through `apps/cli`, `flutter-bridge` / `packages/filefusion_ui`, or `apps/web` with zero mock/stub handlers.
5. Write unit, integration, and edge-case/corrupt-fixture tests (>= 90% core, >= 95% critical logic).

## Validation

- Run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and end-to-end app verification.
