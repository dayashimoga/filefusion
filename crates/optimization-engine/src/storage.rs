//! Stage 5 file intelligence and storage analytics (`FR-DUP-001`, `FR-DUP-002`, `FR-DET-004`).
//!
//! Delivers:
//! - Multi-stage exact duplicate scanning (`size -> 4 KB head/tail -> full streaming BLAKE3`)
//! - Perceptual image similarity clustering (`aHash` / `dHash` / `pHash`)
//! - Hierarchical storage treemap & format/domain breakdown analytics

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use file_core::{
    CORE_SCHEMA_VERSION, ErrorCode, FileFusionError, FileFusionResult, FormatId, JobContext,
    ProcessingDomain, classify_processing_domain, detect_signature_from_path,
};
use serde::{Deserialize, Serialize};

use crate::dedup::{
    PerceptualHashSet, are_perceptually_similar, compute_perceptual_hash_set, hamming_distance_u64,
};

const PARTIAL_HASH_WINDOW_BYTES: usize = 4_096;
const FULL_STREAM_BUFFER_BYTES: usize = 65_536;
const MAX_DECODE_IMAGE_BYTES: u64 = 128 * 1024 * 1024;
const DEFAULT_TREEMAP_MAX_DEPTH: usize = 4;

/// Configuration options controlling Stage 5 directory traversal and filtering (`FR-DUP-001`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DuplicateScanOptions {
    /// Minimum regular-file byte length eligible for scanning (defaults to `1` to skip empty stubs).
    pub min_file_bytes: u64,
    /// Whether dot-prefixed hidden files and directories should be traversed.
    pub include_hidden: bool,
    /// Whether symlinks should be followed with canonical cycle protection.
    pub follow_symlinks: bool,
    /// Optional maximum directory recursion depth relative to each scan root.
    pub max_depth: Option<usize>,
    /// Directory names always skipped during scanning (defaults to `[".filefusion-quarantine"]`).
    pub ignore_dir_names: Vec<String>,
}

impl Default for DuplicateScanOptions {
    fn default() -> Self {
        Self {
            min_file_bytes: 1,
            include_hidden: false,
            follow_symlinks: false,
            max_depth: None,
            ignore_dir_names: vec![String::from(".filefusion-quarantine")],
        }
    }
}

/// Group of bit-exact duplicate files sharing an identical byte length and `BLAKE3` digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExactDuplicateGroup {
    /// Canonical `BLAKE3` 256-bit hex digest shared by every member.
    pub blake3_hex: String,
    /// Byte length of each file in the group.
    pub file_size_bytes: u64,
    /// Reclaimable duplicate bytes `(member_paths.len() - 1) * file_size_bytes`.
    pub wasted_bytes: u64,
    /// Deterministically sorted paths; index `0` is the recommended canonical keeper.
    pub member_paths: Vec<String>,
}

/// Full audit report produced by the 3-stage exact duplicate scanner (`FR-DUP-001`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExactDuplicateScanReport {
    /// Machine-readable JSON schema version.
    pub schema_version: u32,
    /// Scanned root paths.
    pub roots: Vec<String>,
    /// Total regular files matching filter options.
    pub files_scanned: usize,
    /// Total logical bytes across scanned files.
    pub total_bytes_scanned: u64,
    /// Files remaining after Stage 1 (same-size bucket `>= 2`).
    pub stage1_size_candidates: usize,
    /// Files remaining after Stage 2 (`4 KB` head + `4 KB` tail `BLAKE3` partial hash `>= 2`).
    pub stage2_partial_candidates: usize,
    /// Files subjected to Stage 3 full streaming `BLAKE3` hashing.
    pub files_full_hashed: usize,
    /// Actual disk bytes read across Stage 2 and Stage 3 (proving minimal I/O).
    pub bytes_actually_read: u64,
    /// Confirmed exact duplicate groups sorted descending by `wasted_bytes`.
    pub duplicate_groups: Vec<ExactDuplicateGroup>,
    /// Count of redundant duplicate files (`sum(group.len() - 1)`).
    pub total_duplicate_files: usize,
    /// Total reclaimable duplicate bytes across all groups.
    pub total_wasted_bytes: u64,
    /// Non-fatal traversal or unreadable-entry warnings.
    pub warnings: Vec<String>,
}

/// Tuning parameters for perceptual near-duplicate image detection (`FR-DUP-002`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NearDuplicateOptions {
    /// Underlying traversal filter configuration.
    pub scan: DuplicateScanOptions,
    /// Maximum Hamming bit distance (`0..=64`) considered visually similar (default: `8`).
    pub max_hamming_distance: u32,
}

impl Default for NearDuplicateOptions {
    fn default() -> Self {
        Self {
            scan: DuplicateScanOptions::default(),
            max_hamming_distance: 8,
        }
    }
}

/// Single image inside a perceptual near-duplicate cluster (`FR-DUP-002`, `FR-DET-004`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimilarityMember {
    /// Image file path.
    pub path: String,
    /// Detected image format (`PNG`, `JPEG`, `WebP`, etc.).
    pub format: FormatId,
    /// File byte size.
    pub file_size_bytes: u64,
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// Hex-encoded 64-bit `aHash`.
    pub ahash_hex: String,
    /// Hex-encoded 64-bit `dHash`.
    pub dhash_hex: String,
    /// Hex-encoded 64-bit `pHash`.
    pub phash_hex: String,
    /// Hamming distance against the cluster keeper's `pHash`.
    pub phash_distance_from_keeper: u32,
    /// Hamming distance against the cluster keeper's `dHash`.
    pub dhash_distance_from_keeper: u32,
    /// Hamming distance against the cluster keeper's `aHash`.
    pub ahash_distance_from_keeper: u32,
    /// `true` for the highest-resolution / canonical keeper in the cluster.
    pub is_recommended_keeper: bool,
}

/// Visually similar image group discovered via perceptual hashing (`FR-DUP-002`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NearDuplicateCluster {
    /// Recommended highest-quality canonical keeper path.
    pub keeper_path: String,
    /// Maximum pairwise `pHash`/`dHash` distance relative to the keeper.
    pub max_distance_observed: u32,
    /// Estimated byte savings if non-keeper images in this cluster were cleaned up.
    pub potential_savings_bytes: u64,
    /// Members sorted with recommended keeper first.
    pub members: Vec<SimilarityMember>,
}

/// Complete report emitted by [`scan_near_duplicate_images`] (`FR-DUP-002`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NearDuplicateScanReport {
    /// Machine-readable JSON schema version.
    pub schema_version: u32,
    /// Scanned root paths.
    pub roots: Vec<String>,
    /// Candidate raster image files decoded and fingerprinted.
    pub images_analyzed: usize,
    /// Visually similar clusters sorted descending by `potential_savings_bytes`.
    pub clusters: Vec<NearDuplicateCluster>,
    /// Aggregate byte savings across all non-keeper near-duplicates.
    pub total_potential_savings_bytes: u64,
    /// Non-fatal traversal or corrupt-image warnings.
    pub warnings: Vec<String>,
}

/// Per-processing-domain disk usage rollup (`FR-DUP-003`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainStorageBreakdown {
    /// Classified domain (`image`, `video`, `audio`, `document`, `archive`, `data`, `unknown`).
    pub domain: ProcessingDomain,
    /// File count in domain.
    pub file_count: usize,
    /// Total bytes occupied.
    pub total_bytes: u64,
}

/// Per-format disk usage rollup (`FR-DET-001`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormatStorageBreakdown {
    /// Magic-byte detected format.
    pub format: FormatId,
    /// File count of this format.
    pub file_count: usize,
    /// Total bytes occupied.
    pub total_bytes: u64,
}

/// Largest-file leaderboard entry for storage analytics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LargestFileItem {
    /// File path.
    pub path: String,
    /// File size in bytes.
    pub size_bytes: u64,
    /// Magic-byte detected format.
    pub format: FormatId,
    /// Processing domain.
    pub domain: ProcessingDomain,
}

/// Recursive directory size summary node forming a bounded storage treemap.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryUsageNode {
    /// Directory path.
    pub path: String,
    /// Bytes stored directly inside files in this directory.
    pub direct_bytes: u64,
    /// Cumulative bytes across this directory and all subdirectories.
    pub cumulative_bytes: u64,
    /// Cumulative regular file count in subtree.
    pub file_count: usize,
    /// Direct child directory rollups sorted descending by `cumulative_bytes`.
    pub children: Vec<DirectoryUsageNode>,
}

/// Hierarchical storage analytics and treemap report (`FR-DUP-003`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageTreemapReport {
    /// Machine-readable JSON schema version.
    pub schema_version: u32,
    /// Analyzed root paths.
    pub roots: Vec<String>,
    /// Total regular files scanned.
    pub total_files: usize,
    /// Total disk footprint in bytes.
    pub total_bytes: u64,
    /// Count of zero-byte empty files observed.
    pub zero_byte_files: usize,
    /// Domain usage distribution sorted descending by bytes.
    pub domain_breakdown: Vec<DomainStorageBreakdown>,
    /// Format usage distribution sorted descending by bytes.
    pub format_breakdown: Vec<FormatStorageBreakdown>,
    /// Largest individual files up to the requested limit.
    pub largest_files: Vec<LargestFileItem>,
    /// Recursive directory treemap nodes capped cleanly to 4 levels deep.
    pub treemap_roots: Vec<DirectoryUsageNode>,
    /// Non-fatal traversal or inspection warnings.
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
struct ScannedFile {
    path: PathBuf,
    size_bytes: u64,
}

#[derive(Debug, Clone)]
struct DecodedImageFingerprint {
    path: String,
    format: FormatId,
    file_size_bytes: u64,
    width: u32,
    height: u32,
    hashes: PerceptualHashSet,
}

#[derive(Debug, Default)]
struct DirAccumulator {
    direct_bytes: u64,
    cumulative_bytes: u64,
    file_count: usize,
    child_dirs: Vec<PathBuf>,
}

/// Runs the 3-stage exact duplicate scanner (`size -> 4 KB head/tail -> full streaming BLAKE3`)
/// over `roots`, minimizing disk I/O by pruning unique candidates at every stage (`FR-DUP-001`).
///
/// # Errors
/// Returns [`FileFusionError`] if `roots` is empty or cancellation is requested.
pub fn scan_exact_duplicates(
    roots: &[&Path],
    opts: &DuplicateScanOptions,
    ctx: &JobContext,
) -> FileFusionResult<ExactDuplicateScanReport> {
    validate_roots(roots)?;
    let mut warnings = Vec::new();
    let files = collect_candidate_files(roots, opts, ctx, &mut warnings)?;
    let files_scanned = files.len();
    let total_bytes_scanned = files
        .iter()
        .fold(0u64, |acc, f| acc.saturating_add(f.size_bytes));

    let size_candidates = group_size_candidates(files);
    let stage1_size_candidates = size_candidates.len();

    let mut bytes_actually_read = 0u64;
    let partial_candidates = prune_partial_hash_candidates(
        size_candidates,
        ctx,
        &mut bytes_actually_read,
        &mut warnings,
    )?;
    let stage2_partial_candidates = partial_candidates.len();

    let mut files_full_hashed = 0usize;
    let duplicate_groups = resolve_full_hash_duplicate_groups(
        partial_candidates,
        ctx,
        &mut bytes_actually_read,
        &mut files_full_hashed,
        &mut warnings,
    )?;

    let total_duplicate_files = duplicate_groups.iter().fold(0usize, |acc, g| {
        acc.saturating_add(g.member_paths.len().saturating_sub(1))
    });
    let total_wasted_bytes = duplicate_groups
        .iter()
        .fold(0u64, |acc, g| acc.saturating_add(g.wasted_bytes));

    Ok(ExactDuplicateScanReport {
        schema_version: CORE_SCHEMA_VERSION,
        roots: roots
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect(),
        files_scanned,
        total_bytes_scanned,
        stage1_size_candidates,
        stage2_partial_candidates,
        files_full_hashed,
        bytes_actually_read,
        duplicate_groups,
        total_duplicate_files,
        total_wasted_bytes,
        warnings,
    })
}

fn group_size_candidates(files: Vec<ScannedFile>) -> Vec<ScannedFile> {
    let mut size_buckets: HashMap<u64, Vec<ScannedFile>> = HashMap::new();
    for entry in files {
        size_buckets
            .entry(entry.size_bytes)
            .or_default()
            .push(entry);
    }
    size_buckets
        .into_values()
        .filter(|bucket| bucket.len() >= 2)
        .flatten()
        .collect()
}

fn prune_partial_hash_candidates(
    size_candidates: Vec<ScannedFile>,
    ctx: &JobContext,
    bytes_actually_read: &mut u64,
    warnings: &mut Vec<String>,
) -> FileFusionResult<Vec<ScannedFile>> {
    let mut partial_buckets: HashMap<(u64, String), Vec<ScannedFile>> = HashMap::new();
    for candidate in size_candidates {
        ctx.check_cancelled()?;
        match compute_partial_head_tail_hash(&candidate.path, candidate.size_bytes) {
            Ok((partial_hex, read_now)) => {
                *bytes_actually_read = bytes_actually_read.saturating_add(read_now);
                partial_buckets
                    .entry((candidate.size_bytes, partial_hex))
                    .or_default()
                    .push(candidate);
            }
            Err(err) => warnings.push(format!(
                "skipped partial hash on `{}`: {}",
                candidate.path.display(),
                err.diagnostic.message
            )),
        }
    }
    Ok(partial_buckets
        .into_values()
        .filter(|bucket| bucket.len() >= 2)
        .flatten()
        .collect())
}

fn resolve_full_hash_duplicate_groups(
    partial_candidates: Vec<ScannedFile>,
    ctx: &JobContext,
    bytes_actually_read: &mut u64,
    files_full_hashed: &mut usize,
    warnings: &mut Vec<String>,
) -> FileFusionResult<Vec<ExactDuplicateGroup>> {
    let mut full_buckets: HashMap<(u64, String), Vec<String>> = HashMap::new();
    for candidate in partial_candidates {
        ctx.check_cancelled()?;
        match compute_streaming_blake3(&candidate.path, ctx) {
            Ok((full_hex, read_now)) => {
                *bytes_actually_read = bytes_actually_read.saturating_add(read_now);
                *files_full_hashed = files_full_hashed.saturating_add(1);
                full_buckets
                    .entry((candidate.size_bytes, full_hex))
                    .or_default()
                    .push(candidate.path.to_string_lossy().into_owned());
            }
            Err(err) if err.diagnostic.code == ErrorCode::JobCancelled => return Err(err),
            Err(err) => warnings.push(format!(
                "skipped full hash on `{}`: {}",
                candidate.path.display(),
                err.diagnostic.message
            )),
        }
    }

    let mut duplicate_groups: Vec<ExactDuplicateGroup> = full_buckets
        .into_iter()
        .filter_map(|((file_size_bytes, blake3_hex), mut member_paths)| {
            if member_paths.len() < 2 {
                return None;
            }
            member_paths.sort();
            let dup_count = u64::try_from(member_paths.len().saturating_sub(1)).unwrap_or(0);
            let wasted_bytes = file_size_bytes.saturating_mul(dup_count);
            Some(ExactDuplicateGroup {
                blake3_hex,
                file_size_bytes,
                wasted_bytes,
                member_paths,
            })
        })
        .collect();

    duplicate_groups.sort_by(|a, b| {
        b.wasted_bytes
            .cmp(&a.wasted_bytes)
            .then_with(|| a.blake3_hex.cmp(&b.blake3_hex))
    });
    Ok(duplicate_groups)
}

/// Discovers visually similar (resized/re-encoded) raster images across `roots` using
/// `aHash`, `dHash`, and `pHash` perceptual fingerprints (`FR-DUP-002`, `FR-DET-004`).
///
/// # Errors
/// Returns [`FileFusionError`] if `roots` is empty or cancellation is requested.
pub fn scan_near_duplicate_images(
    roots: &[&Path],
    opts: &NearDuplicateOptions,
    ctx: &JobContext,
) -> FileFusionResult<NearDuplicateScanReport> {
    validate_roots(roots)?;
    let mut warnings = Vec::new();
    let files = collect_candidate_files(roots, &opts.scan, ctx, &mut warnings)?;

    let mut analyzed = Vec::new();
    for entry in files {
        ctx.check_cancelled()?;
        if entry.size_bytes > MAX_DECODE_IMAGE_BYTES {
            continue;
        }
        let Ok(sig) = detect_signature_from_path(&entry.path) else {
            continue;
        };
        if !is_decodable_raster_format(sig.detected_format) {
            continue;
        }
        match image::open(&entry.path) {
            Ok(img) => {
                let hashes = compute_perceptual_hash_set(&img);
                analyzed.push(DecodedImageFingerprint {
                    path: entry.path.to_string_lossy().into_owned(),
                    format: sig.detected_format,
                    file_size_bytes: entry.size_bytes,
                    width: img.width(),
                    height: img.height(),
                    hashes,
                });
            }
            Err(err) => warnings.push(format!(
                "skipped unreadable image `{}`: {err}",
                entry.path.display()
            )),
        }
    }

    let images_analyzed = analyzed.len();
    let clusters = build_similarity_clusters(&analyzed, opts.max_hamming_distance);
    let total_potential_savings_bytes = clusters
        .iter()
        .fold(0u64, |acc, c| acc.saturating_add(c.potential_savings_bytes));

    Ok(NearDuplicateScanReport {
        schema_version: CORE_SCHEMA_VERSION,
        roots: roots
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect(),
        images_analyzed,
        clusters,
        total_potential_savings_bytes,
        warnings,
    })
}

/// Computes hierarchical directory disk usage, format/domain distributions, and top-N largest files
/// across `roots` using bounded streaming metadata detection (`FR-DUP-003`, `FR-DET-001`).
///
/// # Errors
/// Returns [`FileFusionError`] if `roots` is empty or cancellation is requested.
pub fn analyze_storage_treemap(
    roots: &[&Path],
    opts: &DuplicateScanOptions,
    top_n_largest: usize,
    ctx: &JobContext,
) -> FileFusionResult<StorageTreemapReport> {
    validate_roots(roots)?;
    let mut warnings = Vec::new();
    // Always include all non-empty and 0-byte files in treemap visibility unless caller overrides.
    let mut treemap_scan_opts = opts.clone();
    treemap_scan_opts.min_file_bytes = 0;

    let files = collect_candidate_files(roots, &treemap_scan_opts, ctx, &mut warnings)?;
    let mut domain_counts: BTreeMap<String, (ProcessingDomain, usize, u64)> = BTreeMap::new();
    let mut format_counts: BTreeMap<String, (FormatId, usize, u64)> = BTreeMap::new();
    let mut largest_files = Vec::with_capacity(files.len());
    let mut dir_acc: HashMap<PathBuf, DirAccumulator> = HashMap::new();

    let mut total_bytes = 0u64;
    let mut zero_byte_files = 0usize;

    for entry in &files {
        ctx.check_cancelled()?;
        total_bytes = total_bytes.saturating_add(entry.size_bytes);
        if entry.size_bytes == 0 {
            zero_byte_files = zero_byte_files.saturating_add(1);
        }

        let (format, spoofed) = detect_signature_from_path(&entry.path)
            .map_or((FormatId::Unknown, false), |sig| {
                (sig.detected_format, sig.spoofing_mismatch)
            });
        let domain = classify_processing_domain(format, spoofed, false);

        let d_key = format!("{domain:?}");
        let d_slot = domain_counts.entry(d_key).or_insert((domain, 0, 0));
        d_slot.1 = d_slot.1.saturating_add(1);
        d_slot.2 = d_slot.2.saturating_add(entry.size_bytes);

        let f_key = format!("{format:?}");
        let f_slot = format_counts.entry(f_key).or_insert((format, 0, 0));
        f_slot.1 = f_slot.1.saturating_add(1);
        f_slot.2 = f_slot.2.saturating_add(entry.size_bytes);

        if entry.size_bytes >= opts.min_file_bytes {
            largest_files.push(LargestFileItem {
                path: entry.path.to_string_lossy().into_owned(),
                size_bytes: entry.size_bytes,
                format,
                domain,
            });
        }

        record_file_in_dir_tree(&entry.path, entry.size_bytes, &mut dir_acc);
    }

    let mut domain_breakdown: Vec<DomainStorageBreakdown> = domain_counts
        .into_values()
        .map(|(domain, file_count, bytes)| DomainStorageBreakdown {
            domain,
            file_count,
            total_bytes: bytes,
        })
        .collect();
    domain_breakdown.sort_by_key(|a| std::cmp::Reverse(a.total_bytes));

    let mut format_breakdown: Vec<FormatStorageBreakdown> = format_counts
        .into_values()
        .map(|(format, file_count, bytes)| FormatStorageBreakdown {
            format,
            file_count,
            total_bytes: bytes,
        })
        .collect();
    format_breakdown.sort_by_key(|a| std::cmp::Reverse(a.total_bytes));

    largest_files.sort_by(|a, b| {
        b.size_bytes
            .cmp(&a.size_bytes)
            .then_with(|| a.path.cmp(&b.path))
    });
    if top_n_largest > 0 {
        largest_files.truncate(top_n_largest);
    }

    let mut treemap_roots = Vec::with_capacity(roots.len());
    for root in roots {
        let key = normalize_treemap_key(root);
        treemap_roots.push(build_treemap_node(
            &key,
            &dir_acc,
            0,
            DEFAULT_TREEMAP_MAX_DEPTH,
        ));
    }

    Ok(StorageTreemapReport {
        schema_version: CORE_SCHEMA_VERSION,
        roots: roots
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect(),
        total_files: files.len(),
        total_bytes,
        zero_byte_files,
        domain_breakdown,
        format_breakdown,
        largest_files,
        treemap_roots,
        warnings,
    })
}

fn validate_roots(roots: &[&Path]) -> FileFusionResult<()> {
    if roots.is_empty() {
        return Err(FileFusionError::new(
            ErrorCode::FmtUnsupported,
            "at least one root path is required",
        ));
    }
    for root in roots {
        if !root.exists() {
            return Err(FileFusionError::new(
                ErrorCode::IoNotFound,
                format!("scan root does not exist: `{}`", root.display()),
            ));
        }
    }
    Ok(())
}

fn collect_candidate_files(
    roots: &[&Path],
    opts: &DuplicateScanOptions,
    ctx: &JobContext,
    warnings: &mut Vec<String>,
) -> FileFusionResult<Vec<ScannedFile>> {
    let mut visited_dirs: HashSet<PathBuf> = HashSet::new();
    let mut seen_files: HashSet<PathBuf> = HashSet::new();
    let mut results = Vec::new();
    let mut stack: Vec<(PathBuf, usize)> = roots
        .iter()
        .map(|root| (root.to_path_buf(), 0usize))
        .collect();

    while let Some((current, depth)) = stack.pop() {
        ctx.check_cancelled()?;
        let meta = match if opts.follow_symlinks {
            fs::metadata(&current)
        } else {
            fs::symlink_metadata(&current)
        } {
            Ok(m) => m,
            Err(err) => {
                warnings.push(format!("cannot stat `{}`: {err}", current.display()));
                continue;
            }
        };

        if meta.file_type().is_symlink() && !opts.follow_symlinks {
            continue;
        }
        if meta.is_file() {
            let size_bytes = meta.len();
            if size_bytes >= opts.min_file_bytes {
                let dedup_key = fs::canonicalize(&current).unwrap_or_else(|_| current.clone());
                if seen_files.insert(dedup_key) {
                    results.push(ScannedFile {
                        path: current,
                        size_bytes,
                    });
                }
            }
            continue;
        }
        if !meta.is_dir() {
            continue;
        }
        if opts.max_depth.is_some_and(|max| depth > max) {
            continue;
        }
        if let Ok(canon_dir) = fs::canonicalize(&current)
            && !visited_dirs.insert(canon_dir)
        {
            continue;
        }

        let entries = match fs::read_dir(&current) {
            Ok(rd) => rd,
            Err(err) => {
                warnings.push(format!("cannot read dir `{}`: {err}", current.display()));
                continue;
            }
        };
        for entry_res in entries {
            ctx.check_cancelled()?;
            let Ok(entry) = entry_res else {
                continue;
            };
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if opts
                .ignore_dir_names
                .iter()
                .any(|ign| ign == name_str.as_ref())
            {
                continue;
            }
            if !opts.include_hidden && name_str.starts_with('.') {
                continue;
            }
            stack.push((entry.path(), depth.saturating_add(1)));
        }
    }

    results.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(results)
}

fn compute_partial_head_tail_hash(
    path: &Path,
    expected_size: u64,
) -> FileFusionResult<(String, u64)> {
    let mut file = File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(&expected_size.to_le_bytes());

    let mut read_bytes = 0u64;
    let mut buf = [0u8; PARTIAL_HASH_WINDOW_BYTES];
    let head_target = usize::try_from(expected_size)
        .unwrap_or(usize::MAX)
        .min(buf.len());
    if head_target > 0 {
        let mut filled = 0usize;
        while filled < head_target {
            let n = file.read(&mut buf[filled..head_target])?;
            if n == 0 {
                break;
            }
            filled += n;
        }
        hasher.update(&buf[..filled]);
        read_bytes = read_bytes.saturating_add(u64::try_from(filled).unwrap_or(0));
    }

    let head_u64 = u64::try_from(PARTIAL_HASH_WINDOW_BYTES).unwrap_or(4096);
    if expected_size > head_u64 {
        let tail_u64 = (expected_size - head_u64).min(head_u64);
        let tail_target = usize::try_from(tail_u64).unwrap_or(PARTIAL_HASH_WINDOW_BYTES);
        let seek_offset = i64::try_from(tail_u64).unwrap_or(4096);
        file.seek(SeekFrom::End(-seek_offset))?;
        let mut filled = 0usize;
        while filled < tail_target {
            let n = file.read(&mut buf[filled..tail_target])?;
            if n == 0 {
                break;
            }
            filled += n;
        }
        hasher.update(&buf[..filled]);
        read_bytes = read_bytes.saturating_add(u64::try_from(filled).unwrap_or(0));
    }

    Ok((hasher.finalize().to_hex().to_string(), read_bytes))
}

fn compute_streaming_blake3(path: &Path, ctx: &JobContext) -> FileFusionResult<(String, u64)> {
    let mut file = File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; FULL_STREAM_BUFFER_BYTES];
    let mut total_read = 0u64;

    loop {
        ctx.check_cancelled()?;
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        total_read = total_read.saturating_add(u64::try_from(n).unwrap_or(0));
    }

    Ok((hasher.finalize().to_hex().to_string(), total_read))
}

const fn is_decodable_raster_format(format: FormatId) -> bool {
    matches!(
        format,
        FormatId::Png
            | FormatId::Jpeg
            | FormatId::Webp
            | FormatId::Gif
            | FormatId::Bmp
            | FormatId::Tiff
            | FormatId::Ico
    )
}

fn build_similarity_clusters(
    items: &[DecodedImageFingerprint],
    max_distance: u32,
) -> Vec<NearDuplicateCluster> {
    let n = items.len();
    if n < 2 {
        return Vec::new();
    }
    let mut parent: Vec<usize> = (0..n).collect();
    for i in 0..n {
        for j in (i + 1)..n {
            let a = &items[i].hashes;
            let b = &items[j].hashes;
            let phash_close = are_perceptually_similar(a.phash, b.phash, max_distance);
            let dual_close = are_perceptually_similar(a.dhash, b.dhash, max_distance)
                && are_perceptually_similar(a.ahash, b.ahash, max_distance);
            if phash_close || dual_close {
                uf_union(&mut parent, i, j);
            }
        }
    }

    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for idx in 0..n {
        let root = uf_find(&mut parent, idx);
        groups.entry(root).or_default().push(idx);
    }

    let mut clusters: Vec<NearDuplicateCluster> = groups
        .into_values()
        .filter_map(|mut idxs| {
            if idxs.len() < 2 {
                return None;
            }
            // Select keeper with highest pixel resolution, then byte size, then lexical path.
            idxs.sort_by(|&ia, &ib| {
                let a = &items[ia];
                let b = &items[ib];
                let area_a = u64::from(a.width).saturating_mul(u64::from(a.height));
                let area_b = u64::from(b.width).saturating_mul(u64::from(b.height));
                area_b
                    .cmp(&area_a)
                    .then_with(|| b.file_size_bytes.cmp(&a.file_size_bytes))
                    .then_with(|| a.path.cmp(&b.path))
            });
            let keeper = &items[idxs[0]];
            let mut max_distance_observed = 0u32;
            let mut potential_savings_bytes = 0u64;
            let mut members = Vec::with_capacity(idxs.len());

            for (pos, &idx) in idxs.iter().enumerate() {
                let m = &items[idx];
                let p_dist = hamming_distance_u64(keeper.hashes.phash, m.hashes.phash);
                let d_dist = hamming_distance_u64(keeper.hashes.dhash, m.hashes.dhash);
                let a_dist = hamming_distance_u64(keeper.hashes.ahash, m.hashes.ahash);
                let is_keeper = pos == 0;
                if !is_keeper {
                    potential_savings_bytes =
                        potential_savings_bytes.saturating_add(m.file_size_bytes);
                    max_distance_observed = max_distance_observed.max(p_dist.min(d_dist));
                }
                members.push(SimilarityMember {
                    path: m.path.clone(),
                    format: m.format,
                    file_size_bytes: m.file_size_bytes,
                    width: m.width,
                    height: m.height,
                    ahash_hex: format!("{:016x}", m.hashes.ahash),
                    dhash_hex: format!("{:016x}", m.hashes.dhash),
                    phash_hex: format!("{:016x}", m.hashes.phash),
                    phash_distance_from_keeper: p_dist,
                    dhash_distance_from_keeper: d_dist,
                    ahash_distance_from_keeper: a_dist,
                    is_recommended_keeper: is_keeper,
                });
            }

            Some(NearDuplicateCluster {
                keeper_path: keeper.path.clone(),
                max_distance_observed,
                potential_savings_bytes,
                members,
            })
        })
        .collect();

    clusters.sort_by(|a, b| {
        b.potential_savings_bytes
            .cmp(&a.potential_savings_bytes)
            .then_with(|| a.keeper_path.cmp(&b.keeper_path))
    });
    clusters
}

fn uf_find(parent: &mut [usize], mut x: usize) -> usize {
    while parent[x] != x {
        parent[x] = parent[parent[x]];
        x = parent[x];
    }
    x
}

fn uf_union(parent: &mut [usize], a: usize, b: usize) {
    let ra = uf_find(parent, a);
    let rb = uf_find(parent, b);
    if ra != rb {
        parent[rb] = ra;
    }
}

fn normalize_treemap_key(path: &Path) -> PathBuf {
    let normalized: PathBuf = path.components().collect();
    if normalized.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        normalized
    }
}

fn record_file_in_dir_tree(
    file_path: &Path,
    size_bytes: u64,
    dir_acc: &mut HashMap<PathBuf, DirAccumulator>,
) {
    let Some(parent_dir) = file_path.parent() else {
        return;
    };
    let parent_buf = normalize_treemap_key(parent_dir);
    {
        let direct = dir_acc.entry(parent_buf.clone()).or_default();
        direct.direct_bytes = direct.direct_bytes.saturating_add(size_bytes);
    }

    let mut cur = parent_buf;
    loop {
        let acc = dir_acc.entry(cur.clone()).or_default();
        acc.cumulative_bytes = acc.cumulative_bytes.saturating_add(size_bytes);
        acc.file_count = acc.file_count.saturating_add(1);

        let Some(up) = cur.parent() else {
            break;
        };
        if up.as_os_str().is_empty() || up == cur {
            break;
        }
        let up_buf = up.to_path_buf();
        let parent_acc = dir_acc.entry(up_buf.clone()).or_default();
        if !parent_acc.child_dirs.contains(&cur) {
            parent_acc.child_dirs.push(cur.clone());
        }
        cur = up_buf;
    }
}

fn build_treemap_node(
    dir: &Path,
    dir_acc: &HashMap<PathBuf, DirAccumulator>,
    depth: usize,
    max_depth: usize,
) -> DirectoryUsageNode {
    let Some(acc) = dir_acc.get(dir) else {
        return DirectoryUsageNode {
            path: dir.to_string_lossy().into_owned(),
            direct_bytes: 0,
            cumulative_bytes: 0,
            file_count: 0,
            children: Vec::new(),
        };
    };

    let mut children = if depth < max_depth {
        acc.child_dirs
            .iter()
            .map(|child| build_treemap_node(child, dir_acc, depth + 1, max_depth))
            .collect()
    } else {
        Vec::new()
    };
    children.sort_by(|a, b| {
        b.cumulative_bytes
            .cmp(&a.cumulative_bytes)
            .then_with(|| a.path.cmp(&b.path))
    });

    DirectoryUsageNode {
        path: dir.to_string_lossy().into_owned(),
        direct_bytes: acc.direct_bytes,
        cumulative_bytes: acc.cumulative_bytes,
        file_count: acc.file_count,
        children,
    }
}
