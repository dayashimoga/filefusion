//! `file-core`: L0 foundation crate for `FileFusion`.
//!
//! Defines workspace identity, canonical error taxonomy, privacy-safe structured
//! logging, format/media taxonomies, resource and security limits, monotonic
//! progress events, and CLI exit code contracts (`docs/CONTRACTS.md` / `FF-CTR-001`).

#![forbid(unsafe_code)]

pub mod classification;
pub mod config;
pub mod contracts;
pub mod error;
pub mod limits;
pub mod logging;
pub mod metadata;
pub mod pipeline;
pub mod signature;

use serde::{Deserialize, Serialize};

pub use classification::{
    CompressibilityTier, FileClassification, ProcessingDomain, classify_inspection_report,
    classify_processing_domain, compute_entropy_milli_bits,
};

pub use config::{
    BatchDefaults, CliConfigOverrides, CollisionPolicy, ConfigSource, FileFusionConfig,
    LoadedConfig, PathConfig, PrivacyConfig, load_config_from_path, load_effective_config,
    load_effective_config_with, resolve_default_config_path, resolve_default_config_path_with,
    save_config_atomic,
};
pub use contracts::{
    CompressionEngine, CompressionOutcome, CompressionRequest, ConversionEngine,
    ConversionFidelityReport, ConversionRequest, ConversionRoute, ExternalToolAdapter,
    FidelitySeverity, IsolatedProcessRunner, OutputValidator, PreflightAssessment,
    PreflightWarning, ProcessInvocation, ProcessOutput, QualityEvaluator, QualityMetrics,
    ValidationOutcome,
};
pub use error::{ErrorCategory, ErrorCode, ErrorDiagnostic, FileFusionError, FileFusionResult};
pub use limits::{
    DeviceProfile, MemoryBudgetSnapshot, MemoryGovernor, MemoryReservation, copy_bounded_streaming,
    parse_proc_meminfo_mb,
};
pub use logging::{LogFormat, LogLevel, LoggingConfig, init_logging, redact_path_for_log};
pub use metadata::{
    DigestSet, FileInspectionReport, MetadataSummary, RecommendedAction, StreamDescriptor,
    StreamKind, compute_digests_from_bytes, compute_digests_from_reader, inspect_bytes,
    inspect_path, inspect_reader,
};
pub use pipeline::{
    CallbackProgressSink, CancelSubscription, CancellationToken, CompressionProfile,
    CompressionStatus, ItemOutcome, ItemStatus, JobContext, JobReceipt, MonotonicProgressSink,
    NoOpProgressSink, ProcessingHandler, ProcessingOperation, ProcessingRequest, ProgressSink,
    StagedPath, ThrottledProgressSink, ValidatedSource, VecProgressSink,
    resolve_output_destination,
};
pub use signature::{
    DetectionConfidence, SignatureMatch, detect_signature_from_bytes, detect_signature_from_path,
    detect_signature_from_reader, enforce_spoofing_policy,
};

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
    /// Office, markup, ebook, structured text, or PDF document.
    Document,
    /// Archive or compressed byte stream.
    Archive,
    /// Native executable binary or script payload (blocked/flagged in spoofing checks).
    Executable,
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
    /// HEIC / HEIF image.
    Heic,
    /// GIF image.
    Gif,
    /// Windows Bitmap image.
    Bmp,
    /// TIFF image.
    Tiff,
    /// Windows Icon / Cursor image.
    Ico,
    /// Scalable Vector Graphics document.
    Svg,
    /// MPEG-4 video container.
    Mp4,
    /// Apple `QuickTime` container.
    Mov,
    /// Matroska video container.
    Mkv,
    /// `WebM` video container.
    Webm,
    /// Audio Video Interleave container.
    Avi,
    /// Free Lossless Audio Codec stream.
    Flac,
    /// Waveform Audio container.
    Wav,
    /// MPEG Layer-3 audio stream.
    Mp3,
    /// Ogg Vorbis / generic Ogg container.
    Ogg,
    /// Ogg Opus audio stream.
    Opus,
    /// Advanced Audio Coding ADTS stream.
    Aac,
    /// MPEG-4 audio container (`.m4a`).
    M4a,
    /// Portable Document Format.
    Pdf,
    /// Microsoft Word OOXML document.
    Docx,
    /// Microsoft Excel OOXML workbook.
    Xlsx,
    /// Microsoft `PowerPoint` OOXML presentation.
    Pptx,
    /// `OpenDocument` Text document.
    Odt,
    /// `OpenDocument` Spreadsheet document.
    Ods,
    /// `OpenDocument` Presentation document.
    Odp,
    /// Electronic Publication ebook container.
    Epub,
    /// Mobipocket / Kindle ebook format.
    Mobi,
    /// Rich Text Format document.
    Rtf,
    /// `HyperText` Markup Language document.
    Html,
    /// Extensible Markup Language document.
    Xml,
    /// JavaScript Object Notation document.
    Json,
    /// YAML Ain't Markup Language document.
    Yaml,
    /// Comma-Separated Values tabular document.
    Csv,
    /// Markdown document.
    Markdown,
    /// Plain UTF-8 / UTF-16 text document.
    PlainText,
    /// ZIP archive.
    Zip,
    /// POSIX / USTAR TAR archive.
    Tar,
    /// Gzip compressed stream.
    Gzip,
    /// Bzip2 compressed stream.
    Bzip2,
    /// XZ / LZMA2 compressed stream.
    Xz,
    /// Zstandard compressed stream.
    Zstd,
    /// LZ4 frame stream.
    Lz4,
    /// 7-Zip archive.
    SevenZ,
    /// `FileFusion` authenticated encrypted container (`.ffenc`).
    Ffenc,
    /// Native executable binary (PE, ELF, Mach-O) or shell shebang script.
    Executable,
    /// Unclassified binary payload.
    Unknown,
}

impl FormatId {
    /// Returns the primary MIME type associated with this format.
    #[must_use]
    pub const fn mime_type(self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
            Self::Webp => "image/webp",
            Self::Avif => "image/avif",
            Self::Heic => "image/heic",
            Self::Gif => "image/gif",
            Self::Bmp => "image/bmp",
            Self::Tiff => "image/tiff",
            Self::Ico => "image/x-icon",
            Self::Svg => "image/svg+xml",
            Self::Mp4 => "video/mp4",
            Self::Mov => "video/quicktime",
            Self::Mkv => "video/x-matroska",
            Self::Webm => "video/webm",
            Self::Avi => "video/x-msvideo",
            Self::Flac => "audio/flac",
            Self::Wav => "audio/wav",
            Self::Mp3 => "audio/mpeg",
            Self::Ogg => "audio/ogg",
            Self::Opus => "audio/opus",
            Self::Aac => "audio/aac",
            Self::M4a => "audio/mp4",
            Self::Pdf => "application/pdf",
            Self::Docx => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            Self::Xlsx => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            Self::Pptx => {
                "application/vnd.openxmlformats-officedocument.presentationml.presentation"
            }
            Self::Odt => "application/vnd.oasis.opendocument.text",
            Self::Ods => "application/vnd.oasis.opendocument.spreadsheet",
            Self::Odp => "application/vnd.oasis.opendocument.presentation",
            Self::Epub => "application/epub+zip",
            Self::Mobi => "application/x-mobipocket-ebook",
            Self::Rtf => "application/rtf",
            Self::Html => "text/html",
            Self::Xml => "application/xml",
            Self::Json => "application/json",
            Self::Yaml => "application/yaml",
            Self::Csv => "text/csv",
            Self::Markdown => "text/markdown",
            Self::PlainText => "text/plain",
            Self::Zip => "application/zip",
            Self::Tar => "application/x-tar",
            Self::Gzip => "application/gzip",
            Self::Bzip2 => "application/x-bzip2",
            Self::Xz => "application/x-xz",
            Self::Zstd => "application/zstd",
            Self::Lz4 => "application/x-lz4",
            Self::SevenZ => "application/x-7z-compressed",
            Self::Ffenc => "application/vnd.filefusion.encrypted",
            Self::Executable => "application/x-executable",
            Self::Unknown => "application/octet-stream",
        }
    }

    /// Returns the high-level [`MediaKind`] bucket for this format.
    #[must_use]
    pub const fn media_kind(self) -> MediaKind {
        match self {
            Self::Jpeg
            | Self::Png
            | Self::Webp
            | Self::Avif
            | Self::Heic
            | Self::Gif
            | Self::Bmp
            | Self::Tiff
            | Self::Ico
            | Self::Svg => MediaKind::Image,
            Self::Mp4 | Self::Mov | Self::Mkv | Self::Webm | Self::Avi => MediaKind::Video,
            Self::Flac | Self::Wav | Self::Mp3 | Self::Ogg | Self::Opus | Self::Aac | Self::M4a => {
                MediaKind::Audio
            }
            Self::Pdf
            | Self::Docx
            | Self::Xlsx
            | Self::Pptx
            | Self::Odt
            | Self::Ods
            | Self::Odp
            | Self::Epub
            | Self::Mobi
            | Self::Rtf
            | Self::Html
            | Self::Xml
            | Self::Json
            | Self::Yaml
            | Self::Csv
            | Self::Markdown
            | Self::PlainText => MediaKind::Document,
            Self::Zip
            | Self::Tar
            | Self::Gzip
            | Self::Bzip2
            | Self::Xz
            | Self::Zstd
            | Self::Lz4
            | Self::SevenZ
            | Self::Ffenc => MediaKind::Archive,
            Self::Executable => MediaKind::Executable,
            Self::Unknown => MediaKind::Unknown,
        }
    }

    /// Returns canonical lowercase file extensions associated with this format.
    #[must_use]
    pub const fn canonical_extensions(self) -> &'static [&'static str] {
        match self {
            Self::Jpeg => &["jpg", "jpeg", "jpe"],
            Self::Png => &["png", "apng"],
            Self::Webp => &["webp"],
            Self::Avif => &["avif"],
            Self::Heic => &["heic", "heif"],
            Self::Gif => &["gif"],
            Self::Bmp => &["bmp"],
            Self::Tiff => &["tif", "tiff"],
            Self::Ico => &["ico", "cur"],
            Self::Svg => &["svg"],
            Self::Mp4 => &["mp4", "m4v"],
            Self::Mov => &["mov", "qt"],
            Self::Mkv => &["mkv"],
            Self::Webm => &["webm"],
            Self::Avi => &["avi"],
            Self::Flac => &["flac"],
            Self::Wav => &["wav", "wave"],
            Self::Mp3 => &["mp3"],
            Self::Ogg => &["ogg", "oga", "ogv"],
            Self::Opus => &["opus", "ogg"],
            Self::Aac => &["aac"],
            Self::M4a => &["m4a", "m4b"],
            Self::Pdf => &["pdf"],
            Self::Docx => &["docx", "docm"],
            Self::Xlsx => &["xlsx", "xlsm"],
            Self::Pptx => &["pptx", "pptm"],
            Self::Odt => &["odt"],
            Self::Ods => &["ods"],
            Self::Odp => &["odp"],
            Self::Epub => &["epub"],
            Self::Mobi => &["mobi", "azw", "azw3", "prc"],
            Self::Rtf => &["rtf"],
            Self::Html => &["html", "htm", "xhtml"],
            Self::Xml => &["xml", "xsd", "xsl"],
            Self::Json => &["json", "jsonl"],
            Self::Yaml => &["yaml", "yml"],
            Self::Csv => &["csv", "tsv"],
            Self::Markdown => &["md", "markdown"],
            Self::PlainText => &["txt", "text", "log", "ini", "toml", "cfg", "conf"],
            Self::Zip => &["zip", "jar", "apk", "cbz"],
            Self::Tar => &["tar"],
            Self::Gzip => &["gz", "tgz"],
            Self::Bzip2 => &["bz2", "tbz2"],
            Self::Xz => &["xz", "txz"],
            Self::Zstd => &["zst", "tzst"],
            Self::Lz4 => &["lz4"],
            Self::SevenZ => &["7z"],
            Self::Ffenc => &["ffenc"],
            Self::Executable => &["exe", "dll", "so", "dylib", "elf", "bin", "sh"],
            Self::Unknown => &[],
        }
    }
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
#[serde(default)]
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
        limits::detect_default_limits()
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
