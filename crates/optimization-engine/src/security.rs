//! Unified Stage 4 Encryption & Container Security Orchestration (`FR-SEC-002`,
//! `FR-PDF-004`) over `.ffenc` (`AES-256-GCM`/`ChaCha20-Poly1305` + `Argon2id`) and `PDF`
//! (`AES-256`/`AES-128` + `/Sig` inspection).

use std::fs;
use std::path::Path;

use compression_archive::{
    FfencCipherSuite, FfencHeaderInfo, FfencKdfParams, decrypt_from_ffenc_file,
    encrypt_to_ffenc_file, inspect_ffenc_header,
};
use compression_pdf::{PdfProtectionOptions, decrypt_pdf_bytes, encrypt_pdf_bytes};
use file_core::{ErrorCode, FileFusionError, FileFusionResult, JobContext, StagedPath};

/// Inspects an `.ffenc` container header on disk without decrypting plaintext (`FR-SEC-002`).
///
/// # Errors
/// Returns [`FileFusionError`] on missing file, invalid magic, or malformed header.
pub fn inspect_ffenc_file(path: &Path, ctx: &JobContext) -> FileFusionResult<FfencHeaderInfo> {
    ctx.check_cancelled()?;
    if !path.is_file() {
        return Err(FileFusionError::new(
            ErrorCode::IoNotFound,
            format!(".ffenc file not found: `{}`", path.display()),
        ));
    }
    let raw = fs::read(path)?;
    inspect_ffenc_header(&raw)
}

/// Encrypts any local source file into an authenticated `.ffenc` container staged atomically at
/// `staged_output` (`FR-SEC-002`).
///
/// # Errors
/// Returns [`FileFusionError`] on empty passphrase, memory budget failure, I/O error, or cancellation.
pub fn encrypt_file_to_ffenc_staged(
    input_path: &Path,
    staged_output: &StagedPath,
    passphrase: &[u8],
    suite: FfencCipherSuite,
    kdf: FfencKdfParams,
    ctx: &JobContext,
) -> FileFusionResult<u64> {
    encrypt_to_ffenc_file(
        input_path,
        staged_output.staged_path(),
        passphrase,
        suite,
        kdf,
        ctx,
    )
}

/// Decrypts and authenticates an `.ffenc` container at `input_path` into `staged_output`, enforcing
/// passphrase AEAD verification and expansion-budget caps (`FR-SEC-002`).
///
/// # Errors
/// Returns [`FileFusionError`] (`FmtDrm` on wrong passphrase/tamper, `SecBomb`, or cancellation).
pub fn decrypt_ffenc_file_to_staged(
    input_path: &Path,
    staged_output: &StagedPath,
    passphrase: &[u8],
    ctx: &JobContext,
) -> FileFusionResult<u64> {
    decrypt_from_ffenc_file(input_path, staged_output.staged_path(), passphrase, ctx)
}

/// Encrypts an unencrypted PDF file into `staged_output` using PDF 2.0 `AES-256` or `AES-128` and
/// ISO-32000 permission flags (`FR-PDF-004`).
///
/// # Errors
/// Returns [`FileFusionError`] on corrupt/already-encrypted PDF or cooperative cancellation.
pub fn encrypt_pdf_file_to_staged(
    input_path: &Path,
    staged_output: &StagedPath,
    opts: &PdfProtectionOptions,
    ctx: &JobContext,
) -> FileFusionResult<u64> {
    ctx.check_cancelled()?;
    let raw = fs::read(input_path)?;
    let encrypted = encrypt_pdf_bytes(&raw, opts, ctx)?;
    fs::write(staged_output.staged_path(), &encrypted)?;
    Ok(u64::try_from(encrypted.len()).unwrap_or(u64::MAX))
}

/// Unlocks and removes `/Encrypt` security handlers from a password-protected PDF file into
/// `staged_output` (`FR-PDF-004`).
///
/// # Errors
/// Returns [`FileFusionError`] (`FmtDrm` on wrong password) or cancellation.
pub fn decrypt_pdf_file_to_staged(
    input_path: &Path,
    staged_output: &StagedPath,
    password: &str,
    ctx: &JobContext,
) -> FileFusionResult<u64> {
    ctx.check_cancelled()?;
    let raw = fs::read(input_path)?;
    let unlocked = decrypt_pdf_bytes(&raw, password, ctx)?;
    fs::write(staged_output.staged_path(), &unlocked)?;
    Ok(u64::try_from(unlocked.len()).unwrap_or(u64::MAX))
}
