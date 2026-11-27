# FileFusion — Performance Budgets & Benchmark Log

**Document ID:** `FF-PERF-001`  
**Version:** `1.0.0`  
**Requirements Reference:** [REQUIREMENTS.md](REQUIREMENTS.md) §5 (`NFR-PERF-001`..`003`, `NFR-OPS-001`)

---

## 1. Performance & Resource Targets

| Metric ID | Target | Measurement Method |
| :--- | :--- | :--- |
| **`NFR-PERF-001`** | `< 512 MB` RSS for typical single-file ops; configurable global ceiling `<= min(4096 MB, system_ram / 4)` | Streaming `64 KB`–`1 MB` buffers + RSS profiling |
| **`NFR-PERF-002`** | CLI cold start `< 50 ms`; Desktop/Mobile shell launch `< 800 ms` | Startup harness on reference hardware |
| **`NFR-PERF-003`** | Bounded CPU worker pool: `max(1, min(logical_cpus - 1, avail_ram / per_job))` | 1,000-job batch stress test on 16 GB RAM host |
| **`NFR-REL-002`** | `<= 500 ms` cancellation latency & staging cleanup | Cooperative `CancellationToken` & process-group kill test |
| **`NFR-OPS-001`** | `<= 30 GB` total local Podman container storage budget | `podman system df` audit |

---

## 2. Benchmarking Policy (`Criterion`)

1. Permanent benchmarks reside in `tests/benchmarks/` (activated in Stage 1–2) and execute separately from production binaries (`FF-ARCH-001` §7).
2. Every benchmark record logs:
   - Host hardware profile (OS, CPU model/cores, RAM, CPU-only vs hardware backend).
   - Input fixture dimensions/entropy/size.
   - Throughput (`MB/s`), peak RSS (`MB`), output size ratio, and quality score (`SSIM`, `PSNR`, `VMAF`, or bit-exact confirmation).
3. Hardware acceleration (`NVENC`, `QSV`, `AMF`, `VideoToolbox`, `MediaCodec`) is always benchmarked separately from CPU-only baselines (`SVT-AV1`, `x265`, `x264`, `ravif`).

---

## 3. Observed Baseline Measurements

### Stage 0 Host Baseline (`2026-11-27`)
- **Reference Development Host:** Windows 11 (`DESKTOP-F1EE2JP`), 12 logical processors, **16.0 GB physical RAM** (`17,015,664,640` bytes), `65.59 GB` free on workspace drive `H:\`, `76.14 GB` free on `C:\`, CPU-only baseline.
- **Rust Workspace Anchor (`file-core v0.1.0-alpha.0`, `0.3.2`):**
  - `cargo clippy --workspace --all-targets -- -D warnings`: `0.80s` check (`stable-x86_64-pc-windows-gnullvm`)
  - `cargo test --workspace`: `0.10s` test compile, `0.00s` execution (`1 passed; 0 failed`)

*Stage 1–2 throughput, compression ratio, SSIM/PSNR/VMAF, and memory benchmarks will be appended chronologically below as crates are implemented.*
