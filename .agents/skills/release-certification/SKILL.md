---
name: release-certification
description: >-
  Verifies all Section 19 production-readiness gates (builds, artifacts, checksums,
  SBOM, license audit, security audit, benchmarks, and docs) before tagging a
  release. Use during Stage 11–12 or any tagged release candidate.
---

# Release Certification

## Steps

1. Audit codebase and UI against Section 19 ("No critical placeholder or fake implementation remains").
2. Verify all required CI targets in `.github/workflows/build-apps.yml` and release workflow build cleanly with independent artifacts and SHA-256 checksums.
3. Verify SBOM generation, `deny.toml` / `LICENSES.md` compliance, and Tier 2 attribution notices.
4. Confirm security audit (`tests/security/` + fuzz/malformed corpus), benchmark reports (`PERFORMANCE.md`), and fidelity reports pass.
5. Verify `CHANGELOG.md`, `USER_GUIDE.md`, and installation instructions match the tagged version.

## Validation

- Written certification checklist in `docs/PROGRESS.md` with 100% pass evidence across every gate.
