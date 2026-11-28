//! `compression-pdf`: L3 pure-Rust (`lopdf` + `flate2` + `compression-image`) and optional
//! Tier 2 `qpdf` PDF stream/image compression, metadata privacy scrubbing, and structural
//! repair diagnostics (`FR-PDF-001`..`002`, `FR-REP-001`..`002`, `FR-CMP-001`..`002`).

#![forbid(unsafe_code)]

pub mod optimizer;
pub mod repair;

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

pub use optimizer::{PdfCompressionOptions, PdfCompressionReport, optimize_pdf_bytes};
pub use repair::{PdfHealthReport, PdfRepairAssessment, QpdfMode, inspect_pdf_health};

#[cfg(feature = "native-process")]
pub use repair::build_qpdf_optimize_invocation;

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
}
