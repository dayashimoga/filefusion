//! Stage 5 CLI subcommands (`dedup`, `diff`, `storage`) delegated cleanly
//! through L5 `JobManager` (`MOD-RULE-03`).

use std::fmt::Write as _;
use std::path::PathBuf;

use clap::{Args, Subcommand, ValueEnum};
use file_core::{CliExitCode, CollisionPolicy, FileFusionConfig};
use job_manager::{
    CdcDeduplicationReport, DuplicateScanOptions, ExactDuplicateScanReport, FastCdcConfig,
    FileDiffOptions, FileDiffReport, JobCleanupExecutionResult, JobManager, NearDuplicateOptions,
    NearDuplicateScanReport, QuarantinePurgeReport, QuarantineRestoreReport, ReclamationStrategy,
    StorageTreemapReport,
};

use crate::{emit_json_stdout, emit_text_stdout, report_cli_error};

/// Safe duplicate reclamation strategy selector for `filefusion dedup cleanup`.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum CliCleanupStrategy {
    Quarantine,
    Hardlink,
    Delete,
}

impl From<CliCleanupStrategy> for ReclamationStrategy {
    fn from(value: CliCleanupStrategy) -> Self {
        match value {
            CliCleanupStrategy::Quarantine => Self::Quarantine,
            CliCleanupStrategy::Hardlink => Self::Hardlink,
            CliCleanupStrategy::Delete => Self::PermanentDelete,
        }
    }
}

/// Collision resolution policy for `filefusion dedup restore`.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum CliRestoreCollision {
    Skip,
    Rename,
    Overwrite,
}

impl From<CliRestoreCollision> for CollisionPolicy {
    fn from(value: CliRestoreCollision) -> Self {
        match value {
            CliRestoreCollision::Skip => Self::Skip,
            CliRestoreCollision::Rename => Self::RenameWithSuffix,
            CliRestoreCollision::Overwrite => Self::OverwriteWithBackup,
        }
    }
}

/// Subcommands under `filefusion dedup`.
#[derive(Debug, Subcommand)]
pub(crate) enum DedupAction {
    /// Scan directory trees with 3-stage `size -> 4 KB head/tail -> full BLAKE3` deduplication (`FR-DUP-001`).
    Exact {
        /// Root directories or files to scan.
        #[arg(value_name = "ROOTS", required = true)]
        roots: Vec<PathBuf>,
        /// Minimum file byte size included in scanning.
        #[arg(long, default_value_t = 1)]
        min_bytes: u64,
        /// Traverse dot-prefixed hidden files and directories.
        #[arg(long)]
        include_hidden: bool,
        /// Follow symlinks with canonical loop detection.
        #[arg(long)]
        follow_symlinks: bool,
        /// Optional maximum directory recursion depth.
        #[arg(long)]
        max_depth: Option<usize>,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Detect visually similar resized/re-encoded images via `aHash`/`dHash`/`pHash` (`FR-DUP-002`).
    Near {
        /// Root directories or files to scan.
        #[arg(value_name = "ROOTS", required = true)]
        roots: Vec<PathBuf>,
        /// Maximum perceptual Hamming bit distance (`0..=64`).
        #[arg(long, default_value_t = 8)]
        max_distance: u32,
        /// Traverse dot-prefixed hidden files and directories.
        #[arg(long)]
        include_hidden: bool,
        /// Follow symlinks with canonical loop detection.
        #[arg(long)]
        follow_symlinks: bool,
        /// Optional maximum directory recursion depth.
        #[arg(long)]
        max_depth: Option<usize>,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Analyze sub-file `FastCDC` chunk deduplication savings across explicit files (`FR-DUP-002`).
    Cdc {
        /// Input file paths to stream and chunk.
        #[arg(value_name = "FILES", required = true)]
        files: Vec<PathBuf>,
        /// Minimum chunk size in bytes.
        #[arg(long, default_value_t = 4096)]
        min_chunk: usize,
        /// Average target chunk size in bytes.
        #[arg(long, default_value_t = 16384)]
        avg_chunk: usize,
        /// Maximum chunk size in bytes.
        #[arg(long, default_value_t = 65536)]
        max_chunk: usize,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Plan (dry-run by default) or apply safe duplicate space reclamation (`FR-DUP-003`).
    Cleanup {
        /// Root directories or files to analyze and reclaim.
        #[arg(value_name = "ROOTS", required = true)]
        roots: Vec<PathBuf>,
        /// Reclamation strategy (`quarantine`, `hardlink`, or `delete`).
        #[arg(long, value_enum, default_value = "quarantine")]
        strategy: CliCleanupStrategy,
        /// Custom quarantine directory override.
        #[arg(long, value_name = "DIR")]
        quarantine_dir: Option<PathBuf>,
        /// Execute filesystem changes (omit for read-only dry-run preview).
        #[arg(long)]
        apply: bool,
        /// Explicit confirmation required with `--strategy delete --apply`.
        #[arg(long)]
        yes_permanent_delete: bool,
        /// Minimum file byte size included in scanning.
        #[arg(long, default_value_t = 1)]
        min_bytes: u64,
        /// Traverse dot-prefixed hidden files and directories.
        #[arg(long)]
        include_hidden: bool,
        /// Follow symlinks with canonical loop detection.
        #[arg(long)]
        follow_symlinks: bool,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Restore files from a quarantine `manifest.json` (`FR-DUP-003`).
    Restore {
        /// Path to quarantine batch `manifest.json`.
        #[arg(value_name = "MANIFEST")]
        manifest: PathBuf,
        /// Collision policy if an original path now exists (`skip`, `rename`, or `overwrite`).
        #[arg(long, value_enum, default_value = "rename")]
        collision: CliRestoreCollision,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Permanently delete remaining quarantined payloads recorded in a `manifest.json`.
    Purge {
        /// Path to quarantine batch `manifest.json`.
        #[arg(value_name = "MANIFEST")]
        manifest: PathBuf,
        /// Explicit confirmation required to permanently delete quarantined copies.
        #[arg(long)]
        confirm: bool,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
}

/// Arguments for `filefusion diff`.
#[derive(Debug, Args)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct DiffArgs {
    /// First input file.
    #[arg(value_name = "FILE_A")]
    pub(crate) path_a: PathBuf,
    /// Second input file.
    #[arg(value_name = "FILE_B")]
    pub(crate) path_b: PathBuf,
    /// Skip `FastCDC` sub-file chunk overlap analysis.
    #[arg(long)]
    pub(crate) no_cdc: bool,
    /// Skip visual `SSIM`/`PSNR`/perceptual image diffing.
    #[arg(long)]
    pub(crate) no_visual: bool,
    /// Skip bounded UTF-8 text line diffing.
    #[arg(long)]
    pub(crate) no_text: bool,
    /// Emit machine-readable JSON output.
    #[arg(long)]
    pub(crate) json: bool,
}

/// Arguments for `filefusion storage`.
#[derive(Debug, Args)]
pub(crate) struct StorageArgs {
    /// Root directories or files to profile.
    #[arg(value_name = "ROOTS", required = true)]
    pub(crate) roots: Vec<PathBuf>,
    /// Maximum number of largest files returned.
    #[arg(long, default_value_t = 10)]
    pub(crate) top: usize,
    /// Minimum byte size for top-largest-file listing.
    #[arg(long, default_value_t = 0)]
    pub(crate) min_bytes: u64,
    /// Traverse dot-prefixed hidden files and directories.
    #[arg(long)]
    pub(crate) include_hidden: bool,
    /// Follow symlinks with canonical loop detection.
    #[arg(long)]
    pub(crate) follow_symlinks: bool,
    /// Optional maximum directory recursion depth.
    #[arg(long)]
    pub(crate) max_depth: Option<usize>,
    /// Emit machine-readable JSON output.
    #[arg(long)]
    pub(crate) json: bool,
}

pub(crate) const fn dedup_action_wants_json(action: &DedupAction) -> bool {
    match action {
        DedupAction::Exact { json, .. }
        | DedupAction::Near { json, .. }
        | DedupAction::Cdc { json, .. }
        | DedupAction::Cleanup { json, .. }
        | DedupAction::Restore { json, .. }
        | DedupAction::Purge { json, .. } => *json,
    }
}

pub(crate) fn handle_dedup_command(
    action: &DedupAction,
    config: &FileFusionConfig,
    log_json: bool,
) -> CliExitCode {
    let mgr = JobManager::from_config(config);
    let ctx = mgr.create_context("cli-dedup");
    match action {
        DedupAction::Exact {
            roots,
            min_bytes,
            include_hidden,
            follow_symlinks,
            max_depth,
            json,
        } => {
            let opts = make_scan_opts(*min_bytes, *include_hidden, *follow_symlinks, *max_depth);
            match mgr.scan_exact_duplicates(roots, &opts, &ctx) {
                Ok(rep) if *json => emit_json_stdout(&rep),
                Ok(rep) => emit_text_stdout(&format_exact_human(&rep), false),
                Err(err) => report_cli_error(&err, *json || log_json),
            }
        }
        DedupAction::Near {
            roots,
            max_distance,
            include_hidden,
            follow_symlinks,
            max_depth,
            json,
        } => {
            let opts = NearDuplicateOptions {
                scan: make_scan_opts(1, *include_hidden, *follow_symlinks, *max_depth),
                max_hamming_distance: *max_distance,
            };
            match mgr.scan_near_duplicates(roots, &opts, &ctx) {
                Ok(rep) if *json => emit_json_stdout(&rep),
                Ok(rep) => emit_text_stdout(&format_near_human(&rep), false),
                Err(err) => report_cli_error(&err, *json || log_json),
            }
        }
        DedupAction::Cdc {
            files,
            min_chunk,
            avg_chunk,
            max_chunk,
            json,
        } => {
            let cfg = FastCdcConfig {
                min_size: *min_chunk,
                avg_size: *avg_chunk,
                max_size: *max_chunk,
            };
            match mgr.analyze_cdc_dedup(files, cfg, &ctx) {
                Ok(rep) if *json => emit_json_stdout(&rep),
                Ok(rep) => emit_text_stdout(&format_cdc_human(&rep), false),
                Err(err) => report_cli_error(&err, *json || log_json),
            }
        }
        DedupAction::Cleanup {
            roots,
            strategy,
            quarantine_dir,
            apply,
            yes_permanent_delete,
            min_bytes,
            include_hidden,
            follow_symlinks,
            json,
        } => {
            let opts = make_scan_opts(*min_bytes, *include_hidden, *follow_symlinks, None);
            match mgr.cleanup_duplicates(
                roots,
                &opts,
                (*strategy).into(),
                quarantine_dir.as_deref(),
                !*apply,
                *yes_permanent_delete,
                &ctx,
            ) {
                Ok(rep) if *json => emit_json_stdout(&rep),
                Ok(rep) => emit_text_stdout(&format_cleanup_human(&rep), false),
                Err(err) => report_cli_error(&err, *json || log_json),
            }
        }
        DedupAction::Restore {
            manifest,
            collision,
            json,
        } => match mgr.restore_quarantine(manifest, (*collision).into(), &ctx) {
            Ok(rep) if *json => emit_json_stdout(&rep),
            Ok(rep) => emit_text_stdout(&format_restore_human(&rep), false),
            Err(err) => report_cli_error(&err, *json || log_json),
        },
        DedupAction::Purge {
            manifest,
            confirm,
            json,
        } => match mgr.purge_quarantine(manifest, *confirm, &ctx) {
            Ok(rep) if *json => emit_json_stdout(&rep),
            Ok(rep) => emit_text_stdout(&format_purge_human(&rep), false),
            Err(err) => report_cli_error(&err, *json || log_json),
        },
    }
}

pub(crate) fn handle_diff_command(
    args: &DiffArgs,
    config: &FileFusionConfig,
    log_json: bool,
) -> CliExitCode {
    let mgr = JobManager::from_config(config);
    let ctx = mgr.create_context("cli-diff");
    let opts = FileDiffOptions {
        include_cdc_delta: !args.no_cdc,
        include_visual_diff: !args.no_visual,
        include_text_diff: !args.no_text,
        ..FileDiffOptions::default()
    };
    match mgr.diff_files(&args.path_a, &args.path_b, &opts, &ctx) {
        Ok(rep) if args.json => emit_json_stdout(&rep),
        Ok(rep) => emit_text_stdout(&format_diff_human(&rep), false),
        Err(err) => report_cli_error(&err, args.json || log_json),
    }
}

pub(crate) fn handle_storage_command(
    args: &StorageArgs,
    config: &FileFusionConfig,
    log_json: bool,
) -> CliExitCode {
    let mgr = JobManager::from_config(config);
    let ctx = mgr.create_context("cli-storage");
    let opts = make_scan_opts(
        args.min_bytes,
        args.include_hidden,
        args.follow_symlinks,
        args.max_depth,
    );
    match mgr.analyze_storage_treemap(&args.roots, &opts, args.top, &ctx) {
        Ok(rep) if args.json => emit_json_stdout(&rep),
        Ok(rep) => emit_text_stdout(&format_storage_human(&rep), false),
        Err(err) => report_cli_error(&err, args.json || log_json),
    }
}

fn make_scan_opts(
    min_file_bytes: u64,
    include_hidden: bool,
    follow_symlinks: bool,
    max_depth: Option<usize>,
) -> DuplicateScanOptions {
    DuplicateScanOptions {
        min_file_bytes,
        include_hidden,
        follow_symlinks,
        max_depth,
        ..DuplicateScanOptions::default()
    }
}

fn format_exact_human(rep: &ExactDuplicateScanReport) -> String {
    let mut out = format!(
        "Exact Duplicates: files_scanned={}, groups={}, duplicate_files={}, wasted={} bytes (read {} / {} bytes)",
        rep.files_scanned,
        rep.duplicate_groups.len(),
        rep.total_duplicate_files,
        rep.total_wasted_bytes,
        rep.bytes_actually_read,
        rep.total_bytes_scanned
    );
    for g in rep.duplicate_groups.iter().take(10) {
        let _ = write!(
            out,
            "\n  group [{}]: {} files x {} bytes (wasted {} bytes)",
            &g.blake3_hex[..12.min(g.blake3_hex.len())],
            g.member_paths.len(),
            g.file_size_bytes,
            g.wasted_bytes
        );
    }
    out
}

fn format_near_human(rep: &NearDuplicateScanReport) -> String {
    format!(
        "Near-Duplicate Images: analyzed={}, clusters={}, potential_savings={} bytes",
        rep.images_analyzed,
        rep.clusters.len(),
        rep.total_potential_savings_bytes
    )
}

fn format_cdc_human(rep: &CdcDeduplicationReport) -> String {
    format!(
        "FastCDC Analysis: chunks={}/{}, total={} bytes, unique={} bytes, duplicate={} bytes",
        rep.unique_chunk_count,
        rep.chunk_count,
        rep.total_bytes,
        rep.unique_bytes,
        rep.duplicate_bytes
    )
}

fn format_cleanup_human(rep: &JobCleanupExecutionResult) -> String {
    match &rep.execution {
        Some(exec) => format!(
            "Cleanup Applied ({:?}): applied={}, skipped={}, reclaimed={} bytes, manifest={}",
            exec.strategy,
            exec.applied_count,
            exec.skipped_count,
            exec.reclaimed_bytes,
            exec.manifest_path.as_deref().unwrap_or("none")
        ),
        None => format!(
            "Cleanup Dry-Run Preview ({:?}): candidates={}, estimated_reclaimable={} bytes (pass `--apply` to execute)",
            rep.plan.strategy,
            rep.plan.actions.len(),
            rep.plan.estimated_reclaimable_bytes
        ),
    }
}

fn format_restore_human(rep: &QuarantineRestoreReport) -> String {
    format!(
        "Quarantine Restore ({}): restored={}, skipped={}, bytes={}",
        rep.batch_id, rep.restored_count, rep.skipped_count, rep.restored_bytes
    )
}

fn format_purge_human(rep: &QuarantinePurgeReport) -> String {
    format!(
        "Quarantine Purge ({}): purged={}, bytes={}",
        rep.batch_id, rep.purged_count, rep.purged_bytes
    )
}

fn format_diff_human(rep: &FileDiffReport) -> String {
    format!(
        "Diff `{}` vs `{}`: bit_exact={}, size_a={}, size_b={}, summary={}",
        rep.path_a, rep.path_b, rep.bit_exact, rep.size_bytes_a, rep.size_bytes_b, rep.summary
    )
}

fn format_storage_human(rep: &StorageTreemapReport) -> String {
    let mut out = format!(
        "Storage Analytics: files={}, total_bytes={}, zero_byte_files={}",
        rep.total_files, rep.total_bytes, rep.zero_byte_files
    );
    for item in rep.largest_files.iter().take(5) {
        let _ = write!(
            out,
            "\n  {} ({} bytes, {:?})",
            item.path, item.size_bytes, item.format
        );
    }
    out
}
