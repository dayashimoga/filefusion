# Rule 09 — Git and GitHub Standards

- **Branch & Commit Hygiene:** Keep `main` always buildable and tested. Use Conventional Commits (`feat(...)`, `fix(...)`, `docs(...)`, `chore(...)`, `ci(...)`, `sec(...)`, `test(...)`, `perf(...)`).
- **Subphase Tagging:** Tag every verified subphase milestone (`stage-<X.Y.Z>`) and backfill its exact short SHA in `docs/PROGRESS.md`.
- **Line Endings & Ignores:** Respect `.gitattributes` (LF default, CRLF for `.ps1`/`.bat`/`.cmd`, binary for fixtures) and `.gitignore` (never commit `target/`, `node_modules/`, `.dart_tool/`, `.db*`, or secrets).
- **GitHub Actions Security:** Pin third-party actions to immutable commit SHAs, declare least-privilege `permissions:`, never leak secrets in logs, and upload independent per-target artifacts (`actions/upload-artifact` v4+).
- **Worktrees for Concurrency:** Use isolated Git worktrees when running parallel code-writing subagents; never let concurrent agents edit the same file simultaneously.
