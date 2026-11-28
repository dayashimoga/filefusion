//! `compression-image`: L3 raster image compression, SIMD resizing, EXIF auto-orient,
//! and empirical quality verification (`FR-IMG-001`..`003`, `MOD-RULE-02`, `MOD-RULE-05`).

#![forbid(unsafe_code)]

use std::fs::{self, File};
use std::io::BufWriter;
use std::path::Path;

use compression_core::{
    CompressionProfileConfig, effective_target_format, finalize_staged_compression,
    prescreen_incompressible_noop,
};
use fast_image_resize::images::Image as FirImage;
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};
use file_core::{
    CompressionEngine, CompressionOutcome, CompressionProfile, CompressionRequest, ErrorCode,
    FileFusionError, FileFusionResult, FileInspectionReport, FormatId, JobContext,
    QualityEvaluator, QualityMetrics, StagedPath, ValidatedSource,
};
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::codecs::webp::WebPEncoder;
use image::{ColorType, DynamicImage, ImageEncoder, ImageFormat};
use oxipng::{Options as OxiOptions, StripChunks};
use quality_engine::{BitExactQualityEvaluator, evaluate_pixel_buffers};
use ravif::{Encoder as AvifEncoder, Img as AvifImg};
use rgb::FromSlice;
use serde::{Deserialize, Serialize};
use validation_engine::MagicByteOutputValidator;

/// Supported SIMD convolution filters for image resizing (`FR-IMG-002`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResizeFilter {
    /// Sharp Lanczos-3 windowed sinc filter (recommended for photographic downscaling).
    #[default]
    Lanczos3,
    /// Crisp cubic Catmull-Rom filter.
    CatmullRom,
    /// Smooth Mitchell-Netravali cubic filter.
    Mitchell,
    /// Fast linear interpolation filter.
    Bilinear,
}

impl ResizeFilter {
    const fn to_fir_filter(self) -> FilterType {
        match self {
            Self::Lanczos3 => FilterType::Lanczos3,
            Self::CatmullRom => FilterType::CatmullRom,
            Self::Mitchell => FilterType::Mitchell,
            Self::Bilinear => FilterType::Bilinear,
        }
    }
}

/// Optional bounding-box resize parameters (`FR-IMG-002`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageResizeSpec {
    /// Optional target maximum width in pixels (`0` rejected).
    pub max_width: Option<u32>,
    /// Optional target maximum height in pixels (`0` rejected).
    pub max_height: Option<u32>,
    /// Resampling kernel.
    pub filter: ResizeFilter,
    /// Maintain source aspect ratio when fitting inside `(max_width, max_height)`.
    pub preserve_aspect_ratio: bool,
}

/// Extended options for image compression pipelines (`FR-IMG-001`..`003`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageCompressionOptions {
    /// Core compression request (`profile`, `target_format`, `min_ssim`, `no_op_if_larger`, etc.).
    pub base: CompressionRequest,
    /// Optional resize specification.
    pub resize: Option<ImageResizeSpec>,
    /// Apply EXIF orientation normalization prior to pixel encoding (`FR-IMG-002`).
    pub auto_orient: bool,
}

/// Reads standard EXIF TIFF orientation tag (`0x0112`) from raw JPEG bytes (`1..=8`).
#[must_use]
pub fn detect_exif_orientation(bytes: &[u8]) -> u16 {
    // Walk JPEG APP1 markers for `Exif\0\0` header and TIFF IFD0 orientation tag `0x0112`.
    if bytes.len() < 16 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return 1;
    }
    let mut pos = 2usize;
    while pos + 4 <= bytes.len() {
        if bytes[pos] != 0xFF {
            break;
        }
        let marker = bytes[pos + 1];
        if marker == 0xDA || marker == 0xD9 {
            break;
        }
        if pos + 4 > bytes.len() {
            break;
        }
        let seg_len = usize::from(u16::from_be_bytes([bytes[pos + 2], bytes[pos + 3]]));
        if seg_len < 2 || pos + 2 + seg_len > bytes.len() {
            break;
        }
        let payload = &bytes[pos + 4..pos + 2 + seg_len];
        if marker == 0xE1 && payload.starts_with(b"Exif\0\0") && payload.len() >= 14 {
            return parse_tiff_orientation(&payload[6..]).unwrap_or(1);
        }
        pos += 2 + seg_len;
    }
    1
}

fn parse_tiff_orientation(tiff: &[u8]) -> Option<u16> {
    if tiff.len() < 8 {
        return None;
    }
    let little_endian = match &tiff[..2] {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let read_u16 = |off: usize| -> Option<u16> {
        let pair = [tiff.get(off).copied()?, tiff.get(off + 1).copied()?];
        Some(if little_endian {
            u16::from_le_bytes(pair)
        } else {
            u16::from_be_bytes(pair)
        })
    };
    let read_u32 = |off: usize| -> Option<u32> {
        let quad = [
            tiff.get(off).copied()?,
            tiff.get(off + 1).copied()?,
            tiff.get(off + 2).copied()?,
            tiff.get(off + 3).copied()?,
        ];
        Some(if little_endian {
            u32::from_le_bytes(quad)
        } else {
            u32::from_be_bytes(quad)
        })
    };

    if read_u16(2)? != 42 {
        return None;
    }
    let ifd_offset = usize::try_from(read_u32(4)?).ok()?;
    let entry_count = usize::from(read_u16(ifd_offset)?);
    for idx in 0..entry_count.min(128) {
        let entry_off = ifd_offset + 2 + idx * 12;
        if entry_off + 12 > tiff.len() {
            break;
        }
        let tag = read_u16(entry_off)?;
        if tag == 0x0112 {
            let val = read_u16(entry_off + 8)?;
            if (1..=8).contains(&val) {
                return Some(val);
            }
        }
    }
    None
}

/// Applies EXIF orientation tag (`1..=8`) to normalize pixel layout (`FR-IMG-002`).
#[must_use]
pub fn apply_exif_orientation(img: DynamicImage, orientation: u16) -> DynamicImage {
    match orientation {
        2 => img.fliph(),
        3 => img.rotate180(),
        4 => img.flipv(),
        5 => img.rotate90().fliph(),
        6 => img.rotate90(),
        7 => img.rotate270().fliph(),
        8 => img.rotate270(),
        _ => img,
    }
}

fn compute_target_dimensions(
    src_w: u32,
    src_h: u32,
    spec: &ImageResizeSpec,
) -> FileFusionResult<(u32, u32)> {
    if spec.max_width == Some(0) || spec.max_height == Some(0) {
        return Err(FileFusionError::new(
            ErrorCode::FmtUnsupported,
            "resize dimensions must be greater than zero",
        ));
    }
    if src_w == 0 || src_h == 0 {
        return Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            "source image has zero dimensions",
        ));
    }

    let bound_w = spec.max_width.unwrap_or(src_w);
    let bound_h = spec.max_height.unwrap_or(src_h);
    if !spec.preserve_aspect_ratio {
        return Ok((bound_w, bound_h));
    }
    if src_w <= bound_w && src_h <= bound_h {
        return Ok((src_w, src_h));
    }

    let scale_w = f64::from(bound_w) / f64::from(src_w);
    let scale_h = f64::from(bound_h) / f64::from(src_h);
    let scale = scale_w.min(scale_h);

    let new_w = ((f64::from(src_w) * scale).round())
        .clamp(1.0, f64::from(u32::MAX))
        .to_bits();
    let _ = new_w;
    // Safe positive rounding without truncation ambiguity
    let dst_w = (1..=bound_w)
        .rfind(|&cand| f64::from(cand) <= f64::from(src_w) * scale + 0.5)
        .unwrap_or(1);
    let dst_h = (1..=bound_h)
        .rfind(|&cand| f64::from(cand) <= f64::from(src_h) * scale + 0.5)
        .unwrap_or(1);
    Ok((dst_w, dst_h))
}

/// Resizes a [`DynamicImage`] cleanly using SIMD `fast_image_resize` (`FR-IMG-002`).
///
/// # Errors
/// Returns [`FileFusionError`] on invalid target dimensions or resizer errors.
pub fn resize_dynamic_image(
    img: &DynamicImage,
    spec: &ImageResizeSpec,
) -> FileFusionResult<DynamicImage> {
    let src_w = img.width();
    let src_h = img.height();
    let (dst_w, dst_h) = compute_target_dimensions(src_w, src_h, spec)?;
    if dst_w == src_w && dst_h == src_h {
        return Ok(img.clone());
    }

    let rgba = img.to_rgba8();
    let src_fir =
        FirImage::from_vec_u8(src_w, src_h, rgba.into_raw(), PixelType::U8x4).map_err(|err| {
            FileFusionError::new(
                ErrorCode::FmtCorrupt,
                format!("failed to prepare source image buffer for SIMD resize: {err}"),
            )
        })?;
    let mut dst_fir = FirImage::new(dst_w, dst_h, PixelType::U8x4);
    let mut resizer = Resizer::new();
    let opts = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(spec.filter.to_fir_filter()));
    resizer
        .resize(&src_fir, &mut dst_fir, Some(&opts))
        .map_err(|err| {
            FileFusionError::new(
                ErrorCode::IntInvariant,
                format!("SIMD image resize failed: {err}"),
            )
        })?;

    let resized_rgba =
        image::RgbaImage::from_raw(dst_w, dst_h, dst_fir.into_vec()).ok_or_else(|| {
            FileFusionError::new(ErrorCode::IntInvariant, "resized buffer geometry mismatch")
        })?;
    Ok(DynamicImage::ImageRgba8(resized_rgba))
}

const fn map_format_to_image_crate(format: FormatId) -> Option<ImageFormat> {
    match format {
        FormatId::Jpeg => Some(ImageFormat::Jpeg),
        FormatId::Png => Some(ImageFormat::Png),
        FormatId::Webp => Some(ImageFormat::WebP),
        FormatId::Gif => Some(ImageFormat::Gif),
        FormatId::Bmp => Some(ImageFormat::Bmp),
        FormatId::Tiff => Some(ImageFormat::Tiff),
        FormatId::Ico => Some(ImageFormat::Ico),
        _ => None,
    }
}

/// Raster-aware [`QualityEvaluator`] decoding original & processed pixels into BT.601 luma/RGBA
/// buffers for empirical SSIM, PSNR, and pixel-exact equality (`FF-CTR-001` §3.3).
#[derive(Debug, Clone, Copy, Default)]
pub struct RasterQualityEvaluator;

impl QualityEvaluator for RasterQualityEvaluator {
    fn evaluator_id(&self) -> &'static str {
        "quality-engine-raster"
    }

    fn evaluate(
        &self,
        original: &ValidatedSource,
        processed_path: &Path,
        ctx: &JobContext,
    ) -> FileFusionResult<QualityMetrics> {
        ctx.check_cancelled()?;
        let orig_bytes = fs::read(&original.path)?;
        let proc_bytes = fs::read(processed_path)?;

        let Ok(orig_img) = image::load_from_memory(&orig_bytes) else {
            return BitExactQualityEvaluator.evaluate(original, processed_path, ctx);
        };
        let Ok(proc_img) = image::load_from_memory(&proc_bytes) else {
            return BitExactQualityEvaluator.evaluate(original, processed_path, ctx);
        };

        if orig_img.width() != proc_img.width() || orig_img.height() != proc_img.height() {
            return BitExactQualityEvaluator.evaluate(original, processed_path, ctx);
        }

        let w = usize::try_from(orig_img.width()).unwrap_or(0);
        let h = usize::try_from(orig_img.height()).unwrap_or(0);
        let orig_rgba = orig_img.to_rgba8().into_raw();
        let proc_rgba = proc_img.to_rgba8().into_raw();
        evaluate_pixel_buffers(w, h, 4, &orig_rgba, &proc_rgba)
    }
}

/// Pure-Rust L3 raster compression engine supporting `JPEG`, `PNG` (`oxipng`), `WebP`,
/// `AVIF` (`ravif`), `GIF`, `BMP`, `TIFF`, and `ICO` (`FR-IMG-001`..`003`).
#[derive(Debug, Clone, Copy, Default)]
pub struct RasterCompressionEngine;

impl RasterCompressionEngine {
    /// Executes image compression/transcoding/resizing directly to `staged_output`,
    /// returning the validated [`CompressionOutcome`] (`FR-CMP-002` compliant).
    ///
    /// # Errors
    /// Returns [`FileFusionError`] on decode/encode failure, cancellation, or memory ceiling.
    pub fn compress_with_options(
        &self,
        input: &ValidatedSource,
        staged_output: &StagedPath,
        opts: &ImageCompressionOptions,
        ctx: &JobContext,
    ) -> FileFusionResult<CompressionOutcome> {
        ctx.check_cancelled()?;
        let req = &opts.base;
        let target_format = effective_target_format(input, req);
        if !matches!(
            target_format,
            FormatId::Jpeg
                | FormatId::Png
                | FormatId::Webp
                | FormatId::Avif
                | FormatId::Gif
                | FormatId::Bmp
                | FormatId::Tiff
                | FormatId::Ico
        ) {
            return Err(FileFusionError::new(
                ErrorCode::FmtUnsupported,
                format!("unsupported raster target format {target_format:?}"),
            ));
        }

        if opts.resize.is_none() && target_format != FormatId::Png {
            if let Some(noop) = prescreen_incompressible_noop(input, req, self.engine_id()) {
                return Ok(noop);
            }
        }

        let est_mb = self.estimate_memory_mb(&input.inspection, req);
        let _mem_guard = ctx.reserve_working_set_mb(est_mb, "raster_compression")?;

        let raw_bytes = fs::read(&input.path)?;
        ctx.check_cancelled()?;
        let cfg = CompressionProfileConfig::for_profile(req.profile);

        // Fast lossless PNG path via `oxipng` when no resizing/re-orient/transcode requested.
        if target_format == FormatId::Png
            && input.inspection.detected_format == FormatId::Png
            && opts.resize.is_none()
        {
            return compress_png_bytes(input, &raw_bytes, staged_output, req, cfg, ctx);
        }

        let decoded_fmt =
            map_format_to_image_crate(input.inspection.detected_format).ok_or_else(|| {
                FileFusionError::new(
                    ErrorCode::FmtUnsupported,
                    format!(
                        "raster decoder unavailable for {:?}",
                        input.inspection.detected_format
                    ),
                )
            })?;
        let mut img =
            image::load_from_memory_with_format(&raw_bytes, decoded_fmt).map_err(|err| {
                FileFusionError::new(
                    ErrorCode::FmtCorrupt,
                    format!("failed to decode raster image: {err}"),
                )
            })?;
        ctx.check_cancelled()?;

        if opts.auto_orient && input.inspection.detected_format == FormatId::Jpeg {
            let orient = detect_exif_orientation(&raw_bytes);
            img = apply_exif_orientation(img, orient);
        }

        if let Some(resize_spec) = &opts.resize {
            img = resize_dynamic_image(&img, resize_spec)?;
            ctx.check_cancelled()?;
        }

        encode_target_image(&img, target_format, staged_output.staged_path(), req, cfg)?;
        ctx.check_cancelled()?;

        let validator = MagicByteOutputValidator;
        let eval = RasterQualityEvaluator;
        let codec_name = codec_label_for(target_format, req.profile);
        finalize_staged_compression(
            input,
            staged_output,
            req,
            &validator,
            Some(&eval),
            codec_name,
            ctx,
        )
    }
}

fn compress_png_bytes(
    input: &ValidatedSource,
    raw_bytes: &[u8],
    staged_output: &StagedPath,
    req: &CompressionRequest,
    cfg: CompressionProfileConfig,
    ctx: &JobContext,
) -> FileFusionResult<CompressionOutcome> {
    let preset = match cfg.effort_level {
        0..=3 => 1,
        4..=6 => 2,
        7..=8 => 3,
        _ => 4,
    };
    let mut oxi_opts = OxiOptions::from_preset(preset);
    oxi_opts.strip = if req.strip_metadata || cfg.strip_metadata_default {
        StripChunks::Safe
    } else {
        StripChunks::None
    };
    let optimized = oxipng::optimize_from_memory(raw_bytes, &oxi_opts).map_err(|err| {
        FileFusionError::new(
            ErrorCode::FmtCorrupt,
            format!("oxipng optimization failed: {err}"),
        )
    })?;
    fs::write(staged_output.staged_path(), &optimized)?;
    ctx.check_cancelled()?;

    let validator = MagicByteOutputValidator;
    let eval = RasterQualityEvaluator;
    finalize_staged_compression(
        input,
        staged_output,
        req,
        &validator,
        Some(&eval),
        "tier1-oxipng",
        ctx,
    )
}

fn codec_label_for(format: FormatId, profile: CompressionProfile) -> String {
    format!("tier1-image-{format:?}-{}", profile.slug()).to_ascii_lowercase()
}

fn encode_jpeg_output(
    img: &DynamicImage,
    out_path: &Path,
    cfg: CompressionProfileConfig,
) -> FileFusionResult<()> {
    let quality = cfg.lossy_quality_q.clamp(40, 95);
    let rgb = img.to_rgb8();
    let file = File::create(out_path)?;
    let writer = BufWriter::new(file);
    let mut encoder = JpegEncoder::new_with_quality(writer, quality);
    encoder
        .encode(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            ColorType::Rgb8.into(),
        )
        .map_err(|err| {
            FileFusionError::new(
                ErrorCode::IntInvariant,
                format!("JPEG encoding failed: {err}"),
            )
        })
}

fn encode_png_output(
    img: &DynamicImage,
    out_path: &Path,
    req: &CompressionRequest,
    cfg: CompressionProfileConfig,
) -> FileFusionResult<()> {
    let mut encoded_buf = Vec::new();
    let rgba = img.to_rgba8();
    PngEncoder::new(&mut encoded_buf)
        .write_image(
            rgba.as_raw(),
            rgba.width(),
            rgba.height(),
            ColorType::Rgba8.into(),
        )
        .map_err(|err| {
            FileFusionError::new(
                ErrorCode::IntInvariant,
                format!("PNG encoding failed: {err}"),
            )
        })?;
    let mut oxi_opts = OxiOptions::from_preset(2);
    oxi_opts.strip = if req.strip_metadata || cfg.strip_metadata_default {
        StripChunks::Safe
    } else {
        StripChunks::None
    };
    let final_png = oxipng::optimize_from_memory(&encoded_buf, &oxi_opts).map_err(|err| {
        FileFusionError::new(
            ErrorCode::IntInvariant,
            format!("oxipng pass failed: {err}"),
        )
    })?;
    fs::write(out_path, final_png)?;
    Ok(())
}

fn encode_avif_output(
    img: &DynamicImage,
    out_path: &Path,
    cfg: CompressionProfileConfig,
) -> FileFusionResult<()> {
    let rgba = img.to_rgba8();
    let width = usize::try_from(rgba.width()).unwrap_or(1);
    let height = usize::try_from(rgba.height()).unwrap_or(1);
    let pixels = rgba.as_raw().as_rgba();
    let avif_img = AvifImg::new(pixels, width, height);
    let quality = f32::from(cfg.lossy_quality_q.clamp(35, 100));
    let speed = if cfg.effort_level >= 9 { 4 } else { 6 };
    let encoded = AvifEncoder::new()
        .with_quality(quality)
        .with_speed(speed)
        .encode_rgba(avif_img)
        .map_err(|err| {
            FileFusionError::new(
                ErrorCode::IntInvariant,
                format!("AVIF encoding failed: {err}"),
            )
        })?;
    fs::write(out_path, encoded.avif_file)?;
    Ok(())
}

fn encode_target_image(
    img: &DynamicImage,
    target_format: FormatId,
    out_path: &Path,
    req: &CompressionRequest,
    cfg: CompressionProfileConfig,
) -> FileFusionResult<()> {
    match target_format {
        FormatId::Jpeg => encode_jpeg_output(img, out_path, cfg),
        FormatId::Png => encode_png_output(img, out_path, req, cfg),
        FormatId::Avif => encode_avif_output(img, out_path, cfg),
        FormatId::Webp => {
            let rgba = img.to_rgba8();
            let file = File::create(out_path)?;
            let writer = BufWriter::new(file);
            WebPEncoder::new_lossless(writer)
                .encode(
                    rgba.as_raw(),
                    rgba.width(),
                    rgba.height(),
                    ColorType::Rgba8.into(),
                )
                .map_err(|err| {
                    FileFusionError::new(
                        ErrorCode::IntInvariant,
                        format!("WebP encoding failed: {err}"),
                    )
                })
        }
        other => {
            let image_fmt = map_format_to_image_crate(other).ok_or_else(|| {
                FileFusionError::new(
                    ErrorCode::FmtUnsupported,
                    format!("unsupported image encoder {other:?}"),
                )
            })?;
            let file = File::create(out_path)?;
            let mut writer = BufWriter::new(file);
            img.write_to(&mut writer, image_fmt).map_err(|err| {
                FileFusionError::new(
                    ErrorCode::IntInvariant,
                    format!("image encoding failed for {other:?}: {err}"),
                )
            })
        }
    }
}

impl CompressionEngine for RasterCompressionEngine {
    fn engine_id(&self) -> &'static str {
        "tier1-compression-image"
    }

    fn supported_formats(&self) -> &[FormatId] {
        &[
            FormatId::Jpeg,
            FormatId::Png,
            FormatId::Webp,
            FormatId::Avif,
            FormatId::Gif,
            FormatId::Bmp,
            FormatId::Tiff,
            FormatId::Ico,
        ]
    }

    fn estimate_memory_mb(
        &self,
        inspection: &FileInspectionReport,
        _opts: &CompressionRequest,
    ) -> u64 {
        let w = u64::from(inspection.metadata_summary.width.unwrap_or(1024));
        let h = u64::from(inspection.metadata_summary.height.unwrap_or(1024));
        let raw_rgba = w.saturating_mul(h).saturating_mul(4);
        // Working set includes source decode, resize target, and encoded scratch (~3x raw RGBA).
        let est_bytes = raw_rgba.saturating_mul(3);
        est_bytes.div_ceil(1_048_576).max(4)
    }

    fn compress(
        &self,
        input: &ValidatedSource,
        staged_output: &StagedPath,
        req: &CompressionRequest,
        ctx: &JobContext,
    ) -> FileFusionResult<CompressionOutcome> {
        let opts = ImageCompressionOptions {
            base: req.clone(),
            resize: None,
            auto_orient: true,
        };
        self.compress_with_options(input, staged_output, &opts, ctx)
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use file_core::{CompressionStatus, ResourceLimits, inspect_path};
    use image::Rgba;

    use super::*;

    fn temp_img_dir(label: &str) -> Result<std::path::PathBuf, Box<dyn Error>> {
        let dir =
            std::env::temp_dir().join(format!("filefusion_img_{label}_{}", std::process::id()));
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    fn generate_test_rgba_png(width: u32, height: u32, path: &Path) -> Result<(), Box<dyn Error>> {
        let mut img = image::RgbaImage::new(width, height);
        for y in 0..height {
            for x in 0..width {
                let r = u8::try_from((x * 3) % 255).unwrap_or(0);
                let g = u8::try_from((y * 5) % 255).unwrap_or(0);
                let b = u8::try_from(((x + y) * 2) % 255).unwrap_or(0);
                img.put_pixel(x, y, Rgba([r, g, b, 255]));
            }
        }
        // Write intentionally unoptimized uncompressed PNG to give oxipng optimization headroom.
        let file = File::create(path)?;
        let encoder = image::codecs::png::PngEncoder::new_with_quality(
            file,
            image::codecs::png::CompressionType::Fast,
            image::codecs::png::FilterType::NoFilter,
        );
        encoder.write_image(img.as_raw(), width, height, ColorType::Rgba8.into())?;
        Ok(())
    }

    #[test]
    fn oxipng_optimizes_png_losslessly_and_triggers_noop_on_second_pass()
    -> Result<(), Box<dyn Error>> {
        let dir = temp_img_dir("oxipng_noop")?;
        let src = dir.join("unopt.png");
        let dest1 = dir.join("opt1.png");
        generate_test_rgba_png(64, 64, &src)?;

        let source = ValidatedSource {
            path: src.clone(),
            inspection: inspect_path(&src, false)?,
        };
        let limits = ResourceLimits::default();
        let ctx = JobContext::new("img_png_01", limits);
        let engine = RasterCompressionEngine;
        let req = CompressionRequest {
            profile: CompressionProfile::LosslessMaximum,
            target_format: None,
            target_max_bytes: None,
            min_ssim: Some(0.999),
            strip_metadata: true,
            no_op_if_larger: true,
        };

        let staged1 = StagedPath::create_sibling(&dest1)?;
        let out1 = engine.compress(&source, &staged1, &req, &ctx)?;
        assert_eq!(out1.status, CompressionStatus::Optimized);
        assert!(out1.bytes_out < out1.bytes_in);
        let q1 = out1.quality.as_ref().ok_or("missing quality metrics")?;
        assert_eq!(q1.bit_exact_lossless, Some(true));
        let final1 = staged1.promote(file_core::CollisionPolicy::RenameWithSuffix)?;

        // Second pass on already-optimized image should hit NoOpRetainedOriginal cleanly.
        let source2 = ValidatedSource {
            path: final1.clone(),
            inspection: inspect_path(&final1, false)?,
        };
        let dest2 = dir.join("opt2.png");
        let staged2 = StagedPath::create_sibling(&dest2)?;
        let out2 = engine.compress(&source2, &staged2, &req, &ctx)?;
        assert_eq!(out2.status, CompressionStatus::NoOpRetainedOriginal);
        assert_eq!(out2.bytes_out, source2.inspection.byte_size);

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn transcodes_png_to_jpeg_webp_avif_and_resizes_via_simd() -> Result<(), Box<dyn Error>> {
        let dir = temp_img_dir("transcode_resize")?;
        let src = dir.join("photo.png");
        generate_test_rgba_png(64, 48, &src)?;
        let source = ValidatedSource {
            path: src.clone(),
            inspection: inspect_path(&src, false)?,
        };
        let engine = RasterCompressionEngine;
        let ctx = JobContext::new("img_transcode", ResourceLimits::default());

        // PNG -> JPEG with high SSIM verification
        let jpg_dest = dir.join("out.jpg");
        let jpg_stage = StagedPath::create_sibling(&jpg_dest)?;
        let jpg_req = CompressionRequest {
            profile: CompressionProfile::BalancedOptimal,
            target_format: Some(FormatId::Jpeg),
            target_max_bytes: None,
            min_ssim: Some(0.90),
            strip_metadata: true,
            no_op_if_larger: true,
        };
        let jpg_out = engine.compress(&source, &jpg_stage, &jpg_req, &ctx)?;
        assert_eq!(jpg_out.status, CompressionStatus::Optimized);
        assert!(jpg_out.quality.and_then(|q| q.ssim).unwrap_or(0.0) >= 0.90);
        jpg_stage.promote(file_core::CollisionPolicy::RenameWithSuffix)?;
        assert!(jpg_dest.exists());

        // PNG -> WebP lossless + SIMD Resize to max 32x32 (preserving 4:3 -> 32x24)
        let webp_dest = dir.join("thumb.webp");
        let webp_stage = StagedPath::create_sibling(&webp_dest)?;
        let webp_opts = ImageCompressionOptions {
            base: CompressionRequest {
                profile: CompressionProfile::WebSharingReady,
                target_format: Some(FormatId::Webp),
                target_max_bytes: None,
                min_ssim: None,
                strip_metadata: true,
                no_op_if_larger: false,
            },
            resize: Some(ImageResizeSpec {
                max_width: Some(32),
                max_height: Some(32),
                filter: ResizeFilter::Lanczos3,
                preserve_aspect_ratio: true,
            }),
            auto_orient: true,
        };
        let webp_out = engine.compress_with_options(&source, &webp_stage, &webp_opts, &ctx)?;
        assert_eq!(webp_out.status, CompressionStatus::Optimized);
        webp_stage.promote(file_core::CollisionPolicy::RenameWithSuffix)?;
        let decoded_webp = image::open(&webp_dest)?;
        assert_eq!(decoded_webp.width(), 32);
        assert_eq!(decoded_webp.height(), 24);

        // PNG -> AVIF pure Rust ravif encode + MagicByteOutputValidator verification
        let avif_dest = dir.join("modern.avif");
        let avif_stage = StagedPath::create_sibling(&avif_dest)?;
        let avif_req = CompressionRequest {
            profile: CompressionProfile::WebSharingReady,
            target_format: Some(FormatId::Avif),
            target_max_bytes: None,
            min_ssim: None,
            strip_metadata: true,
            no_op_if_larger: false,
        };
        let avif_out = engine.compress(&source, &avif_stage, &avif_req, &ctx)?;
        assert_eq!(avif_out.status, CompressionStatus::Optimized);
        avif_stage.promote(file_core::CollisionPolicy::RenameWithSuffix)?;
        let avif_inspected = inspect_path(&avif_dest, true)?;
        assert_eq!(avif_inspected.detected_format, FormatId::Avif);

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn detects_and_applies_exif_orientation() {
        let mut exif_jpeg = Vec::from([
            0xFFu8, 0xD8, 0xFF, 0xE1, 0x00, 0x20, b'E', b'x', b'i', b'f', 0x00, 0x00, b'I', b'I',
            0x2A, 0x00, 0x08, 0x00, 0x00, 0x00, 0x01, 0x00, 0x12, 0x01, 0x03, 0x00, 0x01, 0x00,
            0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0xFF, 0xD9,
        ]);
        // Match APP1 length to actual TIFF segment bytes (`2 + 6 + 22 = 30`)
        let app1_len = u16::try_from(exif_jpeg.len() - 6)
            .unwrap_or(0)
            .to_be_bytes();
        exif_jpeg[4] = app1_len[0];
        exif_jpeg[5] = app1_len[1];
        assert_eq!(detect_exif_orientation(&exif_jpeg), 6);

        let rect = DynamicImage::new_rgba8(4, 2);
        let oriented = apply_exif_orientation(rect, 6);
        assert_eq!(oriented.width(), 2);
        assert_eq!(oriented.height(), 4);
    }
}
