//! Pure-Rust Stage 4 PDF password protection (`AES-256` / `AES-128`), granular permission
//! flags, decryption, security inspection, and `/Sig` digital signature structural/`ByteRange`
//! verification (`FR-PDF-004`).

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use file_core::{ErrorCode, FileFusionError, FileFusionResult, JobContext};
use lopdf::encryption::crypt_filters::{Aes128CryptFilter, Aes256CryptFilter, CryptFilter};
use lopdf::encryption::{EncryptionState, EncryptionVersion, Permissions};
use lopdf::{Dictionary, Document, LoadOptions, Object, ObjectId};
use serde::{Deserialize, Serialize};

#[cfg(feature = "native-process")]
use std::ffi::OsString;
#[cfg(feature = "native-process")]
use std::path::Path;

#[cfg(feature = "native-process")]
use file_core::ProcessInvocation;

use crate::studio::{load_unencrypted_pdf, serialize_pdf};

/// PDF standard security-handler cipher selection (`FR-PDF-004`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PdfEncryptionAlgorithm {
    /// PDF 2.0 `V=5, R=6` 256-bit `AES-CBC` with `StdCF`/`AESV3` (recommended default).
    #[default]
    Aes256,
    /// Legacy PDF 1.5 `V=4, R=4` 128-bit `AES-CBC` with `StdCF`/`AESV2`.
    Aes128,
}

/// Granular ISO-32000 `/P` document permission flags (`FR-PDF-004`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct PdfPermissions {
    /// Permit printing at low or standard quality.
    pub allow_print: bool,
    /// Permit high-resolution faithful printing.
    pub allow_print_high_quality: bool,
    /// Permit general document content modifications.
    pub allow_modify: bool,
    /// Permit copying text and graphics from the document.
    pub allow_copy: bool,
    /// Permit adding or modifying annotations and interactive form fields.
    pub allow_annotate: bool,
    /// Permit filling existing interactive form/signature fields.
    pub allow_fill_forms: bool,
    /// Permit text/graphics extraction for assistive accessibility technologies.
    pub allow_accessibility_extract: bool,
    /// Permit inserting, rotating, or deleting pages and bookmarks.
    pub allow_assemble: bool,
}

impl Default for PdfPermissions {
    fn default() -> Self {
        Self::all()
    }
}

impl PdfPermissions {
    /// Grants all standard PDF operations.
    #[must_use]
    pub const fn all() -> Self {
        Self {
            allow_print: true,
            allow_print_high_quality: true,
            allow_modify: true,
            allow_copy: true,
            allow_annotate: true,
            allow_fill_forms: true,
            allow_accessibility_extract: true,
            allow_assemble: true,
        }
    }

    /// Denies editing/copying/printing except assistive accessibility reading.
    #[must_use]
    pub const fn read_only() -> Self {
        Self {
            allow_print: false,
            allow_print_high_quality: false,
            allow_modify: false,
            allow_copy: false,
            allow_annotate: false,
            allow_fill_forms: false,
            allow_accessibility_extract: true,
            allow_assemble: false,
        }
    }

    /// Permits printing and accessibility reading while blocking copy, edit, and page assembly.
    #[must_use]
    pub const fn restricted_print_only() -> Self {
        Self {
            allow_print: true,
            allow_print_high_quality: false,
            allow_modify: false,
            allow_copy: false,
            allow_annotate: false,
            allow_fill_forms: false,
            allow_accessibility_extract: true,
            allow_assemble: false,
        }
    }

    fn to_lopdf(self) -> Permissions {
        let mut p = Permissions::empty();
        if self.allow_print {
            p |= Permissions::PRINTABLE;
        }
        if self.allow_print_high_quality {
            p |= Permissions::PRINTABLE_IN_HIGH_QUALITY;
        }
        if self.allow_modify {
            p |= Permissions::MODIFIABLE;
        }
        if self.allow_copy {
            p |= Permissions::COPYABLE;
        }
        if self.allow_annotate {
            p |= Permissions::ANNOTABLE;
        }
        if self.allow_fill_forms {
            p |= Permissions::FILLABLE;
        }
        if self.allow_accessibility_extract {
            p |= Permissions::COPYABLE_FOR_ACCESSIBILITY;
        }
        if self.allow_assemble {
            p |= Permissions::ASSEMBLABLE;
        }
        p
    }

    fn from_lopdf(p: Permissions) -> Self {
        Self {
            allow_print: p.contains(Permissions::PRINTABLE),
            allow_print_high_quality: p.contains(Permissions::PRINTABLE_IN_HIGH_QUALITY),
            allow_modify: p.contains(Permissions::MODIFIABLE),
            allow_copy: p.contains(Permissions::COPYABLE),
            allow_annotate: p.contains(Permissions::ANNOTABLE),
            allow_fill_forms: p.contains(Permissions::FILLABLE),
            allow_accessibility_extract: p.contains(Permissions::COPYABLE_FOR_ACCESSIBILITY),
            allow_assemble: p.contains(Permissions::ASSEMBLABLE),
        }
    }
}

/// Configuration for pure-Rust PDF encryption and permissions enforcement (`FR-PDF-004`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PdfProtectionOptions {
    /// Optional open password (`None` locks only owner permissions without an open prompt).
    pub user_password: Option<String>,
    /// Mandatory non-empty owner/permissions password.
    pub owner_password: String,
    /// Encryption cipher strength (`Aes256` default).
    pub algorithm: PdfEncryptionAlgorithm,
    /// Document permission flags enforced for user sessions.
    pub permissions: PdfPermissions,
    /// Whether XMP `/Metadata` streams are also encrypted (`true` recommended).
    pub encrypt_metadata: bool,
}

/// Read-only inspection report for a PDF's `/Encrypt` dictionary and permissions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PdfSecurityReport {
    /// True if the PDF contains a `/Encrypt` security handler.
    pub is_encrypted: bool,
    /// ISO-32000 `/V` algorithm version (`1`, `2`, `4`, or `5`).
    pub version: Option<i64>,
    /// Security handler `/R` revision (`2`..=`6`).
    pub revision: Option<i64>,
    /// Effective encryption key length in bits (`40`, `128`, or `256`).
    pub key_bits: Option<u32>,
    /// Human-readable cipher description (`AES-256 (V5/R6)`, `AES-128 (V4/R4)`, `None`, etc.).
    pub cipher_name: String,
    /// Whether document `/Metadata` streams are encrypted (`EncryptMetadata`).
    pub encrypt_metadata: Option<bool>,
    /// Decoded `/P` permissions bitmask when present.
    pub permissions: Option<PdfPermissions>,
    /// If a probe password was supplied, whether it unlocked or authenticated cleanly.
    pub password_verified: Option<bool>,
}

/// Offline structural & `/ByteRange` verification verdict for an individual `/Sig` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PdfSignatureStatus {
    /// No signature dictionaries exist in the PDF.
    Unsigned,
    /// `/ByteRange` covers `[0..gap_start]` and `[gap_end..file_len]`, `/Contents` matches the
    /// excluded gap and begins with an ASN.1 DER `SEQUENCE` (`0x30`) PKCS#7/CMS envelope.
    SignedByteRangeIntact,
    /// `/ByteRange` fails to cover the entire file (e.g. post-signing incremental modification).
    SignedTamperedAfterSigning,
    /// Signature dictionary exists but lacks valid `/ByteRange` or hex DER `/Contents`.
    SignedMalformed,
}

/// Detailed metadata and structural `/ByteRange` verification status for one PDF signature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PdfSignatureInfo {
    /// Associated `/AcroForm` field partial name (`/T`), if linked.
    pub field_name: Option<String>,
    /// Signature handler filter (`/Filter`), e.g. `Adobe.PPKLite`.
    pub filter: Option<String>,
    /// Cryptographic sub-filter (`/SubFilter`), e.g. `adbe.pkcs7.detached`.
    pub sub_filter: Option<String>,
    /// Declared signer identity (`/Name`), if populated.
    pub signer_name: Option<String>,
    /// Declared signing reason (`/Reason`), if populated.
    pub reason: Option<String>,
    /// Declared signing timestamp string (`/M`), if populated.
    pub signing_time: Option<String>,
    /// Parsed `/ByteRange` `[off1, len1, off2, len2]` array.
    pub byte_range: Option<[usize; 4]>,
    /// True if `/ByteRange` spans the full PDF byte sequence except the `/Contents` hex gap.
    pub covers_entire_file: bool,
    /// Non-zero unpadded byte length decoded from `/Contents`.
    pub contents_byte_len: usize,
    /// True if decoded `/Contents` starts with an ASN.1 DER `SEQUENCE` (`0x30`) tag.
    pub der_sequence_valid: bool,
    /// Honest verification verdict for this signature dictionary.
    pub status: PdfSignatureStatus,
    /// Actionable human-readable diagnostic summary.
    pub diagnostic: String,
}

/// Document-level `/Sig` digital signature audit report (`FR-PDF-004`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PdfSignatureReport {
    /// True if one or more `/Sig` signature dictionaries or signed form fields were detected.
    pub has_signatures: bool,
    /// Total count of signature dictionaries audited.
    pub signature_count: usize,
    /// True if every detected signature has intact full-file `/ByteRange` & DER `/Contents` framing.
    pub all_byte_ranges_intact: bool,
    /// Per-signature inspection details.
    pub signatures: Vec<PdfSignatureInfo>,
    /// Explicit disclosure of offline trust-store scope.
    pub certificate_trust_note: String,
}

/// Encrypts an unencrypted PDF using PDF 2.0 `AES-256` (`V5`/`R6`) or `AES-128` (`V4`/`R4`)
/// with custom permission flags (`FR-PDF-004`).
///
/// # Errors
/// Returns [`FileFusionError`] on empty owner password, corrupt/already-encrypted PDFs, or
/// cooperative job cancellation.
pub fn encrypt_pdf_bytes(
    raw_pdf: &[u8],
    opts: &PdfProtectionOptions,
    ctx: &JobContext,
) -> FileFusionResult<Vec<u8>> {
    ctx.check_cancelled()?;
    if opts.owner_password.is_empty() {
        return Err(FileFusionError::new(
            ErrorCode::FmtUnsupported,
            "owner password must be non-empty when applying PDF encryption",
        ));
    }

    let mut doc = load_unencrypted_pdf(raw_pdf)?;
    doc.compress();
    ensure_trailer_id(&mut doc)?;
    ctx.check_cancelled()?;

    let user_pw = opts.user_password.as_deref().unwrap_or("");
    let permissions = opts.permissions.to_lopdf();

    let mut crypt_filters: BTreeMap<Vec<u8>, Arc<dyn CryptFilter>> = BTreeMap::new();
    let filter: Arc<dyn CryptFilter> = match opts.algorithm {
        PdfEncryptionAlgorithm::Aes256 => Arc::new(Aes256CryptFilter),
        PdfEncryptionAlgorithm::Aes128 => Arc::new(Aes128CryptFilter),
    };
    crypt_filters.insert(b"StdCF".to_vec(), filter);

    let mut file_key = [0u8; 32];
    if matches!(opts.algorithm, PdfEncryptionAlgorithm::Aes256) {
        fill_csprng_bytes(&mut file_key)?;
    }

    let state = match opts.algorithm {
        PdfEncryptionAlgorithm::Aes256 => EncryptionState::try_from(EncryptionVersion::V5 {
            encrypt_metadata: opts.encrypt_metadata,
            crypt_filters,
            file_encryption_key: &file_key,
            stream_filter: b"StdCF".to_vec(),
            string_filter: b"StdCF".to_vec(),
            owner_password: &opts.owner_password,
            user_password: user_pw,
            permissions,
        }),
        PdfEncryptionAlgorithm::Aes128 => EncryptionState::try_from(EncryptionVersion::V4 {
            document: &doc,
            encrypt_metadata: opts.encrypt_metadata,
            crypt_filters,
            stream_filter: b"StdCF".to_vec(),
            string_filter: b"StdCF".to_vec(),
            owner_password: &opts.owner_password,
            user_password: user_pw,
            permissions,
        }),
    }
    .map_err(|err| {
        FileFusionError::new(
            ErrorCode::IntInvariant,
            format!("failed to initialize PDF encryption handler: {err}"),
        )
    })?;

    doc.encrypt(&state).map_err(|err| {
        FileFusionError::new(
            ErrorCode::IntInvariant,
            format!("failed to encrypt PDF objects: {err}"),
        )
    })?;

    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|err| {
        FileFusionError::new(
            ErrorCode::IntInvariant,
            format!("failed to serialize encrypted PDF: {err}"),
        )
    })?;
    Ok(out)
}

/// Unlocks and removes `/Encrypt` security handlers from an encrypted PDF given either the user
/// or owner password (`FR-PDF-004`).
///
/// # Errors
/// Returns [`FileFusionError`] (`FmtDrm`) on incorrect password or (`FmtCorrupt`) on invalid or
/// unencrypted input.
pub fn decrypt_pdf_bytes(
    raw_pdf: &[u8],
    password: &str,
    ctx: &JobContext,
) -> FileFusionResult<Vec<u8>> {
    ctx.check_cancelled()?;
    if raw_pdf.is_empty() {
        return Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            "PDF input payload is empty",
        ));
    }

    let sec = inspect_pdf_security(raw_pdf, None)?;
    if !sec.is_encrypted {
        return Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            "PDF is not encrypted",
        ));
    }

    let mut doc = Document::load_mem_with_options(raw_pdf, LoadOptions::with_password(password))
        .map_err(|err| {
            FileFusionError::new(
                ErrorCode::FmtDrm,
                format!("failed to decrypt PDF with supplied password: {err}"),
            )
        })?;

    if doc.is_encrypted() {
        doc.decrypt(password).map_err(|err| {
            FileFusionError::new(
                ErrorCode::FmtDrm,
                format!("PDF password authentication failed: {err}"),
            )
        })?;
    }
    doc.encryption_state = None;
    serialize_pdf(&mut doc)
}

/// Inspects a PDF's `/Encrypt` handler, cipher strength, `/P` permissions, and optionally tests a
/// candidate password without mutating the payload (`FR-PDF-004`).
///
/// # Errors
/// Returns [`FileFusionError`] when `raw_pdf` cannot be parsed as a PDF.
pub fn inspect_pdf_security(
    raw_pdf: &[u8],
    probe_password: Option<&str>,
) -> FileFusionResult<PdfSecurityReport> {
    if raw_pdf.is_empty() {
        return Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            "PDF input payload is empty",
        ));
    }

    let doc = Document::load_mem(raw_pdf).map_err(|err| {
        FileFusionError::new(
            ErrorCode::FmtCorrupt,
            format!("failed to parse PDF structure: {err}"),
        )
    })?;

    let password_verified = probe_password.map(|pw| {
        if !doc.is_encrypted() && !doc.was_encrypted() {
            return true;
        }
        Document::load_mem_with_options(raw_pdf, LoadOptions::with_password(pw))
            .is_ok_and(|unlocked| !unlocked.is_encrypted())
    });

    if let Some(state) = doc.encryption_state.as_ref() {
        let bits = u32::try_from(state.key_length().unwrap_or(match state.version() {
            5 => 256,
            4 => 128,
            _ => 40,
        }))
        .ok();
        let cipher_name = describe_cipher(
            state.version(),
            state.revision(),
            state.default_stream_filter(),
            bits,
        );
        return Ok(PdfSecurityReport {
            is_encrypted: true,
            version: Some(state.version()),
            revision: Some(state.revision()),
            key_bits: bits,
            cipher_name,
            encrypt_metadata: Some(state.encrypt_metadata()),
            permissions: Some(PdfPermissions::from_lopdf(state.permissions())),
            password_verified,
        });
    }

    if let Ok(enc) = doc.get_encrypted() {
        let version = enc.get(b"V").and_then(Object::as_i64).ok();
        let revision = enc.get(b"R").and_then(Object::as_i64).ok();
        let length = enc
            .get(b"Length")
            .and_then(Object::as_i64)
            .ok()
            .and_then(|v| u32::try_from(v).ok())
            .or(match version {
                Some(5) => Some(256),
                Some(4) => Some(128),
                Some(1) => Some(40),
                _ => None,
            });
        let encrypt_metadata = enc.get(b"EncryptMetadata").and_then(Object::as_bool).ok();
        let raw_p = enc.get(b"P").and_then(Object::as_i64).ok();
        let permissions = raw_p.map(|p| {
            #[allow(clippy::cast_sign_loss)]
            let bits = p as u64;
            PdfPermissions::from_lopdf(Permissions::from_bits_retain(bits))
        });
        let stmf = enc
            .get(b"StmF")
            .and_then(Object::as_name)
            .unwrap_or_default();
        let cipher_name =
            describe_cipher(version.unwrap_or(0), revision.unwrap_or(0), stmf, length);

        return Ok(PdfSecurityReport {
            is_encrypted: true,
            version,
            revision,
            key_bits: length,
            cipher_name,
            encrypt_metadata,
            permissions,
            password_verified,
        });
    }

    Ok(PdfSecurityReport {
        is_encrypted: false,
        version: None,
        revision: None,
        key_bits: None,
        cipher_name: String::from("None (unencrypted)"),
        encrypt_metadata: None,
        permissions: None,
        password_verified,
    })
}

/// Inspects all `/Type /Sig` dictionaries and `/AcroForm` `/Sig` values for structural completeness,
/// `/ByteRange` whole-file coverage, and ASN.1 DER PKCS#7/CMS `/Contents` framing (`FR-PDF-004`).
///
/// # Errors
/// Returns [`FileFusionError`] on empty or unparseable PDFs, or job cancellation.
pub fn verify_pdf_signatures(
    raw_pdf: &[u8],
    ctx: &JobContext,
) -> FileFusionResult<PdfSignatureReport> {
    ctx.check_cancelled()?;
    if raw_pdf.is_empty() {
        return Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            "PDF input payload is empty",
        ));
    }

    let doc = Document::load_mem(raw_pdf).map_err(|err| {
        FileFusionError::new(
            ErrorCode::FmtCorrupt,
            format!("failed to parse PDF structure: {err}"),
        )
    })?;

    let field_names = collect_acroform_sig_field_names(&doc);
    let mut visited_ids: HashSet<ObjectId> = HashSet::new();
    let mut signatures = Vec::new();

    for (&obj_id, obj) in &doc.objects {
        ctx.check_cancelled()?;
        let Ok(dict) = obj.as_dict() else {
            continue;
        };
        let is_sig = dict
            .get(b"Type")
            .and_then(Object::as_name)
            .is_ok_and(|n| n == b"Sig")
            || field_names.contains_key(&obj_id);
        if is_sig && visited_ids.insert(obj_id) {
            signatures.push(evaluate_signature_dict(
                dict,
                field_names.get(&obj_id).cloned(),
                raw_pdf,
            ));
        }
    }

    let has_signatures = !signatures.is_empty();
    let signature_count = signatures.len();
    let all_byte_ranges_intact = signatures
        .iter()
        .all(|s| s.status == PdfSignatureStatus::SignedByteRangeIntact);

    Ok(PdfSignatureReport {
        has_signatures,
        signature_count,
        all_byte_ranges_intact,
        signatures,
        certificate_trust_note: String::from(
            "Verified offline /ByteRange whole-file coverage and ASN.1 DER /Contents structure; full X.509 certificate-chain and CRL/OCSP trust validation requires an external system trust store.",
        ),
    })
}

/// Builds a sanitized Tier 2 `qpdf` encryption `ProcessInvocation` (`MOD-RULE-04`).
#[cfg(feature = "native-process")]
#[must_use]
pub fn build_qpdf_encrypt_invocation(
    qpdf_binary: &Path,
    input_path: &Path,
    staged_output_path: &Path,
    user_password: &str,
    owner_password: &str,
    permissions: PdfPermissions,
    timeout_secs: u64,
) -> ProcessInvocation {
    let args = vec![
        OsString::from("--encrypt"),
        OsString::from(user_password),
        OsString::from(owner_password),
        OsString::from("256"),
        OsString::from(if permissions.allow_modify {
            "--modify=all"
        } else {
            "--modify=none"
        }),
        OsString::from(if permissions.allow_copy {
            "--extract=y"
        } else {
            "--extract=n"
        }),
        OsString::from(if permissions.allow_print {
            "--print=full"
        } else {
            "--print=none"
        }),
        OsString::from("--"),
        input_path.as_os_str().to_os_string(),
        staged_output_path.as_os_str().to_os_string(),
    ];
    ProcessInvocation {
        executable: qpdf_binary.to_path_buf(),
        args,
        working_dir: None,
        timeout_secs,
        allow_env_keys: Vec::new(),
    }
}

/// Builds a sanitized Tier 2 `qpdf --decrypt` `ProcessInvocation` (`MOD-RULE-04`).
#[cfg(feature = "native-process")]
#[must_use]
pub fn build_qpdf_decrypt_invocation(
    qpdf_binary: &Path,
    input_path: &Path,
    staged_output_path: &Path,
    password: &str,
    timeout_secs: u64,
) -> ProcessInvocation {
    ProcessInvocation {
        executable: qpdf_binary.to_path_buf(),
        args: vec![
            OsString::from(format!("--password={password}")),
            OsString::from("--decrypt"),
            input_path.as_os_str().to_os_string(),
            staged_output_path.as_os_str().to_os_string(),
        ],
        working_dir: None,
        timeout_secs,
        allow_env_keys: Vec::new(),
    }
}

// =========================================================================
// Internal Helpers
// =========================================================================

fn fill_csprng_bytes(out: &mut [u8]) -> FileFusionResult<()> {
    // Reuses `lopdf`'s internal OS-seeded CSPRNG (`rand::rng()`, `wasm_js`-backed on wasm32)
    // via fresh 16-byte AES-256 IV prefixes so `compression-pdf` adds zero extra crate edges.
    let dummy_key = [0u8; 32];
    for chunk in out.chunks_mut(16) {
        let block = Aes256CryptFilter
            .encrypt(&dummy_key, b"ff")
            .map_err(|err| {
                FileFusionError::new(
                    ErrorCode::IntInvariant,
                    format!("CSPRNG failed generating PDF cryptographic entropy: {err}"),
                )
            })?;
        if block.len() < chunk.len() {
            return Err(FileFusionError::new(
                ErrorCode::IntInvariant,
                "CSPRNG block shorter than requested entropy slice",
            ));
        }
        chunk.copy_from_slice(&block[..chunk.len()]);
    }
    Ok(())
}

fn ensure_trailer_id(doc: &mut Document) -> FileFusionResult<()> {
    let has_valid_id = doc
        .trailer
        .get(b"ID")
        .and_then(Object::as_array)
        .is_ok_and(|arr| !arr.is_empty());
    if has_valid_id {
        return Ok(());
    }
    let mut id_bytes = [0u8; 16];
    fill_csprng_bytes(&mut id_bytes)?;
    let id_vec = id_bytes.to_vec();
    doc.trailer.set(
        "ID",
        Object::Array(vec![
            Object::string_literal(id_vec.clone()),
            Object::string_literal(id_vec),
        ]),
    );
    Ok(())
}

fn describe_cipher(
    version: i64,
    revision: i64,
    stream_filter: &[u8],
    key_bits: Option<u32>,
) -> String {
    match version {
        5 => format!("AES-256 (V5/R{revision})"),
        4 if stream_filter == b"AESV2" || key_bits == Some(128) => {
            format!("AES-128 (V4/R{revision})")
        }
        2 => format!("RC4-128 (V2/R{revision})"),
        1 => format!("RC4-40 (V1/R{revision})"),
        v => format!("Standard Security Handler (V{v}/R{revision})"),
    }
}

fn collect_acroform_sig_field_names(doc: &Document) -> BTreeMap<ObjectId, String> {
    let mut map = BTreeMap::new();
    let Ok(catalog) = doc.catalog() else {
        return map;
    };
    let acro_dict = match catalog.get(b"AcroForm") {
        Ok(Object::Reference(id)) => doc.get_dictionary(*id).ok(),
        Ok(Object::Dictionary(d)) => Some(d),
        _ => None,
    };
    let Some(acro) = acro_dict else {
        return map;
    };
    let fields = match acro.get(b"Fields") {
        Ok(Object::Array(arr)) => arr.as_slice(),
        _ => return map,
    };
    for item in fields {
        let Ok(fid) = item.as_reference() else {
            continue;
        };
        let Ok(fdict) = doc.get_dictionary(fid) else {
            continue;
        };
        if let Ok(sig_id) = fdict.get(b"V").and_then(Object::as_reference) {
            let name = dict_text(fdict, b"T").unwrap_or_else(|| String::from("SignatureField"));
            map.insert(sig_id, name);
        }
    }
    map
}

fn dict_name(dict: &Dictionary, key: &[u8]) -> Option<String> {
    dict.get(key)
        .and_then(Object::as_name)
        .ok()
        .map(|b| String::from_utf8_lossy(b).into_owned())
}

fn dict_text(dict: &Dictionary, key: &[u8]) -> Option<String> {
    dict.get(key)
        .and_then(Object::as_str)
        .ok()
        .map(|b| String::from_utf8_lossy(b).into_owned())
}

fn parse_byte_range(dict: &Dictionary) -> Option<[usize; 4]> {
    let arr = dict.get(b"ByteRange").and_then(Object::as_array).ok()?;
    if arr.len() != 4 {
        return None;
    }
    let v0 = usize::try_from(arr[0].as_i64().ok()?).ok()?;
    let v1 = usize::try_from(arr[1].as_i64().ok()?).ok()?;
    let v2 = usize::try_from(arr[2].as_i64().ok()?).ok()?;
    let v3 = usize::try_from(arr[3].as_i64().ok()?).ok()?;
    Some([v0, v1, v2, v3])
}

fn decode_sig_contents_bytes(dict: &Dictionary) -> Vec<u8> {
    let Ok(bytes) = dict.get(b"Contents").and_then(Object::as_str) else {
        return Vec::new();
    };
    let mut end = bytes.len();
    while end > 0 && bytes[end - 1] == 0 {
        end -= 1;
    }
    bytes[..end].to_vec()
}

fn evaluate_signature_dict(
    dict: &Dictionary,
    field_name: Option<String>,
    raw_pdf: &[u8],
) -> PdfSignatureInfo {
    let filter = dict_name(dict, b"Filter");
    let sub_filter = dict_name(dict, b"SubFilter");
    let signer_name = dict_text(dict, b"Name");
    let reason = dict_text(dict, b"Reason");
    let signing_time = dict_text(dict, b"M");
    let byte_range = parse_byte_range(dict);
    let contents = decode_sig_contents_bytes(dict);
    let contents_byte_len = contents.len();
    let der_sequence_valid = contents.first().copied() == Some(0x30) && contents_byte_len >= 4;

    let (covers_entire_file, status, diagnostic) = match byte_range {
        None => (
            false,
            PdfSignatureStatus::SignedMalformed,
            String::from("Signature dictionary is missing a valid 4-element /ByteRange array."),
        ),
        Some([off1, len1, off2, len2]) => {
            let end1 = off1.saturating_add(len1);
            let end2 = off2.saturating_add(len2);
            let bounds_valid = off1 == 0 && end1 < off2 && end2 <= raw_pdf.len();
            if !bounds_valid {
                (
                    false,
                    PdfSignatureStatus::SignedMalformed,
                    format!(
                        "/ByteRange [{off1}, {len1}, {off2}, {len2}] exceeds or contradicts {}-byte PDF.",
                        raw_pdf.len()
                    ),
                )
            } else if end2 < raw_pdf.len() {
                (
                    false,
                    PdfSignatureStatus::SignedTamperedAfterSigning,
                    format!(
                        "/ByteRange ends at byte {end2}, leaving {} trailing post-signature byte(s).",
                        raw_pdf.len().saturating_sub(end2)
                    ),
                )
            } else if !der_sequence_valid {
                (
                    true,
                    PdfSignatureStatus::SignedMalformed,
                    String::from(
                        "/Contents signature envelope is empty or missing ASN.1 DER SEQUENCE tag (0x30).",
                    ),
                )
            } else {
                let hole = &raw_pdf[end1..off2];
                let hole_is_hex_literal = hole.starts_with(b"<") && hole.ends_with(b">");
                if hole_is_hex_literal {
                    (
                        true,
                        PdfSignatureStatus::SignedByteRangeIntact,
                        format!(
                            "/ByteRange covers entire {}-byte document with valid {contents_byte_len}-byte DER signature envelope.",
                            raw_pdf.len()
                        ),
                    )
                } else {
                    (
                        false,
                        PdfSignatureStatus::SignedMalformed,
                        String::from(
                            "/ByteRange excluded hole does not align cleanly with a hex-encoded /Contents literal.",
                        ),
                    )
                }
            }
        }
    };

    PdfSignatureInfo {
        field_name,
        filter,
        sub_filter,
        signer_name,
        reason,
        signing_time,
        byte_range,
        covers_entire_file,
        contents_byte_len,
        der_sequence_valid,
        status,
        diagnostic,
    }
}
