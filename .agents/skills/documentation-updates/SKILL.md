---
name: documentation-updates
description: >-
  Synchronizes FileFusion governance, architecture, user/developer guides, and
  append-only TODO.md and CHANGELOG.md logs after completing a subphase. Use at the
  end of every subphase before final review and commit.
---

# Documentation Updates

## Steps

1. Update technical docs affected by the change (`REQUIREMENTS.md`, `ARCHITECTURE.md`, `docs/MODULE_BOUNDARIES.md`, `docs/CONTRACTS.md`, `CODEBASE_GUIDE.md`, `USER_GUIDE.md`, `DEVELOPER_GUIDE.md`, `SECURITY.md`, `PERFORMANCE.md`, `LICENSES.md`).
2. Append a chronological entry to `TODO.md` (STRICTLY APPEND-ONLY; never edit or delete prior entries).
3. Append a chronological entry to `CHANGELOG.md` (STRICTLY APPEND-ONLY; never edit or delete prior entries).
4. Update `docs/PROGRESS.md` with actual duration, acceptance criteria results, test evidence, files changed, and prior subphase commit SHA backfill.

## Validation

- `git diff` confirms historical entries in `TODO.md` and `CHANGELOG.md` are untouched and all referenced paths/symbols exist.
