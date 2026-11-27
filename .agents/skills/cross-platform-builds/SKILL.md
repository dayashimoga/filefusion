---
name: cross-platform-builds
description: >-
  Runs and validates local host builds via scripts/build.ps1 or scripts/build.sh
  and verifies GitHub Actions build-apps.yml matrix parity across desktop, mobile,
  web, cli, and api targets. Use during CI/CD setup and multi-platform release work.
---

# Cross-Platform Builds

## Steps

1. Check host OS capabilities and explicitly list targets supported locally vs delegated to native GitHub-hosted runners (never overload a 16 GB RAM host with concurrent heavy builds).
2. Execute local target builds sequentially via `scripts/build.ps1` (Windows) or `scripts/build.sh` (Linux/macOS).
3. Validate `.github/workflows/build-apps.yml` selectable `app` input (`all`, `desktop`, `windows`, `linux`, `macos`, `mobile`, `android`, `ios`, `web`, `cli`, `api`), `build_mode`, and independent artifact uploads.
4. Verify SHA-256 checksums, license notices, and non-nested artifact directory structure.

## Validation

- Selected local build targets succeed cleanly and CI workflow definitions pass syntax/matrix validation.
