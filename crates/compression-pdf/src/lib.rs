//! `compression-pdf`: L3 pure-Rust (`lopdf` + `flate2` + `compression-image`) and optional
//! Tier 2 `qpdf` / `tesseract` PDF stream/image compression, PDF Studio manipulation, OCR,
//! `AES-256` encryption, `/Sig` verification, metadata scrubbing, and structural repair
//! (`FR-PDF-001`..`004`, `FR-REP-001`..`002`, `FR-CMP-001`..`002`).

#![forbid(unsafe_code)]

pub mod ocr;
pub mod optimizer;
pub mod repair;
pub mod security;
pub mod studio;

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use compression_core::{
    effective_target_format, finalize_staged_compression, prescreen_incompressible_noop,
};
use file_core::{
    CompressionEngine, CompressionOutcome, CompressionRequest, ErrorCode, FileFusionError,
    FileFusionResult, FileInspectionReport, FormatId, JobContext, QualityEvaluator, QualityMetrics,
    StagedPath, ValidatedSource,
};
use lopdf::Document;
use quality_engine::BitExactQualityEvaluator;
use validation_engine::MagicByteOutputValidator;

#[cfg(feature = "native-process")]
use std::path::PathBuf;

#[cfg(feature = "native-process")]
use file_core::IsolatedProcessRunner;

pub use ocr::{
    ExtractedPdfImage, OcrDocumentResult, OcrOptions, OcrPageResult, OcrWordBox,
    TesseractOutputMode, build_searchable_pdf_from_image_and_ocr, embed_searchable_ocr_text_layer,
    extract_pdf_page_images, parse_tesseract_hocr, parse_tesseract_tsv,
};
pub use optimizer::{
    PdfCompressionOptions, PdfCompressionReport, optimize_pdf_bytes, update_pdf_metadata_bytes,
};
pub use repair::{
    PdfHealthReport, PdfRepairAssessment, QpdfMode, inspect_pdf_health, repair_pdf_bytes,
};
pub use security::{
    PdfEncryptionAlgorithm, PdfPermissions, PdfProtectionOptions, PdfSecurityReport,
    PdfSignatureInfo, PdfSignatureReport, PdfSignatureStatus, decrypt_pdf_bytes, encrypt_pdf_bytes,
    inspect_pdf_security, verify_pdf_signatures,
};
pub use studio::{
    PdfCropMargins, PdfScanInspection, PdfWatermarkOptions, apply_pdf_text_watermark,
    build_simple_text_pdf_bytes, crop_pdf_pages, extract_or_reorder_pdf_pages,
    inspect_pdf_scan_status, merge_pdfs, parse_page_range_spec, rotate_pdf_pages,
    split_pdf_by_ranges, split_pdf_every_n_pages,
};

#[cfg(feature = "native-process")]
pub use ocr::TesseractAdapter;
#[cfg(feature = "native-process")]
pub use repair::{build_qpdf_optimize_invocation, repair_pdf_to_staged_with_runner};
#[cfg(feature = "native-process")]
pub use security::{build_qpdf_decrypt_invocation, build_qpdf_encrypt_invocation};

/// Validates staged PDF output using [`MagicByteOutputValidator`].
pub type PdfOutputValidator = MagicByteOutputValidator;

/// Evaluates PDF structural fidelity (`page count` & extracted text parity) between source and
/// compressed candidate (`FR-FID-001`).
#[derive(Debug, Clone, Copy, Default)]
pub struct PdfQualityEvaluator;

impl QualityEvaluator for PdfQualityEvaluator {
    fn evaluator_id(&self) -> &'static str {
        "compression-pdf-structural"
    }

    fn evaluate(
        &self,
        original: &ValidatedSource,
        candidate: &Path,
        ctx: &JobContext,
    ) -> FileFusionResult<QualityMetrics> {
        ctx.check_cancelled()?;
        let orig_bytes = fs::read(&original.path)?;
        let cand_bytes = fs::read(candidate)?;

        let (Ok(orig_doc), Ok(cand_doc)) = (
            Document::load_mem(&orig_bytes),
            Document::load_mem(&cand_bytes),
        ) else {
            return BitExactQualityEvaluator.evaluate(original, candidate, ctx);
        };

        let orig_pages: Vec<u32> = orig_doc.get_pages().keys().copied().collect();
        let cand_pages: Vec<u32> = cand_doc.get_pages().keys().copied().collect();
        if orig_pages.len() != cand_pages.len() {
            return Err(FileFusionError::new(
                ErrorCode::IntInvariant,
                format!(
                    "PDF page count drifted during optimization ({} -> {})",
                    orig_pages.len(),
                    cand_pages.len()
                ),
            ));
        }

        let orig_text = orig_doc.extract_text(&orig_pages).unwrap_or_default();
        let cand_text = cand_doc.extract_text(&cand_pages).unwrap_or_default();
        let orig_norm: String = orig_text.split_whitespace().collect();
        let cand_norm: String = cand_text.split_whitespace().collect();
        let exact_text = orig_norm == cand_norm;
        let orig_chars: BTreeSet<char> = orig_norm.chars().collect();
        let cand_chars: BTreeSet<char> = cand_norm.chars().collect();
        if !orig_chars.is_empty() && !orig_chars.is_subset(&cand_chars) {
            return Err(FileFusionError::new(
                ErrorCode::IntInvariant,
                "PDF text character coverage dropped during compression",
            ));
        }

        Ok(QualityMetrics {
            ssim: Some(if exact_text { 1.0 } else { 0.99 }),
            psnr_db: if orig_bytes == cand_bytes {
                Some(99.9)
            } else {
                None
            },
            vmaf: None,
            bit_exact_lossless: Some(orig_bytes == cand_bytes),
        })
    }
}

/// L3 PDF compression engine combining pure-Rust `lopdf` / `flate2` / `compression-image`
/// optimization with optional Tier 2 `qpdf` linearization (`FR-PDF-002`, `FR-CMP-002`).
#[derive(Debug, Clone, Default)]
pub struct PdfCompressionEngine {
    #[cfg(feature = "native-process")]
    qpdf_binary: Option<PathBuf>,
}

impl PdfCompressionEngine {
    /// Creates a pure-Rust Tier 1 PDF compression engine (`Native` + `wasm32`).
    #[must_use]
    pub const fn new_tier1() -> Self {
        Self {
            #[cfg(feature = "native-process")]
            qpdf_binary: None,
        }
    }

    /// Configures a verified Tier 2 `qpdf` executable for native linearization/stream packing.
    #[cfg(feature = "native-process")]
    #[must_use]
    pub fn with_qpdf_binary(qpdf_binary: PathBuf) -> Self {
        Self {
            qpdf_binary: Some(qpdf_binary),
        }
    }

    /// Compresses a PDF using explicit [`PdfCompressionOptions`], enforcing memory limits,
    /// `FR-CMP-002` No-Op Guard, and atomic `StagedPath` staging.
    ///
    /// # Errors
    /// Returns [`FileFusionError`] on invalid target format, encrypted/corrupt PDF, or cancellation.
    pub fn compress_with_options(
        &self,
        input: &ValidatedSource,
        staged_output: &StagedPath,
        opts: &PdfCompressionOptions,
        ctx: &JobContext,
    ) -> FileFusionResult<CompressionOutcome> {
        ctx.check_cancelled()?;
        let req = &opts.base;
        let target_format = effective_target_format(input, req);
        if target_format != FormatId::Pdf {
            return Err(FileFusionError::new(
                ErrorCode::FmtUnsupported,
                format!("compression-pdf only writes FormatId::Pdf, got {target_format:?}"),
            ));
        }

        if let Some(noop) = prescreen_incompressible_noop(input, req, self.engine_id()) {
            return Ok(noop);
        }

        let est_mb = self.estimate_memory_mb(&input.inspection, req);
        let _mem = ctx.reserve_working_set_mb(est_mb, "pdf_compression")?;

        let raw = fs::read(&input.path)?;
        let (optimized_bytes, _report) = optimize_pdf_bytes(&raw, opts, ctx)?;
        fs::write(staged_output.staged_path(), &optimized_bytes)?;

        let validator = MagicByteOutputValidator;
        let evaluator = PdfQualityEvaluator;
        finalize_staged_compression(
            input,
            staged_output,
            req,
            &validator,
            Some(&evaluator),
            self.engine_id(),
            ctx,
        )
    }

    /// Executes pure-Rust Tier 1 optimization followed by an optional Tier 2 `qpdf` pass via
    /// [`IsolatedProcessRunner`] when `qpdf_binary` is configured (`MOD-RULE-04`).
    ///
    /// # Errors
    /// Returns [`FileFusionError`] on subprocess failure, PDF corruption, or cancellation.
    #[cfg(feature = "native-process")]
    pub fn compress_with_runner(
        &self,
        input: &ValidatedSource,
        staged_output: &StagedPath,
        opts: &PdfCompressionOptions,
        runner: Option<&dyn IsolatedProcessRunner>,
        ctx: &JobContext,
    ) -> FileFusionResult<CompressionOutcome> {
        let Some(qpdf_bin) = &self.qpdf_binary else {
            return self.compress_with_options(input, staged_output, opts, ctx);
        };
        let Some(exec) = runner else {
            return self.compress_with_options(input, staged_output, opts, ctx);
        };

        ctx.check_cancelled()?;
        let req = &opts.base;
        let est_mb = self.estimate_memory_mb(&input.inspection, req);
        let _mem = ctx.reserve_working_set_mb(est_mb, "pdf_compression_tier2")?;

        let raw = fs::read(&input.path)?;
        let (tier1_bytes, _) = optimize_pdf_bytes(&raw, opts, ctx)?;
        let tier1_tmp = StagedPath::create_sibling(staged_output.staged_path())?;
        fs::write(tier1_tmp.staged_path(), &tier1_bytes)?;

        let qpdf_inv = build_qpdf_optimize_invocation(
            qpdf_bin,
            tier1_tmp.staged_path(),
            staged_output.staged_path(),
            QpdfMode::OptimizeLinearize,
            opts.linearize_with_qpdf,
            ctx.limits.subprocess_timeout_secs,
        );
        let out = exec.run_isolated(&qpdf_inv, ctx)?;
        if out.exit_code != 0 {
            return Err(FileFusionError::new(
                ErrorCode::IntInvariant,
                format!(
                    "tier2 qpdf optimization exited with code {}: {}",
                    out.exit_code,
                    String::from_utf8_lossy(&out.stderr).trim()
                ),
            ));
        }

        let validator = MagicByteOutputValidator;
        let evaluator = PdfQualityEvaluator;
        finalize_staged_compression(
            input,
            staged_output,
            req,
            &validator,
            Some(&evaluator),
            "tier2-qpdf-pdf",
            ctx,
        )
    }
}

impl CompressionEngine for PdfCompressionEngine {
    fn engine_id(&self) -> &'static str {
        "tier1-compression-pdf"
    }

    fn supported_formats(&self) -> &[FormatId] {
        &[FormatId::Pdf]
    }

    fn estimate_memory_mb(&self, input: &FileInspectionReport, _req: &CompressionRequest) -> u64 {
        // Parsing indirect objects + embedded raster buffers (~4x source byte size, bounded 16..512 MB).
        let bytes_mb = input.byte_size.div_ceil(1_048_576);
        bytes_mb.saturating_mul(4).clamp(16, 512)
    }

    fn compress(
        &self,
        input: &ValidatedSource,
        staged_output: &StagedPath,
        req: &CompressionRequest,
        ctx: &JobContext,
    ) -> FileFusionResult<CompressionOutcome> {
        let opts = PdfCompressionOptions::from_request(req.clone());
        self.compress_with_options(input, staged_output, &opts, ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;
    use std::time::{SystemTime, UNIX_EPOCH};

    use file_core::{CompressionProfile, CompressionStatus, ResourceLimits, inspect_path};
    use lopdf::content::{Content, Operation};
    use lopdf::{Dictionary, Object, Stream, dictionary};

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let dir = std::env::temp_dir().join(format!("ff_pdf_{label}_{nanos}"));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    #[cfg(not(feature = "native-process"))]
    use std::path::PathBuf;

    fn build_bloated_sample_pdf(with_pii: bool, with_uncompressed_img: bool) -> Vec<u8> {
        let mut doc = Document::with_version("1.5");
        let root_pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });

        let mut resources = dictionary! {
            "Font" => dictionary! { "F1" => font_id },
        };
        if with_uncompressed_img {
            // 32x32 uniform gradient RGB bitmap (3072 uncompressed raw bytes)
            let mut pixels = Vec::with_capacity(32 * 32 * 3);
            for y in 0..32u8 {
                for x in 0..32u8 {
                    pixels.extend_from_slice(&[x.wrapping_mul(8), y.wrapping_mul(8), 120]);
                }
            }
            let img_id = doc.add_object(Stream::new(
                dictionary! {
                    "Type" => "XObject",
                    "Subtype" => "Image",
                    "Width" => 32,
                    "Height" => 32,
                    "ColorSpace" => "DeviceRGB",
                    "BitsPerComponent" => 8,
                },
                pixels,
            ));
            resources.set("XObject", dictionary! { "Im0" => img_id });
        }
        let res_id = doc.add_object(resources);

        // Long uncompressed content stream with repetitive comments/whitespace padding.
        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 12.into()]),
                Operation::new("Td", vec![50.into(), 700.into()]),
                Operation::new("Tj", vec![Object::string_literal("FileFusion PDF Test")]),
                Operation::new("ET", vec![]),
            ],
        };
        let mut encoded_ops = content.encode().unwrap_or_default();
        encoded_ops.extend_from_slice(&vec![b' '; 1024]);
        let content_id = doc.add_object(Stream::new(dictionary! {}, encoded_ops));

        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => root_pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Resources" => res_id,
            "Contents" => content_id,
        });
        doc.objects.insert(
            root_pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );

        let mut catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => root_pages_id,
        };
        if with_pii {
            let xmp_id = doc.add_object(Stream::new(
                dictionary! { "Type" => "Metadata", "Subtype" => "XML" },
                b"<x:xmpmeta>Author: Alice</x:xmpmeta>".to_vec(),
            ));
            catalog.set("Metadata", xmp_id);
            let info_id = doc.add_object(dictionary! {
                "Author" => Object::string_literal("Alice Secret"),
                "Creator" => Object::string_literal("Internal Scanner"),
            });
            doc.trailer.set("Info", info_id);
        }

        // Deliberate orphan object to test pruning.
        let _orphan_id = doc.add_object(Dictionary::from_iter(vec![(
            b"Orphan",
            Object::Integer(999),
        )]));

        let catalog_id = doc.add_object(catalog);
        doc.trailer.set("Root", catalog_id);

        let mut out = Vec::new();
        let _ = doc.save_to(&mut out);
        out
    }

    #[test]
    fn compresses_pdf_streams_images_orphans_and_scrubs_pii() -> Result<(), Box<dyn Error>> {
        let dir = unique_temp_dir("opt");
        let src = dir.join("bloated.pdf");
        let dst = dir.join("compressed.pdf");
        let raw = build_bloated_sample_pdf(true, true);
        fs::write(&src, &raw)?;

        let source = ValidatedSource {
            path: src.clone(),
            inspection: inspect_path(&src, false)?,
        };
        let ctx = JobContext::new("pdf-01", ResourceLimits::default());
        let engine = PdfCompressionEngine::new_tier1();
        let staged = StagedPath::create_sibling(&dst)?;
        let req = CompressionRequest {
            profile: CompressionProfile::BalancedOptimal,
            target_format: Some(FormatId::Pdf),
            target_max_bytes: None,
            min_ssim: Some(0.95),
            strip_metadata: true,
            no_op_if_larger: true,
        };

        let outcome = engine.compress(&source, &staged, &req, &ctx)?;
        assert_eq!(outcome.status, CompressionStatus::Optimized);
        assert!(outcome.bytes_out < outcome.bytes_in);

        let promoted = staged.promote(file_core::CollisionPolicy::OverwriteWithBackup)?;
        assert_eq!(promoted, dst);
        let out_bytes = fs::read(&dst)?;
        let out_doc = Document::load_mem(&out_bytes)?;
        assert_eq!(out_doc.get_pages().len(), 1);
        assert!(out_doc.catalog()?.get(b"Metadata").is_err());
        assert!(!String::from_utf8_lossy(&out_bytes).contains("Alice Secret"));

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn assesses_pdf_health_and_rejects_encrypted_pdfs() {
        let healthy = build_bloated_sample_pdf(false, false);
        let report = inspect_pdf_health(&healthy);
        assert_eq!(report.assessment, PdfRepairAssessment::Healthy);
        assert_eq!(report.page_count, 1);

        let truncated = &healthy[..healthy.len().saturating_sub(24)];
        let repaired = inspect_pdf_health(truncated);
        assert_eq!(repaired.assessment, PdfRepairAssessment::FullyRepairable);

        let partial = inspect_pdf_health(b"%PDF-1.5\n1 0 obj\n<< /Type /Catalog");
        assert_eq!(
            partial.assessment,
            PdfRepairAssessment::PartiallySalvageable
        );

        let garbage = inspect_pdf_health(b"NOT_A_PDF_HEADER");
        assert_eq!(garbage.assessment, PdfRepairAssessment::Unrecoverable);

        let mut enc_doc = Document::with_version("1.7");
        let enc_dict_id = enc_doc.add_object(dictionary! {
            "Filter" => "Standard",
            "V" => 4,
            "R" => 4,
            "Length" => 128,
            "P" => -4,
            "O" => Object::string_literal("ownerhash"),
            "U" => Object::string_literal("userhash"),
        });
        let enc_pages_id = enc_doc.add_object(dictionary! {
            "Type" => "Pages",
            "Kids" => Vec::<Object>::new(),
            "Count" => 0,
        });
        let enc_root_id = enc_doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => enc_pages_id,
        });
        enc_doc.trailer.set("Root", enc_root_id);
        enc_doc.trailer.set("Encrypt", enc_dict_id);
        let mut encrypted = Vec::new();
        let _ = enc_doc.save_to(&mut encrypted);
        let ctx = JobContext::new("pdf-enc", ResourceLimits::default());
        let opts = PdfCompressionOptions::from_request(CompressionRequest {
            profile: CompressionProfile::BalancedOptimal,
            target_format: Some(FormatId::Pdf),
            target_max_bytes: None,
            min_ssim: None,
            strip_metadata: false,
            no_op_if_larger: true,
        });
        let err = optimize_pdf_bytes(&encrypted, &opts, &ctx).err();
        assert_eq!(err.map(|e| e.diagnostic.code), Some(ErrorCode::FmtDrm));
    }

    #[test]
    fn enforces_pdf_noop_guard_when_already_compact() -> Result<(), Box<dyn Error>> {
        let dir = unique_temp_dir("noop");
        let src = dir.join("tiny.pdf");
        let dst = dir.join("tiny_out.pdf");

        let mut doc = Document::with_version("1.5");
        let root_pages_id = doc.new_object_id();
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => root_pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 200.into(), 200.into()],
        });
        doc.objects.insert(
            root_pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let catalog_id =
            doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => root_pages_id });
        doc.trailer.set("Root", catalog_id);
        doc.compress();
        doc.renumber_objects();
        let mut raw = Vec::new();
        doc.save_to(&mut raw)?;
        fs::write(&src, &raw)?;

        let source = ValidatedSource {
            path: src.clone(),
            inspection: inspect_path(&src, false)?,
        };
        let ctx = JobContext::new("pdf-noop", ResourceLimits::default());
        let engine = PdfCompressionEngine::new_tier1();
        let staged = StagedPath::create_sibling(&dst)?;
        let req = CompressionRequest {
            profile: CompressionProfile::LosslessMaximum,
            target_format: Some(FormatId::Pdf),
            target_max_bytes: None,
            min_ssim: None,
            strip_metadata: false,
            no_op_if_larger: true,
        };

        let outcome = engine.compress(&source, &staged, &req, &ctx)?;
        assert_eq!(outcome.status, CompressionStatus::NoOpRetainedOriginal);
        assert_eq!(outcome.bytes_out, outcome.bytes_in);
        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[cfg(feature = "native-process")]
    #[test]
    fn invokes_sanitized_qpdf_linearize_via_runner() -> Result<(), Box<dyn Error>> {
        use file_core::{ProcessInvocation, ProcessOutput};

        struct StubQpdfRunner {
            output_bytes: Vec<u8>,
        }
        impl IsolatedProcessRunner for StubQpdfRunner {
            fn run_isolated(
                &self,
                invocation: &ProcessInvocation,
                _ctx: &JobContext,
            ) -> FileFusionResult<ProcessOutput> {
                assert!(
                    invocation
                        .args
                        .iter()
                        .any(|a| a.to_string_lossy() == "--linearize")
                );
                if let Some(out) = invocation.args.last() {
                    fs::write(Path::new(out), &self.output_bytes)?;
                }
                Ok(ProcessOutput {
                    exit_code: 0,
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                    duration_ms: 4,
                    timed_out: false,
                })
            }
        }

        let dir = unique_temp_dir("qpdf");
        let src = dir.join("in.pdf");
        let dst = dir.join("out.pdf");
        let raw = build_bloated_sample_pdf(false, false);
        fs::write(&src, &raw)?;

        let mut compact_doc = Document::load_mem(&raw)?;
        compact_doc.compress();
        compact_doc.renumber_objects();
        let mut small = Vec::new();
        compact_doc.save_to(&mut small)?;

        let source = ValidatedSource {
            path: src.clone(),
            inspection: inspect_path(&src, false)?,
        };
        let ctx = JobContext::new("pdf-qpdf", ResourceLimits::default());
        let engine = PdfCompressionEngine::with_qpdf_binary(PathBuf::from("qpdf"));
        let staged = StagedPath::create_sibling(&dst)?;
        let opts = PdfCompressionOptions::from_request(CompressionRequest {
            profile: CompressionProfile::BalancedOptimal,
            target_format: Some(FormatId::Pdf),
            target_max_bytes: None,
            min_ssim: None,
            strip_metadata: false,
            no_op_if_larger: true,
        });

        let runner = StubQpdfRunner {
            output_bytes: small,
        };
        let outcome = engine.compress_with_runner(&source, &staged, &opts, Some(&runner), &ctx)?;
        assert_eq!(outcome.status, CompressionStatus::Optimized);
        assert!(outcome.bytes_out < outcome.bytes_in);

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn studio_merges_splits_reorders_rotates_crops_and_watermarks_pdfs()
    -> Result<(), Box<dyn Error>> {
        let ctx = JobContext::new("pdf-studio", ResourceLimits::default());
        let pdf_a = build_simple_text_pdf_bytes(&["Page One", "Page Two"]);
        let pdf_b = build_simple_text_pdf_bytes(&["Page Three"]);

        // Merge -> 3 pages preserving extractable text order.
        let merged = merge_pdfs(&[&pdf_a, &pdf_b], &ctx)?;
        let merged_doc = Document::load_mem(&merged)?;
        assert_eq!(merged_doc.get_pages().len(), 3);
        let merged_text = merged_doc.extract_text(&[1, 2, 3])?;
        assert!(merged_text.contains("Page One"));
        assert!(merged_text.contains("Page Two"));
        assert!(merged_text.contains("Page Three"));

        // Reorder `[3, 1]` and verify page count & order.
        let reordered = extract_or_reorder_pdf_pages(&merged, &[3, 1], &ctx)?;
        let reordered_doc = Document::load_mem(&reordered)?;
        assert_eq!(reordered_doc.get_pages().len(), 2);
        assert!(reordered_doc.extract_text(&[1])?.contains("Page Three"));
        assert!(reordered_doc.extract_text(&[2])?.contains("Page One"));

        // Split by ranges & fixed chunk size.
        let splits = split_pdf_by_ranges(&merged, &["1-2", "3"], &ctx)?;
        assert_eq!(splits.len(), 2);
        assert_eq!(Document::load_mem(&splits[0])?.get_pages().len(), 2);
        assert_eq!(Document::load_mem(&splits[1])?.get_pages().len(), 1);
        assert_eq!(split_pdf_every_n_pages(&merged, 2, &ctx)?.len(), 2);

        // Rotate + Crop + Watermark.
        let rotated = rotate_pdf_pages(&merged, Some(&[2]), 90, &ctx)?;
        let rot_doc = Document::load_mem(&rotated)?;
        let p2_id = rot_doc.get_pages()[&2];
        assert_eq!(rot_doc.get_dictionary(p2_id)?.get(b"Rotate")?.as_i64()?, 90);

        let cropped = crop_pdf_pages(
            &rotated,
            None,
            PdfCropMargins {
                left_pt: 36.0,
                bottom_pt: 36.0,
                right_pt: 36.0,
                top_pt: 36.0,
            },
            &ctx,
        )?;
        let crop_doc = Document::load_mem(&cropped)?;
        let p1_id = crop_doc.get_pages()[&1];
        assert!(crop_doc.get_dictionary(p1_id)?.has(b"CropBox"));

        let watermarked = apply_pdf_text_watermark(
            &cropped,
            &PdfWatermarkOptions {
                text: String::from("CONFIDENTIAL"),
                ..PdfWatermarkOptions::default()
            },
            &ctx,
        )?;
        let scan_info = inspect_pdf_scan_status(&watermarked)?;
        assert_eq!(scan_info.total_pages, 3);
        assert!(!scan_info.is_scanned_image_only);
        assert!(!scan_info.has_digital_signatures);
        let wm_text = Document::load_mem(&watermarked)?.extract_text(&[1])?;
        assert!(wm_text.contains("CONFIDENTIAL"));

        Ok(())
    }

    #[test]
    fn detects_image_only_scanned_pdf_status() -> Result<(), Box<dyn Error>> {
        let mut scan_doc = Document::with_version("1.5");
        let scan_pages_id = scan_doc.new_object_id();
        let img_id = scan_doc.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => 4,
                "Height" => 4,
                "ColorSpace" => "DeviceRGB",
                "BitsPerComponent" => 8,
            },
            vec![128u8; 4 * 4 * 3],
        ));
        let res_id = scan_doc.add_object(dictionary! {
            "XObject" => dictionary! { "Im0" => img_id },
        });
        let draw_ops = Content {
            operations: vec![Operation::new("Do", vec!["Im0".into()])],
        };
        let stream_id = scan_doc.add_object(Stream::new(dictionary! {}, draw_ops.encode()?));
        let leaf_page_id = scan_doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => scan_pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 200.into(), 200.into()],
            "Resources" => res_id,
            "Contents" => stream_id,
        });
        scan_doc.objects.insert(
            scan_pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![leaf_page_id.into()],
                "Count" => 1,
            }),
        );
        let cat_id = scan_doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => scan_pages_id,
        });
        scan_doc.trailer.set("Root", cat_id);
        let mut scanned_bytes = Vec::new();
        scan_doc.save_to(&mut scanned_bytes)?;

        let scanned_status = inspect_pdf_scan_status(&scanned_bytes)?;
        assert_eq!(scanned_status.total_pages, 1);
        assert!(scanned_status.is_scanned_image_only);
        assert_eq!(scanned_status.image_only_pages, vec![1]);

        Ok(())
    }

    #[test]
    fn studio_preserves_inherited_cropbox_and_rejects_non_finite_params()
    -> Result<(), Box<dyn Error>> {
        let mut doc = Document::with_version("1.5");
        let inherited_pages_id = doc.new_object_id();
        let leaf_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => inherited_pages_id,
        });
        doc.objects.insert(
            inherited_pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
                "CropBox" => vec![20.into(), 20.into(), 592.into(), 772.into()],
                "Rotate" => 180,
                "Kids" => vec![leaf_id.into()],
                "Count" => 1,
            }),
        );
        let root_cat_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => inherited_pages_id,
        });
        doc.trailer.set("Root", root_cat_id);
        let mut raw = Vec::new();
        doc.save_to(&mut raw)?;

        let ctx = JobContext::new("pdf-inherit", ResourceLimits::default());
        let extracted = extract_or_reorder_pdf_pages(&raw, &[1], &ctx)?;
        let out_doc = Document::load_mem(&extracted)?;
        let out_leaf_id = out_doc.get_pages()[&1];
        let leaf_dict = out_doc.get_dictionary(out_leaf_id)?;
        assert!(leaf_dict.has(b"CropBox"));
        assert!(leaf_dict.has(b"MediaBox"));
        assert_eq!(leaf_dict.get(b"Rotate")?.as_i64()?, 180);

        let nan_crop = crop_pdf_pages(
            &extracted,
            None,
            PdfCropMargins {
                left_pt: f32::NAN,
                bottom_pt: 0.0,
                right_pt: 0.0,
                top_pt: 0.0,
            },
            &ctx,
        );
        assert!(nan_crop.is_err());

        let nan_wm = apply_pdf_text_watermark(
            &extracted,
            &PdfWatermarkOptions {
                opacity: f32::NAN,
                ..PdfWatermarkOptions::default()
            },
            &ctx,
        );
        assert!(nan_wm.is_err());

        Ok(())
    }

    #[test]
    fn encrypts_inspects_and_decrypts_aes256_and_aes128_pdfs() -> Result<(), Box<dyn Error>> {
        let plain = build_simple_text_pdf_bytes(&["Confidential Balance Sheet"]);
        let ctx = JobContext::new("pdf-crypto", ResourceLimits::default());

        let aes256_opts = PdfProtectionOptions {
            user_password: Some(String::from("open-secret")),
            owner_password: String::from("owner-master"),
            algorithm: PdfEncryptionAlgorithm::Aes256,
            permissions: PdfPermissions::restricted_print_only(),
            encrypt_metadata: true,
        };
        let locked = encrypt_pdf_bytes(&plain, &aes256_opts, &ctx)?;

        let sec = inspect_pdf_security(&locked, Some("open-secret"))?;
        assert!(sec.is_encrypted);
        assert_eq!(sec.version, Some(5));
        assert_eq!(sec.revision, Some(6));
        assert_eq!(sec.key_bits, Some(256));
        assert_eq!(sec.password_verified, Some(true));
        let perms = sec.permissions.ok_or("missing permissions")?;
        assert!(perms.allow_print);
        assert!(!perms.allow_copy);
        assert!(!perms.allow_modify);

        let wrong = inspect_pdf_security(&locked, Some("wrong-pw"))?;
        assert_eq!(wrong.password_verified, Some(false));

        let wrong_decrypt = decrypt_pdf_bytes(&locked, "wrong-pw", &ctx);
        assert!(wrong_decrypt.is_err());
        if let Err(err) = wrong_decrypt {
            assert_eq!(err.diagnostic.code, ErrorCode::FmtDrm);
        }

        let unlocked = decrypt_pdf_bytes(&locked, "open-secret", &ctx)?;
        let unlocked_sec = inspect_pdf_security(&unlocked, None)?;
        assert!(!unlocked_sec.is_encrypted);
        let recovered_text = Document::load_mem(&unlocked)?.extract_text(&[1])?;
        assert!(recovered_text.contains("Confidential Balance Sheet"));

        let aes128_opts = PdfProtectionOptions {
            user_password: None,
            owner_password: String::from("owner-only-128"),
            algorithm: PdfEncryptionAlgorithm::Aes128,
            permissions: PdfPermissions::read_only(),
            encrypt_metadata: true,
        };
        let owner_locked = encrypt_pdf_bytes(&plain, &aes128_opts, &ctx)?;
        let owner_sec = inspect_pdf_security(&owner_locked, Some("owner-only-128"))?;
        assert!(owner_sec.is_encrypted);
        assert_eq!(owner_sec.version, Some(4));
        assert_eq!(owner_sec.key_bits, Some(128));
        assert_eq!(owner_sec.password_verified, Some(true));
        let owner_unlocked = decrypt_pdf_bytes(&owner_locked, "owner-only-128", &ctx)?;
        assert!(
            Document::load_mem(&owner_unlocked)?
                .extract_text(&[1])?
                .contains("Confidential")
        );

        Ok(())
    }

    #[test]
    fn verifies_pdf_digital_signatures_and_detects_post_sign_tampering()
    -> Result<(), Box<dyn Error>> {
        let ctx = JobContext::new("pdf-sig", ResourceLimits::default());
        let unsigned = build_simple_text_pdf_bytes(&["Unsigned contract"]);
        let unsigned_rep = verify_pdf_signatures(&unsigned, &ctx)?;
        assert!(!unsigned_rep.has_signatures);
        assert_eq!(unsigned_rep.signature_count, 0);

        let mut signed_doc = Document::with_version("1.5");
        let root_pages_id = signed_doc.new_object_id();
        let leaf_page_id = signed_doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => root_pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        });
        let sig_id = signed_doc.add_object(dictionary! {
            "Type" => "Sig",
            "Filter" => "Adobe.PPKLite",
            "SubFilter" => "adbe.pkcs7.detached",
            "Name" => Object::string_literal("FileFusion Signer"),
            "Reason" => Object::string_literal("Integrity Test"),
            "ByteRange" => vec![
                0.into(),
                1_000_000_000.into(),
                1_000_000_000.into(),
                1_000_000_000.into(),
            ],
            "Contents" => Object::String(vec![0x30, 0x06, 0x06, 0x02, 0x2A, 0x03], lopdf::StringFormat::Hexadecimal),
        });
        let field_id = signed_doc.add_object(dictionary! {
            "FT" => "Sig",
            "T" => Object::string_literal("Signature1"),
            "V" => sig_id,
        });
        signed_doc.objects.insert(
            root_pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![leaf_page_id.into()],
                "Count" => 1,
            }),
        );
        let cat_id = signed_doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => root_pages_id,
            "AcroForm" => dictionary! { "Fields" => vec![field_id.into()] },
        });
        signed_doc.trailer.set("Root", cat_id);
        let mut raw_signed = Vec::new();
        signed_doc.save_to(&mut raw_signed)?;

        let hole_start = raw_signed
            .windows(14)
            .position(|w| w == b"<300606022A03>" || w == b"<300606022a03>")
            .ok_or("missing hex Contents placeholder")?;
        let hole_end = hole_start + 14;
        let placeholder = b"[0 1000000000 1000000000 1000000000]";
        let br_pos = raw_signed
            .windows(placeholder.len())
            .position(|w| w == placeholder)
            .ok_or("missing ByteRange placeholder")?;
        let len2 = raw_signed.len() - hole_end;
        let replacement = format!("[0 {hole_start} {hole_end} {len2}]");
        let padded = format!("{replacement:<width$}", width = placeholder.len());
        raw_signed[br_pos..br_pos + placeholder.len()].copy_from_slice(padded.as_bytes());

        let rep = verify_pdf_signatures(&raw_signed, &ctx)?;
        assert!(rep.has_signatures);
        assert_eq!(rep.signature_count, 1);
        assert!(rep.all_byte_ranges_intact);
        assert_eq!(
            rep.signatures[0].status,
            PdfSignatureStatus::SignedByteRangeIntact
        );
        assert_eq!(rep.signatures[0].field_name.as_deref(), Some("Signature1"));

        let mut tampered = raw_signed.clone();
        tampered.extend_from_slice(b"\n% post-sign tamper\n");
        let tampered_rep = verify_pdf_signatures(&tampered, &ctx)?;
        assert!(!tampered_rep.all_byte_ranges_intact);
        assert_eq!(
            tampered_rep.signatures[0].status,
            PdfSignatureStatus::SignedTamperedAfterSigning
        );

        Ok(())
    }

    #[test]
    fn parses_ocr_tsv_hocr_and_embeds_searchable_text_overlay() -> Result<(), Box<dyn Error>> {
        let tsv = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n\
                   5\t1\t1\t1\t1\t1\t40\t60\t120\t24\t96.5\tINVOICE\n\
                   5\t1\t1\t1\t1\t2\t180\t60\t90\t24\t92.0\tTOTAL\n";
        let page_ocr = parse_tesseract_tsv(1, 800, 1000, tsv)?;
        assert_eq!(page_ocr.words.len(), 2);
        assert_eq!(page_ocr.plain_text, "INVOICE TOTAL");
        assert!(page_ocr.mean_confidence > 90.0);

        let hocr = r"<html><body><span class='ocrx_word' title='bbox 10 20 110 50; x_wconf 88'>Sample</span></body></html>";
        let hocr_res = parse_tesseract_hocr(1, 400, 400, hocr)?;
        assert_eq!(hocr_res.words.len(), 1);
        assert_eq!(hocr_res.words[0].width_px, 100);
        assert_eq!(hocr_res.words[0].text, "Sample");

        let img = image::RgbImage::from_pixel(32, 32, image::Rgb([220, 220, 220]));
        let mut png_cur = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img).write_to(&mut png_cur, image::ImageFormat::Png)?;

        let ctx = JobContext::new("pdf-ocr-overlay", ResourceLimits::default());
        let searchable =
            build_searchable_pdf_from_image_and_ocr(&png_cur.into_inner(), &page_ocr, &ctx)?;
        let extracted_images = extract_pdf_page_images(&searchable, &ctx)?;
        assert_eq!(extracted_images.len(), 1);
        let text = Document::load_mem(&searchable)?.extract_text(&[1])?;
        assert!(text.contains("INVOICE"));
        assert!(text.contains("TOTAL"));

        Ok(())
    }

    #[cfg(feature = "native-process")]
    #[test]
    fn tesseract_adapter_and_qpdf_crypto_builders_work_with_mock_runner()
    -> Result<(), Box<dyn Error>> {
        use file_core::{ProcessInvocation, ProcessOutput};

        struct StubTesseractRunner;
        impl IsolatedProcessRunner for StubTesseractRunner {
            fn run_isolated(
                &self,
                invocation: &ProcessInvocation,
                ctx: &JobContext,
            ) -> FileFusionResult<ProcessOutput> {
                ctx.check_cancelled()?;
                assert!(invocation.args.iter().any(|a| a == "eng"));
                assert!(invocation.args.iter().any(|a| a == "tsv"));
                let tsv = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n\
                           5\t1\t1\t1\t1\t1\t4\t6\t16\t8\t97.0\tSCANNED_OK\n";
                Ok(ProcessOutput {
                    exit_code: 0,
                    stdout: tsv.as_bytes().to_vec(),
                    stderr: Vec::new(),
                    duration_ms: 8,
                    timed_out: false,
                })
            }
        }

        let mut scan_doc = Document::with_version("1.5");
        let scan_pages_id = scan_doc.new_object_id();
        let img_id = scan_doc.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => 24,
                "Height" => 24,
                "ColorSpace" => "DeviceRGB",
                "BitsPerComponent" => 8,
            },
            vec![180u8; 24 * 24 * 3],
        ));
        let res_id = scan_doc.add_object(dictionary! {
            "XObject" => dictionary! { "Im0" => img_id },
        });
        let draw_ops = Content {
            operations: vec![Operation::new("Do", vec!["Im0".into()])],
        };
        let stream_id = scan_doc.add_object(Stream::new(dictionary! {}, draw_ops.encode()?));
        let leaf_page_id = scan_doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => scan_pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 200.into(), 200.into()],
            "Resources" => res_id,
            "Contents" => stream_id,
        });
        scan_doc.objects.insert(
            scan_pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![leaf_page_id.into()],
                "Count" => 1,
            }),
        );
        let cat_id = scan_doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => scan_pages_id,
        });
        scan_doc.trailer.set("Root", cat_id);
        let mut scanned_bytes = Vec::new();
        scan_doc.save_to(&mut scanned_bytes)?;

        let ctx = JobContext::new("ocr-runner", ResourceLimits::default());
        let adapter = TesseractAdapter::new(PathBuf::from("tesseract"));
        let runner = StubTesseractRunner;
        let (searchable_pdf, ocr_doc) = adapter.make_pdf_searchable_with_runner(
            &scanned_bytes,
            &OcrOptions::default(),
            &runner,
            &ctx,
        )?;
        assert_eq!(ocr_doc.full_text, "SCANNED_OK");
        let status = inspect_pdf_scan_status(&searchable_pdf)?;
        assert!(!status.is_scanned_image_only);
        assert!(
            Document::load_mem(&searchable_pdf)?
                .extract_text(&[1])?
                .contains("SCANNED_OK")
        );

        let enc_inv = build_qpdf_encrypt_invocation(
            Path::new("qpdf"),
            Path::new("in.pdf"),
            Path::new("out.pdf"),
            "user",
            "owner",
            PdfPermissions::restricted_print_only(),
            30,
        );
        assert!(enc_inv.args.iter().any(|a| a == "--encrypt"));
        let dec_inv = build_qpdf_decrypt_invocation(
            Path::new("qpdf"),
            Path::new("locked.pdf"),
            Path::new("plain.pdf"),
            "user",
            30,
        );
        assert!(dec_inv.args.iter().any(|a| a == "--decrypt"));

        Ok(())
    }
}
