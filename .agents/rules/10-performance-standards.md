# Rule 10 — Performance Standards

- **16 GB RAM & CPU-Only Baseline:** Design for CPU-only workstations (`<= 16 GB` RAM, no mandatory GPU) and 3–6 GB mobile devices (`NFR-PERF-001`..`003`).
- **Streaming Over Buffering:** Use `64 KB`–`1 MB` streaming I/O buffers for hashing, chunking, encryption, and archives; never load multi-GB files wholly into RAM.
- **Bounded Concurrency:** Separate `tokio` async orchestration from a bounded `rayon` compute pool (`max(1, min(cpus - 1, avail_ram / per_job))`) with memory-weighted job permits.
- **Empirical Benchmarking (`Criterion`):** Benchmark critical algorithms in `tests/benchmarks/` on CPU-only vs hardware-accelerated paths separately. Never claim unverified speedups.
- **Container & Build Budget:** Keep local Podman images/volumes strictly under `30 GB` (`NFR-OPS-001`) and run heavy release builds sequentially on 16 GB hosts.
