//! `compression-archive`: L3 multi-format archive, stream compression, `.ffenc`
//! authenticated encryption, and ZIP central-directory repair engine
//! (`FR-ARC-001`..`003`, `FR-SEC-001`..`002`, `FR-REP-001`, `FR-CMP-002`).

#![forbid(unsafe_code)]

pub mod container;
pub mod ffenc;
pub mod repair;
pub mod streams;

use std::fs;
use std::path::Path;

use compression_core::{finalize_staged_compression, prescreen_incompressible_noop};
use file_core::{
    CompressionEngine, CompressionOutcome, CompressionRequest, ErrorCode, FileFusionError,
    FileFusionResult, FileInspectionReport, FormatId, JobContext, OutputValidator,
    QualityEvaluator, QualityMetrics, ResourceLimits, StagedPath, ValidatedSource,
    ValidationOutcome, detect_signature_from_path,
};
use quality_engine::BitExactQualityEvaluator;
use serde::{Deserialize, Serialize};
use validation_engine::MagicByteOutputValidator;

#[cfg(feature = "native-process")]
use compression_core::CompressionProfileConfig;
#[cfg(any(feature = "native-process", test))]
use file_core::CompressionProfile;
#[cfg(feature = "native-process")]
use file_core::{IsolatedProcessRunner, ProcessInvocation};

pub use container::{
    ArchiveContainerKind, ArchiveExtractionReport, ArchiveInputEntry, ArchiveMemberMetadata,
    ArchiveWriteSummary, create_archive_from_entries, extract_archive_safely, list_archive_members,
    read_archive_entries_checked,
};
pub use ffenc::{
    FFENC_MAGIC, FfencCipherSuite, FfencHeaderInfo, FfencKdfParams, decrypt_ffenc_bytes,
    decrypt_from_ffenc_file, encrypt_ffenc_bytes, encrypt_to_ffenc_file, inspect_ffenc_header,
};
pub use repair::{ZipHealthReport, ZipRepairReport, inspect_zip_health, repair_truncated_zip};
pub use streams::{
    StreamCodec, compress_stream_bytes, decompress_stream_bounded, read_stream_bounded,
};

/// Options controlling archive/stream repacking via [`ArchiveCompressionEngine::compress_with_options`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArchiveCompressionOptions {
    /// Standard `FileFusion` compression contract (`FF-CTR-001` §3.1).
    pub base: CompressionRequest,
    /// Optional explicit container target (e.g., `ArchiveContainerKind::TarZst`).
    pub target_container: Option<ArchiveContainerKind>,
}

/// Validates staged archive or stream outputs before atomic commit (`FR-VAL-001`).
#[derive(Debug, Clone, Copy, Default)]
pub struct ArchiveOutputValidator {
    /// Expected target format override (`FormatId::Unknown` allowed only for Brotli `.br`).
    pub allow_brotli: bool,
}

impl OutputValidator for ArchiveOutputValidator {
    fn validator_id(&self) -> &'static str {
        "compression-archive-validator"
    }

    fn validate_staged(
        &self,
        staged_path: &Path,
        expected_format: FormatId,
        limits: &ResourceLimits,
    ) -> FileFusionResult<ValidationOutcome> {
        if self.allow_brotli && expected_format == FormatId::Unknown {
            let bytes = fs::read(staged_path)?;
            let _decoded = decompress_stream_bounded(&bytes, StreamCodec::Brotli, limits, 0)?;
            return Ok(ValidationOutcome {
                valid: true,
                validator_id: String::from(self.validator_id()),
                detail: format!("Verified Brotli stream ({} bytes)", bytes.len()),
            });
        }
        MagicByteOutputValidator.validate_staged(staged_path, expected_format, limits)
    }
}

/// Empirical bit-exact lossless evaluator across archives and compressed byte streams (`FR-ARC-003`).
#[derive(Debug, Clone, Copy, Default)]
pub struct ArchiveQualityEvaluator {
    /// Optional explicit container kind override for the original artifact.
    pub source_kind: Option<ArchiveContainerKind>,
    /// Optional explicit container kind override for the staged/candidate artifact.
    pub target_kind: Option<ArchiveContainerKind>,
}

impl QualityEvaluator for ArchiveQualityEvaluator {
    fn evaluator_id(&self) -> &'static str {
        "compression-archive-bit-exact"
    }

    fn evaluate(
        &self,
        original: &ValidatedSource,
        candidate: &Path,
        ctx: &JobContext,
    ) -> FileFusionResult<QualityMetrics> {
        ctx.check_cancelled()?;
        let orig_kind_res = match self.source_kind {
            Some(kind) => Ok(kind),
            None => ArchiveContainerKind::detect_from_path_with_limits(&original.path, &ctx.limits),
        };
        let cand_kind_res = match self.target_kind {
            Some(kind) => Ok(kind),
            None => ArchiveContainerKind::detect_from_path_with_limits(candidate, &ctx.limits),
        };

        if let (Ok(orig_kind), Ok(cand_kind)) = (orig_kind_res, cand_kind_res) {
            let orig_entries =
                read_archive_entries_checked(&original.path, orig_kind, &ctx.limits, ctx)?;
            let cand_entries =
                read_archive_entries_checked(candidate, cand_kind, &ctx.limits, ctx)?;
            let orig_digest = canonical_entry_digests(orig_entries);
            let cand_digest = canonical_entry_digests(cand_entries);
            let exact = orig_digest == cand_digest;
            return Ok(QualityMetrics {
                ssim: None,
                psnr_db: None,
                vmaf: None,
                bit_exact_lossless: Some(exact),
            });
        }

        if let (Some(orig_codec), Some(cand_codec)) = (
            detect_stream_codec(&original.path),
            detect_stream_codec(candidate),
        ) {
            let orig_plain =
                decompress_stream_bounded(&fs::read(&original.path)?, orig_codec, &ctx.limits, 0)?;
            let cand_plain =
                decompress_stream_bounded(&fs::read(candidate)?, cand_codec, &ctx.limits, 0)?;
            return Ok(QualityMetrics {
                ssim: None,
                psnr_db: None,
                vmaf: None,
                bit_exact_lossless: Some(orig_plain == cand_plain),
            });
        }

        BitExactQualityEvaluator.evaluate(original, candidate, ctx)
    }
}

/// L3 pure-Rust [`CompressionEngine`] handling multi-format archive repacking and stream
/// recompression (`FR-ARC-001`..`003`, `FR-CMP-002`).
#[derive(Debug, Clone, Copy, Default)]
pub struct ArchiveCompressionEngine;

impl ArchiveCompressionEngine {
    /// Recompresses or repacks `input` into `staged_output` with explicit container selection
    /// and mandatory `FR-CMP-002` No-Op inflation guard.
    ///
    /// # Errors
    /// Returns [`FileFusionError`] on unsupported route, corrupt archive, bomb/traversal guard,
    /// cancellation, or memory ceiling violation.
    pub fn compress_with_options(
        &self,
        input: &ValidatedSource,
        staged_output: &StagedPath,
        opts: &ArchiveCompressionOptions,
        ctx: &JobContext,
    ) -> FileFusionResult<CompressionOutcome> {
        ctx.check_cancelled()?;
        let req = &opts.base;

        if opts.target_container.is_none() {
            if let Some(noop) = prescreen_incompressible_noop(input, req, self.engine_id()) {
                return Ok(noop);
            }
        }

        let est_mb = self.estimate_memory_mb(&input.inspection, req);
        let _mem = ctx.reserve_working_set_mb(est_mb, "archive_compression")?;

        if let Some(target_kind) = resolve_target_container(input, req, opts.target_container)? {
            let source_kind = ArchiveContainerKind::detect_from_path(&input.path)?;
            let entries = read_archive_entries_checked(&input.path, source_kind, &ctx.limits, ctx)?;
            let _summary = create_archive_from_entries(
                &entries,
                staged_output.staged_path(),
                target_kind,
                req.profile,
                ctx,
            )?;

            let validator = ArchiveOutputValidator {
                allow_brotli: target_kind == ArchiveContainerKind::TarBrotli,
            };
            let evaluator = ArchiveQualityEvaluator {
                source_kind: Some(source_kind),
                target_kind: Some(target_kind),
            };
            let mut effective_req = req.clone();
            effective_req.target_format = Some(target_kind.outer_format_id());
            return finalize_staged_compression(
                input,
                staged_output,
                &effective_req,
                &validator,
                Some(&evaluator),
                target_kind.codec_label(),
                ctx,
            );
        }

        // Fallback to single compressed byte-stream transcode (e.g. Gzip -> Zstd).
        let source_codec =
            StreamCodec::from_format(input.inspection.detected_format).ok_or_else(|| {
                FileFusionError::new(
                    ErrorCode::FmtUnsupported,
                    format!(
                        "unsupported archive/stream source format {:?}",
                        input.inspection.detected_format
                    ),
                )
            })?;
        let target_format = req
            .target_format
            .unwrap_or(input.inspection.detected_format);
        let target_codec = StreamCodec::from_format(target_format).ok_or_else(|| {
            FileFusionError::new(
                ErrorCode::FmtUnsupported,
                format!("unsupported stream target format {target_format:?}"),
            )
        })?;

        let raw_in = fs::read(&input.path)?;
        let decoded = decompress_stream_bounded(&raw_in, source_codec, &ctx.limits, 0)?;
        ctx.check_cancelled()?;
        let encoded = compress_stream_bytes(&decoded, target_codec, req.profile)?;
        fs::write(staged_output.staged_path(), encoded)?;

        let validator = ArchiveOutputValidator::default();
        finalize_staged_compression(
            input,
            staged_output,
            req,
            &validator,
            Some(&ArchiveQualityEvaluator::default()),
            target_codec.label(),
            ctx,
        )
    }
}

impl CompressionEngine for ArchiveCompressionEngine {
    fn engine_id(&self) -> &'static str {
        "compression-archive-tier1"
    }

    fn supported_formats(&self) -> &[FormatId] {
        &[
            FormatId::Zip,
            FormatId::Tar,
            FormatId::Gzip,
            FormatId::Zstd,
            FormatId::Xz,
            FormatId::Bzip2,
            FormatId::Lz4,
            FormatId::SevenZ,
        ]
    }

    fn estimate_memory_mb(&self, input: &FileInspectionReport, _req: &CompressionRequest) -> u64 {
        let mb = input.byte_size / (1024 * 1024);
        mb.saturating_mul(2).clamp(16, 512)
    }

    fn compress(
        &self,
        input: &ValidatedSource,
        staged_output: &StagedPath,
        req: &CompressionRequest,
        ctx: &JobContext,
    ) -> FileFusionResult<CompressionOutcome> {
        let opts = ArchiveCompressionOptions {
            base: req.clone(),
            target_container: None,
        };
        self.compress_with_options(input, staged_output, &opts, ctx)
    }
}

/// Builds a sanitized argument-array [`ProcessInvocation`] for external Tier 2 `7z` packing
/// (`MOD-RULE-05`, zero shell interpolation).
#[cfg(feature = "native-process")]
#[must_use]
pub fn build_tier2_7z_pack_invocation(
    sevenz_binary: &Path,
    output_archive: &Path,
    input_paths: &[&Path],
    profile: CompressionProfile,
    timeout_secs: u64,
) -> ProcessInvocation {
    let cfg = CompressionProfileConfig::for_profile(profile);
    let mx_flag = format!("-mx={}", cfg.effort_level.clamp(1, 9));
    let mut args = vec![
        std::ffi::OsString::from("a"),
        std::ffi::OsString::from("-t7z"),
        std::ffi::OsString::from(mx_flag),
        output_archive.as_os_str().to_os_string(),
    ];
    for p in input_paths {
        args.push(p.as_os_str().to_os_string());
    }
    ProcessInvocation {
        executable: sevenz_binary.to_path_buf(),
        args,
        working_dir: None,
        timeout_secs,
        allow_env_keys: Vec::new(),
    }
}

/// Invokes Tier 2 `7z` CLI via an explicit [`IsolatedProcessRunner`] (`FR-ARC-001`, `MOD-RULE-05`).
///
/// # Errors
/// Returns [`FileFusionError`] on non-zero exit or cancellation.
#[cfg(feature = "native-process")]
pub fn pack_via_tier2_7z(
    runner: &dyn IsolatedProcessRunner,
    sevenz_binary: &Path,
    output_archive: &Path,
    input_paths: &[&Path],
    profile: CompressionProfile,
    ctx: &JobContext,
) -> FileFusionResult<()> {
    ctx.check_cancelled()?;
    let invocation = build_tier2_7z_pack_invocation(
        sevenz_binary,
        output_archive,
        input_paths,
        profile,
        ctx.limits.subprocess_timeout_secs,
    );
    let out = runner.run_isolated(&invocation, ctx)?;
    if out.exit_code != 0 {
        return Err(FileFusionError::new(
            ErrorCode::IntInvariant,
            format!(
                "tier2 7z exited with code {}: {}",
                out.exit_code,
                String::from_utf8_lossy(&out.stderr).trim()
            ),
        ));
    }
    Ok(())
}

fn resolve_target_container(
    input: &ValidatedSource,
    req: &CompressionRequest,
    explicit: Option<ArchiveContainerKind>,
) -> FileFusionResult<Option<ArchiveContainerKind>> {
    if let Some(kind) = explicit {
        return Ok(Some(kind));
    }
    let source_is_tar_or_container = matches!(
        input.inspection.detected_format,
        FormatId::Zip | FormatId::Tar | FormatId::SevenZ
    ) || ArchiveContainerKind::detect_from_path(&input.path)
        .is_ok();

    if !source_is_tar_or_container {
        return Ok(None);
    }

    match req.target_format {
        None => ArchiveContainerKind::detect_from_path(&input.path).map(Some),
        Some(FormatId::Zip) => Ok(Some(ArchiveContainerKind::Zip)),
        Some(FormatId::Tar) => Ok(Some(ArchiveContainerKind::Tar)),
        Some(FormatId::SevenZ) => Ok(Some(ArchiveContainerKind::SevenZ)),
        Some(FormatId::Gzip) => Ok(Some(ArchiveContainerKind::TarGz)),
        Some(FormatId::Zstd) => Ok(Some(ArchiveContainerKind::TarZst)),
        Some(FormatId::Xz) => Ok(Some(ArchiveContainerKind::TarXz)),
        Some(FormatId::Bzip2) => Ok(Some(ArchiveContainerKind::TarBz2)),
        Some(FormatId::Lz4) => Ok(Some(ArchiveContainerKind::TarLz4)),
        Some(other) => Err(FileFusionError::new(
            ErrorCode::FmtUnsupported,
            format!("unsupported archive container target format {other:?}"),
        )),
    }
}

fn canonical_entry_digests(entries: Vec<ArchiveInputEntry>) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = entries
        .into_iter()
        .map(|e| {
            let hash = file_core::compute_digests_from_bytes(&e.data).blake3;
            (e.path, hash)
        })
        .collect();
    out.sort();
    out
}

fn detect_stream_codec(path: &Path) -> Option<StreamCodec> {
    let sig = detect_signature_from_path(path).ok()?;
    StreamCodec::from_format(sig.detected_format)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;
    use std::fs::File;
    use std::io::Write;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use file_core::{CollisionPolicy, CompressionStatus, ValidatedSource, inspect_path};

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let dir = std::env::temp_dir().join(format!("ff_arc_{label}_{nanos}"));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    fn make_source(path: &Path) -> Result<ValidatedSource, Box<dyn Error>> {
        let inspection = inspect_path(path, false)?;
        Ok(ValidatedSource {
            path: path.to_path_buf(),
            inspection,
        })
    }

    #[test]
    fn stream_codecs_roundtrip_and_enforce_bomb_ratio_limit() -> Result<(), Box<dyn Error>> {
        let payload = b"FileFusion archive stream compression benchmark line.\n".repeat(80);
        let limits = ResourceLimits::default();

        for codec in [
            StreamCodec::Zstd,
            StreamCodec::Lz4,
            StreamCodec::Brotli,
            StreamCodec::Gzip,
            StreamCodec::Xz,
            StreamCodec::Bzip2,
        ] {
            let compressed =
                compress_stream_bytes(&payload, codec, CompressionProfile::BalancedOptimal)?;
            assert!(
                compressed.len() < payload.len(),
                "{codec:?} did not shrink ({} >= {})",
                compressed.len(),
                payload.len()
            );
            let decoded = decompress_stream_bounded(&compressed, codec, &limits, 0)?;
            assert_eq!(decoded, payload, "round-trip mismatch for {codec:?}");
        }

        // Strict ratio ceiling must reject highly compressible bomb stream.
        let zst = compress_stream_bytes(
            &payload,
            StreamCodec::Zstd,
            CompressionProfile::ExtremeSizeTarget,
        )?;
        let tight = ResourceLimits {
            max_expansion_ratio: 2,
            ..ResourceLimits::default()
        };
        let Err(err) = decompress_stream_bounded(&zst, StreamCodec::Zstd, &tight, 0) else {
            return Err("tight ratio cap must reject bomb".into());
        };
        assert_eq!(err.diagnostic.code, ErrorCode::SecBomb);
        Ok(())
    }

    #[test]
    fn containers_create_list_extract_and_block_zip_slip_traversal() -> Result<(), Box<dyn Error>> {
        let dir = unique_temp_dir("containers");
        let ctx = JobContext::new("arc-test", ResourceLimits::default());
        let entries = vec![
            ArchiveInputEntry {
                path: String::from("docs/readme.txt"),
                data: b"Hello FileFusion archive container!\n".repeat(12),
            },
            ArchiveInputEntry {
                path: String::from("nested/data.json"),
                data: br#"{"tool":"filefusion","stage":2}"#.to_vec(),
            },
        ];

        for (ext, kind) in [
            ("zip", ArchiveContainerKind::Zip),
            ("tar", ArchiveContainerKind::Tar),
            ("tar.gz", ArchiveContainerKind::TarGz),
            ("tar.zst", ArchiveContainerKind::TarZst),
            ("tar.xz", ArchiveContainerKind::TarXz),
            ("tar.bz2", ArchiveContainerKind::TarBz2),
            ("tar.lz4", ArchiveContainerKind::TarLz4),
            ("7z", ArchiveContainerKind::SevenZ),
        ] {
            let arc_path = dir.join(format!("bundle.{ext}"));
            let summary = create_archive_from_entries(
                &entries,
                &arc_path,
                kind,
                CompressionProfile::BalancedOptimal,
                &ctx,
            )?;
            assert_eq!(summary.entry_count, 2);

            let listed = list_archive_members(&arc_path, &ctx.limits)?;
            assert_eq!(listed.len(), 2);

            let out_dir = dir.join(format!("out_{ext}"));
            let report = extract_archive_safely(&arc_path, &out_dir, &ctx.limits, &ctx)?;
            assert_eq!(report.extracted_files.len(), 2);
            assert_eq!(fs::read(out_dir.join("nested/data.json"))?, entries[1].data);
        }

        // Craft raw malicious ZIP with `../escape.txt` and ensure extraction aborts & rolls back.
        let evil_zip = dir.join("evil.zip");
        {
            let f = File::create(&evil_zip)?;
            let mut zw = zip::ZipWriter::new(f);
            let opts = zip::write::SimpleFileOptions::default();
            zw.start_file("safe.txt", opts)?;
            zw.write_all(b"ok")?;
            zw.start_file("../escape.txt", opts)?;
            zw.write_all(b"pwn")?;
            zw.finish()?;
        }
        let evil_out = dir.join("evil_extracted");
        let Err(err) = extract_archive_safely(&evil_zip, &evil_out, &ctx.limits, &ctx) else {
            return Err("traversal archive must abort".into());
        };
        assert_eq!(err.diagnostic.code, ErrorCode::SecTraversal);
        assert!(
            !evil_out.exists(),
            "partial extraction folder must be cleaned up"
        );

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn repacks_zip_to_tar_zst_and_enforces_noop_guard() -> Result<(), Box<dyn Error>> {
        let dir = unique_temp_dir("repack");
        let ctx = JobContext::new("arc-repack", ResourceLimits::default());
        let src_zip = dir.join("legacy.zip");
        let entries = vec![
            ArchiveInputEntry {
                path: String::from("a.log"),
                data: b"INFO startup complete; subsystem healthy\n".repeat(40),
            },
            ArchiveInputEntry {
                path: String::from("b.log"),
                data: b"INFO processing pipeline ready; cache warm\n".repeat(40),
            },
        ];
        create_archive_from_entries(
            &entries,
            &src_zip,
            ArchiveContainerKind::Zip,
            CompressionProfile::BalancedOptimal,
            &ctx,
        )?;

        let source = make_source(&src_zip)?;
        let dest_tzst = dir.join("repacked.tar.zst");
        let staged = StagedPath::create_sibling(&dest_tzst)?;
        let opts = ArchiveCompressionOptions {
            base: CompressionRequest {
                profile: CompressionProfile::ArchivalColdStorage,
                target_format: None,
                target_max_bytes: None,
                min_ssim: None,
                strip_metadata: false,
                no_op_if_larger: true,
            },
            target_container: Some(ArchiveContainerKind::TarZst),
        };
        let outcome =
            ArchiveCompressionEngine.compress_with_options(&source, &staged, &opts, &ctx)?;
        assert_eq!(outcome.status, CompressionStatus::Optimized);
        assert!(outcome.bytes_out < outcome.bytes_in);
        assert_eq!(
            outcome.quality.and_then(|q| q.bit_exact_lossless),
            Some(true)
        );
        let committed_tzst = staged.promote(CollisionPolicy::RenameWithSuffix)?;

        // Second pass on already well-compressed .tar.zst at identical format triggers No-Op guard
        // when output would not shrink further.
        let repacked_src = make_source(&committed_tzst)?;
        let reopt_dest = dir.join("reopt.tar.zst");
        let reopt_staged = StagedPath::create_sibling(&reopt_dest)?;
        let mut same_opts = opts;
        same_opts.base.no_op_if_larger = true;
        same_opts.base.profile = CompressionProfile::BalancedOptimal;
        same_opts.target_container = Some(ArchiveContainerKind::TarZst);
        let noop_outcome = ArchiveCompressionEngine.compress_with_options(
            &repacked_src,
            &reopt_staged,
            &same_opts,
            &ctx,
        )?;
        assert_eq!(noop_outcome.status, CompressionStatus::NoOpRetainedOriginal);

        // Verify Zip -> TarBrotli staged repack preserves bit_exact_lossless == Some(true).
        let dest_tbr = dir.join("repacked.tar.br");
        let staged_tbr = StagedPath::create_sibling(&dest_tbr)?;
        let mut br_opts = same_opts.clone();
        br_opts.target_container = Some(ArchiveContainerKind::TarBrotli);
        let br_outcome =
            ArchiveCompressionEngine.compress_with_options(&source, &staged_tbr, &br_opts, &ctx)?;
        assert_eq!(br_outcome.status, CompressionStatus::Optimized);
        assert_eq!(
            br_outcome.quality.and_then(|q| q.bit_exact_lossless),
            Some(true)
        );
        let committed_tbr = staged_tbr.promote(CollisionPolicy::RenameWithSuffix)?;
        let br_members = list_archive_members(&committed_tbr, &ctx.limits)?;
        assert_eq!(br_members.len(), 2);

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn ffenc_encrypts_decrypts_and_detects_tampering() -> Result<(), Box<dyn Error>> {
        let dir = unique_temp_dir("ffenc");
        let ctx = JobContext::new("ffenc-test", ResourceLimits::default());
        let plain_path = dir.join("secret.bin");
        let enc_path = dir.join("secret.ffenc");
        let dec_path = dir.join("restored.bin");
        let payload = b"Sensitive local archive payload for AEAD verification.".repeat(10);
        fs::write(&plain_path, &payload)?;

        for suite in [
            FfencCipherSuite::Aes256Gcm,
            FfencCipherSuite::ChaCha20Poly1305,
        ] {
            encrypt_to_ffenc_file(
                &plain_path,
                &enc_path,
                b"correct horse battery staple",
                suite,
                FfencKdfParams::fast_test(),
                &ctx,
            )?;

            let sig = detect_signature_from_path(&enc_path)?;
            assert_eq!(sig.detected_format, FormatId::Ffenc);

            decrypt_from_ffenc_file(&enc_path, &dec_path, b"correct horse battery staple", &ctx)?;
            assert_eq!(fs::read(&dec_path)?, payload);

            let Err(wrong_pw) = decrypt_from_ffenc_file(&enc_path, &dec_path, b"wrong-pass", &ctx)
            else {
                return Err("wrong passphrase must fail authentication".into());
            };
            assert_eq!(wrong_pw.diagnostic.code, ErrorCode::FmtDrm);

            // Flip a byte inside ciphertext/AAD and verify AEAD tag failure.
            let mut tampered = fs::read(&enc_path)?;
            let last = tampered.len() - 1;
            tampered[last] ^= 0xFF;
            let Err(tampered_err) =
                decrypt_ffenc_bytes(&tampered, b"correct horse battery staple", &ctx.limits)
            else {
                return Err("tampered payload must fail AEAD".into());
            };
            assert_eq!(tampered_err.diagnostic.code, ErrorCode::FmtDrm);
        }

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn repairs_truncated_zip_missing_central_directory() -> Result<(), Box<dyn Error>> {
        let dir = unique_temp_dir("zip_repair");
        let ctx = JobContext::new("zip-repair", ResourceLimits::default());
        let src_zip = dir.join("intact.zip");
        let entries = vec![
            ArchiveInputEntry {
                path: String::from("alpha.txt"),
                data: b"First intact entry payload before central directory truncation.\n"
                    .repeat(6),
            },
            ArchiveInputEntry {
                path: String::from("beta/data.txt"),
                data: b"Second intact compressed entry inside truncated zip.\n".repeat(6),
            },
        ];
        create_archive_from_entries(
            &entries,
            &src_zip,
            ArchiveContainerKind::Zip,
            CompressionProfile::BalancedOptimal,
            &ctx,
        )?;

        // Truncate off Central Directory + EOCD so standard ZipArchive rejects it.
        let mut raw = fs::read(&src_zip)?;
        let cen_pos = raw
            .windows(4)
            .position(|w| w == [0x50, 0x4B, 0x01, 0x02])
            .ok_or("central directory missing from fixture")?;
        raw.truncate(cen_pos);
        let damaged_zip = dir.join("truncated.zip");
        fs::write(&damaged_zip, &raw)?;
        assert!(zip::ZipArchive::new(File::open(&damaged_zip)?).is_err());

        let repaired_zip = dir.join("repaired.zip");
        let report = repair_truncated_zip(&damaged_zip, &repaired_zip, &ctx.limits, &ctx)?;
        assert!(report.rebuilt_central_directory);
        assert_eq!(report.entries_recovered, 2);

        let restored = read_archive_entries_checked(
            &repaired_zip,
            ArchiveContainerKind::Zip,
            &ctx.limits,
            &ctx,
        )?;
        assert_eq!(restored, entries);

        // Enforce SecBomb during Deflate decoding inside repair_truncated_zip.
        let bomb_zip = dir.join("bomb_truncated.zip");
        let bomb_entries = vec![ArchiveInputEntry {
            path: String::from("zeros.bin"),
            data: vec![b'A'; 128 * 1024],
        }];
        create_archive_from_entries(
            &bomb_entries,
            &bomb_zip,
            ArchiveContainerKind::Zip,
            CompressionProfile::ExtremeSizeTarget,
            &ctx,
        )?;
        let mut bomb_raw = fs::read(&bomb_zip)?;
        if let Some(cen) = bomb_raw
            .windows(4)
            .position(|w| w == [0x50, 0x4B, 0x01, 0x02])
        {
            bomb_raw.truncate(cen);
        }
        fs::write(&bomb_zip, &bomb_raw)?;
        let tight_limits = ResourceLimits {
            max_expansion_ratio: 2,
            ..ResourceLimits::default()
        };
        let tight_ctx = JobContext::new("zip-repair-bomb", tight_limits);
        let Err(bomb_err) = repair_truncated_zip(
            &bomb_zip,
            &dir.join("bomb_repaired.zip"),
            &tight_limits,
            &tight_ctx,
        ) else {
            return Err("repair_truncated_zip must reject Deflate bomb".into());
        };
        assert_eq!(bomb_err.diagnostic.code, ErrorCode::SecBomb);

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }
}
