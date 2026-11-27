---
name: architecture-review
description: >-
  Reviews proposed crates, dependencies, or cross-plane changes against FileFusion's
  5-plane architecture, 7-tier acyclic DAG, and ADRs. Use before introducing new
  crates, dependencies, or architectural changes.
---

# Architecture Review

## Steps

1. Inspect `ARCHITECTURE.md`, `docs/MODULE_BOUNDARIES.md`, `docs/CONTRACTS.md`, and `docs/DECISION_LOG.md`.
2. Check crate dependencies using `cargo metadata --no-deps --format-version 1` and verify `MOD-RULE-01`..`MOD-RULE-06`:
   - Zero upward edges (`L0`–`L4` must never depend on `L5`/`L6`/`apps/*`).
   - Zero peer codec cross-dependencies (`compression-video`/`audio`/`archive`).
   - Zero GPL/AGPL static linkage in Tier 1 crates (`deny.toml`).
3. Verify `wasm32` and mobile feature flags (`native-process`, `sqlite-store`, `rayon-parallel`) remain properly isolated.
4. Record any material architectural trade-off as a new `ADR-NNN` entry in `docs/DECISION_LOG.md`.

## Validation

- Confirm acyclic dependency graph and zero `MOD-RULE` violations in the review summary.
