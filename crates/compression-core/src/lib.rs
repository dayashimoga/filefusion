//! `compression-core`: L2 shared compression orchestration (`FR-CMP-001`..`002`,
//! `FF-CTR-001` §3.1, `ADR-007`).
//!
//! Provides reproducible preset parameters across the 5 standard [`CompressionProfile`]
//! tiers, pre-flight high-entropy short-circuiting, mandatory **No-Op / inflation guard**
//! enforcement (`NoOpRetainedOriginal`), `OutputValidator` staging verification, and
//! atomic destination commits.

#![forbid(unsafe_code)]

use std::fs;
use std::path::PathBuf;

use file_core::{
    CollisionPolicy, CompressibilityTier, CompressionOutcome, CompressionProfile,
    CompressionRequest, CompressionStatus, ErrorCode, FileFusionError, FileFusionResult, FormatId,
    JobContext, OutputValidator, QualityEvaluator, QualityMetrics, StagedPath, ValidatedSource,
};
use serde::{Deserialize, Serialize};

/// Deterministic codec tuning parameters derived from a [`CompressionProfile`] (`FR-CMP-001`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CompressionProfileConfig {
    /// Selected standard preset.
    pub profile: CompressionProfile,
    /// Prefer bit-exact lossless coding when supported by target container.
    pub lossless_preferred: bool,
    /// Normalized encoder effort (`1` = fastest, `10` = maximum density).
    pub effort_level: u8,
    /// Normalized lossy quality target (`0..=100`) when lossy quantization is active.
    pub lossy_quality_q: u8,
    /// Recommended perceptual SSIM guardrail (`0.0..=1.0`).
    pub min_ssim_floor: f64,
    /// Strip non-essential metadata (EXIF/GPS/author) unless overridden by caller.
    pub strip_metadata_default: bool,
}

impl CompressionProfileConfig {
    /// Resolves canonical preset settings for `profile` (`FR-CMP-001`).
    #[must_use]
    pub const fn for_profile(profile: CompressionProfile) -> Self {
        match profile {
            CompressionProfile::LosslessMaximum => Self {
                profile,
                lossless_preferred: true,
                effort_level: 9,
                lossy_quality_q: 100,
                min_ssim_floor: 1.0,
                strip_metadata_default: false,
            },
            CompressionProfile::BalancedOptimal => Self {
                profile,
                lossless_preferred: false,
                effort_level: 6,
                lossy_quality_q: 84,
                min_ssim_floor: 0.98,
                strip_metadata_default: false,
            },
            CompressionProfile::WebSharingReady => Self {
                profile,
                lossless_preferred: false,
                effort_level: 6,
                lossy_quality_q: 78,
                min_ssim_floor: 0.96,
                strip_metadata_default: true,
            },
            CompressionProfile::ExtremeSizeTarget => Self {
                profile,
                lossless_preferred: false,
                effort_level: 8,
                lossy_quality_q: 62,
                min_ssim_floor: 0.92,
                strip_metadata_default: true,
            },
            CompressionProfile::ArchivalColdStorage => Self {
                profile,
                lossless_preferred: true,
                effort_level: 10,
                lossy_quality_q: 95,
                min_ssim_floor: 0.99,
                strip_metadata_default: false,
            },
        }
    }
}

/// Checks whether a [`CompressionRequest`] keeps the same container format.
#[must_use]
pub fn is_same_format(input: &ValidatedSource, req: &CompressionRequest) -> bool {
    match req.target_format {
        None => true,
        Some(target) => target == input.inspection.detected_format,
    }
}

/// Returns a short-circuited [`CompressionOutcome`] (`NoOpRetainedOriginal`) when input is
/// already high-entropy / incompressible (`FR-CMP-002`) and same-format optimization is requested
/// with `no_op_if_larger == true`.
#[must_use]
pub fn prescreen_incompressible_noop(
    input: &ValidatedSource,
    req: &CompressionRequest,
    codec_used: impl Into<String>,
) -> Option<CompressionOutcome> {
    if !req.no_op_if_larger || !is_same_format(input, req) {
        return None;
    }
    let classification = input.inspection.classification.as_ref()?;
    if matches!(
        classification.compressibility,
        CompressibilityTier::AlreadyCompressedOrEncrypted
    ) {
        return Some(CompressionOutcome {
            status: CompressionStatus::NoOpRetainedOriginal,
            bytes_in: input.inspection.byte_size,
            bytes_out: input.inspection.byte_size,
            codec_used: codec_used.into(),
            quality: Some(QualityMetrics {
                ssim: None,
                psnr_db: None,
                vmaf: None,
                bit_exact_lossless: Some(true),
            }),
        });
    }
    None
}

/// Validates a staged output candidate via `validator`, enforces the mandatory **No-Op / Inflation
/// Guard** (`FR-CMP-002`), checks optional quality/byte-budget constraints (`FR-CMP-003`), and
/// returns the normalized [`CompressionOutcome`].
///
/// # Errors
/// Returns [`FileFusionError`] on cancellation, staged validation failure, or quality/size
/// constraint violation.
pub fn finalize_staged_compression(
    input: &ValidatedSource,
    staged_output: &StagedPath,
    req: &CompressionRequest,
    validator: &dyn OutputValidator,
    evaluator: Option<&dyn QualityEvaluator>,
    codec_used: impl Into<String>,
    ctx: &JobContext,
) -> FileFusionResult<CompressionOutcome> {
    ctx.check_cancelled()?;
    let target_fmt = req
        .target_format
        .unwrap_or(input.inspection.detected_format);
    let codec = codec_used.into();

    let validation =
        validator.validate_staged(staged_output.staged_path(), target_fmt, &ctx.limits)?;
    if !validation.valid {
        return Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            format!(
                "staged output validation failed for {}: {}",
                target_fmt.mime_type(),
                validation.detail
            ),
        )
        .with_remediation("Original source was preserved untouched."));
    }

    let staged_bytes = fs::metadata(staged_output.staged_path())?.len();
    let same_format = is_same_format(input, req);

    if same_format && req.no_op_if_larger && staged_bytes >= input.inspection.byte_size {
        return Ok(CompressionOutcome {
            status: CompressionStatus::NoOpRetainedOriginal,
            bytes_in: input.inspection.byte_size,
            bytes_out: input.inspection.byte_size,
            codec_used: codec,
            quality: Some(QualityMetrics {
                ssim: None,
                psnr_db: None,
                vmaf: None,
                bit_exact_lossless: Some(true),
            }),
        });
    }

    let quality = if let Some(eval) = evaluator {
        Some(eval.evaluate(input, staged_output.staged_path(), ctx)?)
    } else {
        None
    };

    if let (Some(floor), Some(metrics)) = (req.min_ssim, quality.as_ref()) {
        if let Some(measured_ssim) = metrics.ssim {
            if measured_ssim < floor {
                return Err(FileFusionError::new(
                    ErrorCode::FmtUnsupported,
                    format!(
                        "compressed output SSIM ({measured_ssim:.4}) fell below requested floor ({floor:.4})"
                    ),
                )
                .with_remediation("Use a higher quality profile such as BalancedOptimal or LosslessMaximum."));
            }
        }
    }

    if let Some(max_bytes) = req.target_max_bytes {
        if staged_bytes > max_bytes {
            return Err(FileFusionError::new(
                ErrorCode::ResMemory,
                format!(
                    "staged output size ({staged_bytes} bytes) exceeded target_max_bytes ({max_bytes} bytes)"
                ),
            )
            .with_remediation("Use ExtremeSizeTarget or relax target_max_bytes."));
        }
    }

    Ok(CompressionOutcome {
        status: CompressionStatus::Optimized,
        bytes_in: input.inspection.byte_size,
        bytes_out: staged_bytes,
        codec_used: codec,
        quality,
    })
}

/// Promotes `staged` atomically if [`CompressionStatus::Optimized`], or drops `staged` cleanly
/// leaving the original untouched if [`CompressionStatus::NoOpRetainedOriginal`] (`ADR-007`).
///
/// # Errors
/// Returns [`FileFusionError`] if atomic rename/backup fails.
pub fn commit_compression_stage(
    staged: StagedPath,
    outcome: &CompressionOutcome,
    collision_policy: CollisionPolicy,
) -> FileFusionResult<Option<PathBuf>> {
    match outcome.status {
        CompressionStatus::Optimized => Ok(Some(staged.promote(collision_policy)?)),
        CompressionStatus::NoOpRetainedOriginal => {
            drop(staged);
            Ok(None)
        }
    }
}

/// Resolves target output [`FormatId`] for a compression request.
#[must_use]
pub fn effective_target_format(input: &ValidatedSource, req: &CompressionRequest) -> FormatId {
    req.target_format
        .unwrap_or(input.inspection.detected_format)
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::io::Write;
    use std::path::PathBuf;

    use file_core::{CompressionProfile, ProcessingDomain, ResourceLimits, inspect_path};
    use quality_engine::BitExactQualityEvaluator;
    use validation_engine::MagicByteOutputValidator;

    use super::*;

    fn temp_cmp_dir(label: &str) -> Result<PathBuf, Box<dyn Error>> {
        let dir = std::env::temp_dir().join(format!(
            "filefusion_cmp_core_{label}_{}",
            std::process::id()
        ));
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    #[test]
    fn profile_configs_cover_all_five_presets_monotonically() {
        let lossless = CompressionProfileConfig::for_profile(CompressionProfile::LosslessMaximum);
        let balanced = CompressionProfileConfig::for_profile(CompressionProfile::BalancedOptimal);
        let web = CompressionProfileConfig::for_profile(CompressionProfile::WebSharingReady);
        let extreme = CompressionProfileConfig::for_profile(CompressionProfile::ExtremeSizeTarget);
        let archival =
            CompressionProfileConfig::for_profile(CompressionProfile::ArchivalColdStorage);

        assert!(lossless.lossless_preferred);
        assert!(archival.lossless_preferred);
        assert!(!balanced.lossless_preferred);
        assert!(balanced.lossy_quality_q > web.lossy_quality_q);
        assert!(web.lossy_quality_q > extreme.lossy_quality_q);
        assert_eq!(archival.effort_level, 10);
    }

    #[test]
    fn noop_guard_retains_original_and_cleans_staged_when_inflated() -> Result<(), Box<dyn Error>> {
        let dir = temp_cmp_dir("noop_guard")?;
        let src = dir.join("sample.png");
        let dest = dir.join("sample_out.png");
        let valid_png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x06\x00\x00\x00\x1f\x15\xc4\x89";
        fs::write(&src, valid_png)?;

        let source = ValidatedSource {
            path: src.clone(),
            inspection: inspect_path(&src, false)?,
        };
        let limits = ResourceLimits::default();
        let ctx = JobContext::new("cmp_noop", limits);
        let validator = MagicByteOutputValidator;
        let eval = BitExactQualityEvaluator;

        // Write a valid but LARGER PNG payload into staged output.
        let staged = StagedPath::create_sibling(&dest)?;
        let staged_path_copy = staged.staged_path().to_path_buf();
        let mut bigger_png = valid_png.to_vec();
        bigger_png.extend_from_slice(&[0u8; 64]);
        fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&staged_path_copy)?
            .write_all(&bigger_png)?;

        let req = CompressionRequest {
            profile: CompressionProfile::BalancedOptimal,
            target_format: None,
            target_max_bytes: None,
            min_ssim: None,
            strip_metadata: false,
            no_op_if_larger: true,
        };

        let outcome = finalize_staged_compression(
            &source,
            &staged,
            &req,
            &validator,
            Some(&eval),
            "test-png",
            &ctx,
        )?;
        assert_eq!(outcome.status, CompressionStatus::NoOpRetainedOriginal);
        assert_eq!(outcome.bytes_out, source.inspection.byte_size);

        let committed =
            commit_compression_stage(staged, &outcome, CollisionPolicy::RenameWithSuffix)?;
        assert!(committed.is_none());
        assert!(!dest.exists());
        assert!(!staged_path_copy.exists());

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn smaller_validated_staged_output_promotes_atomically() -> Result<(), Box<dyn Error>> {
        let dir = temp_cmp_dir("promote_smaller")?;
        let src = dir.join("big.png");
        let dest = dir.join("big_opt.png");
        let mut big_png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x06\x00\x00\x00\x1f\x15\xc4\x89".to_vec();
        big_png.extend_from_slice(&[0u8; 128]);
        fs::write(&src, &big_png)?;

        let mut source = ValidatedSource {
            path: src.clone(),
            inspection: inspect_path(&src, false)?,
        };
        // Ensure prescreen knows domain for coverage check
        if let Some(cls) = source.inspection.classification.as_mut() {
            cls.domain = ProcessingDomain::RasterImage;
        }
        let limits = ResourceLimits::default();
        let ctx = JobContext::new("cmp_opt", limits);
        let validator = MagicByteOutputValidator;
        let eval = BitExactQualityEvaluator;

        let staged = StagedPath::create_sibling(&dest)?;
        let smaller_png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x06\x00\x00\x00\x1f\x15\xc4\x89";
        fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(staged.staged_path())?
            .write_all(smaller_png)?;

        let req = CompressionRequest {
            profile: CompressionProfile::LosslessMaximum,
            target_format: None,
            target_max_bytes: Some(64),
            min_ssim: None,
            strip_metadata: false,
            no_op_if_larger: true,
        };
        assert!(prescreen_incompressible_noop(&source, &req, "test-png").is_none());

        let outcome = finalize_staged_compression(
            &source,
            &staged,
            &req,
            &validator,
            Some(&eval),
            "test-png",
            &ctx,
        )?;
        assert_eq!(outcome.status, CompressionStatus::Optimized);
        assert_eq!(
            outcome.bytes_out,
            u64::try_from(smaller_png.len()).unwrap_or(0)
        );

        let committed =
            commit_compression_stage(staged, &outcome, CollisionPolicy::RenameWithSuffix)?;
        assert_eq!(committed, Some(dest.clone()));
        assert_eq!(fs::read(&dest)?, smaller_png);

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }
}
