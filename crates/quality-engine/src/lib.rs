//! `quality-engine`: L1 empirical quality evaluation crate (`FF-CTR-001` §3.3, `FR-VAL-001`).
//!
//! Provides pure-Rust windowed SSIM, PSNR (dB), RGBA pixel-difference metrics,
//! and bounded-streaming bit-exact file comparison without unsafe code or external
//! C/C++ library dependencies (`MOD-RULE-01`, `MOD-RULE-05`).

#![forbid(unsafe_code)]

use std::fs::File;
use std::io::Read;
use std::path::Path;

use file_core::{
    ErrorCode, FileFusionError, FileFusionResult, JobContext, QualityEvaluator, QualityMetrics,
    ValidatedSource,
};
use serde::{Deserialize, Serialize};

/// Finite PSNR ceiling reported for bit-exact identical signals so JSON serialization
/// never emits `null` for `f64::INFINITY`.
pub const LOSSLESS_PSNR_CEILING_DB: f64 = 100.0;

/// Pixel-level difference metrics computed across two equal-dimension pixel buffers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PixelDiffSummary {
    /// Image width in pixels.
    pub width: usize,
    /// Image height in pixels.
    pub height: usize,
    /// Channel count per pixel (`1`, `3`, or `4`).
    pub channels: usize,
    /// Number of pixels differing in at least one channel.
    pub differing_pixels: u64,
    /// Total pixel count (`width * height`).
    pub total_pixels: u64,
    /// Maximum absolute per-channel difference observed (`0..=255`).
    pub max_channel_delta: u8,
    /// Mean absolute per-channel error (`0.0..=255.0`).
    pub mean_abs_error: f64,
    /// `true` when every byte matches identically.
    pub bit_exact: bool,
}

fn validate_image_buffers(
    width: usize,
    height: usize,
    channels: usize,
    a: &[u8],
    b: &[u8],
) -> FileFusionResult<usize> {
    if width == 0 || height == 0 || !matches!(channels, 1 | 3 | 4) {
        return Err(FileFusionError::new(
            ErrorCode::FmtUnsupported,
            format!("invalid image geometry {width}x{height} with {channels} channels"),
        )
        .with_remediation(
            "Supply non-zero dimensions with 1 (luma), 3 (RGB), or 4 (RGBA) channels.",
        ));
    }
    let expected = width
        .checked_mul(height)
        .and_then(|px| px.checked_mul(channels))
        .ok_or_else(|| {
            FileFusionError::new(ErrorCode::SecBomb, "image buffer dimensions overflow usize")
        })?;
    if a.len() != expected || b.len() != expected {
        return Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            format!(
                "pixel buffer length mismatch: expected {expected} bytes, got {} and {}",
                a.len(),
                b.len()
            ),
        ));
    }
    Ok(expected)
}

#[allow(clippy::cast_precision_loss)]
fn to_luma_plane(buf: &[u8], channels: usize) -> Vec<f64> {
    let pixel_count = buf.len() / channels;
    let mut luma = Vec::with_capacity(pixel_count);
    for px in buf.chunks_exact(channels) {
        let y = match channels {
            1 => f64::from(px[0]),
            3 | 4 => 0.299 * f64::from(px[0]) + 0.587 * f64::from(px[1]) + 0.114 * f64::from(px[2]),
            _ => 0.0,
        };
        luma.push(y);
    }
    luma
}

/// Computes empirical pixel difference summary across two raw pixel buffers (`FR-VAL-002`).
///
/// # Errors
/// Returns [`FileFusionError`] if dimensions/channels are invalid or buffer sizes mismatch.
#[allow(clippy::cast_precision_loss)]
pub fn diff_pixel_buffers(
    width: usize,
    height: usize,
    channels: usize,
    orig: &[u8],
    candidate: &[u8],
) -> FileFusionResult<PixelDiffSummary> {
    let expected = validate_image_buffers(width, height, channels, orig, candidate)?;
    let mut differing_pixels = 0u64;
    let mut max_delta = 0u8;
    let mut abs_sum = 0u64;

    for (pa, pb) in orig
        .chunks_exact(channels)
        .zip(candidate.chunks_exact(channels))
    {
        let mut px_diff = false;
        for (&ca, &cb) in pa.iter().zip(pb.iter()) {
            let d = ca.abs_diff(cb);
            if d > 0 {
                px_diff = true;
                max_delta = max_delta.max(d);
                abs_sum = abs_sum.saturating_add(u64::from(d));
            }
        }
        if px_diff {
            differing_pixels = differing_pixels.saturating_add(1);
        }
    }

    Ok(PixelDiffSummary {
        width,
        height,
        channels,
        differing_pixels,
        total_pixels: u64::try_from(width * height).unwrap_or(u64::MAX),
        max_channel_delta: max_delta,
        mean_abs_error: (abs_sum as f64) / (expected as f64),
        bit_exact: differing_pixels == 0,
    })
}

/// Computes Peak Signal-to-Noise Ratio (PSNR in dB) across equal-dimension pixel buffers.
///
/// Identical buffers yield [`LOSSLESS_PSNR_CEILING_DB`] (`100.0` dB).
///
/// # Errors
/// Returns [`FileFusionError`] on dimension or length mismatch.
#[allow(clippy::cast_precision_loss)]
pub fn compute_psnr_db(
    width: usize,
    height: usize,
    channels: usize,
    orig: &[u8],
    candidate: &[u8],
) -> FileFusionResult<f64> {
    let expected = validate_image_buffers(width, height, channels, orig, candidate)?;
    let mut sq_sum = 0u128;
    for (&ca, &cb) in orig.iter().zip(candidate.iter()) {
        let d = u128::from(ca.abs_diff(cb));
        sq_sum += d * d;
    }
    if sq_sum == 0 {
        return Ok(LOSSLESS_PSNR_CEILING_DB);
    }
    let mse = (sq_sum as f64) / (expected as f64);
    Ok(10.0 * ((255.0_f64 * 255.0_f64) / mse).log10())
}

/// Computes Structural Similarity Index (SSIM `0.0..=1.0`) over 8x8 luma windows
/// (falling back to a full-frame window when either side is smaller than `8` pixels).
///
/// # Errors
/// Returns [`FileFusionError`] on dimension or length mismatch.
#[allow(clippy::cast_precision_loss)]
pub fn compute_ssim(
    width: usize,
    height: usize,
    channels: usize,
    orig: &[u8],
    candidate: &[u8],
) -> FileFusionResult<f64> {
    let _ = validate_image_buffers(width, height, channels, orig, candidate)?;
    let y1 = to_luma_plane(orig, channels);
    let y2 = to_luma_plane(candidate, channels);

    let win = 8usize.min(width).min(height).max(1);
    let c1 = (0.01_f64 * 255.0).powi(2);
    let c2 = (0.03_f64 * 255.0).powi(2);
    let step = if width >= 32 && height >= 32 { 4 } else { 1 };

    let mut ssim_total = 0.0_f64;
    let mut windows = 0u64;

    let mut y0 = 0usize;
    while y0 + win <= height {
        let mut x0 = 0usize;
        while x0 + win <= width {
            ssim_total += window_ssim(&y1, &y2, width, x0, y0, win, c1, c2);
            windows = windows.saturating_add(1);
            x0 += step;
        }
        y0 += step;
    }

    if windows == 0 {
        return Ok(window_ssim(&y1, &y2, width, 0, 0, win, c1, c2));
    }
    Ok((ssim_total / (windows as f64)).clamp(0.0, 1.0))
}

#[allow(clippy::too_many_arguments, clippy::cast_precision_loss)]
fn window_ssim(
    y1: &[f64],
    y2: &[f64],
    stride: usize,
    x0: usize,
    y0: usize,
    win: usize,
    c1: f64,
    c2: f64,
) -> f64 {
    let n = (win * win) as f64;
    let mut sum_a = 0.0_f64;
    let mut sum_b = 0.0_f64;
    for row_off in 0..win {
        let row = (y0 + row_off) * stride + x0;
        for col_off in 0..win {
            sum_a += y1[row + col_off];
            sum_b += y2[row + col_off];
        }
    }
    let mean_a = sum_a / n;
    let mean_b = sum_b / n;

    let mut var_a = 0.0_f64;
    let mut var_b = 0.0_f64;
    let mut cov_ab = 0.0_f64;
    for row_off in 0..win {
        let row = (y0 + row_off) * stride + x0;
        for col_off in 0..win {
            let centered_a = y1[row + col_off] - mean_a;
            let centered_b = y2[row + col_off] - mean_b;
            var_a += centered_a * centered_a;
            var_b += centered_b * centered_b;
            cov_ab += centered_a * centered_b;
        }
    }
    let denom_n = if n > 1.0 { n - 1.0 } else { 1.0 };
    var_a /= denom_n;
    var_b /= denom_n;
    cov_ab /= denom_n;

    let num = (2.0 * mean_a * mean_b + c1) * (2.0 * cov_ab + c2);
    let den = (mean_a * mean_a + mean_b * mean_b + c1) * (var_a + var_b + c2);
    if den.abs() <= f64::EPSILON {
        1.0
    } else {
        (num / den).clamp(0.0, 1.0)
    }
}

/// Evaluates raw uncompressed pixel buffers into canonical [`QualityMetrics`].
///
/// # Errors
/// Returns [`FileFusionError`] if dimensions or buffer sizes are invalid.
pub fn evaluate_pixel_buffers(
    width: usize,
    height: usize,
    channels: usize,
    orig: &[u8],
    candidate: &[u8],
) -> FileFusionResult<QualityMetrics> {
    let ssim = compute_ssim(width, height, channels, orig, candidate)?;
    let psnr_db = compute_psnr_db(width, height, channels, orig, candidate)?;
    let bit_exact = orig == candidate;
    Ok(QualityMetrics {
        ssim: Some(ssim),
        psnr_db: Some(psnr_db),
        vmaf: None,
        bit_exact_lossless: Some(bit_exact),
    })
}

/// Streams two files using bounded chunks (`64 KiB`–`1 MiB`) checking cooperative
/// cancellation to determine exact byte-level equivalence (`NFR-PERF-001`).
///
/// # Errors
/// Returns [`FileFusionError`] on file I/O failure or cancellation (`ErrorCode::JobCancelled`).
pub fn files_are_bit_exact(
    path_a: &Path,
    path_b: &Path,
    ctx: &JobContext,
) -> FileFusionResult<bool> {
    ctx.check_cancelled()?;
    let meta_a = std::fs::metadata(path_a)?;
    let meta_b = std::fs::metadata(path_b)?;
    if meta_a.len() != meta_b.len() {
        return Ok(false);
    }

    let chunk_len = ctx.limits.recommended_chunk_bytes();
    let mut file_a = File::open(path_a)?;
    let mut file_b = File::open(path_b)?;
    let mut buf_a = vec![0u8; chunk_len];
    let mut buf_b = vec![0u8; chunk_len];

    loop {
        ctx.check_cancelled()?;
        let n_a = file_a.read(&mut buf_a)?;
        if n_a == 0 {
            let n_b = file_b.read(&mut buf_b[..1])?;
            return Ok(n_b == 0);
        }
        file_b.read_exact(&mut buf_b[..n_a])?;
        if buf_a[..n_a] != buf_b[..n_a] {
            return Ok(false);
        }
    }
}

/// Default L1 [`QualityEvaluator`] performing streaming bit-exact file verification
/// (`FF-CTR-001` §3.3).
#[derive(Debug, Clone, Copy, Default)]
pub struct BitExactQualityEvaluator;

impl QualityEvaluator for BitExactQualityEvaluator {
    fn evaluator_id(&self) -> &'static str {
        "quality-engine-bit-exact"
    }

    fn evaluate(
        &self,
        original: &ValidatedSource,
        processed_path: &Path,
        ctx: &JobContext,
    ) -> FileFusionResult<QualityMetrics> {
        let exact = files_are_bit_exact(&original.path, processed_path, ctx)?;
        Ok(QualityMetrics {
            ssim: None,
            psnr_db: None,
            vmaf: None,
            bit_exact_lossless: Some(exact),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::fs;
    use std::path::PathBuf;

    use file_core::{ResourceLimits, inspect_path};

    use super::*;

    fn temp_qual_dir(label: &str) -> Result<PathBuf, Box<dyn Error>> {
        let dir =
            std::env::temp_dir().join(format!("filefusion_qual_{label}_{}", std::process::id()));
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    #[test]
    fn identical_and_perturbed_pixel_buffers_yield_expected_ssim_and_psnr() -> FileFusionResult<()>
    {
        let width = 16usize;
        let height = 16usize;
        let channels = 3usize;
        let mut orig = Vec::with_capacity(width * height * channels);
        for row in 0..height {
            for col in 0..width {
                let red = u8::try_from((col * 15) % 255).unwrap_or(0);
                let green = u8::try_from((row * 15) % 255).unwrap_or(0);
                let blue = u8::try_from(((col + row) * 7) % 255).unwrap_or(0);
                orig.extend_from_slice(&[red, green, blue]);
            }
        }

        let exact = evaluate_pixel_buffers(width, height, channels, &orig, &orig)?;
        assert_eq!(exact.bit_exact_lossless, Some(true));
        assert!((exact.ssim.unwrap_or(0.0) - 1.0).abs() < 1e-6);
        assert!((exact.psnr_db.unwrap_or(0.0) - LOSSLESS_PSNR_CEILING_DB).abs() < 1e-6);

        let mut noisy = orig.clone();
        for (idx, px) in noisy.iter_mut().enumerate() {
            if idx % 3 == 0 {
                *px = px.saturating_add(4);
            }
        }
        let degraded = evaluate_pixel_buffers(width, height, channels, &orig, &noisy)?;
        assert_eq!(degraded.bit_exact_lossless, Some(false));
        let ssim = degraded.ssim.unwrap_or(0.0);
        let psnr = degraded.psnr_db.unwrap_or(0.0);
        assert!(ssim > 0.90 && ssim < 1.0);
        assert!(psnr > 30.0 && psnr < LOSSLESS_PSNR_CEILING_DB);

        let summary = diff_pixel_buffers(width, height, channels, &orig, &noisy)?;
        assert!(!summary.bit_exact);
        assert_eq!(summary.max_channel_delta, 4);
        assert_eq!(
            summary.differing_pixels,
            u64::try_from(width * height).unwrap_or(0)
        );
        Ok(())
    }

    #[test]
    fn bit_exact_quality_evaluator_streams_files_and_checks_cancellation()
    -> Result<(), Box<dyn Error>> {
        let dir = temp_qual_dir("stream_cmp")?;
        let orig_path = dir.join("orig.bin");
        let same_path = dir.join("same.bin");
        let diff_path = dir.join("diff.bin");
        let payload = vec![0x5Au8; 100_000];
        let mut modified = payload.clone();
        modified[99_999] = 0x5B;

        fs::write(&orig_path, &payload)?;
        fs::write(&same_path, &payload)?;
        fs::write(&diff_path, &modified)?;

        let limits = ResourceLimits::default();
        let source = ValidatedSource {
            path: orig_path.clone(),
            inspection: inspect_path(&orig_path, false)?,
        };
        let ctx = JobContext::new("qual_01", limits);
        let evaluator = BitExactQualityEvaluator;

        let same_metrics = evaluator.evaluate(&source, &same_path, &ctx)?;
        assert_eq!(same_metrics.bit_exact_lossless, Some(true));

        let diff_metrics = evaluator.evaluate(&source, &diff_path, &ctx)?;
        assert_eq!(diff_metrics.bit_exact_lossless, Some(false));

        ctx.cancellation.cancel();
        let aborted = evaluator.evaluate(&source, &same_path, &ctx);
        assert!(aborted.is_err());
        if let Err(err) = aborted {
            assert_eq!(err.diagnostic.code, ErrorCode::JobCancelled);
        }

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }
}
