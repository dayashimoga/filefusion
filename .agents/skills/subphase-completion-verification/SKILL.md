---
name: subphase-completion-verification
description: >-
  Executes the mandatory pre-commit verification gate for an active FileFusion
  subphase (tests, Agent 12 independent review, documentation sync, commit, tag,
  push, and STOP for user confirmation). Use at the close of every subphase.
---

# Subphase Completion Verification

## Steps

1. Confirm only the single active subphase in `docs/PROGRESS.md` was implemented.
2. Run all required build, lint, format, security, and test commands and capture exact output.
3. Invoke an independent reviewer subagent (Agent 12) to audit code, tests, docs, append-only logs, and subphase boundaries.
4. Resolve every blocking defect found by Agent 12 and re-verify affected checks.
5. Set subphase status to `COMPLETE` in `docs/PROGRESS.md`, commit cleanly on `main`, tag `stage-<X.Y.Z>`, and push to `origin`.
6. Present the 13-point Section 18 Subphase Report and **STOP**, waiting for explicit user confirmation before starting the next subphase.

## Validation

- Clean `git status --short`, Agent 12 `PASS` verdict, green test output, and verified remote push.
