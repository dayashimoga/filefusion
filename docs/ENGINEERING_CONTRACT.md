# FileFusion — Global Engineering Contract

**Document ID:** `FF-ENG-001`  
**Version:** `1.0.0`  
**Status:** `BINDING ACROSS ALL STAGES AND AGENTS`  
**Subphase Origin:** `0.3.5 — Create documentation foundation`  
**Owners:** Agent 1 (Principal Architect), Agent 11 (Documentation Engineer), Agent 12 (Independent Reviewer)

---

## 1. Purpose

This contract binds every engineer and AI agent contributing to **FileFusion**. No subphase, pull request, or release candidate may pass review if it violates any section below.

---

## 2. Implementation Integrity

1. **No Fake Functionality:** Never ship simulated processing, mock outputs, hardcoded results, fake progress percentages, or decorative UI buttons.
2. **Real Backend Wiring:** Every visible UI control, CLI flag, and REST endpoint must execute real Rust core logic (`crates/*`) via verified contracts (`docs/CONTRACTS.md`).
3. **Actionable Diagnostics:** Surface typed `FileFusionError` (`FF-ERR-*`) diagnostics explaining what failed, why, and how the user can resolve it.
4. **Cancellation & Atomic Safety:** Every long-running operation must honor `CancellationToken` within `<= 500 ms` (`NFR-REL-002`), write exclusively to `.filefusion.tmp.<uuid>` staging files, verify output integrity via `OutputValidator`, and commit via atomic rename (`NFR-REL-001`).
5. **No Silent Content or Quality Loss:** Emit Pre-Flight unsupported-feature warnings (`FR-FID-002`), enforce the automatic **No-Op Guard** (`FR-CMP-002`), and attach empirical quality/fidelity reports (`FR-VAL-001`, `FR-FID-001`).

---

## 3. Testing & Verification

1. **Coverage Targets (`NFR-QA-001`):**
   - `>= 90%` meaningful line/branch coverage for core processing crates (`crates/*`).
   - `>= 95%` coverage for critical business logic (scheduler, cancellation, atomic staging, security/bomb guards, no-op guard, fidelity calculators).
2. **100% CI Pass Rate:** Required CI checks must pass at 100%. No test may be `#[ignore]`d or relaxed without documented justification in `docs/DECISION_LOG.md` or `docs/PROGRESS.md`.
3. **Real & Adversarial Corpus:** All processing modules must be tested against real-world fixtures alongside corrupt, truncated, zero-byte, oversized, Unicode/long-path, path-traversal, and decompression-bomb inputs.
4. **Meaningful Assertions:** Coverage metrics never substitute for behavioral assertions (decoded content equivalence, SSIM/PSNR/VMAF thresholds, hash equality, exact `FF-ERR-*` codes).
5. **Cross-Platform & Regression Verification:** Every fixed bug requires a permanent regression test; UI tests must verify real backend wiring.

---

## 4. Performance & Resource Discipline

1. **Benchmark Critical Engines (`Criterion`):** Measure competing codecs and hot paths reproducibly (`tests/benchmarks/`) before locking defaults.
2. **Track Memory & Throughput:** Stream large files via `64 KB`–`1 MB` buffers; avoid unnecessary whole-file copies; bound worker concurrency (`NFR-PERF-001`, `NFR-PERF-003`).
3. **CPU-Only First:** All core features must run smoothly on CPU-only environments (including our 16 GB RAM Windows 11 baseline and 3–6 GB mobile devices). Hardware acceleration is measured separately and degrades gracefully.
4. **Truthful Performance Claims:** Never claim speedups or compression improvements without recorded benchmark evidence in `PERFORMANCE.md`.

---

## 5. Seven-Gate Feature Completion Standard

A feature or subphase is **COMPLETE** only when it is:
1. **Implemented** cleanly in production Rust / Dart / TypeScript.
2. **Integrated** across all declared interfaces and module boundaries (`MOD-RULE-01`..`06`).
3. **Tested** with unit, integration, edge-case, and security/regression assertions.
4. **Validated** against real inputs and post-output integrity metrics (`OutputValidator`).
5. **Documented** in `docs/PROGRESS.md`, `TODO.md` (append-only), `CHANGELOG.md` (append-only), and affected guides.
6. **Independently Reviewed** with a written `PASS` audit from Agent 12.
7. **Demonstrated** through actual executable CLI/app behavior (for functional stages).

---

## 6. Strict One-Subphase-At-A-Time Governance

Development progresses strictly one subphase at a time (`docs/PROGRESS.md`). Agents must complete verification, obtain Agent 12's independent review, commit, tag, push, report using the Section 18 template, and **STOP** for explicit user confirmation before advancing.
