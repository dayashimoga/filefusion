//! `optimization-engine`: L4 adaptive codec intelligence, quality-constrained CRF/quantizer
//! binary solver, FastCDC/pHash storage analyzer, and unified L3 compression engine dispatcher
//! (`FR-ADP-001`..`002`, `FR-CMP-003`, `FR-DUP-002`, `FF-MOD-001` §4).

#![forbid(unsafe_code)]

pub mod dedup;
pub mod dispatcher;
pub mod recommender;
pub mod solver;

pub use dedup::{
    CdcChunk, CdcDeduplicationReport, FastCdcConfig, analyze_files_fastcdc,
    are_perceptually_similar, chunk_bytes_fastcdc, compute_image_dhash, compute_image_phash,
    hamming_distance_u64,
};
pub use dispatcher::OptimizationDispatcher;
pub use recommender::{CodecRecommendation, RecommendationHints, recommend_codec};
pub use solver::{
    QualityTargetConstraint, QuantizerProbeSample, SolverResult, SolverTrial, solve_and_stage_jpeg,
    solve_jpeg_quality_target, solve_quantizer_binary_search,
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use file_core::{
        CORE_SCHEMA_VERSION, CompressionEngine, CompressionProfile, CompressionRequest,
        CompressionStatus, FormatId, JobContext, ResourceLimits, StagedPath, ValidatedSource,
        inspect_bytes, inspect_path,
    };
    use hardware_acceleration::{
        HardwareEncoderCapability, HardwareProfile, HardwareVendorBackend, SimdFeatures,
    };
    use image::{DynamicImage, GenericImageView, ImageEncoder, Rgba, RgbaImage};

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let dir = std::env::temp_dir().join(format!("ff_opt_{label}_{nanos}"));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    fn make_hw_profile(with_nvenc: bool) -> HardwareProfile {
        HardwareProfile {
            schema_version: CORE_SCHEMA_VERSION,
            arch: String::from("x86_64"),
            os: String::from("windows"),
            simd: SimdFeatures::detect(),
            encoders: if with_nvenc {
                vec![HardwareEncoderCapability {
                    backend: HardwareVendorBackend::Nvenc,
                    id: String::from("hw-nvenc"),
                    available: true,
                    supported_formats: vec![FormatId::Mp4, FormatId::Mkv],
                    evidence: String::from("mocked"),
                }]
            } else {
                Vec::new()
            },
            wasm_stub: false,
            fallback_to_cpu: !with_nvenc,
            note: String::from("unit test profile"),
        }
    }

    fn gradient_rgba_image(width: u32, height: u32) -> DynamicImage {
        let mut img = RgbaImage::new(width, height);
        for row_y in 0..height {
            for col_x in 0..width {
                let red = u8::try_from((col_x * 3) % 255).unwrap_or(0);
                let green = u8::try_from((row_y * 5) % 255).unwrap_or(0);
                let blue = u8::try_from(((col_x + row_y) * 2) % 255).unwrap_or(0);
                img.put_pixel(col_x, row_y, Rgba([red, green, blue, 255]));
            }
        }
        DynamicImage::ImageRgba8(img)
    }

    #[test]
    fn recommender_selects_adaptive_codecs_for_alpha_video_and_encrypted()
    -> Result<(), Box<dyn Error>> {
        // Synthetic PNG gradient triggers transparent/web image routing.
        let mut png_buf = Vec::new();
        let img = gradient_rgba_image(48, 48);
        image::codecs::png::PngEncoder::new(&mut png_buf).write_image(
            img.to_rgba8().as_raw(),
            48,
            48,
            image::ColorType::Rgba8.into(),
        )?;
        let png_rep = inspect_bytes(&png_buf, Some("ui.png"), true)?;
        let img_rec = recommend_codec(
            &png_rep,
            &RecommendationHints {
                profile: CompressionProfile::ExtremeSizeTarget,
                web_compatible: false,
                lossless_required: false,
                prefer_deterministic_cpu: false,
                strip_metadata: true,
            },
            &make_hw_profile(false),
        );
        assert_eq!(img_rec.target_engine_id, "tier1-compression-image");
        assert_eq!(img_rec.recommended_format, FormatId::Avif);
        assert!(!img_rec.should_skip_noop);

        // Video with NVENC vs deterministic CPU override (`FR-ADP-002`).
        let mut mp4_bytes = Vec::from(&20u32.to_be_bytes()[..]);
        mp4_bytes.extend_from_slice(b"ftypisom\x00\x00\x02\x00isom");
        mp4_bytes.resize(64, 0);
        let vid_rep = inspect_bytes(&mp4_bytes, Some("clip.mp4"), true)?;
        let hw_rec = recommend_codec(
            &vid_rep,
            &RecommendationHints {
                profile: CompressionProfile::BalancedOptimal,
                web_compatible: true,
                lossless_required: false,
                prefer_deterministic_cpu: false,
                strip_metadata: true,
            },
            &make_hw_profile(true),
        );
        assert_eq!(hw_rec.hardware_backend, Some(HardwareVendorBackend::Nvenc));
        assert_eq!(hw_rec.codec_label, "h264_nvenc");

        let cpu_rec = recommend_codec(
            &vid_rep,
            &RecommendationHints {
                prefer_deterministic_cpu: true,
                ..RecommendationHints::default()
            },
            &make_hw_profile(true),
        );
        assert_eq!(cpu_rec.hardware_backend, None);
        assert_eq!(cpu_rec.codec_label, "libx264");

        // Encrypted `.ffenc` container must recommend No-Op skip.
        let mut ffenc = b"FFENC\x01\x00".to_vec();
        ffenc.resize(64, 0xAA);
        let vault_inspection = inspect_bytes(&ffenc, Some("vault.ffenc"), true)?;
        assert_eq!(vault_inspection.detected_format, FormatId::Ffenc);
        let vault_rec = recommend_codec(
            &vault_inspection,
            &RecommendationHints::default(),
            &make_hw_profile(false),
        );
        assert!(vault_rec.should_skip_noop);
        Ok(())
    }

    #[test]
    fn quality_solver_converges_on_target_ssim_for_real_image() -> Result<(), Box<dyn Error>> {
        let dir = unique_temp_dir("solver");
        let src = dir.join("gradient.png");
        let dst = dir.join("solved.jpg");
        let img = gradient_rgba_image(96, 96);
        let (w, h) = img.dimensions();
        let mut png_bytes = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png_bytes).write_image(
            img.to_rgba8().as_raw(),
            w,
            h,
            image::ColorType::Rgba8.into(),
        )?;
        fs::write(&src, &png_bytes)?;

        let source = ValidatedSource {
            path: src.clone(),
            inspection: inspect_path(&src, false)?,
        };
        let ctx = JobContext::new("opt-solver", ResourceLimits::default());
        let staged = StagedPath::create_sibling(&dst)?;
        let constraint = QualityTargetConstraint {
            min_ssim: Some(0.96),
            min_psnr_db: None,
            target_max_bytes: None,
            min_param: 40,
            max_param: 95,
            max_iterations: 7,
        };

        let (outcome, solved) = solve_and_stage_jpeg(&source, &staged, &constraint, false, &ctx)?;
        assert!(solved.target_satisfied);
        assert_eq!(outcome.status, CompressionStatus::Optimized);
        assert!(solved.best_ssim.is_some_and(|s| s >= 0.96));
        assert_ne!(solved.trials.len(), 0);

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn fastcdc_and_perceptual_hashes_detect_subfile_and_resized_duplicates()
    -> Result<(), Box<dyn Error>> {
        // Two streams sharing a 48 KB pseudo-random block shifted by a 13-byte prefix difference.
        let mut shared = vec![0u8; 48 * 1024];
        let mut state = 0x1234_5678_9ABC_DEF0u64;
        for byte in &mut shared {
            state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut mixed = state;
            mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            *byte = u8::try_from((mixed >> 24) & 0xFF).unwrap_or(0);
        }
        let mut file_a = b"PREFIX_A".to_vec();
        file_a.extend_from_slice(&shared);
        let mut file_b = b"INSERTED_HEADER_B_123".to_vec();
        file_b.extend_from_slice(&shared);

        let dir = unique_temp_dir("fastcdc");
        let path_a = dir.join("a.bin");
        let path_b = dir.join("b.bin");
        fs::write(&path_a, &file_a)?;
        fs::write(&path_b, &file_b)?;

        let cfg = FastCdcConfig {
            min_size: 512,
            avg_size: 2048,
            max_size: 8192,
        };
        let ctx = JobContext::new("opt-cdc", ResourceLimits::default());
        let dedup = analyze_files_fastcdc(&[Path::new(&path_a), Path::new(&path_b)], cfg, &ctx)?;
        assert!(
            dedup.duplicate_bytes > 16 * 1024,
            "expected FastCDC to recover > 16 KB shared across shifted offset, got {}",
            dedup.duplicate_bytes
        );

        // Perceptual dHash & pHash stability across resolution scaling.
        let orig = gradient_rgba_image(128, 128);
        let scaled = orig.resize_exact(64, 64, image::imageops::FilterType::Lanczos3);
        let dh1 = compute_image_dhash(&orig);
        let dh2 = compute_image_dhash(&scaled);
        let ph1 = compute_image_phash(&orig);
        let ph2 = compute_image_phash(&scaled);
        assert!(are_perceptually_similar(dh1, dh2, 6));
        assert!(are_perceptually_similar(ph1, ph2, 10));

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn dispatcher_routes_and_compresses_multiple_domains() -> Result<(), Box<dyn Error>> {
        let dir = unique_temp_dir("dispatch");
        let src_png = dir.join("img.png");
        let dst_png = dir.join("img_out.png");
        let img = gradient_rgba_image(64, 64);
        let mut buf = Vec::new();
        image::codecs::png::PngEncoder::new_with_quality(
            &mut buf,
            image::codecs::png::CompressionType::Fast,
            image::codecs::png::FilterType::NoFilter,
        )
        .write_image(
            img.to_rgba8().as_raw(),
            64,
            64,
            image::ColorType::Rgba8.into(),
        )?;
        fs::write(&src_png, &buf)?;

        let source = ValidatedSource {
            path: src_png.clone(),
            inspection: inspect_path(&src_png, false)?,
        };
        let ctx = JobContext::new("opt-dispatch", ResourceLimits::default());
        let dispatcher = OptimizationDispatcher::new_tier1();
        let staged = StagedPath::create_sibling(&dst_png)?;
        let req = CompressionRequest {
            profile: CompressionProfile::LosslessMaximum,
            target_format: Some(FormatId::Png),
            target_max_bytes: None,
            min_ssim: Some(0.99),
            strip_metadata: true,
            no_op_if_larger: true,
        };
        let out = dispatcher.compress(&source, &staged, &req, &ctx)?;
        assert_eq!(out.status, CompressionStatus::Optimized);
        assert!(out.bytes_out < out.bytes_in);

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }
}
