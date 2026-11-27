---
name: project-initialization
description: >-
  Initializes or verifies a FileFusion workspace, toolchain baseline, Git remote,
  and monorepo configuration roots. Use when bootstrapping a fresh clone or validating
  local developer setup before starting subphase work.
---

# Project Initialization

## Steps

1. Verify installed toolchains (`git`, `rustc`, `cargo`, `flutter`, `node`, `pnpm`, `podman`) and record host RAM/CPU limits.
2. Confirm clean Git state (`git status --short`), remote `origin` (`https://github.com/dayashimoga/filefusion`), and `.gitattributes` / `.gitignore` rules.
3. Verify workspace lints and baseline compilation:
   `cargo fmt --all -- --check`
   `cargo clippy --workspace --all-targets -- -D warnings`
   `cargo test --workspace`
4. Read `docs/PROGRESS.md` to confirm the single active subphase before modifying files.

## Validation

- All baseline Rust workspace checks exit `0` with zero warnings and zero unapproved dirty files.
