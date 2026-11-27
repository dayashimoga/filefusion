# Rule 02 — Rust Standards

- **Workspace Lints:** Inherit `[lints] workspace = true` in every crate (`unsafe_code = "forbid"`, `unwrap_used = "deny"`, `panic = "deny"`, `todo = "deny"`, `unimplemented = "deny"`).
- **Acyclic Crate Boundaries (`MOD-RULE-01`..`05`):** Respect `L0`–`L6` dependency direction (`docs/MODULE_BOUNDARIES.md`). Never introduce peer cross-dependencies between format crates (`compression-video`, `compression-audio`, `compression-archive`).
- **Typed Error Handling:** Return `Result<T, FileFusionError>` across public crate boundaries (`thiserror` for structured errors); never leak raw `anyhow::Error` or panics across FFI/WASM/CLI.
- **Feature-Flag Portability:** Gate OS process spawning behind `native-process`, SQLite behind `sqlite-store`, and multi-thread pools behind `rayon-parallel` so `wasm32` and mobile targets stay clean.
- **Verification Gate:** Every Rust edit must pass `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace`.
