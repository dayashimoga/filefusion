# Rule 07 — UI/UX Standards

- **Futuristic & Cohesive Design System:** Maintain synchronized Light and Dark themes (`NFR-UX-001`) with `WCAG 2.1 AA` contrast across Flutter (`packages/filefusion_ui`) and SvelteKit (`apps/web`).
- **14 Core Navigation Studios:** Keep navigation consistent with `docs/USE_CASES.md` §3 (`Home / Workspace`, `Convert`, `Compress & Optimize`, `PDF Studio`, `Image Studio`, `Video Studio`, `Audio Studio`, `Archive Manager`, `OCR & Extraction`, `File Intelligence`, `File Repair`, `Automation Workflows`, `History & Jobs`, `Settings`).
- **Progressive Disclosure (`UXJ-03`):** Present clean 1-click presets upfront while placing expert codec/quantizer/pipeline controls in expandable Advanced drawers.
- **Keyboard & Drop-Zone Ergonomics:** Support universal drag-and-drop (`UXJ-01`), global `Ctrl+K` / `Cmd+K` Command Palette (`UXJ-02`), full keyboard traversal, and screen-reader semantics.
- **Honest States:** Surface real-time progress, actionable error recovery, and instant `< 500 ms` cancel buttons; never render decorative dead controls.
