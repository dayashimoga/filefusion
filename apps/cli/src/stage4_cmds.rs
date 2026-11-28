//! Stage 4 CLI subcommands (`pdf`, `repair`, `metadata`, `encrypt`, `decrypt`) delegated
//! cleanly through L5 `JobManager` (`MOD-RULE-03`).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use clap::{Args, Subcommand, ValueEnum};
use file_core::{
    CliExitCode, CollisionPolicy, ErrorCode, FileFusionConfig, FileFusionError, FileFusionResult,
    FormatId,
};
use job_manager::{
    EditableMetadataFields, FfencCipherSuite, FfencKdfParams, JobEncryptResult, JobManager,
    JobMetadataResult, JobPdfOcrResult, JobPdfResult, JobRepairResult, OcrOptions, PdfCropMargins,
    PdfEncryptionAlgorithm, PdfPermissions, PdfProtectionOptions, PdfWatermarkOptions,
    UnifiedRepairDiagnosis, inspect_file,
};

use crate::{collision_policy, emit_json_stdout, emit_text_stdout, report_cli_error};

/// PDF protection cipher preset for `filefusion pdf protect`.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum CliPdfCipher {
    Aes256,
    Aes128,
}

/// Cryptographic cipher selector for `filefusion encrypt`.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum CliEncryptCipher {
    Aes256Gcm,
    #[value(name = "chacha20-poly1305", alias = "cha-cha20-poly1305")]
    ChaCha20Poly1305,
    PdfAes256,
    PdfAes128,
}

/// Subcommands under `filefusion pdf`.
#[derive(Debug, Subcommand)]
pub(crate) enum PdfAction {
    /// Merge multiple unencrypted PDF documents cleanly in order (`FR-PDF-001`).
    Merge {
        /// Output merged PDF path.
        #[arg(value_name = "OUTPUT")]
        output: PathBuf,
        /// Input PDF paths in desired order.
        #[arg(value_name = "INPUTS", required = true)]
        inputs: Vec<PathBuf>,
        /// Overwrite existing destination file (keeping `.bak`).
        #[arg(long)]
        overwrite: bool,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Split a PDF by page-range groups (`"1-2;3-4"`) or fixed page chunk count (`FR-PDF-001`).
    Split {
        /// Input PDF path.
        #[arg(value_name = "INPUT")]
        input: PathBuf,
        /// Destination directory for split PDFs.
        #[arg(short = 'o', long, value_name = "DIR")]
        output_dir: PathBuf,
        /// Fixed number of pages per output PDF (when `--ranges` is omitted).
        #[arg(long, value_name = "N")]
        every: Option<u32>,
        /// Semicolon-separated page ranges (e.g. `"1-2;3,5"`).
        #[arg(long, value_name = "SPEC")]
        ranges: Option<String>,
        /// Overwrite existing split files (keeping `.bak`).
        #[arg(long)]
        overwrite: bool,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Extract or reorder specific 1-based pages (e.g. `"3,1-2"`) into a new PDF.
    Reorder {
        /// Input PDF path.
        #[arg(value_name = "INPUT")]
        input: PathBuf,
        /// 1-based page order specification (e.g. `"3,1-2"`).
        #[arg(long, value_name = "SPEC")]
        pages: String,
        /// Destination output PDF path.
        #[arg(short, long, value_name = "OUTPUT")]
        output: PathBuf,
        /// Overwrite existing output file (keeping `.bak`).
        #[arg(long)]
        overwrite: bool,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Rotate all or selected PDF pages by 90-degree increments (`90`, `180`, `270`, `-90`).
    Rotate {
        /// Input PDF path.
        #[arg(value_name = "INPUT")]
        input: PathBuf,
        /// Rotation degrees (must be a multiple of 90).
        #[arg(long, allow_hyphen_values = true, value_name = "DEG")]
        degrees: i32,
        /// Optional page selection (e.g. `"1,3-5"`); omits for all pages.
        #[arg(short, long, value_name = "SPEC")]
        pages: Option<String>,
        /// Destination output PDF path.
        #[arg(short, long, value_name = "OUTPUT")]
        output: PathBuf,
        /// Overwrite existing output file (keeping `.bak`).
        #[arg(long)]
        overwrite: bool,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Crop PDF page `/CropBox` margins in PDF points (`72 pt = 1 in`).
    Crop {
        /// Input PDF path.
        #[arg(value_name = "INPUT")]
        input: PathBuf,
        /// Uniform inset margin applied to all 4 edges (overridden by per-edge flags).
        #[arg(long, value_name = "PT")]
        margin: Option<f32>,
        /// Left margin trim in PDF points.
        #[arg(long, value_name = "PT")]
        left: Option<f32>,
        /// Bottom margin trim in PDF points.
        #[arg(long, value_name = "PT")]
        bottom: Option<f32>,
        /// Right margin trim in PDF points.
        #[arg(long, value_name = "PT")]
        right: Option<f32>,
        /// Top margin trim in PDF points.
        #[arg(long, value_name = "PT")]
        top: Option<f32>,
        /// Optional page selection (`"1-3"`).
        #[arg(short, long, value_name = "SPEC")]
        pages: Option<String>,
        /// Destination output PDF path.
        #[arg(short, long, value_name = "OUTPUT")]
        output: PathBuf,
        /// Overwrite existing output file (keeping `.bak`).
        #[arg(long)]
        overwrite: bool,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Apply centered transparent vector text watermark onto PDF pages (`FR-PDF-004`).
    Watermark {
        /// Input PDF path.
        #[arg(value_name = "INPUT")]
        input: PathBuf,
        /// Watermark text string.
        #[arg(long, value_name = "TEXT")]
        text: String,
        /// Font size in points (`8..=144`).
        #[arg(long, default_value_t = 42.0)]
        size: f32,
        /// Alpha opacity (`0.05..=1.0`).
        #[arg(long, default_value_t = 0.25)]
        opacity: f32,
        /// Diagonal rotation angle in degrees.
        #[arg(long, allow_hyphen_values = true, default_value_t = 45.0)]
        angle: f32,
        /// Optional page selection (`"1,2"`).
        #[arg(short, long, value_name = "SPEC")]
        pages: Option<String>,
        /// Destination output PDF path.
        #[arg(short, long, value_name = "OUTPUT")]
        output: PathBuf,
        /// Overwrite existing output file (keeping `.bak`).
        #[arg(long)]
        overwrite: bool,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Detect image-only scanned PDF pages and `/Sig` presence (`FR-PDF-003`).
    ScanCheck {
        /// Input PDF path.
        #[arg(value_name = "INPUT")]
        input: PathBuf,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Verify PDF `/Sig` `/ByteRange` integrity and ASN.1 DER framing (`FR-PDF-004`).
    VerifySig {
        /// Input PDF path.
        #[arg(value_name = "INPUT")]
        input: PathBuf,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Inspect PDF `/Encrypt` cipher handler, permissions, and optional candidate password.
    Security {
        /// Input PDF path.
        #[arg(value_name = "INPUT")]
        input: PathBuf,
        /// Optional password probe to test authentication.
        #[arg(long, value_name = "PW")]
        password: Option<String>,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Run local Tier 2 `Tesseract` OCR and embed invisible `3 Tr` searchable text (`FR-PDF-003`).
    Ocr {
        /// Input scanned PDF path.
        #[arg(value_name = "INPUT")]
        input: PathBuf,
        /// Destination searchable PDF path (defaults to `<stem>.ocr.pdf`).
        #[arg(short, long, value_name = "OUTPUT")]
        output: Option<PathBuf>,
        /// Tesseract language profile (e.g. `"eng"` or `"eng+fra"`).
        #[arg(long, default_value = "eng")]
        lang: String,
        /// Page segmentation mode (`0..=13`).
        #[arg(long, default_value_t = 3)]
        psm: u8,
        /// Overwrite existing output file (keeping `.bak`).
        #[arg(long)]
        overwrite: bool,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Encrypt a PDF with `AES-256` or `AES-128` password protection and permissions (`FR-PDF-004`).
    Protect {
        /// Input unencrypted PDF path.
        #[arg(value_name = "INPUT")]
        input: PathBuf,
        /// Destination encrypted PDF path.
        #[arg(short, long, value_name = "OUTPUT")]
        output: PathBuf,
        /// Mandatory non-empty owner/permissions password.
        #[arg(long, value_name = "PW")]
        owner_password: String,
        /// Optional user open password.
        #[arg(long, value_name = "PW")]
        user_password: Option<String>,
        /// Encryption cipher algorithm.
        #[arg(long, value_enum, default_value = "aes256")]
        cipher: CliPdfCipher,
        /// Lock permissions to read-only (accessibility allowed).
        #[arg(long)]
        read_only: bool,
        /// Allow low-res printing while disabling copy/edit/assembly.
        #[arg(long)]
        print_only: bool,
        /// Overwrite existing output file (keeping `.bak`).
        #[arg(long)]
        overwrite: bool,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Unlock and remove `/Encrypt` protection from a PDF given its password (`FR-PDF-004`).
    Unlock {
        /// Input encrypted PDF path.
        #[arg(value_name = "INPUT")]
        input: PathBuf,
        /// Destination decrypted PDF path.
        #[arg(short, long, value_name = "OUTPUT")]
        output: PathBuf,
        /// User or owner password.
        #[arg(long, value_name = "PW")]
        password: String,
        /// Overwrite existing output file (keeping `.bak`).
        #[arg(long)]
        overwrite: bool,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
}

/// Arguments for `filefusion repair`.
#[derive(Debug, Args)]
pub(crate) struct RepairArgs {
    /// Source damaged or candidate `PDF`, `ZIP`, or `MP4`/`MOV`/`MKV`/`WebM` file.
    #[arg(value_name = "INPUT")]
    pub input: PathBuf,
    /// Destination repaired path (defaults to `<stem>.repaired.<ext>`).
    #[arg(short, long, value_name = "OUTPUT")]
    pub output: Option<PathBuf>,
    /// Diagnose structural recoverability honestly without writing any repaired artifact.
    #[arg(long)]
    pub diagnose_only: bool,
    /// Overwrite existing destination file (keeping `.bak`).
    #[arg(long)]
    pub overwrite: bool,
    /// Emit machine-readable JSON output.
    #[arg(long)]
    pub json: bool,
}

/// Subcommands under `filefusion metadata`.
#[derive(Debug, Subcommand)]
pub(crate) enum MetadataAction {
    /// Inspect normalized container/document/media metadata and privacy risk flags (`FR-MET-001`).
    Inspect {
        /// Input file path.
        #[arg(value_name = "INPUT")]
        input: PathBuf,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Scrub PII/EXIF/XMP/author risks and optionally set/clear mutable tags (`FR-MET-002`).
    Scrub {
        /// Input file path.
        #[arg(value_name = "INPUT")]
        input: PathBuf,
        /// Destination sanitized file path (defaults to `<stem>.clean.<ext>`).
        #[arg(short, long, value_name = "OUTPUT")]
        output: Option<PathBuf>,
        /// Optional new document/track title.
        #[arg(long, value_name = "TITLE")]
        title: Option<String>,
        /// Optional new author/artist tag.
        #[arg(long, value_name = "AUTHOR")]
        author: Option<String>,
        /// Optional new subject/album tag.
        #[arg(long, value_name = "SUBJECT")]
        subject: Option<String>,
        /// Optional new keywords tag.
        #[arg(long, value_name = "KEYWORDS")]
        keywords: Option<String>,
        /// Optional creator software string (PDF).
        #[arg(long, value_name = "CREATOR")]
        creator: Option<String>,
        /// Skip default privacy scrubbing (apply explicit tag edits only).
        #[arg(long)]
        no_scrub: bool,
        /// Overwrite existing destination file (keeping `.bak`).
        #[arg(long)]
        overwrite: bool,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Edit mutable metadata tags while keeping privacy scrub opt-in (`--scrub`).
    Edit {
        /// Input file path.
        #[arg(value_name = "INPUT")]
        input: PathBuf,
        /// Destination updated file path (defaults to `<stem>.edited.<ext>`).
        #[arg(short, long, value_name = "OUTPUT")]
        output: Option<PathBuf>,
        /// New title (`""` clears existing value).
        #[arg(long, value_name = "TITLE")]
        title: Option<String>,
        /// New author/artist (`""` clears existing value).
        #[arg(long, value_name = "AUTHOR")]
        author: Option<String>,
        /// New subject/album.
        #[arg(long, value_name = "SUBJECT")]
        subject: Option<String>,
        /// New keywords.
        #[arg(long, value_name = "KEYWORDS")]
        keywords: Option<String>,
        /// New creator tool.
        #[arg(long, value_name = "CREATOR")]
        creator: Option<String>,
        /// Also scrub sensitive EXIF/GPS/XMP/author metadata before applying edits.
        #[arg(long)]
        scrub: bool,
        /// Overwrite existing destination file (keeping `.bak`).
        #[arg(long)]
        overwrite: bool,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
}

/// Arguments for `filefusion encrypt`.
#[derive(Debug, Args)]
pub(crate) struct EncryptArgs {
    /// Input file to encrypt.
    #[arg(value_name = "INPUT")]
    pub input: PathBuf,
    /// Destination encrypted artifact (defaults to `<file>.ffenc` or `<stem>.enc.pdf`).
    #[arg(short, long, value_name = "OUTPUT")]
    pub output: Option<PathBuf>,
    /// Encryption cipher / container mode (`aes256-gcm` default `.ffenc`).
    #[arg(long, value_enum, default_value = "aes256-gcm")]
    pub cipher: CliEncryptCipher,
    /// Passphrase / PDF user password literal.
    #[arg(long, value_name = "SECRET")]
    pub passphrase: Option<String>,
    /// Environment variable holding passphrase (avoids secret on process argv).
    #[arg(long, value_name = "ENV_VAR")]
    pub passphrase_env: Option<String>,
    /// Owner password for PDF protection (defaults to `--passphrase` when targeting PDF).
    #[arg(long, value_name = "SECRET")]
    pub owner_password: Option<String>,
    /// Use lighter Argon2id work factor (`1 MiB, t=1`) when creating `.ffenc` for fast test runs.
    #[arg(long)]
    pub fast_kdf: bool,
    /// Overwrite existing destination file (keeping `.bak`).
    #[arg(long)]
    pub overwrite: bool,
    /// Emit machine-readable JSON output.
    #[arg(long)]
    pub json: bool,
}

/// Arguments for `filefusion decrypt`.
#[derive(Debug, Args)]
pub(crate) struct DecryptArgs {
    /// Input encrypted `.ffenc` or password-protected PDF file.
    #[arg(value_name = "INPUT")]
    pub input: PathBuf,
    /// Destination decrypted file (defaults to stripping `.ffenc` or `<stem>.unlocked.pdf`).
    #[arg(short, long, value_name = "OUTPUT")]
    pub output: Option<PathBuf>,
    /// Passphrase / PDF password literal.
    #[arg(long, value_name = "SECRET")]
    pub passphrase: Option<String>,
    /// Environment variable holding passphrase.
    #[arg(long, value_name = "ENV_VAR")]
    pub passphrase_env: Option<String>,
    /// Overwrite existing destination file (keeping `.bak`).
    #[arg(long)]
    pub overwrite: bool,
    /// Emit machine-readable JSON output.
    #[arg(long)]
    pub json: bool,
}

pub(crate) const fn pdf_action_wants_json(action: &PdfAction) -> bool {
    match action {
        PdfAction::Merge { json, .. }
        | PdfAction::Split { json, .. }
        | PdfAction::Reorder { json, .. }
        | PdfAction::Rotate { json, .. }
        | PdfAction::Crop { json, .. }
        | PdfAction::Watermark { json, .. }
        | PdfAction::ScanCheck { json, .. }
        | PdfAction::VerifySig { json, .. }
        | PdfAction::Security { json, .. }
        | PdfAction::Ocr { json, .. }
        | PdfAction::Protect { json, .. }
        | PdfAction::Unlock { json, .. } => *json,
    }
}

pub(crate) const fn metadata_action_wants_json(action: &MetadataAction) -> bool {
    match action {
        MetadataAction::Inspect { json, .. }
        | MetadataAction::Scrub { json, .. }
        | MetadataAction::Edit { json, .. } => *json,
    }
}

pub(crate) fn handle_pdf_command(
    action: &PdfAction,
    config: &FileFusionConfig,
    log_json: bool,
) -> CliExitCode {
    let mgr = JobManager::from_config(config);
    let ctx = mgr.create_context("cli-pdf");

    match action {
        PdfAction::Merge { .. }
        | PdfAction::Split { .. }
        | PdfAction::Reorder { .. }
        | PdfAction::Rotate { .. } => handle_pdf_pages(&mgr, &ctx, action, log_json),
        PdfAction::Crop { .. } | PdfAction::Watermark { .. } => {
            handle_pdf_geometry_and_mark(&mgr, &ctx, action, log_json)
        }
        PdfAction::ScanCheck { .. } | PdfAction::VerifySig { .. } | PdfAction::Security { .. } => {
            handle_pdf_inspect(&mgr, &ctx, action, log_json)
        }
        PdfAction::Ocr { .. } | PdfAction::Protect { .. } | PdfAction::Unlock { .. } => {
            handle_pdf_ocr_and_crypto(&mgr, &ctx, action, log_json)
        }
    }
}

fn handle_pdf_pages(
    mgr: &JobManager,
    ctx: &job_manager::JobContext,
    action: &PdfAction,
    log_json: bool,
) -> CliExitCode {
    match action {
        PdfAction::Merge {
            output,
            inputs,
            overwrite,
            json,
        } => emit_pdf_or_err(
            mgr.pdf_merge(inputs, output, collision_policy(*overwrite), ctx),
            *json,
            log_json,
        ),
        PdfAction::Split {
            input,
            output_dir,
            every,
            ranges,
            overwrite,
            json,
        } => emit_pdf_or_err(
            mgr.pdf_split(
                input,
                output_dir,
                *every,
                ranges.as_deref(),
                collision_policy(*overwrite),
                ctx,
            ),
            *json,
            log_json,
        ),
        PdfAction::Reorder {
            input,
            pages,
            output,
            overwrite,
            json,
        } => emit_pdf_or_err(
            mgr.pdf_reorder(input, pages, output, collision_policy(*overwrite), ctx),
            *json,
            log_json,
        ),
        PdfAction::Rotate {
            input,
            degrees,
            pages,
            output,
            overwrite,
            json,
        } => emit_pdf_or_err(
            mgr.pdf_rotate(
                input,
                pages.as_deref(),
                *degrees,
                output,
                collision_policy(*overwrite),
                ctx,
            ),
            *json,
            log_json,
        ),
        _ => CliExitCode::InvalidUsage,
    }
}

fn handle_pdf_geometry_and_mark(
    mgr: &JobManager,
    ctx: &job_manager::JobContext,
    action: &PdfAction,
    log_json: bool,
) -> CliExitCode {
    match action {
        PdfAction::Crop {
            input,
            margin,
            left,
            bottom,
            right,
            top,
            pages,
            output,
            overwrite,
            json,
        } => {
            let base = margin.unwrap_or(0.0);
            let margins = PdfCropMargins {
                left_pt: left.unwrap_or(base),
                bottom_pt: bottom.unwrap_or(base),
                right_pt: right.unwrap_or(base),
                top_pt: top.unwrap_or(base),
            };
            emit_pdf_or_err(
                mgr.pdf_crop(
                    input,
                    pages.as_deref(),
                    margins,
                    output,
                    collision_policy(*overwrite),
                    ctx,
                ),
                *json,
                log_json,
            )
        }
        PdfAction::Watermark {
            input,
            text,
            size,
            opacity,
            angle,
            pages,
            output,
            overwrite,
            json,
        } => handle_pdf_watermark(
            mgr,
            ctx,
            input,
            text,
            *size,
            *opacity,
            *angle,
            pages.as_deref(),
            output,
            *overwrite,
            *json,
            log_json,
        ),
        _ => CliExitCode::InvalidUsage,
    }
}

fn handle_pdf_inspect(
    mgr: &JobManager,
    ctx: &job_manager::JobContext,
    action: &PdfAction,
    log_json: bool,
) -> CliExitCode {
    match action {
        PdfAction::ScanCheck { input, json } => match mgr.pdf_scan_check(input, ctx) {
            Ok(rep) if *json => emit_json_stdout(&rep),
            Ok(rep) => emit_text_stdout(
                &format!(
                    "PDF Scan Check: pages={}, scanned_image_only={}, image_only_pages={:?}, has_signatures={}",
                    rep.total_pages,
                    rep.is_scanned_image_only,
                    rep.image_only_pages,
                    rep.has_digital_signatures
                ),
                false,
            ),
            Err(err) => report_cli_error(&err, *json || log_json),
        },
        PdfAction::VerifySig { input, json } => match mgr.pdf_verify_signatures(input, ctx) {
            Ok(rep) if *json => emit_json_stdout(&rep),
            Ok(rep) => emit_text_stdout(
                &format!(
                    "PDF Signatures: count={}, all_byte_ranges_intact={}\nNote: {}",
                    rep.signature_count, rep.all_byte_ranges_intact, rep.certificate_trust_note
                ),
                false,
            ),
            Err(err) => report_cli_error(&err, *json || log_json),
        },
        PdfAction::Security {
            input,
            password,
            json,
        } => match mgr.pdf_inspect_security(input, password.as_deref(), ctx) {
            Ok(rep) if *json => emit_json_stdout(&rep),
            Ok(rep) => emit_text_stdout(
                &format!(
                    "PDF Security: encrypted={}, cipher={}, key_bits={:?}, password_verified={:?}",
                    rep.is_encrypted, rep.cipher_name, rep.key_bits, rep.password_verified
                ),
                false,
            ),
            Err(err) => report_cli_error(&err, *json || log_json),
        },
        _ => CliExitCode::InvalidUsage,
    }
}

fn handle_pdf_ocr_and_crypto(
    mgr: &JobManager,
    ctx: &job_manager::JobContext,
    action: &PdfAction,
    log_json: bool,
) -> CliExitCode {
    match action {
        PdfAction::Ocr {
            input,
            output,
            lang,
            psm,
            overwrite,
            json,
        } => {
            let dest = output
                .clone()
                .unwrap_or_else(|| default_suffixed_path(input, "ocr", "pdf"));
            let opts = OcrOptions {
                language: lang.clone(),
                page_seg_mode: *psm,
                ..OcrOptions::default()
            };
            match mgr.pdf_ocr(input, &dest, &opts, collision_policy(*overwrite), ctx) {
                Ok(rep) if *json => emit_json_stdout(&rep),
                Ok(rep) => emit_text_stdout(&format_ocr_human(&rep), false),
                Err(err) => report_cli_error(&err, *json || log_json),
            }
        }
        PdfAction::Protect {
            input,
            output,
            owner_password,
            user_password,
            cipher,
            read_only,
            print_only,
            overwrite,
            json,
        } => {
            let permissions = if *read_only {
                PdfPermissions::read_only()
            } else if *print_only {
                PdfPermissions::restricted_print_only()
            } else {
                PdfPermissions::all()
            };
            let opts = PdfProtectionOptions {
                user_password: user_password.clone(),
                owner_password: owner_password.clone(),
                algorithm: match cipher {
                    CliPdfCipher::Aes256 => PdfEncryptionAlgorithm::Aes256,
                    CliPdfCipher::Aes128 => PdfEncryptionAlgorithm::Aes128,
                },
                permissions,
                encrypt_metadata: true,
            };
            match mgr.encrypt_pdf(input, output, &opts, collision_policy(*overwrite), ctx) {
                Ok(rep) if *json => emit_json_stdout(&rep),
                Ok(rep) => emit_text_stdout(&format_encrypt_human(&rep), false),
                Err(err) => report_cli_error(&err, *json || log_json),
            }
        }
        PdfAction::Unlock {
            input,
            output,
            password,
            overwrite,
            json,
        } => match mgr.decrypt_pdf(input, output, password, collision_policy(*overwrite), ctx) {
            Ok(rep) if *json => emit_json_stdout(&rep),
            Ok(rep) => emit_text_stdout(&format_encrypt_human(&rep), false),
            Err(err) => report_cli_error(&err, *json || log_json),
        },
        _ => CliExitCode::InvalidUsage,
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_pdf_watermark(
    mgr: &JobManager,
    ctx: &job_manager::JobContext,
    input: &Path,
    text: &str,
    size: f32,
    opacity: f32,
    angle: f32,
    pages_spec: Option<&str>,
    output: &Path,
    overwrite: bool,
    json: bool,
    log_json: bool,
) -> CliExitCode {
    let opts = PdfWatermarkOptions {
        text: text.to_owned(),
        font_size_pt: size,
        opacity,
        angle_deg: angle,
        pages: None,
    };
    let res = mgr.pdf_watermark(
        input,
        pages_spec,
        &opts,
        output,
        collision_policy(overwrite),
        ctx,
    );
    emit_pdf_or_err(res, json, log_json)
}

pub(crate) fn handle_repair_command(
    args: &RepairArgs,
    config: &FileFusionConfig,
    log_json: bool,
) -> CliExitCode {
    let mgr = JobManager::from_config(config);
    let ctx = mgr.create_context("cli-repair");

    if args.diagnose_only {
        return match mgr.diagnose_repair(&args.input, &ctx) {
            Ok(diag) => {
                if args.json {
                    emit_json_stdout(&diag)
                } else {
                    emit_text_stdout(&format_diagnosis_human(&diag), false)
                }
            }
            Err(err) => report_cli_error(&err, args.json || log_json),
        };
    }

    let ext = args
        .input
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("bin");
    let dest = args
        .output
        .clone()
        .unwrap_or_else(|| default_suffixed_path(&args.input, "repaired", ext));
    match mgr.repair_file(&args.input, &dest, collision_policy(args.overwrite), &ctx) {
        Ok(res) => {
            if args.json {
                emit_json_stdout(&res)
            } else {
                emit_text_stdout(&format_repair_human(&res), false)
            }
        }
        Err(err) => report_cli_error(&err, args.json || log_json),
    }
}

pub(crate) fn handle_metadata_command(
    action: &MetadataAction,
    config: &FileFusionConfig,
    log_json: bool,
) -> CliExitCode {
    let mgr = JobManager::from_config(config);
    let ctx = mgr.create_context("cli-metadata");

    match action {
        MetadataAction::Inspect { input, json } => match mgr.inspect_metadata(input, &ctx) {
            Ok(summary) => {
                if *json {
                    emit_json_stdout(&summary)
                } else {
                    emit_text_stdout(
                        &format!(
                            "Metadata: title={:?}, author={:?}, creator={:?}, encrypted={}, exif={}, xmp={}, icc={}, gps_risk={}, pii_author_risk={}",
                            summary.title,
                            summary.author,
                            summary.creator_tool,
                            summary.is_encrypted,
                            summary.has_exif,
                            summary.has_xmp,
                            summary.has_icc_profile,
                            summary.has_gps_privacy_risk,
                            summary.has_pii_author_risk
                        ),
                        false,
                    )
                }
            }
            Err(err) => report_cli_error(&err, *json || log_json),
        },
        MetadataAction::Scrub {
            input,
            output,
            title,
            author,
            subject,
            keywords,
            creator,
            no_scrub,
            overwrite,
            json,
        } => {
            let ext = input.extension().and_then(|e| e.to_str()).unwrap_or("bin");
            let dest = output
                .clone()
                .unwrap_or_else(|| default_suffixed_path(input, "clean", ext));
            run_metadata_update(
                &mgr,
                &ctx,
                input,
                &dest,
                !*no_scrub,
                title.as_deref(),
                author.as_deref(),
                subject.as_deref(),
                keywords.as_deref(),
                creator.as_deref(),
                collision_policy(*overwrite),
                *json,
                log_json,
            )
        }
        MetadataAction::Edit {
            input,
            output,
            title,
            author,
            subject,
            keywords,
            creator,
            scrub,
            overwrite,
            json,
        } => {
            let ext = input.extension().and_then(|e| e.to_str()).unwrap_or("bin");
            let dest = output
                .clone()
                .unwrap_or_else(|| default_suffixed_path(input, "edited", ext));
            run_metadata_update(
                &mgr,
                &ctx,
                input,
                &dest,
                *scrub,
                title.as_deref(),
                author.as_deref(),
                subject.as_deref(),
                keywords.as_deref(),
                creator.as_deref(),
                collision_policy(*overwrite),
                *json,
                log_json,
            )
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run_metadata_update(
    mgr: &JobManager,
    ctx: &job_manager::JobContext,
    input: &Path,
    dest: &Path,
    scrub_privacy: bool,
    title: Option<&str>,
    author: Option<&str>,
    subject: Option<&str>,
    keywords: Option<&str>,
    creator: Option<&str>,
    policy: CollisionPolicy,
    json: bool,
    log_json: bool,
) -> CliExitCode {
    let to_edit = |v: Option<&str>| {
        v.map(|s| {
            if s.is_empty() {
                None
            } else {
                Some(s.to_owned())
            }
        })
    };
    let edits = EditableMetadataFields {
        title: to_edit(title),
        author: to_edit(author),
        subject_or_comment: to_edit(subject),
        keywords: to_edit(keywords),
        creator_tool: to_edit(creator),
    };
    let has_edits = edits != EditableMetadataFields::default();
    match mgr.sanitize_and_edit_metadata(
        input,
        dest,
        scrub_privacy,
        has_edits.then_some(&edits),
        policy,
        ctx,
    ) {
        Ok(res) => {
            if json {
                emit_json_stdout(&res)
            } else {
                emit_text_stdout(&format_metadata_human(&res), false)
            }
        }
        Err(err) => report_cli_error(&err, json || log_json),
    }
}

pub(crate) fn handle_encrypt_command(
    args: &EncryptArgs,
    config: &FileFusionConfig,
    log_json: bool,
) -> CliExitCode {
    let mgr = JobManager::from_config(config);
    let ctx = mgr.create_context("cli-encrypt");
    let secret = match resolve_secret(args.passphrase.as_deref(), args.passphrase_env.as_deref()) {
        Ok(s) => s,
        Err(err) => return report_cli_error(&err, args.json || log_json),
    };

    match args.cipher {
        CliEncryptCipher::Aes256Gcm | CliEncryptCipher::ChaCha20Poly1305 => {
            let dest = args
                .output
                .clone()
                .unwrap_or_else(|| default_ffenc_encrypt_path(&args.input));
            let suite = match args.cipher {
                CliEncryptCipher::ChaCha20Poly1305 => FfencCipherSuite::ChaCha20Poly1305,
                _ => FfencCipherSuite::Aes256Gcm,
            };
            let kdf = if args.fast_kdf {
                FfencKdfParams::fast_test()
            } else {
                FfencKdfParams::default()
            };
            match mgr.encrypt_ffenc(
                &args.input,
                &dest,
                secret.as_bytes(),
                suite,
                kdf,
                collision_policy(args.overwrite),
                &ctx,
            ) {
                Ok(rep) => {
                    if args.json {
                        emit_json_stdout(&rep)
                    } else {
                        emit_text_stdout(&format_encrypt_human(&rep), false)
                    }
                }
                Err(err) => report_cli_error(&err, args.json || log_json),
            }
        }
        CliEncryptCipher::PdfAes256 | CliEncryptCipher::PdfAes128 => {
            let dest = args
                .output
                .clone()
                .unwrap_or_else(|| default_suffixed_path(&args.input, "enc", "pdf"));
            let owner = args.owner_password.as_deref().unwrap_or(&secret).to_owned();
            let opts = PdfProtectionOptions {
                user_password: Some(secret),
                owner_password: owner,
                algorithm: if matches!(args.cipher, CliEncryptCipher::PdfAes128) {
                    PdfEncryptionAlgorithm::Aes128
                } else {
                    PdfEncryptionAlgorithm::Aes256
                },
                permissions: PdfPermissions::all(),
                encrypt_metadata: true,
            };
            match mgr.encrypt_pdf(
                &args.input,
                &dest,
                &opts,
                collision_policy(args.overwrite),
                &ctx,
            ) {
                Ok(rep) => {
                    if args.json {
                        emit_json_stdout(&rep)
                    } else {
                        emit_text_stdout(&format_encrypt_human(&rep), false)
                    }
                }
                Err(err) => report_cli_error(&err, args.json || log_json),
            }
        }
    }
}

pub(crate) fn handle_decrypt_command(
    args: &DecryptArgs,
    config: &FileFusionConfig,
    log_json: bool,
) -> CliExitCode {
    let mgr = JobManager::from_config(config);
    let ctx = mgr.create_context("cli-decrypt");
    let secret = match resolve_secret(args.passphrase.as_deref(), args.passphrase_env.as_deref()) {
        Ok(s) => s,
        Err(err) => return report_cli_error(&err, args.json || log_json),
    };

    let inspected = match inspect_file(&args.input, false) {
        Ok(r) => r,
        Err(err) => return report_cli_error(&err, args.json || log_json),
    };

    let res = if inspected.detected_format == FormatId::Pdf {
        let dest = args
            .output
            .clone()
            .unwrap_or_else(|| default_suffixed_path(&args.input, "unlocked", "pdf"));
        mgr.decrypt_pdf(
            &args.input,
            &dest,
            &secret,
            collision_policy(args.overwrite),
            &ctx,
        )
    } else {
        let dest = args
            .output
            .clone()
            .unwrap_or_else(|| default_ffenc_decrypt_path(&args.input));
        mgr.decrypt_ffenc(
            &args.input,
            &dest,
            secret.as_bytes(),
            collision_policy(args.overwrite),
            &ctx,
        )
    };

    match res {
        Ok(rep) => {
            if args.json {
                emit_json_stdout(&rep)
            } else {
                emit_text_stdout(&format_encrypt_human(&rep), false)
            }
        }
        Err(err) => report_cli_error(&err, args.json || log_json),
    }
}

fn resolve_secret(literal: Option<&str>, env_var: Option<&str>) -> FileFusionResult<String> {
    if let Some(val) = literal {
        if val.is_empty() {
            return Err(FileFusionError::new(
                ErrorCode::FmtUnsupported,
                "passphrase cannot be empty",
            ));
        }
        return Ok(val.to_owned());
    }
    if let Some(var) = env_var {
        let val = std::env::var(var).map_err(|_| {
            FileFusionError::new(
                ErrorCode::IoNotFound,
                format!("passphrase environment variable `{var}` is not set"),
            )
        })?;
        if val.is_empty() {
            return Err(FileFusionError::new(
                ErrorCode::FmtUnsupported,
                format!("passphrase environment variable `{var}` is empty"),
            ));
        }
        return Ok(val);
    }
    Err(FileFusionError::new(
        ErrorCode::FmtUnsupported,
        "provide `--passphrase <SECRET>` or `--passphrase-env <ENV_VAR>`",
    ))
}

fn append_path_suffix(input: &Path, suffix: &str) -> PathBuf {
    let mut os = input.as_os_str().to_os_string();
    os.push(suffix);
    PathBuf::from(os)
}

fn default_ffenc_encrypt_path(input: &Path) -> PathBuf {
    append_path_suffix(input, ".ffenc")
}

fn default_ffenc_decrypt_path(input: &Path) -> PathBuf {
    if input.extension().is_some_and(|ext| ext == "ffenc") {
        input.with_extension("")
    } else {
        append_path_suffix(input, ".dec")
    }
}

fn default_suffixed_path(input: &Path, suffix: &str, ext: &str) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    let file_name = format!("{stem}.{suffix}.{ext}");
    match input.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join(file_name),
        _ => PathBuf::from(file_name),
    }
}

fn emit_pdf_or_err(res: FileFusionResult<JobPdfResult>, json: bool, log_json: bool) -> CliExitCode {
    match res {
        Ok(rep) => {
            if json {
                emit_json_stdout(&rep)
            } else {
                let mut text = format!(
                    "PDF {}: pages={}, output_bytes={}",
                    rep.operation, rep.page_count, rep.output_bytes
                );
                for p in &rep.output_paths {
                    let _ = write!(text, "\n  -> {p}");
                }
                emit_text_stdout(&text, false)
            }
        }
        Err(err) => report_cli_error(&err, json || log_json),
    }
}

fn format_diagnosis_human(diag: &UnifiedRepairDiagnosis) -> String {
    format!(
        "Diagnosis: format={:?}, status={:?}, engine={}\nSummary: {}",
        diag.detected_format, diag.recoverability, diag.recommended_engine, diag.issue_summary
    )
}

fn format_repair_human(res: &JobRepairResult) -> String {
    format!(
        "Repaired: {} -> {}\nFormat: {:?} | Engine: {} | Recovered: {} (skipped {}) | Bytes: {}",
        res.input_path,
        res.output_path,
        res.outcome.detected_format,
        res.outcome.engine_used,
        res.outcome.items_recovered,
        res.outcome.items_skipped,
        res.outcome.repaired_bytes,
    )
}

fn format_metadata_human(res: &JobMetadataResult) -> String {
    format!(
        "Metadata Updated: {} -> {}\nFormat: {:?} | Engine: {} | Scrubbed: {} | Bytes: {}",
        res.input_path,
        res.output_path,
        res.report.format,
        res.report.engine_used,
        res.report.scrubbed_privacy_fields,
        res.report.output_bytes,
    )
}

fn format_encrypt_human(res: &JobEncryptResult) -> String {
    format!(
        "Crypto ({}): {} -> {} ({} bytes)",
        res.algorithm_label, res.input_path, res.output_path, res.output_bytes
    )
}

fn format_ocr_human(res: &JobPdfOcrResult) -> String {
    format!(
        "PDF OCR Complete: {} -> {} ({} bytes, pages={}, language={})",
        res.input_path,
        res.output_path,
        res.output_bytes,
        res.ocr.pages.len(),
        res.ocr.language,
    )
}
