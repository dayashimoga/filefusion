//! `compression-office`: L3 OOXML (`DOCX`/`XLSX`/`PPTX`), ODF (`ODT`/`ODS`/`ODP`), and `EPUB`
//! structural optimization, embedded media recompression via `compression-image`, and
//! metadata privacy scrubbing (`FR-DOC-002`..`003`, `FR-SEC-001`, `FR-MET-002`, `MOD-RULE-02`).

#![forbid(unsafe_code)]

pub mod container;
pub mod xml;

use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use compression_core::{
    effective_target_format, finalize_staged_compression, prescreen_incompressible_noop,
};
use file_core::{
    CompressionEngine, CompressionOutcome, CompressionRequest, ErrorCode, FileFusionError,
    FileFusionResult, FileInspectionReport, FormatId, JobContext, QualityEvaluator, QualityMetrics,
    StagedPath, ValidatedSource,
};
use validation_engine::MagicByteOutputValidator;
use zip::{CompressionMethod, ZipArchive};

pub use container::{
    OfficeCompressionOptions, OfficeOptimizationStats, optimize_office_container,
    read_validated_zip_entries, requires_uncompressed_mimetype_first,
    sanitize_and_edit_office_metadata, write_office_container_entries,
};
pub use xml::{
    OfficeContainerMetadata, edit_metadata_xml, extract_office_metadata_from_entries,
    extract_visible_xml_text, is_optimizable_xml_entry, minify_xml_bytes, scrub_metadata_xml,
};

/// Validates staged Office/EPUB output containers via [`MagicByteOutputValidator`].
pub type OfficeOutputValidator = MagicByteOutputValidator;

/// Evaluates semantic preservation across `OOXML`, `ODF`, and `EPUB` containers by comparing
/// extracted visible text from non-metadata XML/XHTML entries and verifying spec-required
/// uncompressed `mimetype` ordering (`FR-FID-001`).
#[derive(Debug, Clone, Copy, Default)]
pub struct OfficeQualityEvaluator;

impl QualityEvaluator for OfficeQualityEvaluator {
    fn evaluator_id(&self) -> &'static str {
        "compression-office-structural"
    }

    fn evaluate(
        &self,
        original: &ValidatedSource,
        candidate: &Path,
        ctx: &JobContext,
    ) -> FileFusionResult<QualityMetrics> {
        ctx.check_cancelled()?;
        let fmt = original.inspection.detected_format;
        let orig_text = extract_container_text(&original.path, fmt, false)?;
        let cand_text = extract_container_text(candidate, fmt, true)?;
        if orig_text != cand_text {
            return Err(FileFusionError::new(
                ErrorCode::IntInvariant,
                "Office/EPUB document body text changed during structural compression",
            ));
        }
        let exact_bytes = fs::read(&original.path)? == fs::read(candidate)?;
        Ok(QualityMetrics {
            ssim: Some(1.0),
            psnr_db: if exact_bytes { Some(99.9) } else { None },
            vmaf: None,
            bit_exact_lossless: Some(exact_bytes),
        })
    }
}

fn extract_container_text(
    path: &Path,
    format: FormatId,
    enforce_mimetype_spec: bool,
) -> FileFusionResult<String> {
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file).map_err(|err| {
        FileFusionError::new(
            ErrorCode::FmtCorrupt,
            format!("failed to inspect Office/EPUB ZIP: {err}"),
        )
    })?;

    if enforce_mimetype_spec && requires_uncompressed_mimetype_first(format) {
        if let Ok(first) = archive.by_index(0) {
            if first.name() == "mimetype" && first.compression() != CompressionMethod::Stored {
                return Err(FileFusionError::new(
                    ErrorCode::FmtCorrupt,
                    "ODF/EPUB `mimetype` entry must remain uncompressed (`Stored`)",
                ));
            }
        }
    }

    let mut content_parts = Vec::new();
    for idx in 0..archive.len() {
        let mut entry = archive.by_index(idx).map_err(|err| {
            FileFusionError::new(ErrorCode::FmtCorrupt, format!("corrupt entry {idx}: {err}"))
        })?;
        let name = entry.name().to_owned();
        let lower = name.to_ascii_lowercase();
        if !is_optimizable_xml_entry(&lower)
            || lower.starts_with("docprops/")
            || lower == "meta.xml"
            || Path::new(&name)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("opf"))
        {
            continue;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        let visible = extract_visible_xml_text(&bytes);
        content_parts.push((name, visible));
    }
    content_parts.sort_by(|a, b| a.0.cmp(&b.0));
    let combined: Vec<String> = content_parts.into_iter().map(|(_, t)| t).collect();
    Ok(combined.join("\n"))
}

/// Pure-Rust L3 container optimization engine for `DOCX`, `XLSX`, `PPTX`, `ODT`, `ODS`,
/// `ODP`, and `EPUB` (`FR-CMP-002`, `FR-SEC-001`, `FR-MET-002`).
#[derive(Debug, Clone, Copy, Default)]
pub struct OfficeCompressionEngine;

impl OfficeCompressionEngine {
    /// Compresses an Office or EPUB container using explicit [`OfficeCompressionOptions`].
    ///
    /// # Errors
    /// Returns [`FileFusionError`] on unsupported target format, Zip-Slip/bomb violation, or cancellation.
    pub fn compress_with_options(
        &self,
        input: &ValidatedSource,
        staged_output: &StagedPath,
        opts: &OfficeCompressionOptions,
        ctx: &JobContext,
    ) -> FileFusionResult<CompressionOutcome> {
        ctx.check_cancelled()?;
        let req = &opts.base;
        let target_format = effective_target_format(input, req);
        if !matches!(
            target_format,
            FormatId::Docx
                | FormatId::Xlsx
                | FormatId::Pptx
                | FormatId::Odt
                | FormatId::Ods
                | FormatId::Odp
                | FormatId::Epub
        ) {
            return Err(FileFusionError::new(
                ErrorCode::FmtUnsupported,
                format!("unsupported office/ebook container format {target_format:?}"),
            ));
        }

        if let Some(noop) = prescreen_incompressible_noop(input, req, self.engine_id()) {
            return Ok(noop);
        }

        let est_mb = self.estimate_memory_mb(&input.inspection, req);
        let _mem = ctx.reserve_working_set_mb(est_mb, "office_compression")?;

        let raw = fs::read(&input.path)?;
        let (optimized, _stats) =
            optimize_office_container(&raw, target_format, opts, &ctx.limits, ctx)?;
        fs::write(staged_output.staged_path(), &optimized)?;

        let validator = MagicByteOutputValidator;
        let evaluator = OfficeQualityEvaluator;
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
}

impl CompressionEngine for OfficeCompressionEngine {
    fn engine_id(&self) -> &'static str {
        "tier1-compression-office"
    }

    fn supported_formats(&self) -> &[FormatId] {
        &[
            FormatId::Docx,
            FormatId::Xlsx,
            FormatId::Pptx,
            FormatId::Odt,
            FormatId::Ods,
            FormatId::Odp,
            FormatId::Epub,
        ]
    }

    fn estimate_memory_mb(&self, input: &FileInspectionReport, _req: &CompressionRequest) -> u64 {
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
        let opts = OfficeCompressionOptions::from_request(req.clone());
        self.compress_with_options(input, staged_output, &opts, ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;
    use std::io::Write;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use file_core::{
        CollisionPolicy, CompressionProfile, CompressionStatus, ResourceLimits, inspect_path,
    };
    use flate2::Compression;
    use flate2::write::ZlibEncoder;
    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let dir = std::env::temp_dir().join(format!("ff_office_{label}_{nanos}"));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    fn png_crc32(data: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for &byte in data {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                let mask = (crc & 1).wrapping_neg();
                crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
            }
        }
        !crc
    }

    fn push_png_chunk(out: &mut Vec<u8>, tag: [u8; 4], body: &[u8]) {
        out.extend_from_slice(&(u32::try_from(body.len()).unwrap_or(0)).to_be_bytes());
        out.extend_from_slice(&tag);
        out.extend_from_slice(body);
        let mut crc_buf = Vec::with_capacity(4 + body.len());
        crc_buf.extend_from_slice(&tag);
        crc_buf.extend_from_slice(body);
        out.extend_from_slice(&png_crc32(&crc_buf).to_be_bytes());
    }

    fn unoptimized_png_bytes() -> Result<Vec<u8>, Box<dyn Error>> {
        // Build an intentionally uncompressed 64x64 RGBA PNG so `oxipng` achieves strong savings.
        let mut raw_scanlines = Vec::with_capacity(64 * (1 + 64 * 4));
        for y in 0..64u8 {
            raw_scanlines.push(0); // filter type 0 (None)
            for x in 0..64u8 {
                raw_scanlines.extend_from_slice(&[x.wrapping_mul(4), y.wrapping_mul(4), 180, 255]);
            }
        }
        let mut zlib = ZlibEncoder::new(Vec::new(), Compression::none());
        zlib.write_all(&raw_scanlines)?;
        let idat = zlib.finish()?;

        let mut png = Vec::from(b"\x89PNG\r\n\x1A\n".as_slice());
        let mut ihdr = Vec::with_capacity(13);
        ihdr.extend_from_slice(&64u32.to_be_bytes());
        ihdr.extend_from_slice(&64u32.to_be_bytes());
        ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA
        push_png_chunk(&mut png, *b"IHDR", &ihdr);
        push_png_chunk(&mut png, *b"IDAT", &idat);
        push_png_chunk(&mut png, *b"IEND", &[]);
        Ok(png)
    }

    fn build_sample_docx(png_bytes: &[u8]) -> Result<Vec<u8>, Box<dyn Error>> {
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut writer = ZipWriter::new(&mut cursor);
            let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);

            writer.start_file("[Content_Types].xml", stored)?;
            writer.write_all(
                b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n  <!-- comment -->\n  <Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"></Types>",
            )?;

            writer.start_file("word/document.xml", stored)?;
            writer.write_all(
                b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n  <!-- doc comment -->\n  <w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\n    <w:body>\n      <w:p><w:r><w:t xml:space=\"preserve\">  Hello FileFusion  </w:t></w:r></w:p>\n    </w:body>\n  </w:document>",
            )?;

            writer.start_file("docProps/core.xml", stored)?;
            writer.write_all(
                b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><cp:coreProperties><dc:creator>Alice Private</dc:creator><cp:lastModifiedBy>Bob Editor</cp:lastModifiedBy><cp:revision>42</cp:revision></cp:coreProperties>",
            )?;

            writer.start_file("word/media/image1.png", stored)?;
            writer.write_all(png_bytes)?;
            writer.finish()?;
        }
        Ok(cursor.into_inner())
    }

    #[test]
    fn compresses_docx_media_xml_and_scrubs_privacy_metadata() -> Result<(), Box<dyn Error>> {
        let dir = unique_temp_dir("docx");
        let src = dir.join("sample.docx");
        let dst = dir.join("optimized.docx");
        let raw_png = unoptimized_png_bytes()?;
        let docx_bytes = build_sample_docx(&raw_png)?;
        fs::write(&src, &docx_bytes)?;

        let source = ValidatedSource {
            path: src.clone(),
            inspection: inspect_path(&src, false)?,
        };
        assert_eq!(source.inspection.detected_format, FormatId::Docx);

        let ctx = JobContext::new("off-docx", ResourceLimits::default());
        let engine = OfficeCompressionEngine;
        let staged = StagedPath::create_sibling(&dst)?;
        let req = CompressionRequest {
            profile: CompressionProfile::BalancedOptimal,
            target_format: None,
            target_max_bytes: None,
            min_ssim: Some(0.95),
            strip_metadata: true,
            no_op_if_larger: true,
        };

        let outcome = engine.compress(&source, &staged, &req, &ctx)?;
        assert_eq!(outcome.status, CompressionStatus::Optimized);
        assert!(outcome.bytes_out < outcome.bytes_in);

        let promoted = staged.promote(CollisionPolicy::OverwriteWithBackup)?;
        assert_eq!(promoted, dst);
        let mut out_zip = ZipArchive::new(File::open(&dst)?)?;
        let mut core_xml = String::new();
        out_zip
            .by_name("docProps/core.xml")?
            .read_to_string(&mut core_xml)?;
        assert!(!core_xml.contains("Alice Private"));
        assert!(!core_xml.contains("Bob Editor"));
        assert!(core_xml.contains("<cp:revision>1</cp:revision>"));

        let mut doc_xml = String::new();
        out_zip
            .by_name("word/document.xml")?
            .read_to_string(&mut doc_xml)?;
        assert!(doc_xml.contains("  Hello FileFusion  "));
        assert!(!doc_xml.contains("<!-- doc comment -->"));

        let opt_png = out_zip.by_name("word/media/image1.png")?;
        assert!(usize::try_from(opt_png.size()).unwrap_or(usize::MAX) <= raw_png.len());

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn preserves_epub_uncompressed_mimetype_first_and_enforces_security_gates()
    -> Result<(), Box<dyn Error>> {
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut writer = ZipWriter::new(&mut cursor);
            let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
            writer.start_file("META-INF/container.xml", stored)?;
            writer.write_all(
                b"<?xml version=\"1.0\"?><container><rootfiles><rootfile full-path=\"OEBPS/content.opf\"/></rootfiles></container>",
            )?;
            writer.start_file("mimetype", stored)?;
            writer.write_all(b"application/epub+zip")?;
            writer.start_file("OEBPS/content.opf", stored)?;
            writer.write_all(
                b"<package><metadata><dc:title>Book</dc:title><dc:creator>Author PII</dc:creator></metadata></package>",
            )?;
            writer.finish()?;
        }

        let ctx = JobContext::new("off-epub", ResourceLimits::default());
        let opts = OfficeCompressionOptions::from_request(CompressionRequest {
            profile: CompressionProfile::LosslessMaximum,
            target_format: Some(FormatId::Epub),
            target_max_bytes: None,
            min_ssim: None,
            strip_metadata: true,
            no_op_if_larger: true,
        });
        let (repacked, stats) =
            optimize_office_container(cursor.get_ref(), FormatId::Epub, &opts, &ctx.limits, &ctx)?;
        assert_eq!(stats.metadata_entries_scrubbed, 1);

        let mut archive = ZipArchive::new(std::io::Cursor::new(repacked))?;
        let first = archive.by_index(0)?;
        assert_eq!(first.name(), "mimetype");
        assert_eq!(first.compression(), CompressionMethod::Stored);
        drop(first);

        let mut opf = String::new();
        archive
            .by_name("OEBPS/content.opf")?
            .read_to_string(&mut opf)?;
        assert!(opf.contains("<dc:title>Book</dc:title>"));
        assert!(!opf.contains("Author PII"));

        // Zip-Slip path rejection (`FR-SEC-001`).
        let mut evil = std::io::Cursor::new(Vec::new());
        {
            let mut w = ZipWriter::new(&mut evil);
            let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
            w.start_file("word/../../escape.xml", stored)?;
            w.write_all(b"<bad/>")?;
            w.finish()?;
        }
        let trav_err =
            optimize_office_container(evil.get_ref(), FormatId::Docx, &opts, &ctx.limits, &ctx)
                .err();
        assert_eq!(
            trav_err.map(|e| e.diagnostic.code),
            Some(ErrorCode::SecTraversal)
        );
        Ok(())
    }

    #[test]
    fn enforces_office_noop_guard_when_recompression_does_not_shrink() -> Result<(), Box<dyn Error>>
    {
        let dir = unique_temp_dir("noop");
        let src = dir.join("compact.odt");
        let dst = dir.join("compact_out.odt");

        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut writer = ZipWriter::new(&mut cursor);
            let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
            let best_deflate = SimpleFileOptions::default()
                .compression_method(CompressionMethod::Deflated)
                .compression_level(Some(9));
            writer.start_file("mimetype", stored)?;
            writer.write_all(b"application/vnd.oasis.opendocument.text")?;
            writer.start_file("content.xml", best_deflate)?;
            writer.write_all(b"<office:document-content><office:body><text:p>Hi</text:p></office:body></office:document-content>")?;
            writer.finish()?;
        }
        fs::write(&src, cursor.into_inner())?;

        let source = ValidatedSource {
            path: src.clone(),
            inspection: inspect_path(&src, false)?,
        };
        let ctx = JobContext::new("off-noop", ResourceLimits::default());
        let engine = OfficeCompressionEngine;
        let staged = StagedPath::create_sibling(&dst)?;
        let req = CompressionRequest {
            profile: CompressionProfile::LosslessMaximum,
            target_format: Some(FormatId::Odt),
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
}
