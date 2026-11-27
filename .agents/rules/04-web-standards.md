# Rule 04 — Web Standards (SvelteKit 5 & TypeScript)

- **Stack:** Use SvelteKit, Svelte 5 (runes `$state`, `$derived`, `$effect`), and strict TypeScript (`strict: true`, zero `any` escape hatches).
- **Non-Blocking Processing (`ADR-005`):** Execute Tier 1 Rust WASM (`packages/filefusion_wasm`) inside dedicated Web Workers (`filefusion.worker.ts`) using `Transferable` `ArrayBuffer` handoffs, never on the UI thread.
- **Memory Guard:** Enforce the `<= 2 GB` browser tab heap budget; route oversized or Tier 2 operations cleanly to an optional user-configured API server or inform the user clearly.
- **Accessibility & Semantics:** Use semantic HTML landmarks, ARIA labels, keyboard shortcuts (`Ctrl+K` / `Cmd+K`), and `WCAG 2.1 AA` contrast across Light and Dark themes.
- **Verification Gate:** Run `pnpm check` (`svelte-check`), linter/typecheck, unit tests (`vitest`), and production `pnpm build`.
