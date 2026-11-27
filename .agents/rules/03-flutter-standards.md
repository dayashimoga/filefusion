# Rule 03 — Flutter Standards

- **Thin UI Layer (`MOD-RULE-03`):** Dart code in `packages/filefusion_ui`, `apps/desktop`, and `apps/mobile` must never parse file headers, compute quality metrics, or duplicate Rust business logic.
- **Shared UI Package (`MOD-RULE-06`):** Place all shared 14-studio screens, Light/Dark design tokens, and state controllers in `packages/filefusion_ui`. Keep `apps/desktop` and `apps/mobile` limited to OS shell integrations.
- **Bridge Discipline (`flutter-bridge`):** Use `flutter_rust_bridge` v2 contracts (`docs/CONTRACTS.md` §5.1). Pass file paths, scoped URIs, or small thumbnails across FFI—never copy large raw media buffers into Dart memory.
- **Platform Guardrails:** Respect Android scoped storage / foreground services and iOS app sandbox / background task limits (`PLAT-AND`, `PLAT-IOS`).
- **Verification Gate:** Run `dart analyze`, `dart format --output=none --set-exit-if-changed`, and `flutter test` on touched packages.
