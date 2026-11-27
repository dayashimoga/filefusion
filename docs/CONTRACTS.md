# FileFusion — Rust Core & Application Interface Contracts

**Document ID:** `FF-CTR-001`  
**Version:** `1.0.0`  
**Status:** `APPROVED FOR STAGE 0 BASELINE`  
**Subphase Origin:** `0.2.3 — Define Rust and application contracts`  
**Parent Specifications:** [ARCHITECTURE.md](../ARCHITECTURE.md) (`FF-ARCH-001`), [docs/MODULE_BOUNDARIES.md](MODULE_BOUNDARIES.md) (`FF-MOD-001`)  
**Owners:** Agent 1 (Principal Architect), Agent 2 (Rust Core Engineer), Agent 5 (Flutter Engineer), Agent 6 (Web Engineer)

---

## 1. Purpose & Contract Governance

This specification defines the **authoritative data schemas, Rust trait signatures, error taxonomy, progress/cancellation streams, SQLite persistence contract, and Plane 2 transport protocols** (`flutter_rust_bridge`, Web/WASM Workers, CLI `--json`, and REST API `/api/v1`) before any workspace implementation begins.

### Contract Invariants
1. **No Opaque Errors Across Boundaries:** Every failure surfaced to Flutter, Web, CLI, or API maps to a typed `FileFusionError` with a stable machine code (`FF-ERR-*`), human explanation, and remediation action.
2. **Zero Large-Buffer FFI Copies:** Desktop/Mobile FFI passes filesystem paths or scoped file descriptors/URIs rather than copying multi-hundred-MB payloads across the Dart/Rust boundary.
3. **Cooperative Cancellation Everywhere:** Every long-running trait method accepts a `JobContext` containing a `CancellationToken` and `ProgressSink`, guaranteeing `< 500 ms` abort and temp-file cleanup (`NFR-REL-002`).
4. **Schema Versioning:** All persisted JSON (workflows, presets, `--json` CLI output, SQLite migrations, REST payloads) carries an explicit `schema_version` for backward compatibility.

---

## 2. Core Domain Types & Error Taxonomy (`file-core`)

### 2.1 Canonical Error Taxonomy

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ErrorDiagnostic {
    pub code: ErrorCode,          // e.g. FF-ERR-SEC-BOMB
    pub category: ErrorCategory,  // Input | Security | MissingEngine | ResourceLimit | Cancelled | Io | Internal
    pub message: String,          // Clear human-readable cause
    pub remediation: Option<String>, // Actionable next step (e.g., "Run filefusion doctor")
    pub retryable: bool,
}

#[derive(Debug, thiserror::Error)]
#[error("{diagnostic.code}: {diagnostic.message}")]
pub struct FileFusionError {
    pub diagnostic: ErrorDiagnostic,
    #[source]
    pub source: Option<Box<dyn std::error::Error + Send + Sync>>,
}
```

| Error Code | Category | Trigger Condition | Actionable Remediation |
| :--- | :--- | :--- | :--- |
| `FF-ERR-IO-NOT-FOUND` | `Io` | Source path does not exist or permission denied | Verify path and OS read permissions |
| `FF-ERR-FMT-UNSUPPORTED` | `Input` | Magic-byte signature unrecognized or no route to target | Run `filefusion inspect` or verify target format |
| `FF-ERR-FMT-CORRUPT` | `Input` | Truncated/malformed container or header mismatch | Route file to `File Repair` (`UC-012`) |
| `FF-ERR-FMT-DRM` | `Input` | DRM-protected EPUB/MOBI/media detected | Provide DRM-free source (no circumvention supported) |
| `FF-ERR-SEC-TRAVERSAL` | `Security` | Archive path contains `..`, absolute root, or symlink escape | Extraction aborted; inspect archive safely |
| `FF-ERR-SEC-BOMB` | `Security` | Expansion ratio, depth, or SVG entity count exceeds `ResourceLimits` | Increase explicit limit if trusted, or reject input |
| `FF-ERR-SEC-SPOOFED` | `Security` | Extension contradicts true binary signature in strict mode | Inspect true MIME type before processing |
| `FF-ERR-ENG-MISSING` | `MissingEngine` | Requested operation requires an uninstalled Tier 2 tool | Install tool or run `filefusion doctor` |
| `FF-ERR-RES-MEMORY` | `ResourceLimit` | Estimated working set exceeds configured host RAM ceiling | Reduce concurrency or increase memory cap in Settings |
| `FF-ERR-JOB-CANCELLED` | `Cancelled` | User or timeout triggered `CancellationToken` | Temp files cleaned; original source untouched |
| `FF-ERR-INT-INVARIANT` | `Internal` | Unexpected internal invariant or state machine failure | Report diagnostic bundle with `job_id` |

### 2.2 Security Bounds & File Intelligence Models

```rust
pub struct ResourceLimits {
    pub max_workers: usize,             // Default: max(1, min(cpus - 1, avail_ram / per_job))
    pub max_memory_mb: u64,             // Default: min(4096, total_ram_mb / 4)
    pub max_expansion_ratio: u32,       // Default: 100 (100:1 bomb protection)
    pub max_uncompressed_bytes: u64,    // Configurable ceiling per archive extraction
    pub max_archive_depth: u8,          // Default: 3 nested containers
    pub subprocess_timeout_secs: u64,   // Default: 1800s (configurable per job)
}

pub struct FileInspectionReport {
    pub schema_version: u32,            // 1
    pub detected_format: FormatId,      // True magic-byte format
    pub mime_type: String,
    pub extension_claimed: Option<String>,
    pub spoofing_mismatch: bool,
    pub byte_size: u64,
    pub digests: DigestSet,             // blake3, sha256, xxhash64
    pub streams: Vec<StreamDescriptor>, // Video/Audio/Subtitle/PDF/Archive/Image properties
    pub metadata_summary: MetadataSummary,
    pub recommended_actions: Vec<RecommendedAction>,
}
```

### 2.3 Progress & Cancellation Contract

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProgressEvent {
    pub job_id: String,
    pub item_index: u32,
    pub item_total: u32,
    pub stage: JobStage,                // Queued | Probing | SecurityCheck | Processing | Validating | Committing | Completed | Cancelled | Failed
    pub percent: f32,                   // 0.0..=100.0 (monotonic per item)
    pub bytes_in: u64,
    pub bytes_out_estimate: Option<u64>,
    pub eta_ms: Option<u64>,
    pub detail: String,
}
```

---

## 3. Core Processing & Engine Trait Contracts (`L1`–`L4`)

### 3.1 Compression Engine Contract (`compression-core`)

```rust
pub trait CompressionEngine: Send + Sync {
    fn engine_id(&self) -> &'static str;
    fn supported_formats(&self) -> &[FormatId];
    fn estimate_memory_mb(&self, inspection: &FileInspectionReport, opts: &CompressionRequest) -> u64;
    fn compress(
        &self,
        input: &ValidatedSource,
        staged_output: &StagedPath,
        req: &CompressionRequest,
        ctx: &JobContext,
    ) -> Result<CompressionOutcome, FileFusionError>;
}

pub enum CompressionStatus {
    Optimized,              // Staged output is strictly smaller (or format changed) and verified
    NoOpRetainedOriginal,   // FR-CMP-002: same-format output >= input; original preserved untouched
}
```

### 3.2 Conversion & Fidelity Contract (`conversion-core`)

```rust
pub trait ConversionEngine: Send + Sync {
    fn engine_id(&self) -> &'static str;
    fn routes(&self) -> &[ConversionRoute];
    fn preflight(
        &self,
        inspection: &FileInspectionReport,
        target: FormatId,
    ) -> Result<PreflightAssessment, FileFusionError>; // Emits FR-FID-002 warnings before running
    fn convert(
        &self,
        input: &ValidatedSource,
        staged_output: &StagedPath,
        req: &ConversionRequest,
        ctx: &JobContext,
    ) -> Result<ConversionFidelityReport, FileFusionError>;
}
```

### 3.3 Tier 2 External Tool & Quality/Validation Contracts (`codec-registry`, `quality-engine`, `validation-engine`)

- **`ExternalToolAdapter`:** Must implement `probe() -> ToolAvailability` (binary path, version, SPDX license tag), `build_args(...) -> Vec<OsString>` (never shell strings), and execute strictly via an `IsolatedProcessRunner` trait defined in `file-core` and implemented/injected by `job-manager` through `JobContext` (preserving `MOD-RULE-01`, sanitized env, private temp dir, OS process-group kill on `CancellationToken`).
- **`QualityEvaluator`:** Computes empirical `QualityMetrics` (`ssim: Option<f64>`, `psnr_db: Option<f64>`, `vmaf: Option<f64>`, `bit_exact_lossless: Option<bool>`).
- **`OutputValidator`:** Re-opens `StagedPath` with a format decoder/parser to guarantee syntactic validity (`FR-VAL-001`) before Plane 3 executes the atomic rename.

---

## 4. Job Scheduler & SQLite Persistence Contract (`job-manager`)

### 4.1 Batch & Staging Semantics
- **Collision Policies (`FR-BAT-002`):** `Skip`, `RenameWithSuffix`, `OverwriteWithBackup`.
- **Naming Template Variables:** `{stem}`, `{ext}`, `{date}`, `{hash8}`, `{profile}`, `{index}`.
- **Atomic Staging:** Every engine writes exclusively to `StagedPath` inside the target volume's hidden staging temp file (`.filefusion.tmp.<uuid>`), promoting via atomic rename only after `OutputValidator` passes.

### 4.2 Local SQLite WAL Schema Contract (`FR-LOC-002`)

| Table | Primary Key | Stored Fields & Purpose |
| :--- | :--- | :--- |
| `schema_migrations` | `version` | Monotonic forward-only SQL migration tracker |
| `jobs` | `job_id` (UUIDv7) | `operation`, `status`, `created_at`, `finished_at`, `input_count`, `bytes_in_total`, `bytes_out_total`, `error_code` |
| `job_items` | `item_id` | Foreign key `job_id`, `source_path`, `output_path`, `blake3_in`, `blake3_out`, `quality_json`, `fidelity_json`, `outcome_status` |
| `presets` | `preset_id` | `schema_version`, `name`, `domain`, `config_json`, `is_builtin` |
| `workflows` | `workflow_id` | `schema_version`, `name`, `dag_json`, `schedule_cron`, `updated_at` |
| `benchmark_runs` | `run_id` | `timestamp`, `host_profile_json`, `codec`, `throughput_mb_s`, `compression_ratio`, `ssim`, `vmaf` |

---

## 5. Plane 2 Application & Bridge Contracts

### 5.1 Flutter FFI Bridge (`crates/flutter-bridge`)
- Uses `flutter_rust_bridge` v2 codegen targeting `packages/filefusion_ui/lib/src/bridge/`.
- **Core Bridge Entrypoints:**
  - `initCore(dbPath: String, limits: FrbResourceLimits) -> Result<void, FrbError>`
  - `runDoctor() -> Result<FrbDoctorReport, FrbError>`
  - `inspectPaths(paths: Vec<String>) -> Result<Vec<FrbInspectionReport>, FrbError>`
  - `submitBatchJob(request: FrbBatchRequest) -> Stream<FrbProgressEvent>`
  - `cancelJob(jobId: String) -> Result<bool, FrbError>`
  - `queryHistory(filter: FrbHistoryFilter) -> Result<Vec<FrbJobSummary>, FrbError>`
- **Memory Rule:** Thumbnails/previews return bounded downscaled RGBA/PNG bytes or temp cache file URIs; raw source files are never copied over FFI.

### 5.2 SvelteKit Web / WASM Worker Bridge (`packages/filefusion_wasm` -> `apps/web`)
- Rust WASM compiles into a dedicated **Web Worker** (`filefusion.worker.ts`) so the Svelte 5 UI thread never freezes.
- **Message Protocol:** Typed discriminated union `WorkerCommand` (`INSPECT`, `COMPRESS`, `CONVERT`, `ARCHIVE`, `CANCEL`) <-> `WorkerEvent` (`PROGRESS`, `COMPLETED`, `ERROR`) using `Transferable` `ArrayBuffer` ownership transfer (zero-copy handoff) and enforcing a hard `2 GB` tab memory ceiling (`PLAT-WEB`).

### 5.3 CLI Contract (`apps/cli`)
- **Binary Name:** `filefusion`
- **Subcommands:** `inspect`, `compress`, `convert`, `pdf`, `archive`, `dedup`, `repair`, `metadata`, `workflow`, `diff`, `bench`, `doctor`.
- **Global Flags:** `--json` (emits newline-delimited JSON `ProgressEvent` + final `JobReceipt`), `--quiet`, `--workers <N>`, `--max-memory-mb <MB>`, `--no-color`.
- **Deterministic Exit Codes:**
  - `0`: All items succeeded (including verified no-ops).
  - `1`: Partial or full operational failure in batch.
  - `2`: Invalid CLI arguments or configuration.
  - `3`: Security violation rejected (`FF-ERR-SEC-*`).
  - `4`: Aborted by user cancellation (`SIGINT` / `SIGTERM` triggering `CancellationToken`).
  - `5`: Required Tier 2 external dependency missing.

### 5.4 REST API Contract (`apps/api` — Stage 10)
- **Base Path:** `/api/v1` (OpenAPI 3.1 generated from Rust `utoipa` schemas).
- **Standard Envelope:**
  ```json
  {
    "schema_version": 1,
    "ok": true,
    "request_id": "req_01J...",
    "data": {},
    "error": null
  }
  ```
- **Auth & Safety Headers:** `Authorization: Bearer ff_live_...`, `Idempotency-Key` on job submission (`POST /api/v1/jobs`), `GET /api/v1/jobs/{id}/events` (Server-Sent Events streaming `ProgressEvent`), and standard `X-RateLimit-Limit` / `X-RateLimit-Remaining` / `X-RateLimit-Reset` headers.
