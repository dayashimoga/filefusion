# Rule 08 — Documentation Standards

- **Authoritative Synchronization:** Keep `REQUIREMENTS.md`, `ARCHITECTURE.md`, `docs/MODULE_BOUNDARIES.md`, `docs/CONTRACTS.md`, `docs/DECISION_LOG.md`, `docs/PROGRESS.md`, `CODEBASE_GUIDE.md`, `USER_GUIDE.md`, and `DEVELOPER_GUIDE.md` synchronized with actual code.
- **Append-Only Logs:** `TODO.md` and `CHANGELOG.md` are **strictly APPEND-ONLY**. Never delete or overwrite historical entries.
- **Subphase Completion Traceability:** Every completed subphase must update `docs/PROGRESS.md` (status, acceptance criteria, test evidence, files changed, commit reference), `TODO.md`, and `CHANGELOG.md`.
- **Truthful Claims Only:** Document only verified, implemented behavior, exact file paths, and observed test/benchmark numbers.
- **Clickable & Valid Diagrams:** Use valid Mermaid `flowchart`/`sequenceDiagram` syntax (quote labels containing parentheses or special characters; avoid HTML tags).
