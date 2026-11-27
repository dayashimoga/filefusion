---
name: regression-testing
description: >-
  Executes unit, integration, fidelity, security, and end-to-end regression suites
  to ensure fixes and new features do not break existing FileFusion behavior. Use
  before finalizing any code subphase or bug fix.
---

# Regression Testing

## Steps

1. Identify affected crates, CLI subcommands, FFI bridges, and UI apps via `docs/MODULE_BOUNDARIES.md`.
2. Add a targeted regression test reproducing any newly discovered bug before applying the fix.
3. Run the relevant Rust workspace tests (`cargo test --workspace`), clippy (`cargo clippy --workspace --all-targets -- -D warnings`), format checks (`cargo fmt --all -- --check`), and any active Flutter/Web suites.
4. Verify no tests are `#[ignore]`d without a documented justification.

## Validation

- 100% pass rate across all required unit, integration, security, and regression tests.
