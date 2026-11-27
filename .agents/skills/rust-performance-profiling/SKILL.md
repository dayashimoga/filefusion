---
name: rust-performance-profiling
description: >-
  Profiles CPU throughput, memory consumption, startup latency, and concurrency
  behavior of FileFusion Rust crates on CPU-only hosts. Use when optimizing hot
  paths or verifying NFR-PERF memory and latency budgets.
---

# Rust Performance Profiling

## Steps

1. Establish a reproducible input fixture set and record host hardware (CPU cores, RAM, CPU-only vs hardware acceleration).
2. Run Criterion benchmarks (`cargo bench`) for the target crate in `tests/benchmarks/` without mixing debug instrumentation into release artifacts.
3. Measure peak RSS memory and verify streaming buffers stay within `NFR-PERF-001` (`< 512 MB` typical single-file RSS) and bounded `rayon` pool caps (`<= 16 GB` host safe).
4. Measure cold-start CLI latency (`NFR-PERF-002`, `< 50 ms`) and cancellation cleanup latency (`NFR-REL-002`, `<= 500 ms`).
5. Record observed before/after numbers in `PERFORMANCE.md` (never claim unverified speedups).

## Validation

- Benchmark logs and peak-memory measurements demonstrate compliance without regressions in output integrity.
