//! `optimization-engine`: L4 adaptive codec intelligence, quality-constrained CRF/quantizer
//! binary solver, FastCDC/pHash storage analyzer, and unified L3 compression engine dispatcher
//! (`FR-ADP-001`..`002`, `FR-CMP-003`, `FR-DUP-002`, `FF-MOD-001` §4).

#![forbid(unsafe_code)]

pub mod dedup;
pub mod dispatcher;
pub mod metadata;
pub mod recommender;
pub mod repair;
pub mod security;
pub mod solver;
pub mod storage;

pub use compression_archive::{
    FFENC_MAGIC, FfencCipherSuite, FfencHeaderInfo, FfencKdfParams, ZipHealthReport,
    ZipRepairReport, decrypt_ffenc_bytes, encrypt_ffenc_bytes, inspect_ffenc_header,
    inspect_zip_health,
};
#[cfg(feature = "native-process")]
pub use compression_pdf::TesseractAdapter;
pub use compression_pdf::{
    ExtractedPdfImage, OcrDocumentResult, OcrOptions, OcrPageResult, OcrWordBox, PdfCropMargins,
    PdfEncryptionAlgorithm, PdfHealthReport, PdfPermissions, PdfProtectionOptions,
    PdfRepairAssessment, PdfScanInspection, PdfSecurityReport, PdfSignatureInfo,
    PdfSignatureReport, PdfSignatureStatus, PdfWatermarkOptions, TesseractOutputMode,
    apply_pdf_text_watermark, build_simple_text_pdf_bytes, crop_pdf_pages,
    extract_or_reorder_pdf_pages, inspect_pdf_health, inspect_pdf_scan_status,
    inspect_pdf_security, merge_pdfs, parse_page_range_spec, rotate_pdf_pages, split_pdf_by_ranges,
    split_pdf_every_n_pages, verify_pdf_signatures,
};
pub use dedup::{
    CdcChunk, CdcDeduplicationReport, FastCdcConfig, PerceptualHashSet, analyze_files_fastcdc,
    are_perceptually_similar, chunk_bytes_fastcdc, chunk_stream_fastcdc, compute_image_ahash,
    compute_image_dhash, compute_image_phash, compute_perceptual_hash_set, hamming_distance_u64,
};
pub use dispatcher::{
    ArchiveContainerKind, ArchiveExtractionReport, ArchiveInputEntry, ArchiveMemberMetadata,
    ArchiveWriteSummary, OptimizationDispatcher, StreamCodec, archive_kind_from_output_path,
    collect_archive_input_entries, compress_stream_bytes, create_archive_from_entries,
    extract_archive_safely, list_archive_members,
};
pub use metadata::{
    EditableMetadataFields, MetadataOperationReport, inspect_unified_metadata,
    sanitize_and_edit_metadata_to_staged,
};
pub use recommender::{CodecRecommendation, RecommendationHints, recommend_codec};
#[cfg(feature = "native-process")]
pub use repair::repair_file_to_staged;
pub use repair::{
    RecoverabilityClass, UnifiedRepairDiagnosis, UnifiedRepairOutcome,
    diagnose_file_recoverability, repair_file_to_staged_tier1,
};
pub use security::{
    decrypt_ffenc_file_to_staged, decrypt_pdf_file_to_staged, encrypt_file_to_ffenc_staged,
    encrypt_pdf_file_to_staged, inspect_ffenc_file,
};
pub use solver::{
    QualityTargetConstraint, QuantizerProbeSample, SolverResult, SolverTrial, solve_and_stage_jpeg,
    solve_jpeg_quality_target, solve_quantizer_binary_search,
};
pub use storage::{
    DirectoryUsageNode, DomainStorageBreakdown, DuplicateScanOptions, ExactDuplicateGroup,
    ExactDuplicateScanReport, FormatStorageBreakdown, LargestFileItem, NearDuplicateCluster,
    NearDuplicateOptions, NearDuplicateScanReport, SimilarityMember, StorageTreemapReport,
    analyze_storage_treemap, scan_exact_duplicates, scan_near_duplicate_images,
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

    #[test]
    fn diagnoses_and_repairs_truncated_pdf_zip_and_unfinalized_mp4() -> Result<(), Box<dyn Error>> {
        let dir = unique_temp_dir("repair_orch");
        let ctx = JobContext::new("opt-repair", ResourceLimits::default());

        // 1. Truncated PDF missing trailing startxref / %%EOF -> diagnosed FullyRepairable & repaired natively.
        let healthy_pdf = compression_pdf::build_simple_text_pdf_bytes(&["Stage 4 Recovery"]);
        let cut_pos = healthy_pdf
            .windows(4)
            .position(|w| w == b"xref")
            .ok_or("missing xref in generated PDF")?;
        let broken_pdf_path = dir.join("broken.pdf");
        let fixed_pdf_path = dir.join("fixed.pdf");
        fs::write(&broken_pdf_path, &healthy_pdf[..cut_pos])?;

        let diag_pdf = diagnose_file_recoverability(&broken_pdf_path, &ctx)?;
        assert_eq!(diag_pdf.detected_format, FormatId::Pdf);
        assert_eq!(
            diag_pdf.recoverability,
            RecoverabilityClass::FullyRepairable
        );

        let staged_pdf = StagedPath::create_sibling(&fixed_pdf_path)?;
        let pdf_out = repair_file_to_staged_tier1(
            &broken_pdf_path,
            &staged_pdf,
            &ResourceLimits::default(),
            &ctx,
        )?;
        assert_eq!(pdf_out.detected_format, FormatId::Pdf);
        let post_pdf = compression_pdf::inspect_pdf_health(&fs::read(staged_pdf.staged_path())?);
        assert_eq!(post_pdf.assessment, PdfRepairAssessment::Healthy);

        // 2. Truncated ZIP missing EOCD -> diagnosed & central directory rebuilt.
        let good_zip = dir.join("good.zip");
        create_archive_from_entries(
            &[ArchiveInputEntry {
                path: String::from("hello.txt"),
                data: b"hello zip repair".to_vec(),
            }],
            &good_zip,
            ArchiveContainerKind::Zip,
            CompressionProfile::BalancedOptimal,
            &ctx,
        )?;
        let raw_zip = fs::read(&good_zip)?;
        let broken_zip_path = dir.join("truncated.zip");
        let fixed_zip_path = dir.join("fixed.zip");
        fs::write(&broken_zip_path, &raw_zip[..raw_zip.len() / 2 + 12])?;

        let diag_zip = diagnose_file_recoverability(&broken_zip_path, &ctx)?;
        assert_eq!(diag_zip.detected_format, FormatId::Zip);
        assert_ne!(diag_zip.recoverability, RecoverabilityClass::Healthy);

        // 3. Unfinalized MP4 (ftyp + mdat without moov) -> diagnosed FullyRepairable & gated on native FFmpeg.
        let broken_mp4 = dir.join("unfinalized.mp4");
        let mut mp4_bytes = Vec::new();
        mp4_bytes.extend_from_slice(&24u32.to_be_bytes());
        mp4_bytes.extend_from_slice(b"ftypisom");
        mp4_bytes.extend_from_slice(&0u32.to_be_bytes());
        mp4_bytes.extend_from_slice(b"isommp41");
        mp4_bytes.extend_from_slice(&16u32.to_be_bytes());
        mp4_bytes.extend_from_slice(b"mdat");
        mp4_bytes.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        fs::write(&broken_mp4, &mp4_bytes)?;

        let diag_mp4 = diagnose_file_recoverability(&broken_mp4, &ctx)?;
        assert_eq!(diag_mp4.detected_format, FormatId::Mp4);
        assert_eq!(
            diag_mp4.recoverability,
            RecoverabilityClass::FullyRepairable
        );
        assert!(diag_mp4.media_index_issue.is_some());

        // 4. Random noise -> Unrecoverable.
        let garbage = dir.join("garbage.bin");
        fs::write(&garbage, b"RANDOM_NON_CONTAINER_ENTROPY_BYTES")?;
        let diag_garbage = diagnose_file_recoverability(&garbage, &ctx)?;
        assert_eq!(
            diag_garbage.recoverability,
            RecoverabilityClass::Unrecoverable
        );

        drop(staged_pdf);
        let _ = fixed_zip_path;
        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn sanitizes_edits_metadata_and_encrypts_ffenc_roundtrip() -> Result<(), Box<dyn Error>> {
        let dir = unique_temp_dir("meta_ffenc");
        let ctx = JobContext::new("opt-meta", ResourceLimits::default());

        let pdf_in = dir.join("doc.pdf");
        let pdf_out = dir.join("doc_clean.pdf");
        let raw_pdf = compression_pdf::build_simple_text_pdf_bytes(&["Metadata Body"]);
        fs::write(&pdf_in, &raw_pdf)?;

        let staged_meta = StagedPath::create_sibling(&pdf_out)?;
        let edits = EditableMetadataFields {
            title: Some(Some(String::from("Public Report"))),
            author: Some(Some(String::from("FileFusion Release"))),
            ..EditableMetadataFields::default()
        };
        let meta_rep = sanitize_and_edit_metadata_to_staged(
            &pdf_in,
            &staged_meta,
            true,
            Some(&edits),
            &ResourceLimits::default(),
            &ctx,
        )?;
        assert_eq!(meta_rep.format, FormatId::Pdf);
        assert_eq!(meta_rep.after.title.as_deref(), Some("Public Report"));
        assert_eq!(meta_rep.after.author.as_deref(), Some("FileFusion Release"));

        // Round-trip .ffenc container staging & header inspection.
        let enc_path = dir.join("doc.ffenc");
        let dec_path = dir.join("doc_dec.pdf");
        let staged_enc = StagedPath::create_sibling(&enc_path)?;
        let _ = encrypt_file_to_ffenc_staged(
            staged_meta.staged_path(),
            &staged_enc,
            b"vault-pass-42",
            FfencCipherSuite::ChaCha20Poly1305,
            FfencKdfParams::fast_test(),
            &ctx,
        )?;
        let hdr = inspect_ffenc_file(staged_enc.staged_path(), &ctx)?;
        assert_eq!(hdr.cipher_suite, FfencCipherSuite::ChaCha20Poly1305);

        let staged_dec = StagedPath::create_sibling(&dec_path)?;
        let dec_len = decrypt_ffenc_file_to_staged(
            staged_enc.staged_path(),
            &staged_dec,
            b"vault-pass-42",
            &ctx,
        )?;
        assert_eq!(dec_len, meta_rep.output_bytes);
        assert_eq!(
            fs::read(staged_dec.staged_path())?,
            fs::read(staged_meta.staged_path())?
        );

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn streaming_fastcdc_matches_slice_cuts_and_scans_multistage_exact_dups()
    -> Result<(), Box<dyn Error>> {
        let dir = unique_temp_dir("stage5_exact_dup");
        let sub = dir.join("nested");
        let quarantine = dir.join(".filefusion-quarantine");
        fs::create_dir_all(&sub)?;
        fs::create_dir_all(&quarantine)?;
        let ctx = JobContext::new("opt-s5-exact", ResourceLimits::default());

        // Pseudo-random deterministic 140 KB payload to test multi-chunk streaming FastCDC parity.
        let mut payload = vec![0u8; 140 * 1024];
        let mut rng = 0x9E37_79B9_7F4A_7C15u64;
        for b in &mut payload {
            rng = rng.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            *b = u8::try_from((rng >> 56) & 0xff).unwrap_or(0);
        }
        let slice_chunks = chunk_bytes_fastcdc(&payload, FastCdcConfig::default());
        let mut cursor = std::io::Cursor::new(&payload);
        let mut stream_chunks = Vec::new();
        chunk_stream_fastcdc(&mut cursor, FastCdcConfig::default(), &ctx, |c| {
            stream_chunks.push(c);
            Ok(())
        })?;
        assert_eq!(slice_chunks, stream_chunks);

        // 1. Exact duplicate triplet (24 KB each).
        let dup_payload = payload[..24 * 1024].to_vec();
        fs::write(dir.join("dup_a.bin"), &dup_payload)?;
        fs::write(sub.join("dup_b.bin"), &dup_payload)?;
        fs::write(sub.join("dup_c.bin"), &dup_payload)?;
        // Quarantined copy must be ignored by default.
        fs::write(quarantine.join("dup_quarantined.bin"), &dup_payload)?;

        // 2. Same 24 KB size & identical 4 KB head+tail, differing only in middle byte -> Stage 3 prune.
        let mut mid_diff = dup_payload.clone();
        mid_diff[12 * 1024] ^= 0xff;
        fs::write(dir.join("same_ends_diff_mid.bin"), &mid_diff)?;

        // 3. Two 24 KB files differing only in final byte -> Stage 2 partial hash prune.
        let mut tail_diff = dup_payload.clone();
        tail_diff[24 * 1024 - 1] ^= 0x55;
        fs::write(dir.join("diff_tail.bin"), &tail_diff)?;

        // 4. Unique-size 30 KB file -> Stage 1 size prune (0 reads).
        fs::write(dir.join("unique_30k.bin"), &payload[..30 * 1024])?;
        // 5. Zero-byte stub skipped by default min_file_bytes=1.
        fs::write(dir.join("empty.bin"), b"")?;

        let report =
            scan_exact_duplicates(&[dir.as_path()], &DuplicateScanOptions::default(), &ctx)?;
        assert_eq!(report.files_scanned, 6);
        assert_eq!(report.stage1_size_candidates, 5);
        assert_eq!(report.stage2_partial_candidates, 4);
        assert_eq!(report.files_full_hashed, 4);
        assert!(report.bytes_actually_read < report.total_bytes_scanned);
        assert_eq!(report.duplicate_groups.len(), 1);
        assert_eq!(report.total_duplicate_files, 2);
        assert_eq!(report.total_wasted_bytes, 48 * 1024);
        assert_eq!(report.duplicate_groups[0].member_paths.len(), 3);

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn clusters_perceptual_near_duplicates_and_computes_storage_treemap()
    -> Result<(), Box<dyn Error>> {
        let dir = unique_temp_dir("stage5_near_treemap");
        let media = dir.join("media");
        let docs = dir.join("docs");
        fs::create_dir_all(&media)?;
        fs::create_dir_all(&docs)?;
        let ctx = JobContext::new("opt-s5-near", ResourceLimits::default());

        // Smooth landscape gradient PNG + resized JPEG variant should cluster together.
        let base_img = gradient_rgba_image(128, 96);
        let base_png = media.join("hero_full.png");
        let small_jpg = media.join("hero_thumb.jpg");
        base_img.save(&base_png)?;
        base_img
            .resize_exact(64, 48, image::imageops::FilterType::Lanczos3)
            .to_rgb8()
            .save(&small_jpg)?;

        // Visually orthogonal radial bullseye pattern must stay distinct.
        let mut distinct = RgbaImage::new(128, 96);
        for y in 0..96 {
            for x in 0..128 {
                let cx = i32::try_from(x).unwrap_or(0) - 64;
                let cy = i32::try_from(y).unwrap_or(0) - 48;
                let ring = ((cx * cx + cy * cy) / 48) % 2 == 0;
                let v = if ring { 250 } else { 15 };
                distinct.put_pixel(x, y, Rgba([v, 255 - v, v, 255]));
            }
        }
        DynamicImage::ImageRgba8(distinct).save(media.join("bullseye.png"))?;

        // Docs + zero-byte entry for treemap analytics.
        fs::write(docs.join("notes.json"), br#"{"stage":5,"ok":true}"#)?;
        fs::write(docs.join("empty.txt"), b"")?;

        let near =
            scan_near_duplicate_images(&[dir.as_path()], &NearDuplicateOptions::default(), &ctx)?;
        assert_eq!(near.images_analyzed, 3);
        assert_eq!(near.clusters.len(), 1);
        assert_eq!(
            near.clusters[0].keeper_path,
            base_png.to_string_lossy().as_ref()
        );
        assert_eq!(near.clusters[0].members.len(), 2);

        let tree =
            analyze_storage_treemap(&[dir.as_path()], &DuplicateScanOptions::default(), 3, &ctx)?;
        assert_eq!(tree.total_files, 5);
        assert_eq!(tree.zero_byte_files, 1);
        assert_eq!(tree.largest_files.len(), 3);
        assert_eq!(tree.treemap_roots.len(), 1);
        assert_eq!(tree.treemap_roots[0].file_count, 5);
        assert!(tree.treemap_roots[0].children.len() >= 2);
        assert_ne!(tree.domain_breakdown.len(), 0);

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }
}
