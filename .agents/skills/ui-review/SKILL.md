---
name: ui-review
description: >-
  Audits Flutter and SvelteKit UI screens for design consistency, Light/Dark theme
  parity, WCAG 2.1 AA accessibility, progressive disclosure, and real backend wiring.
  Use when reviewing Desktop, Mobile, or Web UI changes.
---

# UI Review

## Steps

1. Inspect touched views across `packages/filefusion_ui`, `apps/desktop`, `apps/mobile`, and `apps/web`.
2. Verify every visible button, toggle, slider, and menu item triggers real Rust/WASM backend behavior (zero dead or simulated controls).
3. Verify progressive disclosure (`UXJ-03`), universal drag-and-drop (`UXJ-01`), `Ctrl+K` / `Cmd+K` Command Palette (`UXJ-02`), Light/Dark theme contrast (`WCAG 2.1 AA`), and responsive mobile/desktop breakpoints.
4. Check error states and active-job cancellation UI (`<= 500 ms` cleanup and clear remediation messaging).
5. Run platform checks (`flutter test` / `dart analyze` or `pnpm check` / `vitest` / Playwright).

## Validation

- Zero dead controls, zero accessibility regressions, and all UI automated tests pass.
