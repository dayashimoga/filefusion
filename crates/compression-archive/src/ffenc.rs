//! Authenticated encrypted `.ffenc` container (`FR-SEC-002`).
//!
//! Wire format layout (`v1.0`):
//! - `[0..7]`: Magic `b"FFENC\x01\x00"` (recognized by `file_core` `FormatId::Ffenc`)
//! - `[7]`: Cipher suite byte (`1` = `AES-256-GCM`, `2` = `ChaCha20-Poly1305`)
//! - `[8..12]`: Argon2id `m_cost` in KiB (`u32` little-endian)
//! - `[12..16]`: Argon2id `t_cost` iterations (`u32` little-endian)
//! - `[16..20]`: Argon2id `p_cost` parallelism (`u32` little-endian)
//! - `[20..36]`: 16-byte CSPRNG salt (`getrandom`)
//! - `[36..48]`: 12-byte CSPRNG AEAD nonce (`getrandom`)
//! - `[48..]`: Authenticated ciphertext + 16-byte AEAD tag (entire 48-byte header bound as AAD)

use std::fs;
use std::path::Path;

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce as AesNonce};
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{ChaCha20Poly1305, Nonce as ChaChaNonce};
use file_core::{ErrorCode, FileFusionError, FileFusionResult, JobContext, ResourceLimits};
use serde::{Deserialize, Serialize};
use validation_engine::validate_expansion_budget;

/// Fixed 7-byte header magic (`FFENC` + major `1` + minor `0`).
pub const FFENC_MAGIC: &[u8; 7] = b"FFENC\x01\x00";
const FFENC_HEADER_LEN: usize = 48;

/// Authenticated AEAD cipher suite inside `.ffenc` (`FR-SEC-002`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FfencCipherSuite {
    /// AES-256-GCM (hardware AES-NI accelerated where supported).
    Aes256Gcm = 1,
    /// ChaCha20-Poly1305 (constant-time pure software speed on ARM/WASM).
    ChaCha20Poly1305 = 2,
}

impl FfencCipherSuite {
    fn from_byte(byte: u8) -> FileFusionResult<Self> {
        match byte {
            1 => Ok(Self::Aes256Gcm),
            2 => Ok(Self::ChaCha20Poly1305),
            other => Err(FileFusionError::new(
                ErrorCode::FmtUnsupported,
                format!("unsupported .ffenc cipher suite id {other}"),
            )),
        }
    }
}

/// Argon2id key-derivation work factors stored in the authenticated `.ffenc` header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FfencKdfParams {
    /// Memory cost in KiB (minimum `8 * p_cost`).
    pub m_cost_kib: u32,
    /// Iteration count (`t_cost >= 1`).
    pub t_cost: u32,
    /// Lane parallelism (`p_cost >= 1`).
    pub p_cost: u32,
}

impl Default for FfencKdfParams {
    fn default() -> Self {
        // OWASP 2025 interactive Argon2id baseline (~19 MiB, t=2, p=1).
        Self {
            m_cost_kib: 19_456,
            t_cost: 2,
            p_cost: 1,
        }
    }
}

impl FfencKdfParams {
    /// Fast KDF tuning suitable for high-frequency unit tests and low-RAM hosts.
    #[must_use]
    pub const fn fast_test() -> Self {
        Self {
            m_cost_kib: 1024,
            t_cost: 1,
            p_cost: 1,
        }
    }
}

/// Encrypts `plaintext` into a self-contained `.ffenc` container bound to `passphrase`.
///
/// # Errors
/// Returns [`FileFusionError`] on invalid KDF params, empty passphrase, or CSPRNG failure.
pub fn encrypt_ffenc_bytes(
    plaintext: &[u8],
    passphrase: &[u8],
    suite: FfencCipherSuite,
    kdf: FfencKdfParams,
) -> FileFusionResult<Vec<u8>> {
    if passphrase.is_empty() {
        return Err(FileFusionError::new(
            ErrorCode::FmtUnsupported,
            "encryption passphrase cannot be empty",
        ));
    }
    let mut salt = [0u8; 16];
    let mut nonce = [0u8; 12];
    getrandom::fill(&mut salt).map_err(|err| {
        FileFusionError::new(
            ErrorCode::IntInvariant,
            format!("CSPRNG salt generation failed: {err}"),
        )
    })?;
    getrandom::fill(&mut nonce).map_err(|err| {
        FileFusionError::new(
            ErrorCode::IntInvariant,
            format!("CSPRNG nonce generation failed: {err}"),
        )
    })?;

    let key = derive_key_argon2id(passphrase, &salt, kdf)?;

    let mut header = Vec::with_capacity(FFENC_HEADER_LEN);
    header.extend_from_slice(FFENC_MAGIC);
    header.push(suite as u8);
    header.extend_from_slice(&kdf.m_cost_kib.to_le_bytes());
    header.extend_from_slice(&kdf.t_cost.to_le_bytes());
    header.extend_from_slice(&kdf.p_cost.to_le_bytes());
    header.extend_from_slice(&salt);
    header.extend_from_slice(&nonce);

    let payload = Payload {
        msg: plaintext,
        aad: &header,
    };

    let ciphertext = match suite {
        FfencCipherSuite::Aes256Gcm => {
            let cipher = Aes256Gcm::new_from_slice(&key).map_err(|err| {
                FileFusionError::new(
                    ErrorCode::IntInvariant,
                    format!("AES key init failed: {err}"),
                )
            })?;
            cipher
                .encrypt(&AesNonce::from(nonce), payload)
                .map_err(|err| {
                    FileFusionError::new(
                        ErrorCode::IntInvariant,
                        format!("AES encryption failed: {err}"),
                    )
                })?
        }
        FfencCipherSuite::ChaCha20Poly1305 => {
            let cipher = ChaCha20Poly1305::new_from_slice(&key).map_err(|err| {
                FileFusionError::new(
                    ErrorCode::IntInvariant,
                    format!("ChaCha key init failed: {err}"),
                )
            })?;
            cipher
                .encrypt(&ChaChaNonce::from(nonce), payload)
                .map_err(|err| {
                    FileFusionError::new(
                        ErrorCode::IntInvariant,
                        format!("ChaCha encryption failed: {err}"),
                    )
                })?
        }
    };

    let mut out = header;
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Decrypts and authenticates an `.ffenc` payload while checking `limits` size ceilings.
///
/// # Errors
/// Returns [`FileFusionError`] (`FmtCorrupt` for malformed header, `FmtDrm` for wrong passphrase or
/// tampered AAD/ciphertext, `SecBomb` if plaintext exceeds `limits.max_uncompressed_bytes`).
pub fn decrypt_ffenc_bytes(
    container: &[u8],
    passphrase: &[u8],
    limits: &ResourceLimits,
) -> FileFusionResult<Vec<u8>> {
    if container.len() < FFENC_HEADER_LEN + 16 || !container.starts_with(FFENC_MAGIC) {
        return Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            "input is not a valid FFENC v1 container",
        ));
    }

    let header = &container[..FFENC_HEADER_LEN];
    let suite = FfencCipherSuite::from_byte(header[7])?;
    let m_cost_kib = u32::from_le_bytes(read_arr4(header, 8)?);
    let t_cost = u32::from_le_bytes(read_arr4(header, 12)?);
    let p_cost = u32::from_le_bytes(read_arr4(header, 16)?);
    let kdf = FfencKdfParams {
        m_cost_kib,
        t_cost,
        p_cost,
    };

    // Guard against maliciously inflated KDF memory-cost headers (> 512 MiB).
    if kdf.m_cost_kib > 524_288 {
        return Err(FileFusionError::new(
            ErrorCode::SecBomb,
            format!(
                ".ffenc header Argon2id m_cost {} KiB exceeds safety cap 524288 KiB",
                kdf.m_cost_kib
            ),
        ));
    }

    let salt: [u8; 16] = header[20..36].try_into().map_err(|_| {
        FileFusionError::new(ErrorCode::FmtCorrupt, "invalid salt slice in .ffenc header")
    })?;
    let nonce: [u8; 12] = header[36..48].try_into().map_err(|_| {
        FileFusionError::new(
            ErrorCode::FmtCorrupt,
            "invalid nonce slice in .ffenc header",
        )
    })?;
    let ciphertext = &container[FFENC_HEADER_LEN..];

    validate_expansion_budget(container.len() as u64, ciphertext.len() as u64, 0, limits)?;

    let key = derive_key_argon2id(passphrase, &salt, kdf)?;
    let payload = Payload {
        msg: ciphertext,
        aad: header,
    };

    let auth_err = || {
        FileFusionError::new(
            ErrorCode::FmtDrm,
            "failed to authenticate .ffenc container (wrong passphrase or tampered payload)",
        )
        .with_remediation("Verify passphrase and confirm container file was not modified.")
    };

    match suite {
        FfencCipherSuite::Aes256Gcm => {
            let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| auth_err())?;
            cipher
                .decrypt(&AesNonce::from(nonce), payload)
                .map_err(|_| auth_err())
        }
        FfencCipherSuite::ChaCha20Poly1305 => {
            let cipher = ChaCha20Poly1305::new_from_slice(&key).map_err(|_| auth_err())?;
            cipher
                .decrypt(&ChaChaNonce::from(nonce), payload)
                .map_err(|_| auth_err())
        }
    }
}

/// Encrypts `source_path` into `dest_path` as an authenticated `.ffenc` container.
///
/// # Errors
/// Returns [`FileFusionError`] on I/O, cancellation, memory budget, or cipher errors.
pub fn encrypt_to_ffenc_file(
    source_path: &Path,
    dest_path: &Path,
    passphrase: &[u8],
    suite: FfencCipherSuite,
    kdf: FfencKdfParams,
    ctx: &JobContext,
) -> FileFusionResult<u64> {
    ctx.check_cancelled()?;
    let kdf_mb = u64::from(kdf.m_cost_kib / 1024).max(4);
    let _mem = ctx.reserve_working_set_mb(kdf_mb, "ffenc_encrypt")?;
    let plaintext = fs::read(source_path)?;
    ctx.check_cancelled()?;
    let encoded = encrypt_ffenc_bytes(&plaintext, passphrase, suite, kdf)?;
    if let Some(parent) = dest_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(dest_path, &encoded)?;
    Ok(encoded.len() as u64)
}

/// Decrypts an `.ffenc` file at `source_path` and writes verified plaintext to `dest_path`.
///
/// # Errors
/// Returns [`FileFusionError`] on wrong passphrase, tampered container, cancellation, or I/O failure.
pub fn decrypt_from_ffenc_file(
    source_path: &Path,
    dest_path: &Path,
    passphrase: &[u8],
    ctx: &JobContext,
) -> FileFusionResult<u64> {
    ctx.check_cancelled()?;
    let _mem = ctx.reserve_working_set_mb(32, "ffenc_decrypt")?;
    let raw = fs::read(source_path)?;
    ctx.check_cancelled()?;
    let plaintext = decrypt_ffenc_bytes(&raw, passphrase, &ctx.limits)?;
    if let Some(parent) = dest_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(dest_path, &plaintext)?;
    Ok(plaintext.len() as u64)
}

fn derive_key_argon2id(
    passphrase: &[u8],
    salt: &[u8],
    kdf: FfencKdfParams,
) -> FileFusionResult<[u8; 32]> {
    let params = Params::new(kdf.m_cost_kib, kdf.t_cost, kdf.p_cost, Some(32)).map_err(|err| {
        FileFusionError::new(
            ErrorCode::FmtUnsupported,
            format!("invalid Argon2id parameters: {err}"),
        )
    })?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0u8; 32];
    argon2
        .hash_password_into(passphrase, salt, &mut key)
        .map_err(|err| {
            FileFusionError::new(
                ErrorCode::IntInvariant,
                format!("Argon2id key derivation failed: {err}"),
            )
        })?;
    Ok(key)
}

fn read_arr4(slice: &[u8], offset: usize) -> FileFusionResult<[u8; 4]> {
    slice
        .get(offset..offset + 4)
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| FileFusionError::new(ErrorCode::FmtCorrupt, "truncated .ffenc header field"))
}
