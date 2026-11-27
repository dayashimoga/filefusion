---
name: security-audit
description: >-
  Performs threat-model checks, archive bomb/traversal validation, subprocess isolation
  verification, dependency/secret audits, and privacy-gate checks. Use before completing
  security-sensitive subphases or releases.
---

# Security Audit

## Steps

1. Inspect `SECURITY.md` and `FF-ARCH-001` §5 trust boundaries for the modified scope.
2. Verify untrusted-input guards in `validation-engine` and format crates:
   - Path traversal (`..`, absolute paths, drive letters, null bytes) and symlink escapes rejected.
   - `ResourceLimits` (`100:1` ratio, max depth `3`, max uncompressed bytes, SVG anti-XXE) enforced.
3. Verify zero unmanaged `std::process::Command` or shell strings (`sh -c`/`cmd /c`) exist (`MOD-RULE-04`); all Tier 2 tools use `IsolatedProcessRunner`.
4. Scan git diff and tracked files for accidental secrets/tokens and verify opt-in AI/cloud consent gates (`FR-AI-002`).
5. Run security tests in `tests/security/` and `cargo deny check` (when available).

## Validation

- All adversarial malformed/bomb tests pass and zero high/critical findings or secret leaks exist.
