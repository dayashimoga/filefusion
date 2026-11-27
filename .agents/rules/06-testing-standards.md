# Rule 06 — Testing Standards

- **Coverage Targets (`NFR-QA-001`):** Target `>= 90%` meaningful coverage for core Rust processing crates and `>= 95%` for critical business logic (scheduler, security/bomb guards, no-op guard, atomic staging, fidelity calculators).
- **100% CI Pass Rate:** Never ignore, skip, or relax failing tests without written justification in `docs/DECISION_LOG.md` or `docs/PROGRESS.md`.
- **Real & Adversarial Fixtures:** Test with real-world media/documents alongside corrupt headers, truncated archives, zip-slip paths, decompression bombs, zero-byte files, and Unicode/long paths.
- **Meaningful Assertions:** Verify decoded output integrity, SSIM/PSNR/VMAF bounds, bit-exact lossless hashes, and exact diagnostic error codes (`FF-ERR-*`)—never assert `is_ok()` alone.
- **Regression & Cross-Platform Tests:** Every fixed bug requires a regression test; UI tests must verify real end-to-end wiring rather than mocked responses.
