---
name: conversion-fidelity-testing
description: >-
  Validates structural, visual, and semantic preservation across document, ebook,
  and media conversions, including pre-flight warnings and Fidelity Reports. Use when
  implementing or verifying Stage 3 and Stage 4 conversion engines.
---

# Conversion Fidelity Testing

## Steps

1. Run `preflight()` on fixtures containing known unsupported target features (e.g., `XLSX` formulas -> `CSV`, `PPTX` animations -> `PDF`, `XLSM` macros) and verify `FR-FID-002` warnings are emitted.
2. Execute the conversion and confirm `OutputValidator` verifies the output file parses cleanly in its target decoder.
3. Compare source vs output structure (headings, tables, lists, links, embedded image count, page count, audio/video duration, frame count, metadata) to generate a `ConversionFidelityReport` (`FR-FID-001`).
4. Verify DRM-protected inputs fail cleanly with `FF-ERR-FMT-DRM` rather than producing corrupt outputs.

## Validation

- All fidelity assertions pass across real-world and edge-case fixtures in `tests/fidelity/`.
