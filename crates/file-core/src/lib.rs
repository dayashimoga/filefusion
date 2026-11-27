//! `file-core`: L0 foundation crate for `FileFusion`.
//!
//! Defines workspace identity, canonical format/media taxonomies, resource and
//! security limits, monotonic progress events, and CLI exit code contracts
//! (`docs/CONTRACTS.md` / `FF-CTR-001`).

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Canonical package name of the L0 foundation crate.
pub const CRATE_NAME: &str = env!("CARGO_PKG_NAME");

/// Semantic version of the `FileFusion` workspace baseline.
pub const WORKSPACE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Current schema version for serialized core reports.
pub const CORE_SCHEMA_VERSION: u32 = 1;

/// Returns a deterministic identifier string for workspace verification.
#[must_use]
pub fn workspace_identity() -> &'static str {
    concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"))
}

/// High-level media category for detected formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    /// Raster or vector image format.
    Image,
    /// Video container or stream.
    Video,
    /// Audio stream or container.
    Audio,
    /// Office, markup, ebook, or PDF document.
    Document,
    /// Archive or compressed byte stream.
    Archive,
    /// Unrecognized or raw binary payload.
    Unknown,
}

/// Canonical format identifier resolved from magic bytes and container structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormatId {
    /// JPEG image.
    Jpeg,
    /// PNG / APNG image.
    Png,
    /// WebP image.
    Webp,
    /// AVIF image.
    Avif,
    /// Portable Document Format.
    Pdf,
    /// ZIP archive.
    Zip,
    /// TAR archive.
    Tar,
    /// Zstandard stream.
    Zstd,
    /// 7-Zip archive.
    SevenZ,
    /// `FileFusion` authenticated encrypted container (`.ffenc`).
    Ffenc,
    /// Unclassified binary or text format.
    Unknown,
}

/// Execution lifecycle stages emitted in [`ProgressEvent`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStage {
    /// Waiting in bounded scheduler queue.
    Queued,
    /// Reading magic bytes and container headers.
    Probing,
    /// Enforcing bomb, traversal, and size limits.
    SecurityCheck,
    /// Executing compression, conversion, or inspection work.
    Processing,
    /// Running `OutputValidator` checks on staged output.
    Validating,
    /// Atomically committing staged output to destination.
    Committing,
    /// Job completed successfully.
    Completed,
    /// Job aborted cleanly via `CancellationToken`.
    Cancelled,
    /// Job failed with a typed diagnostic.
    Failed,
}

/// Monotonic progress update event emitted across CLI, FFI, WASM, and REST bridges.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProgressEvent {
    /// Unique job identifier.
    pub job_id: String,
    /// 1-based index of active item within the batch.
    pub item_index: u32,
    /// Total items in the batch.
    pub item_total: u32,
    /// Current pipeline stage.
    pub stage: JobStage,
    /// Monotonic completion percentage (`0.0..=100.0`).
    pub percent: f32,
    /// Input byte count for the active item.
    pub bytes_in: u64,
    /// Estimated or actual output bytes.
    pub bytes_out_estimate: Option<u64>,
    /// Estimated remaining milliseconds.
    pub eta_ms: Option<u64>,
    /// Human-readable status detail.
    pub detail: String,
}

/// Resource and untrusted-input safety ceilings (`FF-CTR-001` §2.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum concurrent CPU worker threads.
    pub max_workers: usize,
    /// Global working-set RAM budget in megabytes.
    pub max_memory_mb: u64,
    /// Maximum allowed decompression expansion ratio (default `100:1`).
    pub max_expansion_ratio: u32,
    /// Maximum uncompressed bytes permitted per extracted archive.
    pub max_uncompressed_bytes: u64,
    /// Maximum nested container depth (default `3`).
    pub max_archive_depth: u8,
    /// Maximum wall-clock seconds for a single Tier 2 subprocess.
    pub subprocess_timeout_secs: u64,
}

impl ResourceLimits {
    /// Default expansion ratio ceiling (`100:1`).
    pub const DEFAULT_MAX_EXPANSION_RATIO: u32 = 100;
    /// Default uncompressed extraction ceiling (`4 GiB`).
    pub const DEFAULT_MAX_UNCOMPRESSED_BYTES: u64 = 4 * 1024 * 1024 * 1024;
    /// Default nested archive recursion limit.
    pub const DEFAULT_MAX_ARCHIVE_DEPTH: u8 = 3;
    /// Default Tier 2 subprocess wall-clock timeout (30 minutes).
    pub const DEFAULT_SUBPROCESS_TIMEOUT_SECS: u64 = 1800;

    /// Computes resource-aware worker and RAM ceilings for a host profile.
    ///
    /// Reserves at least 1 core for OS/UI responsiveness on multi-core hosts and
    /// caps memory to `min(4096 MB, max(256 MB, total_ram_mb / 4))`.
    #[must_use]
    pub fn for_host(logical_cpus: usize, total_ram_mb: u64) -> Self {
        let reserved_cpus = if logical_cpus > 1 {
            logical_cpus - 1
        } else {
            1
        };
        let max_memory_mb = (total_ram_mb / 4).clamp(256, 4096);
        let mem_bound_workers = usize::try_from((max_memory_mb / 512).max(1)).unwrap_or(1);
        let max_workers = reserved_cpus.min(mem_bound_workers).max(1);

        Self {
            max_workers,
            max_memory_mb,
            max_expansion_ratio: Self::DEFAULT_MAX_EXPANSION_RATIO,
            max_uncompressed_bytes: Self::DEFAULT_MAX_UNCOMPRESSED_BYTES,
            max_archive_depth: Self::DEFAULT_MAX_ARCHIVE_DEPTH,
            subprocess_timeout_secs: Self::DEFAULT_SUBPROCESS_TIMEOUT_SECS,
        }
    }
}

impl Default for ResourceLimits {
    fn default() -> Self {
        let cpus = std::thread::available_parallelism().map_or(2, std::num::NonZeroUsize::get);
        // Conservative 4096 MB baseline when exact host RAM is not queried directly.
        Self::for_host(cpus, 4096)
    }
}

/// Deterministic CLI process exit codes (`docs/CONTRACTS.md` §5.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CliExitCode {
    /// All items succeeded (including verified no-ops).
    Success = 0,
    /// Partial or full operational failure in batch.
    OperationalFailure = 1,
    /// Invalid CLI arguments or configuration.
    InvalidUsage = 2,
    /// Security violation rejected (`FF-ERR-SEC-*`).
    SecurityRejection = 3,
    /// Aborted by user cancellation.
    Cancelled = 4,
    /// Required Tier 2 external dependency missing.
    MissingEngine = 5,
}

impl From<CliExitCode> for std::process::ExitCode {
    fn from(code: CliExitCode) -> Self {
        Self::from(code as u8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_identity_is_consistent() {
        assert_eq!(CRATE_NAME, "file-core");
        assert_ne!(WORKSPACE_VERSION, "");
        assert_eq!(
            workspace_identity(),
            format!("{CRATE_NAME}/{WORKSPACE_VERSION}")
        );
    }

    #[test]
    fn resource_limits_respects_16gb_host_and_mobile_bounds() {
        // 12-core 16 GB Windows 11 workstation -> 4096 MB cap, <= 8 workers
        let desktop = ResourceLimits::for_host(12, 16_384);
        assert_eq!(desktop.max_memory_mb, 4096);
        assert_eq!(desktop.max_workers, 8);
        assert_eq!(desktop.max_expansion_ratio, 100);
        assert_eq!(desktop.max_archive_depth, 3);

        // 4-core 3 GB mobile device -> 768 MB cap, 1 worker
        let mobile = ResourceLimits::for_host(4, 3072);
        assert_eq!(mobile.max_memory_mb, 768);
        assert_eq!(mobile.max_workers, 1);

        // Zero/minimal degenerate inputs clamp safely to >= 1 worker and >= 256 MB
        let minimal = ResourceLimits::for_host(0, 0);
        assert_eq!(minimal.max_memory_mb, 256);
        assert_eq!(minimal.max_workers, 1);
    }

    #[test]
    fn cli_exit_codes_match_contract_values() {
        assert_eq!(CliExitCode::Success as u8, 0);
        assert_eq!(CliExitCode::OperationalFailure as u8, 1);
        assert_eq!(CliExitCode::InvalidUsage as u8, 2);
        assert_eq!(CliExitCode::SecurityRejection as u8, 3);
        assert_eq!(CliExitCode::Cancelled as u8, 4);
        assert_eq!(CliExitCode::MissingEngine as u8, 5);
    }
}
