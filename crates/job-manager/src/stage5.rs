//! Stage 5 L5 control-plane orchestration on [`JobManager`]:
//! - 3-stage exact duplicate scanning (`scan_exact_duplicates`)
//! - Perceptual near-duplicate image clustering (`scan_near_duplicates`)
//! - Streaming `FastCDC` sub-file analysis (`analyze_cdc_dedup`)
//! - Hierarchical storage treemap analytics (`analyze_storage_treemap`)
//! - Unified file comparison/diff (`diff_files`)
//! - Safe cleanup planning, atomic hardlinks, and reversible quarantine (`plan_duplicate_cleanup`,
//!   `execute_duplicate_cleanup`, `restore_quarantine`, `purge_quarantine`)

use std::path::{Path, PathBuf};

use file_core::{CORE_SCHEMA_VERSION, CollisionPolicy, FileFusionResult, JobContext};
use optimization_engine::{
    CdcDeduplicationReport, DuplicateScanOptions, ExactDuplicateScanReport, FastCdcConfig,
    FileDiffOptions, FileDiffReport, NearDuplicateOptions, NearDuplicateScanReport,
    QuarantinePurgeReport, QuarantineRestoreReport, ReclamationExecutionReport, ReclamationPlan,
    ReclamationStrategy, StorageTreemapReport, analyze_files_fastcdc as opt_analyze_files_fastcdc,
    analyze_storage_treemap as opt_analyze_storage_treemap,
    build_reclamation_plan as opt_build_reclamation_plan, compare_files as opt_compare_files,
    execute_reclamation_plan as opt_execute_reclamation_plan,
    purge_quarantine_batch as opt_purge_quarantine_batch,
    restore_quarantine_batch as opt_restore_quarantine_batch,
    scan_exact_duplicates as opt_scan_exact_duplicates,
    scan_near_duplicate_images as opt_scan_near_duplicate_images,
};
use serde::{Deserialize, Serialize};

use crate::{ActiveJobGuard, JobManager};

/// Combined scan + dry-run space-reclamation plan returned by [`JobManager::plan_duplicate_cleanup`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobCleanupPlanResult {
    /// Machine-readable JSON schema version.
    pub schema_version: u32,
    /// Underlying exact duplicate scan report.
    pub scan: ExactDuplicateScanReport,
    /// Actionable dry-run cleanup plan.
    pub plan: ReclamationPlan,
}

/// Combined scan, plan, and execution outcome returned by [`JobManager::execute_duplicate_cleanup`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobCleanupExecutionResult {
    /// Machine-readable JSON schema version.
    pub schema_version: u32,
    /// Underlying exact duplicate scan report.
    pub scan: ExactDuplicateScanReport,
    /// Actionable plan executed (or previewed when `dry_run == true`).
    pub plan: ReclamationPlan,
    /// Execution summary (`None` when `dry_run == true`).
    pub execution: Option<ReclamationExecutionReport>,
}

impl JobManager {
    /// Runs the 3-stage exact duplicate scanner (`size -> 4 KB head/tail -> full BLAKE3`)
    /// across `roots` under cooperative cancellation governance (`FR-DUP-001`).
    ///
    /// # Errors
    /// Returns [`file_core::FileFusionError`] on invalid roots or cancellation.
    pub fn scan_exact_duplicates(
        &self,
        roots: &[PathBuf],
        opts: &DuplicateScanOptions,
        ctx: &JobContext,
    ) -> FileFusionResult<ExactDuplicateScanReport> {
        ctx.check_cancelled()?;
        let _active = ActiveJobGuard::register(
            &self.active_jobs,
            &ctx.job_id,
            ctx.cancellation.clone(),
            None,
        );
        let root_refs: Vec<&Path> = roots.iter().map(PathBuf::as_path).collect();
        opt_scan_exact_duplicates(&root_refs, opts, ctx)
    }

    /// Scans `roots` for visually similar (resized/re-encoded) raster images via `aHash`/`dHash`/`pHash`
    /// (`FR-DUP-002`, `FR-DET-004`).
    ///
    /// # Errors
    /// Returns [`file_core::FileFusionError`] on invalid roots or cancellation.
    pub fn scan_near_duplicates(
        &self,
        roots: &[PathBuf],
        opts: &NearDuplicateOptions,
        ctx: &JobContext,
    ) -> FileFusionResult<NearDuplicateScanReport> {
        ctx.check_cancelled()?;
        let _active = ActiveJobGuard::register(
            &self.active_jobs,
            &ctx.job_id,
            ctx.cancellation.clone(),
            None,
        );
        let root_refs: Vec<&Path> = roots.iter().map(PathBuf::as_path).collect();
        opt_scan_near_duplicate_images(&root_refs, opts, ctx)
    }

    /// Computes streaming sub-file `FastCDC` deduplication statistics across explicit file paths
    /// (`FR-DUP-002`, `NFR-PERF-001`).
    ///
    /// # Errors
    /// Returns [`file_core::FileFusionError`] on I/O failures or cancellation.
    pub fn analyze_cdc_dedup(
        &self,
        files: &[PathBuf],
        cfg: FastCdcConfig,
        ctx: &JobContext,
    ) -> FileFusionResult<CdcDeduplicationReport> {
        ctx.check_cancelled()?;
        let _active = ActiveJobGuard::register(
            &self.active_jobs,
            &ctx.job_id,
            ctx.cancellation.clone(),
            None,
        );
        let refs: Vec<&Path> = files.iter().map(PathBuf::as_path).collect();
        opt_analyze_files_fastcdc(&refs, cfg, ctx)
    }

    /// Builds a hierarchical directory treemap and domain/format storage breakdown (`FR-DUP-003`).
    ///
    /// # Errors
    /// Returns [`file_core::FileFusionError`] on invalid roots or cancellation.
    pub fn analyze_storage_treemap(
        &self,
        roots: &[PathBuf],
        opts: &DuplicateScanOptions,
        top_n_largest: usize,
        ctx: &JobContext,
    ) -> FileFusionResult<StorageTreemapReport> {
        ctx.check_cancelled()?;
        let _active = ActiveJobGuard::register(
            &self.active_jobs,
            &ctx.job_id,
            ctx.cancellation.clone(),
            None,
        );
        let root_refs: Vec<&Path> = roots.iter().map(PathBuf::as_path).collect();
        opt_analyze_storage_treemap(&root_refs, opts, top_n_largest, ctx)
    }

    /// Compares two files across byte equality, cryptographic digests, `FastCDC` overlap,
    /// visual `SSIM`/`PSNR`/perceptual metrics, LCS UTF-8 text hunks, and metadata (`FR-VAL-002`).
    ///
    /// # Errors
    /// Returns [`file_core::FileFusionError`] when inputs are missing/non-files or cancelled.
    pub fn diff_files(
        &self,
        path_a: &Path,
        path_b: &Path,
        opts: &FileDiffOptions,
        ctx: &JobContext,
    ) -> FileFusionResult<FileDiffReport> {
        ctx.check_cancelled()?;
        let _active = ActiveJobGuard::register(
            &self.active_jobs,
            &ctx.job_id,
            ctx.cancellation.clone(),
            None,
        );
        opt_compare_files(path_a, path_b, opts, ctx)
    }

    /// Scans `roots` for exact duplicates and generates an actionable dry-run cleanup plan
    /// (`FR-DUP-003`).
    ///
    /// # Errors
    /// Returns [`file_core::FileFusionError`] on invalid roots or cancellation.
    pub fn plan_duplicate_cleanup(
        &self,
        roots: &[PathBuf],
        scan_opts: &DuplicateScanOptions,
        strategy: ReclamationStrategy,
        quarantine_root: Option<&Path>,
        ctx: &JobContext,
    ) -> FileFusionResult<JobCleanupPlanResult> {
        ctx.check_cancelled()?;
        let _active = ActiveJobGuard::register(
            &self.active_jobs,
            &ctx.job_id,
            ctx.cancellation.clone(),
            None,
        );
        let root_refs: Vec<&Path> = roots.iter().map(PathBuf::as_path).collect();
        let scan = opt_scan_exact_duplicates(&root_refs, scan_opts, ctx)?;
        let plan = opt_build_reclamation_plan(&scan, strategy, quarantine_root);
        Ok(JobCleanupPlanResult {
            schema_version: CORE_SCHEMA_VERSION,
            scan,
            plan,
        })
    }

    /// Executes an explicit pre-built [`ReclamationPlan`] under job cancellation tracking (`FR-DUP-003`).
    ///
    /// # Errors
    /// Returns [`file_core::FileFusionError`] on missing consent for permanent deletion, I/O errors,
    /// or cancellation.
    pub fn execute_reclamation_plan(
        &self,
        plan: &ReclamationPlan,
        allow_permanent_delete: bool,
        ctx: &JobContext,
    ) -> FileFusionResult<ReclamationExecutionReport> {
        ctx.check_cancelled()?;
        let _active = ActiveJobGuard::register(
            &self.active_jobs,
            &ctx.job_id,
            ctx.cancellation.clone(),
            None,
        );
        opt_execute_reclamation_plan(plan, allow_permanent_delete, ctx)
    }

    /// Scans, plans, and optionally executes duplicate cleanup (`dry_run = true` skips all disk writes)
    /// (`FR-DUP-003`).
    ///
    /// # Errors
    /// Returns [`file_core::FileFusionError`] on invalid roots, missing consent for permanent delete,
    /// or cancellation.
    #[allow(clippy::too_many_arguments)]
    pub fn cleanup_duplicates(
        &self,
        roots: &[PathBuf],
        scan_opts: &DuplicateScanOptions,
        strategy: ReclamationStrategy,
        quarantine_root: Option<&Path>,
        dry_run: bool,
        allow_permanent_delete: bool,
        ctx: &JobContext,
    ) -> FileFusionResult<JobCleanupExecutionResult> {
        ctx.check_cancelled()?;
        let _active = ActiveJobGuard::register(
            &self.active_jobs,
            &ctx.job_id,
            ctx.cancellation.clone(),
            None,
        );
        let root_refs: Vec<&Path> = roots.iter().map(PathBuf::as_path).collect();
        let scan = opt_scan_exact_duplicates(&root_refs, scan_opts, ctx)?;
        let plan = opt_build_reclamation_plan(&scan, strategy, quarantine_root);
        let execution = if dry_run {
            None
        } else {
            Some(opt_execute_reclamation_plan(
                &plan,
                allow_permanent_delete,
                ctx,
            )?)
        };
        Ok(JobCleanupExecutionResult {
            schema_version: CORE_SCHEMA_VERSION,
            scan,
            plan,
            execution,
        })
    }

    /// Restores files from a quarantine `manifest.json` while honoring `collision_policy` (`FR-DUP-003`).
    ///
    /// # Errors
    /// Returns [`file_core::FileFusionError`] on missing/corrupt manifest or cancellation.
    pub fn restore_quarantine(
        &self,
        manifest_path: &Path,
        collision_policy: CollisionPolicy,
        ctx: &JobContext,
    ) -> FileFusionResult<QuarantineRestoreReport> {
        ctx.check_cancelled()?;
        let _active = ActiveJobGuard::register(
            &self.active_jobs,
            &ctx.job_id,
            ctx.cancellation.clone(),
            None,
        );
        opt_restore_quarantine_batch(manifest_path, collision_policy, ctx)
    }

    /// Permanently purges remaining quarantined items recorded in `manifest.json` (`FR-DUP-003`).
    ///
    /// # Errors
    /// Returns [`file_core::FileFusionError`] if `confirm_purge` is `false`, manifest is corrupt,
    /// or cancelled.
    pub fn purge_quarantine(
        &self,
        manifest_path: &Path,
        confirm_purge: bool,
        ctx: &JobContext,
    ) -> FileFusionResult<QuarantinePurgeReport> {
        ctx.check_cancelled()?;
        let _active = ActiveJobGuard::register(
            &self.active_jobs,
            &ctx.job_id,
            ctx.cancellation.clone(),
            None,
        );
        opt_purge_quarantine_batch(manifest_path, confirm_purge, ctx)
    }
}
