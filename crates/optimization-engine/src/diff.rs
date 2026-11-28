//! Unified file comparison & diff engine (`FR-VAL-002`, `FR-DET-004`, `FR-DUP-002`).
//!
//! Compares arbitrary file pairs across:
//! - Streaming byte equality + cryptographic digests (`BLAKE3`, `SHA-256`, `xxHash64`)
//! - Streaming `FastCDC` content-defined chunk overlap (`byte_reuse_ratio`)
//! - Visual pixel & perceptual comparison (`SSIM`, `PSNR dB`, `aHash`/`dHash`/`pHash` Hamming)
//! - Structured text line delta summary & normalized metadata field differences

use std::collections::{BTreeSet, HashMap};
use std::fs::{self, File};
use std::path::Path;

use file_core::{
    CORE_SCHEMA_VERSION, DigestSet, ErrorCode, FileFusionError, FileFusionResult, FormatId,
    JobContext, inspect_path,
};
use quality_engine::{
    PixelDiffSummary, diff_pixel_buffers, evaluate_pixel_buffers, files_are_bit_exact,
};
use serde::{Deserialize, Serialize};

use crate::dedup::{
    FastCdcConfig, chunk_stream_fastcdc, compute_perceptual_hash_set, hamming_distance_u64,
};

const DEFAULT_MAX_TEXT_DIFF_BYTES: usize = 2 * 1024 * 1024;
const MAX_LINE_HUNKS: usize = 64;
const MAX_DIFF_PREVIEW_CHARS: usize = 160;

/// Options controlling what analytical layers [`compare_files`] computes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileDiffOptions {
    /// Whether to compute streaming `FastCDC` sub-file chunk overlap metrics.
    pub include_cdc_delta: bool,
    /// `FastCDC` configuration used when `include_cdc_delta` is `true`.
    pub cdc_config: FastCdcConfig,
    /// Whether to decode matching raster formats and compute `SSIM`/`PSNR`/perceptual delta.
    pub include_visual_diff: bool,
    /// Whether to compute line-level text delta when both inputs are readable UTF-8 text.
    pub include_text_diff: bool,
    /// Byte ceiling per file for bounded UTF-8 text line diffing (default: `2 MiB`).
    pub max_text_diff_bytes: usize,
}

impl Default for FileDiffOptions {
    fn default() -> Self {
        Self {
            include_cdc_delta: true,
            cdc_config: FastCdcConfig::default(),
            include_visual_diff: true,
            include_text_diff: true,
            max_text_diff_bytes: DEFAULT_MAX_TEXT_DIFF_BYTES,
        }
    }
}

/// Content-defined `FastCDC` chunk overlap metrics between two files (`FR-DUP-002`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CdcDeltaSummary {
    /// Chunk count in file A.
    pub chunks_a: usize,
    /// Chunk count in file B.
    pub chunks_b: usize,
    /// Distinct shared chunk digests present in both files.
    pub shared_unique_chunks: usize,
    /// Bytes from file B covered by chunks identical to file A.
    pub shared_bytes_b: u64,
    /// Fraction `[0.0..=1.0]` of union chunk bytes shared across A and B.
    pub byte_similarity: f64,
}

/// Visual & perceptual image comparison metrics (`FR-VAL-002`, `FR-DET-004`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VisualDiffSummary {
    /// Width/height of file A.
    pub dimensions_a: (u32, u32),
    /// Width/height of file B.
    pub dimensions_b: (u32, u32),
    /// `true` when both images share identical pixel dimensions.
    pub same_dimensions: bool,
    /// Perceptual `aHash` Hamming distance (`0..=64`).
    pub ahash_distance: u32,
    /// Perceptual `dHash` Hamming distance (`0..=64`).
    pub dhash_distance: u32,
    /// Perceptual `pHash` Hamming distance (`0..=64`).
    pub phash_distance: u32,
    /// Windowed luma Structural Similarity Index (`0.0..=1.0`) when dimensions match.
    pub ssim: Option<f64>,
    /// Peak Signal-to-Noise Ratio in dB when dimensions match.
    pub psnr_db: Option<f64>,
    /// Detailed per-channel pixel diff statistics when dimensions match.
    pub pixel_summary: Option<PixelDiffSummary>,
}

/// Single line-level change record inside a text diff report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextDiffHunk {
    /// Change kind (`"added"`, `"removed"`, or `"modified"`).
    pub kind: String,
    /// 1-based line number in file A (`None` for pure insertions).
    pub line_a: Option<usize>,
    /// 1-based line number in file B (`None` for pure deletions).
    pub line_b: Option<usize>,
    /// Truncated preview of the changed line content.
    pub preview: String,
}

/// Bounded UTF-8 text line diff summary (`FR-VAL-002`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextDiffSummary {
    /// Line count in file A.
    pub lines_a: usize,
    /// Line count in file B.
    pub lines_b: usize,
    /// Unchanged lines matched between A and B.
    pub common_lines: usize,
    /// Lines present only in B.
    pub added_lines: usize,
    /// Lines present only in A.
    pub removed_lines: usize,
    /// Sample hunks capped deterministically (`<= 64`).
    pub sample_hunks: Vec<TextDiffHunk>,
    /// `true` if more hunks existed beyond `sample_hunks`.
    pub hunks_truncated: bool,
}

/// Difference in a single normalized metadata attribute across two files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetadataFieldDiff {
    /// Normalized metadata key.
    pub field: String,
    /// Value in file A (`None` if absent).
    pub value_a: Option<String>,
    /// Value in file B (`None` if absent).
    pub value_b: Option<String>,
}

/// Comprehensive unified comparison report returned by [`compare_files`] (`FR-VAL-002`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileDiffReport {
    /// Machine-readable JSON schema version.
    pub schema_version: u32,
    /// Path to file A.
    pub path_a: String,
    /// Path to file B.
    pub path_b: String,
    /// Format detected for file A.
    pub format_a: FormatId,
    /// Format detected for file B.
    pub format_b: FormatId,
    /// Byte size of file A.
    pub size_bytes_a: u64,
    /// Byte size of file B.
    pub size_bytes_b: u64,
    /// Signed size delta `size_bytes_b - size_bytes_a`.
    pub size_delta_bytes: i128,
    /// `true` when every byte of A and B is identical.
    pub bit_exact: bool,
    /// Full cryptographic + fast digests for file A.
    pub digests_a: DigestSet,
    /// Full cryptographic + fast digests for file B.
    pub digests_b: DigestSet,
    /// Streaming `FastCDC` chunk overlap metrics, when enabled.
    pub cdc_delta: Option<CdcDeltaSummary>,
    /// Visual SSIM/PSNR/perceptual comparison, when both are decodable images.
    pub visual_diff: Option<VisualDiffSummary>,
    /// Bounded UTF-8 text line diff, when both files are valid text.
    pub text_diff: Option<TextDiffSummary>,
    /// Normalized metadata differences across inspection attributes and camera/profile flags.
    pub metadata_diffs: Vec<MetadataFieldDiff>,
    /// Actionable human summary sentence describing the diff result.
    pub summary: String,
}

/// Compares `path_a` and `path_b` across byte equality, cryptographic digests, `FastCDC` chunks,
/// visual raster metrics, UTF-8 text lines, and normalized metadata (`FR-VAL-002`, `FR-DET-004`).
///
/// # Errors
/// Returns [`FileFusionError`] when either input is missing/unreadable or on cancellation.
pub fn compare_files(
    path_a: &Path,
    path_b: &Path,
    opts: &FileDiffOptions,
    ctx: &JobContext,
) -> FileFusionResult<FileDiffReport> {
    ctx.check_cancelled()?;
    ensure_regular_file(path_a)?;
    ensure_regular_file(path_b)?;

    let rep_a = inspect_path(path_a, false)?;
    ctx.check_cancelled()?;
    let rep_b = inspect_path(path_b, false)?;
    ctx.check_cancelled()?;

    let bit_exact =
        rep_a.digests.blake3 == rep_b.digests.blake3 && files_are_bit_exact(path_a, path_b, ctx)?;
    let metadata_diffs = collect_metadata_diffs(&rep_a, &rep_b);

    let cdc_delta = if opts.include_cdc_delta {
        Some(compute_cdc_delta(path_a, path_b, opts.cdc_config, ctx)?)
    } else {
        None
    };

    let visual_diff = if opts.include_visual_diff
        && is_visual_format(rep_a.detected_format)
        && is_visual_format(rep_b.detected_format)
    {
        compute_visual_diff(path_a, path_b, ctx)?
    } else {
        None
    };

    let text_diff = if opts.include_text_diff
        && rep_a.byte_size <= u64::try_from(opts.max_text_diff_bytes).unwrap_or(u64::MAX)
        && rep_b.byte_size <= u64::try_from(opts.max_text_diff_bytes).unwrap_or(u64::MAX)
        && is_text_candidate(rep_a.detected_format, rep_b.detected_format)
    {
        compute_text_diff(path_a, path_b, opts.max_text_diff_bytes, ctx)?
    } else {
        None
    };

    let size_delta_bytes = i128::from(rep_b.byte_size) - i128::from(rep_a.byte_size);
    let summary = format_diff_summary(
        bit_exact,
        size_delta_bytes,
        cdc_delta.as_ref(),
        visual_diff.as_ref(),
        text_diff.as_ref(),
        metadata_diffs.len(),
    );

    Ok(FileDiffReport {
        schema_version: CORE_SCHEMA_VERSION,
        path_a: path_a.to_string_lossy().into_owned(),
        path_b: path_b.to_string_lossy().into_owned(),
        format_a: rep_a.detected_format,
        format_b: rep_b.detected_format,
        size_bytes_a: rep_a.byte_size,
        size_bytes_b: rep_b.byte_size,
        size_delta_bytes,
        bit_exact,
        digests_a: rep_a.digests,
        digests_b: rep_b.digests,
        cdc_delta,
        visual_diff,
        text_diff,
        metadata_diffs,
        summary,
    })
}

fn ensure_regular_file(path: &Path) -> FileFusionResult<()> {
    if !path.exists() {
        return Err(FileFusionError::new(
            ErrorCode::IoNotFound,
            format!("comparison input does not exist: `{}`", path.display()),
        ));
    }
    if !path.is_file() {
        return Err(FileFusionError::new(
            ErrorCode::FmtUnsupported,
            format!(
                "comparison input is not a regular file: `{}`",
                path.display()
            ),
        ));
    }
    Ok(())
}

#[allow(clippy::cast_precision_loss)]
fn compute_cdc_delta(
    path_a: &Path,
    path_b: &Path,
    cfg: FastCdcConfig,
    ctx: &JobContext,
) -> FileFusionResult<CdcDeltaSummary> {
    let mut map_a: HashMap<String, u64> = HashMap::new();
    let mut chunks_a = 0usize;
    let mut file_a = File::open(path_a)?;
    chunk_stream_fastcdc(&mut file_a, cfg, ctx, |c| {
        chunks_a = chunks_a.saturating_add(1);
        map_a
            .entry(c.blake3_hex)
            .or_insert(u64::try_from(c.length).unwrap_or(0));
        Ok(())
    })?;

    let mut seen_b: HashMap<String, u64> = HashMap::new();
    let mut chunks_b = 0usize;
    let mut shared_bytes_b = 0u64;
    let mut file_b = File::open(path_b)?;
    chunk_stream_fastcdc(&mut file_b, cfg, ctx, |c| {
        chunks_b = chunks_b.saturating_add(1);
        let len_u64 = u64::try_from(c.length).unwrap_or(0);
        if map_a.contains_key(&c.blake3_hex) {
            shared_bytes_b = shared_bytes_b.saturating_add(len_u64);
        }
        seen_b.entry(c.blake3_hex).or_insert(len_u64);
        Ok(())
    })?;

    let mut shared_unique_chunks = 0usize;
    let mut shared_unique_bytes = 0u64;
    let mut union_bytes = 0u64;
    for (hash, &len) in &map_a {
        union_bytes = union_bytes.saturating_add(len);
        if seen_b.contains_key(hash) {
            shared_unique_chunks = shared_unique_chunks.saturating_add(1);
            shared_unique_bytes = shared_unique_bytes.saturating_add(len);
        }
    }
    for (hash, &len) in &seen_b {
        if !map_a.contains_key(hash) {
            union_bytes = union_bytes.saturating_add(len);
        }
    }

    let byte_similarity = if union_bytes == 0 {
        if chunks_a == 0 && chunks_b == 0 {
            1.0
        } else {
            0.0
        }
    } else {
        (shared_unique_bytes as f64) / (union_bytes as f64)
    };

    Ok(CdcDeltaSummary {
        chunks_a,
        chunks_b,
        shared_unique_chunks,
        shared_bytes_b,
        byte_similarity: byte_similarity.clamp(0.0, 1.0),
    })
}

const fn is_visual_format(fmt: FormatId) -> bool {
    matches!(
        fmt,
        FormatId::Png
            | FormatId::Jpeg
            | FormatId::Webp
            | FormatId::Avif
            | FormatId::Gif
            | FormatId::Bmp
            | FormatId::Tiff
            | FormatId::Ico
    )
}

fn compute_visual_diff(
    path_a: &Path,
    path_b: &Path,
    ctx: &JobContext,
) -> FileFusionResult<Option<VisualDiffSummary>> {
    ctx.check_cancelled()?;
    let Ok(img_a) = image::open(path_a) else {
        return Ok(None);
    };
    ctx.check_cancelled()?;
    let Ok(img_b) = image::open(path_b) else {
        return Ok(None);
    };
    ctx.check_cancelled()?;

    let h_a = compute_perceptual_hash_set(&img_a);
    let h_b = compute_perceptual_hash_set(&img_b);
    let dim_a = (img_a.width(), img_a.height());
    let dim_b = (img_b.width(), img_b.height());
    let same_dimensions = dim_a == dim_b;

    let (ssim, psnr_db, pixel_summary) = if same_dimensions {
        let rgba_a = img_a.to_rgba8();
        let rgba_b = img_b.to_rgba8();
        let w = usize::try_from(dim_a.0).unwrap_or(0);
        let h = usize::try_from(dim_a.1).unwrap_or(0);
        let q = evaluate_pixel_buffers(w, h, 4, rgba_a.as_raw(), rgba_b.as_raw())?;
        let p = diff_pixel_buffers(w, h, 4, rgba_a.as_raw(), rgba_b.as_raw())?;
        (q.ssim, q.psnr_db, Some(p))
    } else {
        (None, None, None)
    };

    Ok(Some(VisualDiffSummary {
        dimensions_a: dim_a,
        dimensions_b: dim_b,
        same_dimensions,
        ahash_distance: hamming_distance_u64(h_a.ahash, h_b.ahash),
        dhash_distance: hamming_distance_u64(h_a.dhash, h_b.dhash),
        phash_distance: hamming_distance_u64(h_a.phash, h_b.phash),
        ssim,
        psnr_db,
        pixel_summary,
    }))
}

const fn is_text_format(f: FormatId) -> bool {
    matches!(
        f,
        FormatId::PlainText
            | FormatId::Markdown
            | FormatId::Html
            | FormatId::Csv
            | FormatId::Json
            | FormatId::Yaml
            | FormatId::Xml
            | FormatId::Svg
            | FormatId::Unknown
    )
}

const fn is_text_candidate(a: FormatId, b: FormatId) -> bool {
    is_text_format(a) && is_text_format(b)
}

fn compute_text_diff(
    path_a: &Path,
    path_b: &Path,
    max_bytes: usize,
    ctx: &JobContext,
) -> FileFusionResult<Option<TextDiffSummary>> {
    ctx.check_cancelled()?;
    let bytes_a = fs::read(path_a)?;
    ctx.check_cancelled()?;
    let bytes_b = fs::read(path_b)?;
    ctx.check_cancelled()?;
    if bytes_a.len() > max_bytes || bytes_b.len() > max_bytes {
        return Ok(None);
    }
    let (Ok(text_a), Ok(text_b)) = (std::str::from_utf8(&bytes_a), std::str::from_utf8(&bytes_b))
    else {
        return Ok(None);
    };

    let lines_a: Vec<&str> = text_a.lines().collect();
    let lines_b: Vec<&str> = text_b.lines().collect();
    let (common_lines, added_lines, removed_lines, sample_hunks, hunks_truncated) =
        compute_lcs_line_diff(&lines_a, &lines_b);

    Ok(Some(TextDiffSummary {
        lines_a: lines_a.len(),
        lines_b: lines_b.len(),
        common_lines,
        added_lines,
        removed_lines,
        sample_hunks,
        hunks_truncated,
    }))
}

fn compute_lcs_line_diff(
    lines_a: &[&str],
    lines_b: &[&str],
) -> (usize, usize, usize, Vec<TextDiffHunk>, bool) {
    const MAX_DP_CELLS: usize = 1_500_000;
    let m = lines_a.len();
    let n = lines_b.len();
    let cols = n.saturating_add(1);
    let cells = (m.saturating_add(1)).saturating_mul(cols);

    let mut common_lines = 0usize;
    let mut added_lines = 0usize;
    let mut removed_lines = 0usize;
    let mut sample_hunks = Vec::new();
    let mut hunks_truncated = false;

    if cells > MAX_DP_CELLS {
        return compute_positional_line_diff(lines_a, lines_b);
    }

    let mut dp = vec![0u32; cells];
    for i in (0..m).rev() {
        for j in (0..n).rev() {
            dp[i * cols + j] = if lines_a[i] == lines_b[j] {
                dp[(i + 1) * cols + (j + 1)].saturating_add(1)
            } else {
                dp[(i + 1) * cols + j].max(dp[i * cols + (j + 1)])
            };
        }
    }

    let (mut i, mut j) = (0usize, 0usize);
    while i < m || j < n {
        if i < m && j < n && lines_a[i] == lines_b[j] {
            common_lines = common_lines.saturating_add(1);
            i += 1;
            j += 1;
        } else if j < n && (i == m || dp[i * cols + (j + 1)] >= dp[(i + 1) * cols + j]) {
            added_lines = added_lines.saturating_add(1);
            push_hunk(
                &mut sample_hunks,
                &mut hunks_truncated,
                "added",
                None,
                Some(j + 1),
                &truncate_line(lines_b[j]),
            );
            j += 1;
        } else {
            removed_lines = removed_lines.saturating_add(1);
            push_hunk(
                &mut sample_hunks,
                &mut hunks_truncated,
                "removed",
                Some(i + 1),
                None,
                &truncate_line(lines_a[i]),
            );
            i += 1;
        }
    }

    (
        common_lines,
        added_lines,
        removed_lines,
        sample_hunks,
        hunks_truncated,
    )
}

fn compute_positional_line_diff(
    lines_a: &[&str],
    lines_b: &[&str],
) -> (usize, usize, usize, Vec<TextDiffHunk>, bool) {
    let mut common_lines = 0usize;
    let mut added_lines = 0usize;
    let mut removed_lines = 0usize;
    let mut sample_hunks = Vec::new();
    let mut hunks_truncated = false;

    let max_len = lines_a.len().max(lines_b.len());
    for idx in 0..max_len {
        match (lines_a.get(idx), lines_b.get(idx)) {
            (Some(la), Some(lb)) if la == lb => common_lines = common_lines.saturating_add(1),
            (Some(la), Some(lb)) => {
                removed_lines = removed_lines.saturating_add(1);
                added_lines = added_lines.saturating_add(1);
                push_hunk(
                    &mut sample_hunks,
                    &mut hunks_truncated,
                    "modified",
                    Some(idx + 1),
                    Some(idx + 1),
                    &format!("{} -> {}", truncate_line(la), truncate_line(lb)),
                );
            }
            (Some(la), None) => {
                removed_lines = removed_lines.saturating_add(1);
                push_hunk(
                    &mut sample_hunks,
                    &mut hunks_truncated,
                    "removed",
                    Some(idx + 1),
                    None,
                    &truncate_line(la),
                );
            }
            (None, Some(lb)) => {
                added_lines = added_lines.saturating_add(1);
                push_hunk(
                    &mut sample_hunks,
                    &mut hunks_truncated,
                    "added",
                    None,
                    Some(idx + 1),
                    &truncate_line(lb),
                );
            }
            (None, None) => {}
        }
    }

    (
        common_lines,
        added_lines,
        removed_lines,
        sample_hunks,
        hunks_truncated,
    )
}

fn push_hunk(
    out: &mut Vec<TextDiffHunk>,
    truncated: &mut bool,
    kind: &str,
    line_a: Option<usize>,
    line_b: Option<usize>,
    preview: &str,
) {
    if out.len() >= MAX_LINE_HUNKS {
        *truncated = true;
        return;
    }
    out.push(TextDiffHunk {
        kind: String::from(kind),
        line_a,
        line_b,
        preview: preview.to_owned(),
    });
}

fn truncate_line(line: &str) -> String {
    let mut trimmed: String = line.chars().take(MAX_DIFF_PREVIEW_CHARS).collect();
    if line.chars().count() > MAX_DIFF_PREVIEW_CHARS {
        trimmed.push_str("...");
    }
    trimmed
}

fn collect_metadata_diffs(
    rep_a: &file_core::FileInspectionReport,
    rep_b: &file_core::FileInspectionReport,
) -> Vec<MetadataFieldDiff> {
    let mut keys: BTreeSet<String> = BTreeSet::new();
    for k in rep_a.metadata_summary.attributes.keys() {
        keys.insert(k.clone());
    }
    for k in rep_b.metadata_summary.attributes.keys() {
        keys.insert(k.clone());
    }

    let mut diffs = Vec::new();
    for key in keys {
        let va = rep_a.metadata_summary.attributes.get(&key).cloned();
        let vb = rep_b.metadata_summary.attributes.get(&key).cloned();
        if va != vb {
            diffs.push(MetadataFieldDiff {
                field: key,
                value_a: va,
                value_b: vb,
            });
        }
    }

    let flag_pairs = [
        (
            "has_gps_privacy_risk",
            rep_a.metadata_summary.has_gps_privacy_risk.to_string(),
            rep_b.metadata_summary.has_gps_privacy_risk.to_string(),
        ),
        (
            "has_pii_author_risk",
            rep_a.metadata_summary.has_pii_author_risk.to_string(),
            rep_b.metadata_summary.has_pii_author_risk.to_string(),
        ),
        (
            "has_icc_profile",
            rep_a.metadata_summary.has_icc_profile.to_string(),
            rep_b.metadata_summary.has_icc_profile.to_string(),
        ),
    ];
    for (field, va, vb) in flag_pairs {
        if va != vb {
            diffs.push(MetadataFieldDiff {
                field: String::from(field),
                value_a: Some(va),
                value_b: Some(vb),
            });
        }
    }
    diffs
}

fn format_diff_summary(
    bit_exact: bool,
    size_delta: i128,
    cdc: Option<&CdcDeltaSummary>,
    visual: Option<&VisualDiffSummary>,
    text: Option<&TextDiffSummary>,
    meta_diff_count: usize,
) -> String {
    if bit_exact {
        return String::from(
            "Files are bit-exact duplicates (identical BLAKE3 digest and payload).",
        );
    }
    let mut parts = vec![format!("size_delta={size_delta:+} bytes")];
    if let Some(c) = cdc {
        parts.push(format!("cdc_similarity={:.1}%", c.byte_similarity * 100.0));
    }
    if let Some(v) = visual {
        if let (Some(ssim), Some(psnr)) = (v.ssim, v.psnr_db) {
            parts.push(format!("ssim={ssim:.4}, psnr={psnr:.2} dB"));
        } else {
            parts.push(format!("phash_dist={}", v.phash_distance));
        }
    }
    if let Some(t) = text {
        parts.push(format!(
            "text(+{} / -{} lines)",
            t.added_lines, t.removed_lines
        ));
    }
    if meta_diff_count > 0 {
        parts.push(format!("metadata_diffs={meta_diff_count}"));
    }
    parts.join("; ")
}
