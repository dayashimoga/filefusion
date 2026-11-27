# Rule 01 — Global Engineering Contract

- **Production Code Only:** Zero mock processing, fake progress bars, hardcoded sample file results, or non-functional UI controls.
- **Real Backend Integration:** Every CLI flag, REST endpoint, and Flutter/SvelteKit UI control must connect to its real Rust core engine (`docs/CONTRACTS.md`).
- **Actionable Errors:** Surface typed `FileFusionError` diagnostics (`FF-ERR-*`) with clear causes and remediation hints; never swallow errors silently.
- **Atomic File Safety & Cancellation:** Write to `.filefusion.tmp.<uuid>` staging files and promote via atomic rename only after `OutputValidator` passes. Honor `CancellationToken` within `<= 500 ms` and remove staging artifacts.
- **No Silent Content Loss:** Run `preflight()` warnings before destructive/lossy format changes (`FR-FID-002`) and enforce the **No-Op Guard** on same-format optimization (`FR-CMP-002`).
- **Seven-Gate Completion Standard:** A feature is complete only when implemented, integrated, tested, validated, documented, independently reviewed, and demonstrated via real app behavior.
