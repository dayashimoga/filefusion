//! `filefusion` CLI entrypoint (`apps/cli`).
//!
//! Thin Plane 1/2 adapter over `job-manager` (`MOD-RULE-03`) with versioned
//! configuration resolution and structured `tracing` diagnostics routed strictly
//! to `stderr`.

#![forbid(unsafe_code)]

mod stage4_cmds;
mod stage5_cmds;

use std::fmt::Write as _;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use file_core::{
    CORE_SCHEMA_VERSION, CliConfigOverrides, CliExitCode, CollisionPolicy, CompressionProfile,
    CompressionRequest, CompressionStatus, ConversionRequest, ErrorCode, FileFusionError,
    FileInspectionReport, FormatId, LogFormat, LogLevel, PreflightAssessment, SignatureMatch,
    init_logging, resolve_default_config_path, save_config_atomic, workspace_identity,
};
use job_manager::{
    ArchiveCreateReport, BenchSuiteReport, CompressFileReport, ConvertFileReport, DoctorReport,
    JobManager, inspect_file, load_runtime_config, probe_file_signature,
    run_doctor_probe_with_config,
};
use serde::Serialize;

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliLogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl From<CliLogLevel> for LogLevel {
    fn from(value: CliLogLevel) -> Self {
        match value {
            CliLogLevel::Error => Self::Error,
            CliLogLevel::Warn => Self::Warn,
            CliLogLevel::Info => Self::Info,
            CliLogLevel::Debug => Self::Debug,
            CliLogLevel::Trace => Self::Trace,
        }
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "filefusion",
    version,
    about = "FileFusion — Universal File Intelligence, Transformation & Optimization CLI"
)]
struct Cli {
    /// Path to an explicit TOML configuration file (`FILEFUSION_CONFIG`).
    #[arg(long, global = true, value_name = "PATH")]
    config: Option<PathBuf>,

    /// Maximum concurrent CPU workers (`FILEFUSION_WORKERS`).
    #[arg(long, global = true, value_name = "N")]
    workers: Option<usize>,

    /// Global working-set RAM budget in MB (`FILEFUSION_MAX_MEMORY_MB`).
    #[arg(long, global = true, value_name = "MB")]
    max_memory_mb: Option<u64>,

    /// Suppress non-error stderr diagnostics.
    #[arg(short, long, global = true)]
    quiet: bool,

    /// Disable ANSI color sequences in stderr logs and terminal output.
    #[arg(long, global = true)]
    no_color: bool,

    /// Minimum log level emitted to stderr.
    #[arg(long, global = true, value_enum)]
    log_level: Option<CliLogLevel>,

    /// Emit structured JSON log lines to stderr.
    #[arg(long, global = true)]
    log_json: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliProfile {
    Lossless,
    Balanced,
    Web,
    Extreme,
    Archival,
}

impl From<CliProfile> for CompressionProfile {
    fn from(value: CliProfile) -> Self {
        match value {
            CliProfile::Lossless => Self::LosslessMaximum,
            CliProfile::Balanced => Self::BalancedOptimal,
            CliProfile::Web => Self::WebSharingReady,
            CliProfile::Extreme => Self::ExtremeSizeTarget,
            CliProfile::Archival => Self::ArchivalColdStorage,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliTargetFormat {
    Jpeg,
    Png,
    Webp,
    Avif,
    Gif,
    Bmp,
    Tiff,
    Ico,
    Pdf,
    Docx,
    Xlsx,
    Pptx,
    Odt,
    Ods,
    Odp,
    Epub,
    Mobi,
    Rtf,
    Html,
    Xml,
    Json,
    #[value(name = "yaml", alias = "yml")]
    Yaml,
    Csv,
    #[value(name = "markdown", alias = "md")]
    Markdown,
    #[value(name = "txt", alias = "plain-text")]
    PlainText,
    Mp4,
    Mov,
    Mkv,
    Webm,
    Avi,
    Wav,
    Flac,
    Mp3,
    Ogg,
    Opus,
    Aac,
    M4a,
    Zip,
    Tar,
    Gzip,
    Zstd,
    Xz,
    Bzip2,
    Lz4,
    #[value(name = "7z")]
    SevenZ,
}

impl From<CliTargetFormat> for FormatId {
    fn from(value: CliTargetFormat) -> Self {
        match value {
            CliTargetFormat::Jpeg => Self::Jpeg,
            CliTargetFormat::Png => Self::Png,
            CliTargetFormat::Webp => Self::Webp,
            CliTargetFormat::Avif => Self::Avif,
            CliTargetFormat::Gif => Self::Gif,
            CliTargetFormat::Bmp => Self::Bmp,
            CliTargetFormat::Tiff => Self::Tiff,
            CliTargetFormat::Ico => Self::Ico,
            CliTargetFormat::Pdf => Self::Pdf,
            CliTargetFormat::Docx => Self::Docx,
            CliTargetFormat::Xlsx => Self::Xlsx,
            CliTargetFormat::Pptx => Self::Pptx,
            CliTargetFormat::Odt => Self::Odt,
            CliTargetFormat::Ods => Self::Ods,
            CliTargetFormat::Odp => Self::Odp,
            CliTargetFormat::Epub => Self::Epub,
            CliTargetFormat::Mobi => Self::Mobi,
            CliTargetFormat::Rtf => Self::Rtf,
            CliTargetFormat::Html => Self::Html,
            CliTargetFormat::Xml => Self::Xml,
            CliTargetFormat::Json => Self::Json,
            CliTargetFormat::Yaml => Self::Yaml,
            CliTargetFormat::Csv => Self::Csv,
            CliTargetFormat::Markdown => Self::Markdown,
            CliTargetFormat::PlainText => Self::PlainText,
            CliTargetFormat::Mp4 => Self::Mp4,
            CliTargetFormat::Mov => Self::Mov,
            CliTargetFormat::Mkv => Self::Mkv,
            CliTargetFormat::Webm => Self::Webm,
            CliTargetFormat::Avi => Self::Avi,
            CliTargetFormat::Wav => Self::Wav,
            CliTargetFormat::Flac => Self::Flac,
            CliTargetFormat::Mp3 => Self::Mp3,
            CliTargetFormat::Ogg => Self::Ogg,
            CliTargetFormat::Opus => Self::Opus,
            CliTargetFormat::Aac => Self::Aac,
            CliTargetFormat::M4a => Self::M4a,
            CliTargetFormat::Zip => Self::Zip,
            CliTargetFormat::Tar => Self::Tar,
            CliTargetFormat::Gzip => Self::Gzip,
            CliTargetFormat::Zstd => Self::Zstd,
            CliTargetFormat::Xz => Self::Xz,
            CliTargetFormat::Bzip2 => Self::Bzip2,
            CliTargetFormat::Lz4 => Self::Lz4,
            CliTargetFormat::SevenZ => Self::SevenZ,
        }
    }
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Inspect host environment, resource ceilings, and Tier 1/Tier 2 engine readiness.
    Doctor {
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Inspect file signature, streaming digests, container streams, and metadata (`FR-DET-001..004`).
    Inspect {
        /// Target file path to inspect.
        #[arg(value_name = "PATH")]
        path: PathBuf,
        /// Fail with exit code 3 (`FF-ERR-SEC-SPOOFED`) if extension contradicts true signature.
        #[arg(long)]
        strict: bool,
        /// Run fast header/container signature probing only (skip full-file streaming digests).
        #[arg(long)]
        signature_only: bool,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Compress and optimize media, document, or archive files with adaptive codec selection (`FR-CMP-001..003`).
    Compress {
        /// Input file path to compress.
        #[arg(value_name = "INPUT")]
        input: PathBuf,
        /// Destination output path (defaults to sibling `<stem>.opt.<ext>`).
        #[arg(short, long, value_name = "OUTPUT")]
        output: Option<PathBuf>,
        /// Preset optimization profile (`FR-CMP-001`).
        #[arg(long, value_enum, default_value = "balanced")]
        profile: CliProfile,
        /// Explicit target format override.
        #[arg(long, value_enum)]
        format: Option<CliTargetFormat>,
        /// Optional maximum target output size in bytes (`FR-CMP-003`).
        #[arg(long, value_name = "BYTES")]
        target_max_bytes: Option<u64>,
        /// Optional minimum perceptual SSIM floor (`0.0..=1.0`, `FR-CMP-003`).
        #[arg(long, value_name = "SSIM")]
        min_ssim: Option<f64>,
        /// Run quality-constrained binary solver when targeting `JPEG` (`FR-CMP-003`).
        #[arg(long)]
        solve_quality: bool,
        /// Strip non-essential identifying metadata (`FR-MET-002`).
        #[arg(long)]
        strip_metadata: bool,
        /// Allow writing a same-format output even if it is not smaller than the input.
        #[arg(long)]
        no_noop_guard: bool,
        /// Overwrite existing destination file (saving a `.bak` backup per safety policy).
        #[arg(long)]
        overwrite: bool,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Convert documents, markup, tabular datasets, ebooks, archives, or media files (`FR-DOC-001`..`003`, `FR-FID-001`..`002`).
    Convert {
        /// Input file path to convert.
        #[arg(value_name = "INPUT")]
        input: PathBuf,
        /// Target format to convert into.
        #[arg(long = "to", value_enum)]
        target_format: CliTargetFormat,
        /// Destination output path (defaults to sibling `<stem>.<ext>`).
        #[arg(short, long, value_name = "OUTPUT")]
        output: Option<PathBuf>,
        /// Run pre-flight compatibility & feature-loss check without writing any output (`FR-FID-002`).
        #[arg(long)]
        preflight_only: bool,
        /// Allow conversion even when pre-flight reports breaking structural loss (e.g. XLSX -> CSV).
        #[arg(long)]
        force_loss: bool,
        /// Do not attempt structural preservation during conversion.
        #[arg(long)]
        no_preserve_structure: bool,
        /// Strip non-essential author/generator/EXIF metadata (`FR-MET-002`).
        #[arg(long)]
        strip_metadata: bool,
        /// Overwrite existing destination file (saving a `.bak` backup per safety policy).
        #[arg(long)]
        overwrite: bool,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Create, list, or safely extract multi-format archives (`FR-ARC-001..003`, `FR-SEC-001`).
    Archive {
        #[command(subcommand)]
        action: ArchiveAction,
    },
    /// PDF Studio manipulation, OCR, AES-256 protection, and signature verification (`FR-PDF-001`..`004`).
    Pdf {
        #[command(subcommand)]
        action: stage4_cmds::PdfAction,
    },
    /// Diagnose and repair damaged `PDF`, `ZIP`, or media container files (`FR-REP-001`..`002`).
    Repair(Box<stage4_cmds::RepairArgs>),
    /// Inspect, privacy-scrub, or edit normalized file metadata (`FR-MET-001`..`002`).
    Metadata {
        #[command(subcommand)]
        action: stage4_cmds::MetadataAction,
    },
    /// Encrypt files into authenticated `.ffenc` containers or AES-256 PDFs (`FR-SEC-002`, `FR-PDF-004`).
    Encrypt(Box<stage4_cmds::EncryptArgs>),
    /// Decrypt authenticated `.ffenc` containers or password-protected PDFs (`FR-SEC-002`, `FR-PDF-004`).
    Decrypt(Box<stage4_cmds::DecryptArgs>),
    /// Multi-stage exact/near/CDC deduplication & safe quarantine/hardlink cleanup (`FR-DUP-001`..`003`).
    Dedup {
        #[command(subcommand)]
        action: stage5_cmds::DedupAction,
    },
    /// Compare two files across byte, digest, `FastCDC`, visual `SSIM`/`PSNR`, text & metadata (`FR-VAL-002`).
    Diff(Box<stage5_cmds::DiffArgs>),
    /// Analyze directory treemap, domain/format disk footprint & largest files (`FR-DUP-003`).
    Storage(Box<stage5_cmds::StorageArgs>),
    /// Run deterministic host throughput micro-benchmarks across `FastCDC` and `Zstd`.
    Bench {
        /// Iterations per micro-benchmark workload (`1..=200`).
        #[arg(long, default_value_t = 8)]
        iterations: u32,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Manage `FileFusion` TOML configuration, defaults, and precedence resolution.
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Debug, Subcommand)]
enum ArchiveAction {
    /// Create a `.zip`, `.tar`, `.tar.gz`, `.tar.zst`, `.tar.xz`, `.tar.bz2`, `.tar.lz4`, `.tar.br`, or `.7z` archive.
    Create {
        /// Output archive file path.
        #[arg(value_name = "ARCHIVE")]
        output: PathBuf,
        /// Input files or directories to archive.
        #[arg(value_name = "INPUTS", required = true)]
        inputs: Vec<PathBuf>,
        /// Compression profile preset.
        #[arg(long, value_enum, default_value = "balanced")]
        profile: CliProfile,
        /// Overwrite existing archive (keeping a `.bak` safety backup).
        #[arg(long)]
        overwrite: bool,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// List entries inside an archive without extracting to disk.
    List {
        /// Archive file path.
        #[arg(value_name = "ARCHIVE")]
        archive: PathBuf,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Safely extract an archive into a target directory with traversal and bomb protection.
    Extract {
        /// Archive file path.
        #[arg(value_name = "ARCHIVE")]
        archive: PathBuf,
        /// Destination extraction directory.
        #[arg(short, long, value_name = "DIR", default_value = ".")]
        output: PathBuf,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum ConfigAction {
    /// Display the resolved effective configuration (defaults -> TOML -> env -> CLI).
    Show {
        /// Emit machine-readable JSON output including provenance metadata.
        #[arg(long)]
        json: bool,
    },
    /// Print the resolved default configuration file path for this platform.
    Path {
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Write an initial or updated `config.toml` file atomically.
    Init {
        /// Overwrite an existing configuration file.
        #[arg(long)]
        force: bool,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Validate active configuration syntax, limits, and privacy/security invariants.
    Validate {
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Serialize)]
struct ConfigStatusOutput {
    schema_version: u32,
    ok: bool,
    path: Option<String>,
    action: &'static str,
}

fn format_doctor_human(report: &DoctorReport) -> String {
    let mut out = format!(
        "FileFusion ({}) — Doctor Diagnostics\nHost: {} ({}) | Logical CPUs: {}\nDefault ResourceLimits: max_workers={}, max_memory_mb={} MB, max_expansion_ratio={}:1, max_archive_depth={}\nEffective ResourceLimits: max_workers={}, max_memory_mb={} MB, max_expansion_ratio={}:1, max_archive_depth={}\nCapabilities:\n",
        workspace_identity(),
        report.host_os,
        report.host_arch,
        report.logical_cpus,
        report.default_limits.max_workers,
        report.default_limits.max_memory_mb,
        report.default_limits.max_expansion_ratio,
        report.default_limits.max_archive_depth,
        report.effective_limits.max_workers,
        report.effective_limits.max_memory_mb,
        report.effective_limits.max_expansion_ratio,
        report.effective_limits.max_archive_depth,
    );
    for cap in &report.capabilities {
        let state = if cap.available { "READY" } else { "MISSING" };
        let _ = writeln!(
            out,
            "  - [{}] {} ({:?}, {}) — {}",
            state, cap.id, cap.tier, cap.spdx, cap.note
        );
    }
    out
}

fn write_stream(stream: &mut dyn Write, content: &str) -> io::Result<()> {
    stream.write_all(content.as_bytes())?;
    if !content.ends_with('\n') {
        stream.write_all(b"\n")?;
    }
    stream.flush()
}

pub(crate) fn emit_text_stdout(content: &str, json_err_mode: bool) -> CliExitCode {
    let mut stdout = io::stdout().lock();
    if let Err(err) = write_stream(&mut stdout, content) {
        let ff_err = FileFusionError::from(err);
        return report_cli_error(&ff_err, json_err_mode);
    }
    CliExitCode::Success
}

pub(crate) fn emit_json_stdout<T: Serialize>(value: &T) -> CliExitCode {
    match serde_json::to_string_pretty(value) {
        Ok(rendered) => emit_text_stdout(&rendered, true),
        Err(err) => {
            let ff_err = FileFusionError::new(
                ErrorCode::IntInvariant,
                format!("failed to serialize JSON output: {err}"),
            );
            report_cli_error(&ff_err, true)
        }
    }
}

pub(crate) fn report_cli_error(err: &FileFusionError, json_mode: bool) -> CliExitCode {
    tracing::error!(
        code = %err.diagnostic.code,
        category = ?err.diagnostic.category,
        "{}",
        err.diagnostic.message
    );
    let rendered = if json_mode {
        serde_json::to_string_pretty(&err.diagnostic)
            .unwrap_or_else(|_| format!("{{\"error\":\"{}\"}}", err.diagnostic.code))
    } else if let Some(hint) = &err.diagnostic.remediation {
        format!(
            "Error [{}]: {}\nHint: {}",
            err.diagnostic.code, err.diagnostic.message, hint
        )
    } else {
        format!(
            "Error [{}]: {}",
            err.diagnostic.code, err.diagnostic.message
        )
    };
    let mut stderr = io::stderr().lock();
    let _ = write_stream(&mut stderr, &rendered);
    err.exit_code()
}

fn format_signature_human(probe: &SignatureMatch) -> String {
    let ext = probe.claimed_extension.as_deref().unwrap_or("<none>");
    let spoof_line = if probe.spoofing_mismatch {
        format!(
            "Spoofing Alert: YES ({})",
            probe
                .spoofing_reason
                .as_deref()
                .unwrap_or("extension mismatch")
        )
    } else {
        String::from("Spoofing Alert: none")
    };
    format!(
        "Detected Format: {:?}\nMIME Type: {}\nMedia Kind: {:?}\nConfidence: {:?} ({})\nClaimed Extension: {}\n{}",
        probe.detected_format,
        probe.mime_type,
        probe.media_kind,
        probe.confidence,
        probe.matched_rule,
        ext,
        spoof_line,
    )
}

fn format_inspection_human(report: &FileInspectionReport) -> String {
    let ext = report.extension_claimed.as_deref().unwrap_or("<none>");
    let mut out = format!(
        "Detected Format: {:?}\nMIME Type: {}\nClaimed Extension: {}\nSpoofing Mismatch: {}\nSize (bytes): {}\nDigests:\n  BLAKE3:   {}\n  SHA-256:  {}\n  xxHash64: {}\nMetadata Summary: encrypted={}, exif={}, xmp={}, icc={}, gps_risk={}, pii_author_risk={}\nStreams ({}):\n",
        report.detected_format,
        report.mime_type,
        ext,
        report.spoofing_mismatch,
        report.byte_size,
        report.digests.blake3,
        report.digests.sha256,
        report.digests.xxhash64,
        report.metadata_summary.is_encrypted,
        report.metadata_summary.has_exif,
        report.metadata_summary.has_xmp,
        report.metadata_summary.has_icc_profile,
        report.metadata_summary.has_gps_privacy_risk,
        report.metadata_summary.has_pii_author_risk,
        report.streams.len(),
    );
    for stream in &report.streams {
        let codec = stream.codec.as_deref().unwrap_or("unknown");
        let detail = stream.detail.as_deref().unwrap_or("");
        let _ = writeln!(
            out,
            "  #{} [{:?}] {} — {}",
            stream.index, stream.kind, codec, detail
        );
    }
    if let Some(cls) = &report.classification {
        let _ = writeln!(
            out,
            "Classification: domain={:?}, compressibility={:?}, entropy={:.3} bits/byte, privacy_sensitive={}, security_review={}",
            cls.domain,
            cls.compressibility,
            cls.entropy_bits_per_byte(),
            cls.privacy_sensitive,
            cls.requires_security_review
        );
    }
    if !report.recommended_actions.is_empty() {
        out.push_str("Recommended Actions:\n");
        for act in &report.recommended_actions {
            let _ = writeln!(
                out,
                "  - [{}] {} — {}",
                act.action_id, act.title, act.reason
            );
        }
    }
    out
}

const fn command_requests_json(command: &Commands) -> bool {
    match command {
        Commands::Doctor { json }
        | Commands::Inspect { json, .. }
        | Commands::Compress { json, .. }
        | Commands::Convert { json, .. }
        | Commands::Bench { json, .. } => *json,
        Commands::Archive { action } => match action {
            ArchiveAction::Create { json, .. }
            | ArchiveAction::List { json, .. }
            | ArchiveAction::Extract { json, .. } => *json,
        },
        Commands::Config { action } => match action {
            ConfigAction::Show { json }
            | ConfigAction::Path { json }
            | ConfigAction::Init { json, .. }
            | ConfigAction::Validate { json } => *json,
        },
        Commands::Pdf { action } => stage4_cmds::pdf_action_wants_json(action),
        Commands::Repair(args) => args.json,
        Commands::Metadata { action } => stage4_cmds::metadata_action_wants_json(action),
        Commands::Encrypt(args) => args.json,
        Commands::Decrypt(args) => args.json,
        Commands::Dedup { action } => stage5_cmds::dedup_action_wants_json(action),
        Commands::Diff(args) => args.json,
        Commands::Storage(args) => args.json,
    }
}

fn default_compressed_output_path(input: &Path, target_format: Option<FormatId>) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    let ext = target_format
        .and_then(|f| f.canonical_extensions().first().copied())
        .or_else(|| input.extension().and_then(|e| e.to_str()))
        .unwrap_or("bin");
    let file_name = format!("{stem}.opt.{ext}");
    match input.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join(file_name),
        _ => PathBuf::from(file_name),
    }
}

fn format_compress_human(report: &CompressFileReport) -> String {
    let status_label = match report.status {
        CompressionStatus::Optimized => "OPTIMIZED",
        CompressionStatus::NoOpRetainedOriginal => "NO-OP (retained original)",
    };
    let dest = report.output_path.as_deref().unwrap_or(&report.input_path);
    format!(
        "Compression: {}\nInput:  {} ({} bytes, {:?})\nOutput: {} ({} bytes, {:?})\nSaved:  {} bytes ({:.1}%)\nCodec:  {}",
        status_label,
        report.input_path,
        report.bytes_in,
        report.detected_format,
        dest,
        report.bytes_out,
        report.target_format,
        report.savings_bytes,
        report.savings_pct,
        report.codec_used,
    )
}

fn format_archive_create_human(report: &ArchiveCreateReport) -> String {
    format!(
        "Archive Created: {}\nContainer: {:?}\nEntries: {}\nInput Size: {} bytes\nOutput Size: {} bytes",
        report.output_path,
        report.kind,
        report.entry_count,
        report.total_input_bytes,
        report.total_output_bytes,
    )
}

fn format_bench_human(report: &BenchSuiteReport) -> String {
    let mut out = format!(
        "FileFusion Bench ({}) — {} / {} ({} logical CPUs)\n",
        report.workspace_version, report.host_os, report.host_arch, report.logical_cpus
    );
    for item in &report.results {
        let ratio_str = item
            .compression_ratio
            .map_or_else(String::new, |r| format!(", ratio={r:.3}"));
        let _ = writeln!(
            out,
            "  - {}: {} iters, {:.2} ms, {:.2} MiB/s{}",
            item.workload, item.iterations, item.elapsed_ms, item.throughput_mb_s, ratio_str
        );
    }
    out
}

fn handle_config_command(action: &ConfigAction, overrides: &CliConfigOverrides) -> CliExitCode {
    match action {
        ConfigAction::Path { json } => {
            let target = overrides
                .config_path
                .clone()
                .or_else(resolve_default_config_path);
            let path_str = target.map(|p| p.to_string_lossy().into_owned());
            if *json {
                emit_json_stdout(&ConfigStatusOutput {
                    schema_version: CORE_SCHEMA_VERSION,
                    ok: path_str.is_some(),
                    path: path_str,
                    action: "path",
                })
            } else {
                let text = path_str.unwrap_or_else(|| String::from("<unresolved>"));
                emit_text_stdout(&text, false)
            }
        }
        ConfigAction::Show { json } => match load_runtime_config(overrides) {
            Ok(loaded) => {
                if *json {
                    emit_json_stdout(&loaded)
                } else {
                    match loaded.config.to_toml_string_pretty() {
                        Ok(toml_str) => emit_text_stdout(&toml_str, false),
                        Err(err) => report_cli_error(&err, false),
                    }
                }
            }
            Err(err) => report_cli_error(&err, *json),
        },
        ConfigAction::Init { force, json } => {
            let Some(target_path) = overrides
                .config_path
                .clone()
                .or_else(resolve_default_config_path)
            else {
                let err = FileFusionError::new(
                    ErrorCode::IoNotFound,
                    "unable to determine platform configuration path",
                )
                .with_exit_code(CliExitCode::InvalidUsage)
                .with_remediation("Pass an explicit path using `--config <PATH>`.");
                return report_cli_error(&err, *json);
            };
            let mut cfg = file_core::FileFusionConfig::default();
            cfg.apply_cli_overrides(overrides);
            if let Err(err) = save_config_atomic(&target_path, &cfg, *force) {
                return report_cli_error(&err, *json);
            }
            let rendered_path = target_path.to_string_lossy().into_owned();
            if *json {
                emit_json_stdout(&ConfigStatusOutput {
                    schema_version: CORE_SCHEMA_VERSION,
                    ok: true,
                    path: Some(rendered_path),
                    action: "init",
                })
            } else {
                emit_text_stdout(
                    &format!("Initialized configuration at {rendered_path}"),
                    false,
                )
            }
        }
        ConfigAction::Validate { json } => match load_runtime_config(overrides) {
            Ok(loaded) => {
                let source_path = match loaded.source {
                    file_core::ConfigSource::Default => None,
                    file_core::ConfigSource::UserConfigFile(path)
                    | file_core::ConfigSource::ExplicitConfigFile(path) => Some(path),
                };
                if *json {
                    emit_json_stdout(&ConfigStatusOutput {
                        schema_version: CORE_SCHEMA_VERSION,
                        ok: true,
                        path: source_path,
                        action: "validate",
                    })
                } else {
                    emit_text_stdout("Configuration is valid.", false)
                }
            }
            Err(err) => report_cli_error(&err, *json),
        },
    }
}

fn run() -> CliExitCode {
    let cli = Cli::parse();
    let wants_json = command_requests_json(&cli.command);

    let overrides = CliConfigOverrides {
        config_path: cli.config.clone(),
        workers: cli.workers,
        max_memory_mb: cli.max_memory_mb,
        log_level: cli.log_level.map(LogLevel::from),
        log_json: cli.log_json.then_some(true),
        quiet: cli.quiet,
        no_color: cli.no_color,
        offline_only: None,
        redact_paths_in_logs: None,
    };

    if let Commands::Config { action } = &cli.command {
        // Initialize logging best-effort for config subcommands before handling validation errors.
        let fallback_logging = file_core::LoggingConfig {
            level: overrides.log_level.unwrap_or(if overrides.quiet {
                LogLevel::Error
            } else {
                LogLevel::Warn
            }),
            format: if cli.log_json {
                LogFormat::Json
            } else {
                LogFormat::Pretty
            },
            respect_env: true,
            include_target: false,
            ansi_color: !cli.no_color,
        };
        let _ = init_logging(&fallback_logging);
        return handle_config_command(action, &overrides);
    }

    let loaded = match load_runtime_config(&overrides) {
        Ok(loaded) => loaded,
        Err(err) => return report_cli_error(&err, wants_json || cli.log_json),
    };
    if let Err(err) = init_logging(&loaded.config.logging) {
        return report_cli_error(&err, wants_json || cli.log_json);
    }

    match cli.command {
        Commands::Doctor { json } => {
            let report = run_doctor_probe_with_config(&loaded.config);
            if json {
                emit_json_stdout(&report)
            } else {
                emit_text_stdout(&format_doctor_human(&report), false)
            }
        }
        cmd @ Commands::Inspect { .. } => handle_inspect_command(&cmd, cli.log_json),
        cmd @ Commands::Compress { .. } => {
            handle_compress_command(&cmd, &loaded.config, cli.log_json)
        }
        cmd @ Commands::Convert { .. } => {
            handle_convert_command(&cmd, &loaded.config, cli.log_json)
        }
        Commands::Archive { action } => {
            handle_archive_command(&action, &loaded.config, cli.log_json)
        }
        Commands::Pdf { action } => {
            stage4_cmds::handle_pdf_command(&action, &loaded.config, cli.log_json)
        }
        Commands::Repair(args) => {
            stage4_cmds::handle_repair_command(&args, &loaded.config, cli.log_json)
        }
        Commands::Metadata { action } => {
            stage4_cmds::handle_metadata_command(&action, &loaded.config, cli.log_json)
        }
        Commands::Encrypt(args) => {
            stage4_cmds::handle_encrypt_command(&args, &loaded.config, cli.log_json)
        }
        Commands::Decrypt(args) => {
            stage4_cmds::handle_decrypt_command(&args, &loaded.config, cli.log_json)
        }
        Commands::Dedup { action } => {
            stage5_cmds::handle_dedup_command(&action, &loaded.config, cli.log_json)
        }
        Commands::Diff(args) => {
            stage5_cmds::handle_diff_command(&args, &loaded.config, cli.log_json)
        }
        Commands::Storage(args) => {
            stage5_cmds::handle_storage_command(&args, &loaded.config, cli.log_json)
        }
        Commands::Bench { iterations, json } => {
            let mgr = JobManager::from_config(&loaded.config);
            let ctx = mgr.create_context("cli-bench");
            match mgr.run_quick_benchmark(iterations, &ctx) {
                Ok(report) => {
                    if json {
                        emit_json_stdout(&report)
                    } else {
                        emit_text_stdout(&format_bench_human(&report), false)
                    }
                }
                Err(err) => report_cli_error(&err, json || cli.log_json),
            }
        }
        Commands::Config { .. } => CliExitCode::Success,
    }
}

fn handle_inspect_command(command: &Commands, log_json: bool) -> CliExitCode {
    let Commands::Inspect {
        path,
        strict,
        signature_only,
        json,
    } = command
    else {
        return CliExitCode::InvalidUsage;
    };
    if *signature_only {
        match probe_file_signature(path, *strict) {
            Ok(probe) => {
                if *json {
                    emit_json_stdout(&probe)
                } else {
                    emit_text_stdout(&format_signature_human(&probe), false)
                }
            }
            Err(err) => report_cli_error(&err, *json || log_json),
        }
    } else {
        match inspect_file(path, *strict) {
            Ok(report) => {
                if *json {
                    emit_json_stdout(&report)
                } else {
                    emit_text_stdout(&format_inspection_human(&report), false)
                }
            }
            Err(err) => report_cli_error(&err, *json || log_json),
        }
    }
}

fn handle_compress_command(
    command: &Commands,
    config: &file_core::FileFusionConfig,
    log_json: bool,
) -> CliExitCode {
    let Commands::Compress {
        input,
        output,
        profile,
        format,
        target_max_bytes,
        min_ssim,
        solve_quality,
        strip_metadata,
        no_noop_guard,
        overwrite,
        json,
    } = command
    else {
        return CliExitCode::InvalidUsage;
    };

    let mgr = JobManager::from_config(config);
    let ctx = mgr.create_context("cli-compress");
    let target_fmt = (*format).map(FormatId::from);
    let dest = output
        .clone()
        .unwrap_or_else(|| default_compressed_output_path(input, target_fmt));
    let req = CompressionRequest {
        profile: CompressionProfile::from(*profile),
        target_format: target_fmt,
        target_max_bytes: *target_max_bytes,
        min_ssim: *min_ssim,
        strip_metadata: *strip_metadata,
        no_op_if_larger: !*no_noop_guard,
    };
    let policy = collision_policy(*overwrite);
    match mgr.compress_file(input, &dest, &req, *solve_quality, policy, &ctx) {
        Ok(report) => {
            if *json {
                emit_json_stdout(&report)
            } else {
                emit_text_stdout(&format_compress_human(&report), false)
            }
        }
        Err(err) => report_cli_error(&err, *json || log_json),
    }
}

fn default_converted_output_path(input: &Path, target_format: FormatId) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("converted");
    let ext = target_format
        .canonical_extensions()
        .first()
        .copied()
        .unwrap_or("out");
    let file_name = format!("{stem}.{ext}");
    match input.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join(file_name),
        _ => PathBuf::from(file_name),
    }
}

fn format_preflight_human(assessment: &PreflightAssessment, target: FormatId) -> String {
    let mut out = format!(
        "Pre-flight Assessment (target={:?}): supported={}, estimated_memory={} MB\n",
        target, assessment.supported, assessment.estimated_memory_mb
    );
    for w in &assessment.warnings {
        let _ = writeln!(out, "  - [{:?} / {}] {}", w.severity, w.code, w.message);
    }
    out
}

fn format_convert_human(report: &ConvertFileReport) -> String {
    let mut out = format!(
        "Converted: {} -> {}\nFormats:   {:?} -> {:?}\nSize:      {} bytes -> {} bytes\nWarnings:  {}",
        report.input_path,
        report.output_path,
        report.detected_format,
        report.target_format,
        report.bytes_in,
        report.bytes_out,
        report.fidelity.warnings.len(),
    );
    if let (Some(h), Some(t), Some(l), Some(i)) = (
        report.fidelity.headings_preserved,
        report.fidelity.tables_preserved,
        report.fidelity.links_preserved,
        report.fidelity.images_preserved,
    ) {
        let _ = write!(
            out,
            "\nFidelity:  headings={h}, tables={t}, links={l}, images={i}"
        );
    }
    out
}

fn handle_convert_command(
    command: &Commands,
    config: &file_core::FileFusionConfig,
    log_json: bool,
) -> CliExitCode {
    let Commands::Convert {
        input,
        target_format,
        output,
        preflight_only,
        force_loss,
        no_preserve_structure,
        strip_metadata,
        overwrite,
        json,
    } = command
    else {
        return CliExitCode::InvalidUsage;
    };

    let mgr = JobManager::from_config(config);
    let target = FormatId::from(*target_format);
    if *preflight_only {
        return match mgr.preflight_conversion(input, target) {
            Ok(assessment) => {
                if *json {
                    emit_json_stdout(&assessment)
                } else {
                    emit_text_stdout(&format_preflight_human(&assessment, target), false)
                }
            }
            Err(err) => report_cli_error(&err, *json || log_json),
        };
    }

    let ctx = mgr.create_context("cli-convert");
    let dest = output
        .clone()
        .unwrap_or_else(|| default_converted_output_path(input, target));
    let req = ConversionRequest {
        target_format: target,
        strip_metadata: *strip_metadata,
        preserve_structure: !*no_preserve_structure,
        fail_on_breaking_loss: !*force_loss,
    };
    let policy = collision_policy(*overwrite);
    match mgr.convert_file(input, &dest, &req, policy, &ctx) {
        Ok(report) => {
            if *json {
                emit_json_stdout(&report)
            } else {
                emit_text_stdout(&format_convert_human(&report), false)
            }
        }
        Err(err) => report_cli_error(&err, *json || log_json),
    }
}

fn handle_archive_command(
    action: &ArchiveAction,
    config: &file_core::FileFusionConfig,
    log_json: bool,
) -> CliExitCode {
    let mgr = JobManager::from_config(config);
    let ctx = mgr.create_context("cli-archive");
    match action {
        ArchiveAction::Create {
            output,
            inputs,
            profile,
            overwrite,
            json,
        } => {
            let policy = collision_policy(*overwrite);
            match mgr.create_archive(
                inputs,
                output,
                None,
                CompressionProfile::from(*profile),
                policy,
                &ctx,
            ) {
                Ok(report) => {
                    if *json {
                        emit_json_stdout(&report)
                    } else {
                        emit_text_stdout(&format_archive_create_human(&report), false)
                    }
                }
                Err(err) => report_cli_error(&err, *json || log_json),
            }
        }
        ArchiveAction::List { archive, json } => match mgr.list_archive(archive, &ctx) {
            Ok(entries) => {
                if *json {
                    emit_json_stdout(&entries)
                } else {
                    let mut text = format!("Archive Entries ({}):\n", entries.len());
                    for member in &entries {
                        let _ = writeln!(
                            text,
                            "  {} ({} bytes)",
                            member.path, member.uncompressed_bytes
                        );
                    }
                    emit_text_stdout(&text, false)
                }
            }
            Err(err) => report_cli_error(&err, *json || log_json),
        },
        ArchiveAction::Extract {
            archive,
            output,
            json,
        } => match mgr.extract_archive(archive, output, &ctx) {
            Ok(report) => {
                if *json {
                    emit_json_stdout(&report)
                } else {
                    emit_text_stdout(
                        &format!(
                            "Extracted {} file(s) ({} bytes) to {}",
                            report.extracted_files.len(),
                            report.total_bytes_written,
                            output.display()
                        ),
                        false,
                    )
                }
            }
            Err(err) => report_cli_error(&err, *json || log_json),
        },
    }
}

fn main() -> ExitCode {
    let code = std::thread::Builder::new()
        .name(String::from("filefusion-main"))
        .stack_size(8 * 1024 * 1024)
        .spawn(run)
        .map_or_else(
            |_| run(),
            |handle| handle.join().unwrap_or(CliExitCode::OperationalFailure),
        );
    ExitCode::from(code)
}

pub(crate) const fn collision_policy(overwrite: bool) -> CollisionPolicy {
    if overwrite {
        CollisionPolicy::OverwriteWithBackup
    } else {
        CollisionPolicy::RenameWithSuffix
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_doctor_output_includes_workspace_and_tier1_capability() {
        let report = job_manager::run_doctor_probe();
        let rendered = format_doctor_human(&report);
        assert!(rendered.contains("file-core"));
        assert!(rendered.contains("file-core-workspace"));
        assert!(rendered.contains("Effective ResourceLimits"));
        assert!(rendered.contains("READY"));
    }
}
