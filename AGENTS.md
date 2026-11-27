# FileFusion — Project Engineering Governance

All agents working in `filefusion` must uphold the project's **Global Engineering Contract**, **Strict One-Subphase-At-A-Time Governance**, and **Architecture & Module Boundaries**:

- **Authoritative Specifications:** `REQUIREMENTS.md`, `ARCHITECTURE.md`, `docs/MODULE_BOUNDARIES.md`, `docs/CONTRACTS.md`, `docs/DECISION_LOG.md`, `docs/SCOPE_AND_MVP.md`, `docs/PROGRESS.md`.
- **No Fake Functionality:** Never ship simulated processing, hardcoded file outputs, or decorative UI buttons. Every control must execute real, verified logic.
- **Rust-First & Local-First:** Core logic resides in `crates/*` (`unsafe_code = "forbid"`), executes offline by default, and supports `< 500 ms` cancellation and atomic temp-file staging.
- **One Subphase at a Time:** Implement only the active subphase in `docs/PROGRESS.md`, run tests, obtain Agent 12 independent review, update `TODO.md` / `CHANGELOG.md` (append-only), commit, push, and wait for explicit user confirmation before advancing.
- **Modular Rules & Skills:** Detailed domain rules reside in `.agents/rules/*.md` and reusable procedures in `.agents/skills/*/SKILL.md`.
