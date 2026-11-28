//! Safe space-reclamation planning, atomic same-volume hardlink consolidation,
//! and reversible quarantine management (`FR-DUP-003`, `DEF-009`).

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use file_core::{
    CORE_SCHEMA_VERSION, CollisionPolicy, ErrorCode, FileFusionError, FileFusionResult, JobContext,
    StagedPath,
};
use serde::{Deserialize, Serialize};

use crate::storage::ExactDuplicateScanReport;

const STREAM_VERIFY_BUF_BYTES: usize = 65_536;
const QUARANTINE_DIR_NAME: &str = ".filefusion-quarantine";
const MANIFEST_FILE_NAME: &str = "manifest.json";

static OP_NONCE: AtomicU64 = AtomicU64::new(1);

/// Safe reclamation action mode for confirmed duplicate files (`FR-DUP-003`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReclamationStrategy {
    /// Move redundant copies into a reversible quarantine directory with a JSON restore manifest.
    Quarantine,
    /// Atomically replace redundant copies with same-volume OS hardlinks to the canonical keeper.
    Hardlink,
    /// Permanently unlink redundant copies (allowed only when explicitly consented by caller).
    PermanentDelete,
}

/// Planned operation targeting a single redundant duplicate copy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReclamationAction {
    /// Retained canonical copy path.
    pub keeper_path: String,
    /// Redundant duplicate path slated for cleanup.
    pub target_path: String,
    /// Verified `BLAKE3` hex digest shared by keeper and target at scan time.
    pub blake3_hex: String,
    /// Byte length expected on disk.
    pub file_size_bytes: u64,
    /// Reclamation action selected.
    pub strategy: ReclamationStrategy,
}

/// Actionable dry-run reclamation plan constructed from [`ExactDuplicateScanReport`] (`FR-DUP-003`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReclamationPlan {
    /// Machine-readable JSON schema version.
    pub schema_version: u32,
    /// Strategy selected for every action in this plan.
    pub strategy: ReclamationStrategy,
    /// Target quarantine directory path (when [`ReclamationStrategy::Quarantine`]).
    pub quarantine_root: Option<String>,
    /// Individual planned actions.
    pub actions: Vec<ReclamationAction>,
    /// Estimated disk space reclaimed on execution.
    pub estimated_reclaimable_bytes: u64,
}

/// Persisted reversible record for a single quarantined duplicate file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuarantineEntry {
    /// Unique entry identifier within the batch.
    pub entry_id: String,
    /// Original file path prior to quarantine.
    pub original_path: String,
    /// Path where the payload is safely stored inside quarantine.
    pub quarantined_path: String,
    /// Canonical keeper path that remains in place.
    pub keeper_path: String,
    /// Verified `BLAKE3` digest at quarantine time.
    pub blake3_hex: String,
    /// File byte size.
    pub file_size_bytes: u64,
    /// `true` once restored back cleanly.
    pub restored: bool,
    /// `true` once permanently purged from quarantine.
    pub purged: bool,
}

/// Persisted batch manifest stored inside `.filefusion-quarantine/<batch_id>/manifest.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuarantineBatchManifest {
    /// Machine-readable JSON schema version.
    pub schema_version: u32,
    /// Batch identifier.
    pub batch_id: String,
    /// Directory containing this batch's quarantined payloads and manifest.
    pub batch_dir: String,
    /// Creation timestamp (UNIX seconds).
    pub created_unix_s: u64,
    /// Quarantined entries inside this batch.
    pub entries: Vec<QuarantineEntry>,
}

/// Execution outcome returned by [`execute_reclamation_plan`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReclamationExecutionReport {
    /// Machine-readable JSON schema version.
    pub schema_version: u32,
    /// Executed strategy.
    pub strategy: ReclamationStrategy,
    /// Actions completed successfully.
    pub applied_count: usize,
    /// Actions skipped safely (e.g., modified since scan, already hardlinked, cross-volume).
    pub skipped_count: usize,
    /// Confirmed reclaimed disk bytes.
    pub reclaimed_bytes: u64,
    /// Created quarantine batch ID when [`ReclamationStrategy::Quarantine`].
    pub quarantine_batch_id: Option<String>,
    /// Path to persisted quarantine `manifest.json` when created.
    pub manifest_path: Option<String>,
    /// Detailed non-fatal warnings describing skipped items or cross-volume constraints.
    pub warnings: Vec<String>,
}

/// Result returned when restoring a previously quarantined batch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuarantineRestoreReport {
    /// Machine-readable JSON schema version.
    pub schema_version: u32,
    /// Restored batch identifier.
    pub batch_id: String,
    /// Number of files restored cleanly.
    pub restored_count: usize,
    /// Total bytes restored.
    pub restored_bytes: u64,
    /// Number of entries already restored, purged, or skipped.
    pub skipped_count: usize,
    /// Warnings for missing/skipped entries.
    pub warnings: Vec<String>,
}

/// Result returned when permanently purging a quarantined batch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuarantinePurgeReport {
    /// Machine-readable JSON schema version.
    pub schema_version: u32,
    /// Purged batch identifier.
    pub batch_id: String,
    /// Number of quarantined files permanently removed.
    pub purged_count: usize,
    /// Bytes permanently freed from quarantine.
    pub purged_bytes: u64,
    /// Warnings for missing/skipped entries.
    pub warnings: Vec<String>,
}

/// Builds an inspectable dry-run [`ReclamationPlan`] from an exact duplicate scan report (`FR-DUP-003`).
#[must_use]
pub fn build_reclamation_plan(
    scan: &ExactDuplicateScanReport,
    strategy: ReclamationStrategy,
    quarantine_root: Option<&Path>,
) -> ReclamationPlan {
    let mut actions = Vec::new();
    let mut estimated_reclaimable_bytes = 0u64;

    for group in &scan.duplicate_groups {
        let Some(keeper_path) = group.member_paths.first() else {
            continue;
        };
        for target_path in group.member_paths.iter().skip(1) {
            estimated_reclaimable_bytes =
                estimated_reclaimable_bytes.saturating_add(group.file_size_bytes);
            actions.push(ReclamationAction {
                keeper_path: keeper_path.clone(),
                target_path: target_path.clone(),
                blake3_hex: group.blake3_hex.clone(),
                file_size_bytes: group.file_size_bytes,
                strategy,
            });
        }
    }

    let resolved_qroot = if matches!(strategy, ReclamationStrategy::Quarantine) {
        let base = quarantine_root.map(PathBuf::from).or_else(|| {
            scan.roots
                .first()
                .map(|r| Path::new(r).join(QUARANTINE_DIR_NAME))
        });
        base.map(|p| p.to_string_lossy().into_owned())
    } else {
        None
    };

    ReclamationPlan {
        schema_version: CORE_SCHEMA_VERSION,
        strategy,
        quarantine_root: resolved_qroot,
        actions,
        estimated_reclaimable_bytes,
    }
}

/// Executes a [`ReclamationPlan`] cleanly with TOCTOU digest verification, same-volume atomic
/// hardlink swapping, or reversible quarantine manifest generation (`FR-DUP-003`).
///
/// Permanent deletion requires `allow_permanent_delete == true` to guard against accidental loss.
///
/// # Errors
/// Returns [`FileFusionError`] on missing permissions, invalid consent gate, or cancellation.
pub fn execute_reclamation_plan(
    plan: &ReclamationPlan,
    allow_permanent_delete: bool,
    ctx: &JobContext,
) -> FileFusionResult<ReclamationExecutionReport> {
    ctx.check_cancelled()?;
    if matches!(plan.strategy, ReclamationStrategy::PermanentDelete) && !allow_permanent_delete {
        return Err(FileFusionError::new(
            ErrorCode::FmtUnsupported,
            "permanent duplicate deletion requires explicit confirmation flag",
        )
        .with_remediation(
            "Use reversible quarantine/hardlink cleanup or explicitly pass permanent-delete consent.",
        ));
    }

    match plan.strategy {
        ReclamationStrategy::Quarantine => execute_quarantine(plan, ctx),
        ReclamationStrategy::Hardlink => execute_hardlinks(plan, ctx),
        ReclamationStrategy::PermanentDelete => execute_permanent_delete(plan, ctx),
    }
}

/// Restores all non-purged files recorded inside a quarantine `manifest.json` (`FR-DUP-003`).
///
/// # Errors
/// Returns [`FileFusionError`] on missing/corrupt manifest or cancellation.
pub fn restore_quarantine_batch(
    manifest_path: &Path,
    collision_policy: CollisionPolicy,
    ctx: &JobContext,
) -> FileFusionResult<QuarantineRestoreReport> {
    ctx.check_cancelled()?;
    let mut manifest = read_manifest(manifest_path)?;
    let mut restored_count = 0usize;
    let mut restored_bytes = 0u64;
    let mut skipped_count = 0usize;
    let mut warnings = Vec::new();
    let mut manifest_dirty = false;

    for entry in &mut manifest.entries {
        match restore_single_entry(entry, collision_policy, ctx, &mut warnings) {
            Ok(Some(bytes)) => {
                manifest_dirty = true;
                restored_count = restored_count.saturating_add(1);
                restored_bytes = restored_bytes.saturating_add(bytes);
            }
            Ok(None) => {
                skipped_count = skipped_count.saturating_add(1);
            }
            Err(err) => {
                if manifest_dirty {
                    let _ = write_manifest_atomic(manifest_path, &manifest);
                }
                return Err(err);
            }
        }
    }

    write_manifest_atomic(manifest_path, &manifest)?;

    Ok(QuarantineRestoreReport {
        schema_version: CORE_SCHEMA_VERSION,
        batch_id: manifest.batch_id,
        restored_count,
        restored_bytes,
        skipped_count,
        warnings,
    })
}

fn restore_single_entry(
    entry: &mut QuarantineEntry,
    collision_policy: CollisionPolicy,
    ctx: &JobContext,
    warnings: &mut Vec<String>,
) -> FileFusionResult<Option<u64>> {
    ctx.check_cancelled()?;
    if entry.restored || entry.purged {
        return Ok(None);
    }
    let q_path = Path::new(&entry.quarantined_path);
    let orig_path = Path::new(&entry.original_path);
    if !q_path.exists() {
        warnings.push(format!(
            "missing quarantined payload `{}`",
            q_path.display()
        ));
        return Ok(None);
    }
    if !verify_file_matches(q_path, entry.file_size_bytes, &entry.blake3_hex, ctx)? {
        warnings.push(format!(
            "quarantined payload `{}` failed BLAKE3 integrity verification",
            q_path.display()
        ));
        return Ok(None);
    }
    let Some(target_dest) = resolve_restore_destination(orig_path, collision_policy)? else {
        warnings.push(format!(
            "skipped restore of `{}`: destination already exists",
            orig_path.display()
        ));
        return Ok(None);
    };
    if let Some(parent) = target_dest.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    move_file_with_policy(q_path, &target_dest, collision_policy, ctx)?;
    if target_dest != orig_path {
        warnings.push(format!(
            "restored `{}` to `{}` due to collision policy",
            orig_path.display(),
            target_dest.display()
        ));
    }
    entry.restored = true;
    Ok(Some(entry.file_size_bytes))
}

/// Permanently deletes remaining un-restored payloads inside a quarantine batch (`FR-DUP-003`).
///
/// # Errors
/// Returns [`FileFusionError`] if consent flag is false, manifest is unreadable, or cancelled.
pub fn purge_quarantine_batch(
    manifest_path: &Path,
    confirm_purge: bool,
    ctx: &JobContext,
) -> FileFusionResult<QuarantinePurgeReport> {
    ctx.check_cancelled()?;
    if !confirm_purge {
        return Err(FileFusionError::new(
            ErrorCode::FmtUnsupported,
            "quarantine purge requires explicit confirmation",
        ));
    }
    let mut manifest = read_manifest(manifest_path)?;
    let mut purged_count = 0usize;
    let mut purged_bytes = 0u64;
    let mut warnings = Vec::new();

    for entry in &mut manifest.entries {
        ctx.check_cancelled()?;
        if entry.restored || entry.purged {
            continue;
        }
        let q_path = Path::new(&entry.quarantined_path);
        if q_path.exists() {
            match fs::remove_file(q_path) {
                Ok(()) => {
                    purged_count = purged_count.saturating_add(1);
                    purged_bytes = purged_bytes.saturating_add(entry.file_size_bytes);
                    entry.purged = true;
                }
                Err(err) => warnings.push(format!("failed to purge `{}`: {err}", q_path.display())),
            }
        } else {
            entry.purged = true;
        }
    }

    write_manifest_atomic(manifest_path, &manifest)?;

    Ok(QuarantinePurgeReport {
        schema_version: CORE_SCHEMA_VERSION,
        batch_id: manifest.batch_id,
        purged_count,
        purged_bytes,
        warnings,
    })
}

fn execute_quarantine(
    plan: &ReclamationPlan,
    ctx: &JobContext,
) -> FileFusionResult<ReclamationExecutionReport> {
    let batch_id = format!(
        "q_{}_{}",
        unix_timestamp_secs(),
        OP_NONCE.fetch_add(1, Ordering::Relaxed)
    );
    let q_base = plan
        .quarantine_root
        .as_deref()
        .map(PathBuf::from)
        .ok_or_else(|| {
            FileFusionError::new(
                ErrorCode::FmtUnsupported,
                "quarantine directory could not be resolved",
            )
        })?;
    let batch_dir = q_base.join(&batch_id);
    fs::create_dir_all(&batch_dir)?;

    let mut warnings = Vec::new();
    let mut applied_count = 0usize;
    let mut skipped_count = 0usize;
    let mut reclaimed_bytes = 0u64;
    let manifest_path = batch_dir.join(MANIFEST_FILE_NAME);
    let mut manifest = QuarantineBatchManifest {
        schema_version: CORE_SCHEMA_VERSION,
        batch_id: batch_id.clone(),
        batch_dir: batch_dir.to_string_lossy().into_owned(),
        created_unix_s: unix_timestamp_secs(),
        entries: Vec::new(),
    };

    for (idx, action) in plan.actions.iter().enumerate() {
        if let Err(err) = ctx.check_cancelled() {
            if !manifest.entries.is_empty() {
                let _ = write_manifest_atomic(&manifest_path, &manifest);
            }
            return Err(err);
        }
        let keeper = Path::new(&action.keeper_path);
        let target = Path::new(&action.target_path);
        match verified_action_pair(keeper, target, action, ctx, &mut warnings) {
            Ok(true) => {}
            Ok(false) => {
                skipped_count = skipped_count.saturating_add(1);
                continue;
            }
            Err(err) => {
                if !manifest.entries.is_empty() {
                    let _ = write_manifest_atomic(&manifest_path, &manifest);
                }
                return Err(err);
            }
        }

        let name = target
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("duplicate.bin");
        let dest = batch_dir.join(format!("{idx:04}_{name}"));
        if let Err(err) = move_file_safe(target, &dest, ctx) {
            if !manifest.entries.is_empty() {
                let _ = write_manifest_atomic(&manifest_path, &manifest);
            }
            return Err(err);
        }

        manifest.entries.push(QuarantineEntry {
            entry_id: format!("item-{idx:04}"),
            original_path: action.target_path.clone(),
            quarantined_path: dest.to_string_lossy().into_owned(),
            keeper_path: action.keeper_path.clone(),
            blake3_hex: action.blake3_hex.clone(),
            file_size_bytes: action.file_size_bytes,
            restored: false,
            purged: false,
        });
        applied_count = applied_count.saturating_add(1);
        reclaimed_bytes = reclaimed_bytes.saturating_add(action.file_size_bytes);
    }

    write_manifest_atomic(&manifest_path, &manifest)?;

    Ok(ReclamationExecutionReport {
        schema_version: CORE_SCHEMA_VERSION,
        strategy: ReclamationStrategy::Quarantine,
        applied_count,
        skipped_count,
        reclaimed_bytes,
        quarantine_batch_id: Some(batch_id),
        manifest_path: Some(manifest_path.to_string_lossy().into_owned()),
        warnings,
    })
}

fn execute_hardlinks(
    plan: &ReclamationPlan,
    ctx: &JobContext,
) -> FileFusionResult<ReclamationExecutionReport> {
    let mut warnings = Vec::new();
    let mut applied_count = 0usize;
    let mut skipped_count = 0usize;
    let mut reclaimed_bytes = 0u64;

    for action in &plan.actions {
        ctx.check_cancelled()?;
        let keeper = Path::new(&action.keeper_path);
        let target = Path::new(&action.target_path);
        if !verified_action_pair(keeper, target, action, ctx, &mut warnings)? {
            skipped_count = skipped_count.saturating_add(1);
            continue;
        }
        if already_same_inode(keeper, target) {
            skipped_count = skipped_count.saturating_add(1);
            warnings.push(format!(
                "already hardlinked `{}` -> `{}`",
                target.display(),
                keeper.display()
            ));
            continue;
        }

        match atomic_hardlink_replace(keeper, target) {
            Ok(()) => {
                applied_count = applied_count.saturating_add(1);
                reclaimed_bytes = reclaimed_bytes.saturating_add(action.file_size_bytes);
            }
            Err(err) => {
                skipped_count = skipped_count.saturating_add(1);
                warnings.push(format!(
                    "hardlink skipped `{}` -> `{}`: {err}",
                    target.display(),
                    keeper.display()
                ));
            }
        }
    }

    Ok(ReclamationExecutionReport {
        schema_version: CORE_SCHEMA_VERSION,
        strategy: ReclamationStrategy::Hardlink,
        applied_count,
        skipped_count,
        reclaimed_bytes,
        quarantine_batch_id: None,
        manifest_path: None,
        warnings,
    })
}

fn execute_permanent_delete(
    plan: &ReclamationPlan,
    ctx: &JobContext,
) -> FileFusionResult<ReclamationExecutionReport> {
    let mut warnings = Vec::new();
    let mut applied_count = 0usize;
    let mut skipped_count = 0usize;
    let mut reclaimed_bytes = 0u64;

    for action in &plan.actions {
        ctx.check_cancelled()?;
        let keeper = Path::new(&action.keeper_path);
        let target = Path::new(&action.target_path);
        if !verified_action_pair(keeper, target, action, ctx, &mut warnings)? {
            skipped_count = skipped_count.saturating_add(1);
            continue;
        }
        match fs::remove_file(target) {
            Ok(()) => {
                applied_count = applied_count.saturating_add(1);
                reclaimed_bytes = reclaimed_bytes.saturating_add(action.file_size_bytes);
            }
            Err(err) => {
                skipped_count = skipped_count.saturating_add(1);
                warnings.push(format!("failed to delete `{}`: {err}", target.display()));
            }
        }
    }

    Ok(ReclamationExecutionReport {
        schema_version: CORE_SCHEMA_VERSION,
        strategy: ReclamationStrategy::PermanentDelete,
        applied_count,
        skipped_count,
        reclaimed_bytes,
        quarantine_batch_id: None,
        manifest_path: None,
        warnings,
    })
}

fn verified_action_pair(
    keeper: &Path,
    target: &Path,
    action: &ReclamationAction,
    ctx: &JobContext,
    warnings: &mut Vec<String>,
) -> FileFusionResult<bool> {
    if !keeper.exists() || !target.exists() {
        warnings.push(format!(
            "skipped missing keeper/target pair `{}` / `{}`",
            keeper.display(),
            target.display()
        ));
        return Ok(false);
    }
    if let (Ok(ck), Ok(ct)) = (fs::canonicalize(keeper), fs::canonicalize(target))
        && ck == ct
    {
        warnings.push(format!(
            "refused self-cleanup where keeper equals target `{}`",
            target.display()
        ));
        return Ok(false);
    }
    if !verify_file_matches(keeper, action.file_size_bytes, &action.blake3_hex, ctx)? {
        warnings.push(format!(
            "keeper `{}` changed since scan; skipping target `{}`",
            keeper.display(),
            target.display()
        ));
        return Ok(false);
    }
    if !verify_file_matches(target, action.file_size_bytes, &action.blake3_hex, ctx)? {
        warnings.push(format!(
            "target `{}` modified since scan; skipping",
            target.display()
        ));
        return Ok(false);
    }
    Ok(true)
}

fn verify_file_matches(
    path: &Path,
    expected_size: u64,
    expected_blake3: &str,
    ctx: &JobContext,
) -> FileFusionResult<bool> {
    let Ok(meta) = fs::metadata(path) else {
        return Ok(false);
    };
    if !meta.is_file() || meta.len() != expected_size {
        return Ok(false);
    }
    let mut file = File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; STREAM_VERIFY_BUF_BYTES];
    loop {
        ctx.check_cancelled()?;
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().to_hex().as_str() == expected_blake3)
}

#[cfg(unix)]
fn already_same_inode(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (fs::metadata(a), fs::metadata(b)) {
        (Ok(ma), Ok(mb)) => ma.dev() == mb.dev() && ma.ino() == mb.ino(),
        _ => false,
    }
}

#[cfg(not(unix))]
const fn already_same_inode(_a: &Path, _b: &Path) -> bool {
    false
}

/// Replaces `target` with a hardlink to `keeper` safely: links into a sibling staging path first
/// so cross-volume errors fail before touching `target`, swaps atomically, and rolls back on error.
fn atomic_hardlink_replace(keeper: &Path, target: &Path) -> io::Result<()> {
    let parent = target
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let nonce = OP_NONCE.fetch_add(1, Ordering::Relaxed);
    let staged_link = parent.join(format!(".filefusion.hlink.{nonce}.tmp"));
    let backup_path = parent.join(format!(".filefusion.hbak.{nonce}.tmp"));

    if let Err(err) = fs::hard_link(keeper, &staged_link) {
        let _ = fs::remove_file(&staged_link);
        return Err(err);
    }

    if let Err(err) = fs::rename(target, &backup_path) {
        let _ = fs::remove_file(&staged_link);
        return Err(err);
    }

    if let Err(err) = fs::rename(&staged_link, target) {
        let _ = fs::rename(&backup_path, target);
        let _ = fs::remove_file(&staged_link);
        return Err(err);
    }

    let _ = fs::remove_file(&backup_path);
    Ok(())
}

fn move_file_safe(src: &Path, dst: &Path, ctx: &JobContext) -> FileFusionResult<()> {
    if fs::rename(src, dst).is_ok() {
        return Ok(());
    }
    let staged = StagedPath::create_sibling(dst)?;
    copy_verified_streaming(src, staged.staged_path(), ctx)?;
    let _ = staged.promote(CollisionPolicy::OverwriteWithBackup)?;
    fs::remove_file(src)?;
    Ok(())
}

fn resolve_restore_destination(
    orig_path: &Path,
    policy: CollisionPolicy,
) -> FileFusionResult<Option<PathBuf>> {
    if !orig_path.exists() {
        return Ok(Some(orig_path.to_path_buf()));
    }
    match policy {
        CollisionPolicy::Skip => Ok(None),
        CollisionPolicy::OverwriteWithBackup => Ok(Some(orig_path.to_path_buf())),
        CollisionPolicy::RenameWithSuffix => {
            let parent = orig_path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            let stem = orig_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("restored");
            let ext = orig_path.extension().and_then(|e| e.to_str());
            for n in 1u32..10_000 {
                let candidate_name = match ext {
                    Some(e) if !e.is_empty() => format!("{stem}_{n}.{e}"),
                    _ => format!("{stem}_{n}"),
                };
                let candidate = parent.join(candidate_name);
                if !candidate.exists() {
                    return Ok(Some(candidate));
                }
            }
            Err(FileFusionError::new(
                ErrorCode::IntInvariant,
                format!(
                    "exhausted rename suffixes restoring `{}`",
                    orig_path.display()
                ),
            ))
        }
    }
}

fn move_file_with_policy(
    src: &Path,
    dst: &Path,
    policy: CollisionPolicy,
    ctx: &JobContext,
) -> FileFusionResult<()> {
    let staged = StagedPath::create_sibling(dst)?;
    copy_verified_streaming(src, staged.staged_path(), ctx)?;
    let _ = staged.promote(policy)?;
    fs::remove_file(src)?;
    Ok(())
}

fn copy_verified_streaming(src: &Path, dst: &Path, ctx: &JobContext) -> FileFusionResult<()> {
    let mut reader = File::open(src)?;
    let mut writer = File::create(dst)?;
    let mut buf = vec![0u8; STREAM_VERIFY_BUF_BYTES];
    loop {
        ctx.check_cancelled()?;
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        writer.write_all(&buf[..n])?;
    }
    writer.sync_all()?;
    Ok(())
}

fn read_manifest(path: &Path) -> FileFusionResult<QuarantineBatchManifest> {
    if !path.exists() {
        return Err(FileFusionError::new(
            ErrorCode::IoNotFound,
            format!("quarantine manifest does not exist: `{}`", path.display()),
        ));
    }
    let raw = fs::read_to_string(path)?;
    serde_json::from_str(&raw).map_err(|err| {
        FileFusionError::new(
            ErrorCode::FmtCorrupt,
            format!("invalid quarantine manifest `{}`: {err}", path.display()),
        )
    })
}

fn write_manifest_atomic(path: &Path, manifest: &QuarantineBatchManifest) -> FileFusionResult<()> {
    let json = serde_json::to_string_pretty(manifest).map_err(|err| {
        FileFusionError::new(
            ErrorCode::IntInvariant,
            format!("failed to serialize quarantine manifest: {err}"),
        )
    })?;
    let staged = StagedPath::create_sibling(path)?;
    fs::write(staged.staged_path(), json)?;
    // Atomically replace `manifest.json` directly without emitting `manifest.json.bak` clutter.
    let _ = staged.promote(CollisionPolicy::Skip)?;
    Ok(())
}

fn unix_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
