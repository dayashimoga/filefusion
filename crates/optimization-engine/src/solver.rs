//! Quality-constrained CRF / quantizer binary-search solver (`FR-CMP-003`).
//!
//! Finds the lowest bitrate / smallest byte output parameter that still honors a minimum
//! perceptual SSIM/PSNR floor (`min_ssim`) or fits inside `target_max_bytes`.

use std::fs;

use file_core::{
    CollisionPolicy, CompressionOutcome, CompressionStatus, ErrorCode, FileFusionError,
    FileFusionResult, FormatId, JobContext, QualityMetrics, StagedPath, ValidatedSource,
};
use image::ColorType;
use image::codecs::jpeg::JpegEncoder;
use quality_engine::evaluate_pixel_buffers;
use serde::{Deserialize, Serialize};
use validation_engine::MagicByteOutputValidator;

/// Goal constraints supplied to the quality-constrained binary solver (`FR-CMP-003`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct QualityTargetConstraint {
    /// Minimum inclusive perceptual SSIM (`0.0..=1.0`, e.g. `0.97`).
    pub min_ssim: Option<f64>,
    /// Minimum inclusive PSNR in dB (`>= 0.0`).
    pub min_psnr_db: Option<f64>,
    /// Optional strict maximum output byte size ceiling.
    pub target_max_bytes: Option<u64>,
    /// Inclusive lower bound for monotonic quality/quantizer parameter (`10..=95`).
    pub min_param: u8,
    /// Inclusive upper bound for monotonic quality/quantizer parameter (`10..=95`).
    pub max_param: u8,
    /// Maximum binary-search probe iterations (typically `6..=8` for `0..=100`).
    pub max_iterations: u8,
}

impl Default for QualityTargetConstraint {
    fn default() -> Self {
        Self {
            min_ssim: Some(0.97),
            min_psnr_db: None,
            target_max_bytes: None,
            min_param: 35,
            max_param: 95,
            max_iterations: 7,
        }
    }
}

/// Sample returned by a single encoder probe at parameter `param`.
#[derive(Debug, Clone, PartialEq)]
pub struct QuantizerProbeSample<T> {
    /// Encoded output byte size.
    pub bytes_out: u64,
    /// Empirical SSIM against original (`0.0..=1.0`).
    pub ssim: Option<f64>,
    /// Empirical PSNR in dB against original.
    pub psnr_db: Option<f64>,
    /// Candidate artifact/bytes carried forward without re-encoding on win.
    pub payload: T,
}

/// Per-iteration telemetry trace recorded by [`solve_quantizer_binary_search`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SolverTrial {
    /// Zero-based iteration index.
    pub iteration: u8,
    /// Quality parameter probed.
    pub param: u8,
    /// Measured encoded bytes.
    pub bytes_out: u64,
    /// Measured SSIM if computed.
    pub ssim: Option<f64>,
    /// True if candidate satisfied all active constraints.
    pub constraints_met: bool,
}

/// Solved output from [`solve_quantizer_binary_search`].
#[derive(Debug, Clone, PartialEq)]
pub struct SolverResult<T> {
    /// Optimal quality parameter discovered.
    pub best_param: u8,
    /// Encoded byte count of `best_payload`.
    pub best_bytes: u64,
    /// Measured SSIM of `best_payload`.
    pub best_ssim: Option<f64>,
    /// Measured PSNR (dB) of `best_payload`.
    pub best_psnr_db: Option<f64>,
    /// True when at least one trial satisfied every requested constraint.
    pub target_satisfied: bool,
    /// Chronological probe trace.
    pub trials: Vec<SolverTrial>,
    /// Winning encoded payload.
    pub best_payload: T,
}

/// Generic monotonic binary-search solver (`FR-CMP-003`) assuming higher `param` yields
/// **higher quality and larger (or equal) file size** (e.g., JPEG quality `35..=95`).
///
/// Checks `ctx.check_cancelled()?` prior to every probe.
///
/// # Errors
/// Returns [`FileFusionError`] on invalid bounds, cancelled job, or encoder failure.
pub fn solve_quantizer_binary_search<T, F>(
    constraint: &QualityTargetConstraint,
    ctx: &JobContext,
    mut probe_fn: F,
) -> FileFusionResult<SolverResult<T>>
where
    F: FnMut(u8) -> FileFusionResult<QuantizerProbeSample<T>>,
{
    if constraint.min_param > constraint.max_param || constraint.max_iterations == 0 {
        return Err(FileFusionError::new(
            ErrorCode::IntInvariant,
            "invalid quality solver parameter bounds",
        ));
    }

    let mut lo = constraint.min_param;
    let mut hi = constraint.max_param;
    let mut trials = Vec::new();
    let mut best_feasible: Option<(u8, QuantizerProbeSample<T>)> = None;
    let mut best_fallback: Option<(u8, QuantizerProbeSample<T>)> = None;

    for iter in 0..constraint.max_iterations {
        ctx.check_cancelled()?;
        if lo > hi {
            break;
        }
        let mid = lo + (hi - lo) / 2;
        let sample = probe_fn(mid)?;
        let meets_quality = meets_quality_floors(&sample, constraint);
        let meets_size = constraint
            .target_max_bytes
            .is_none_or(|cap| sample.bytes_out <= cap);
        let feasible = meets_quality && meets_size;

        trials.push(SolverTrial {
            iteration: iter,
            param: mid,
            bytes_out: sample.bytes_out,
            ssim: sample.ssim,
            constraints_met: feasible,
        });

        if feasible {
            let replace = best_feasible
                .as_ref()
                .is_none_or(|(_, cur)| sample.bytes_out < cur.bytes_out);
            if replace {
                best_feasible = Some((mid, sample));
            }
            if mid == lo {
                break;
            }
            // Try lowering quality slightly for even smaller bytes if quality floor allows.
            hi = mid.saturating_sub(1);
        } else {
            let replace_fb = best_fallback
                .as_ref()
                .is_none_or(|(_, cur)| sample.ssim.unwrap_or(0.0) > cur.ssim.unwrap_or(0.0));
            if replace_fb {
                best_fallback = Some((mid, sample));
            }
            if meets_quality {
                // Quality passed, size exceeded `target_max_bytes`: move downward.
                if mid == lo {
                    break;
                }
                hi = mid.saturating_sub(1);
            } else {
                // Quality too low: move upward cleanly.
                if mid == hi {
                    break;
                }
                lo = mid.saturating_add(1);
            }
        }
    }

    let (target_satisfied, (best_param, winner)) = if let Some(found) = best_feasible {
        (true, found)
    } else if let Some(fb) = best_fallback {
        (false, fb)
    } else {
        return Err(FileFusionError::new(
            ErrorCode::IntInvariant,
            "quality solver executed zero probes",
        ));
    };

    Ok(SolverResult {
        best_param,
        best_bytes: winner.bytes_out,
        best_ssim: winner.ssim,
        best_psnr_db: winner.psnr_db,
        target_satisfied,
        trials,
        best_payload: winner.payload,
    })
}

fn meets_quality_floors<T>(
    sample: &QuantizerProbeSample<T>,
    constraint: &QualityTargetConstraint,
) -> bool {
    let ssim_ok = constraint
        .min_ssim
        .is_none_or(|floor| sample.ssim.is_some_and(|s| s >= floor));
    let psnr_ok = constraint
        .min_psnr_db
        .is_none_or(|floor| sample.psnr_db.is_some_and(|p| p >= floor));
    ssim_ok && psnr_ok
}

/// Real image quality-constrained solver sweeping `JPEG` quality `[min_param, max_param]` and
/// evaluating empirical pixel-exact BT.601 SSIM/PSNR via [`evaluate_pixel_buffers`] (`FR-CMP-003`).
///
/// # Errors
/// Returns [`FileFusionError`] on decode/encode error, missing constraint satisfaction when
/// strict, or cancellation.
pub fn solve_jpeg_quality_target(
    input_bytes: &[u8],
    constraint: &QualityTargetConstraint,
    ctx: &JobContext,
) -> FileFusionResult<SolverResult<Vec<u8>>> {
    let orig_img = image::load_from_memory(input_bytes).map_err(|err| {
        FileFusionError::new(
            ErrorCode::FmtCorrupt,
            format!("failed to decode source image for quality solver: {err}"),
        )
    })?;
    let w = usize::try_from(orig_img.width()).unwrap_or(1);
    let h = usize::try_from(orig_img.height()).unwrap_or(1);
    let orig_rgba = orig_img.to_rgba8().into_raw();
    let rgb = orig_img.to_rgb8();

    solve_quantizer_binary_search(constraint, ctx, |q| {
        let mut encoded = Vec::new();
        JpegEncoder::new_with_quality(&mut encoded, q)
            .encode(
                rgb.as_raw(),
                rgb.width(),
                rgb.height(),
                ColorType::Rgb8.into(),
            )
            .map_err(|err| {
                FileFusionError::new(
                    ErrorCode::IntInvariant,
                    format!("solver JPEG probe q={q} failed: {err}"),
                )
            })?;
        let decoded = image::load_from_memory(&encoded).map_err(|err| {
            FileFusionError::new(
                ErrorCode::IntInvariant,
                format!("solver failed to decode probe q={q}: {err}"),
            )
        })?;
        let metrics = evaluate_pixel_buffers(w, h, 4, &orig_rgba, decoded.to_rgba8().as_raw())?;
        Ok(QuantizerProbeSample {
            bytes_out: u64::try_from(encoded.len()).unwrap_or(u64::MAX),
            ssim: metrics.ssim,
            psnr_db: metrics.psnr_db,
            payload: encoded,
        })
    })
}

/// Stages and validates solved JPEG bytes directly onto `staged_output`, enforcing `FR-CMP-002`
/// No-Op retention when input is already `JPEG` and output does not shrink.
///
/// # Errors
/// Returns [`FileFusionError`] if no trial satisfies `constraint`, staged validation fails, or
/// job is cancelled.
pub fn solve_and_stage_jpeg(
    input: &ValidatedSource,
    staged_output: &StagedPath,
    constraint: &QualityTargetConstraint,
    no_op_if_larger: bool,
    ctx: &JobContext,
) -> FileFusionResult<(CompressionOutcome, SolverResult<Vec<u8>>)> {
    ctx.check_cancelled()?;
    let raw = fs::read(&input.path)?;
    let result = solve_jpeg_quality_target(&raw, constraint, ctx)?;
    if !result.target_satisfied {
        return Err(FileFusionError::new(
            ErrorCode::FmtUnsupported,
            "quality solver could not satisfy requested SSIM/byte-budget constraints",
        ));
    }

    if no_op_if_larger
        && input.inspection.detected_format == FormatId::Jpeg
        && result.best_bytes >= input.inspection.byte_size
    {
        let outcome = CompressionOutcome {
            status: CompressionStatus::NoOpRetainedOriginal,
            bytes_in: input.inspection.byte_size,
            bytes_out: input.inspection.byte_size,
            codec_used: format!("solver-jpeg-q{}", result.best_param),
            quality: Some(QualityMetrics {
                ssim: Some(1.0),
                psnr_db: Some(99.9),
                vmaf: None,
                bit_exact_lossless: Some(true),
            }),
        };
        return Ok((outcome, result));
    }

    fs::write(staged_output.staged_path(), &result.best_payload)?;
    let validator = MagicByteOutputValidator;
    let valid = file_core::OutputValidator::validate_staged(
        &validator,
        staged_output.staged_path(),
        FormatId::Jpeg,
        &ctx.limits,
    )?;
    if !valid.valid {
        return Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            format!("solved output validation failed: {}", valid.detail),
        ));
    }

    let outcome = CompressionOutcome {
        status: CompressionStatus::Optimized,
        bytes_in: input.inspection.byte_size,
        bytes_out: result.best_bytes,
        codec_used: format!("solver-jpeg-q{}", result.best_param),
        quality: Some(QualityMetrics {
            ssim: result.best_ssim,
            psnr_db: result.best_psnr_db,
            vmaf: None,
            bit_exact_lossless: Some(false),
        }),
    };
    Ok((outcome, result))
}

/// Convenience promotion wrapper forwarding `CollisionPolicy`.
#[must_use]
pub fn default_solver_collision_policy() -> CollisionPolicy {
    CollisionPolicy::OverwriteWithBackup
}
