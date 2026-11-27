---
name: compression-benchmarking
description: >-
  Benchmarks competing compression codecs, presets, and adaptive selection heuristics
  across size, speed, and perceptual quality (SSIM, PSNR, VMAF). Use when selecting
  or tuning encoders in Stage 2 and optimization-engine.
---

# Compression Benchmarking

## Steps

1. Select representative fixtures across media categories (photos, UI graphics with alpha, video clips, speech/music audio, documents, archives).
2. Benchmark candidate encoders/presets separately on CPU-only vs optional hardware backends via Criterion / `filefusion bench`.
3. Capture compression ratio, throughput (`MB/s`), peak RSS, and objective quality (`SSIM`, `PSNR`, `VMAF`, or bit-exact lossless confirmation).
4. Verify the **No-Op Guard** (`FR-CMP-002`) rejects outputs larger than same-format inputs.
5. Update `codec-registry` presets and `PERFORMANCE.md` only with empirically measured results.

## Validation

- Reproducible benchmark CSV/JSON reports confirm superior Pareto efficiency without violating quality floors.
