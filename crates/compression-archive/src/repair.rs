//! Structural ZIP central-directory recovery (`FR-REP-001`, `FF-MOD-001` §4).

use std::fs::{self, File};
use std::io::{Cursor, Read, Write};
use std::path::Path;

use crc32fast::Hasher as Crc32Hasher;
use file_core::{ErrorCode, FileFusionError, FileFusionResult, JobContext, ResourceLimits};
use flate2::read::DeflateDecoder;
use serde::{Deserialize, Serialize};
use validation_engine::{validate_archive_member_path, validate_expansion_budget};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

const ZIP_LFH_SIG: [u8; 4] = [0x50, 0x4B, 0x03, 0x04];
const ZIP_CEN_SIG: [u8; 4] = [0x50, 0x4B, 0x01, 0x02];
const ZIP_EOCD_SIG: [u8; 4] = [0x50, 0x4B, 0x05, 0x06];

/// Diagnostic report summarizing ZIP local-header scanning and central-directory reconstruction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZipRepairReport {
    /// Number of intact file entries recovered and re-indexed.
    pub entries_recovered: usize,
    /// Number of damaged or unrecoverable local-file records skipped.
    pub entries_skipped_corrupt: usize,
    /// Total uncompressed bytes recovered across valid entries.
    pub bytes_recovered: u64,
    /// Always `true` when at least one valid entry was rewritten into a clean Central Directory.
    pub rebuilt_central_directory: bool,
    /// Sanitized member paths recovered.
    pub recovered_paths: Vec<String>,
}

/// Scans sequential Local File Headers (`PK\x03\x04`) in a damaged or truncated ZIP archive,
/// verifies CRC-32 checksums & Zip-Slip safety, enforces [`ResourceLimits`] bomb ceilings,
/// and rebuilds a clean ZIP with a valid Central Directory and EOCD (`FR-REP-001`).
///
/// # Errors
/// Returns [`FileFusionError`] on unsafe path traversal (`SecTraversal`), bomb limits (`SecBomb`),
/// cancellation, or if zero intact entries can be recovered (`FmtCorrupt`).
pub fn repair_truncated_zip(
    corrupt_zip_path: &Path,
    repaired_zip_path: &Path,
    limits: &ResourceLimits,
    ctx: &JobContext,
) -> FileFusionResult<ZipRepairReport> {
    ctx.check_cancelled()?;
    let raw = fs::read(corrupt_zip_path)?;
    let archive_len = (raw.len() as u64).max(1);
    let est_mb = (archive_len / (1024 * 1024)).clamp(1, 512);
    let _mem = ctx.reserve_working_set_mb(est_mb, "zip_repair")?;

    let (recovered, entries_skipped_corrupt, cumulative_bytes) =
        scan_recovered_zip_entries(&raw, archive_len, limits, ctx)?;

    if recovered.is_empty() {
        return Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            "no intact local file entries could be recovered from damaged ZIP archive",
        ));
    }

    if let Some(parent) = repaired_zip_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let out_file = File::create(repaired_zip_path)?;
    let mut writer = ZipWriter::new(out_file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(6));

    let mut recovered_paths = Vec::with_capacity(recovered.len());
    for (name, bytes) in recovered {
        ctx.check_cancelled()?;
        writer.start_file(&name, options).map_err(|err| {
            FileFusionError::new(
                ErrorCode::IntInvariant,
                format!("zip repair write failed: {err}"),
            )
        })?;
        writer.write_all(&bytes)?;
        recovered_paths.push(name);
    }

    writer.finish().map_err(|err| {
        FileFusionError::new(
            ErrorCode::IntInvariant,
            format!("zip repair finalize failed: {err}"),
        )
    })?;

    Ok(ZipRepairReport {
        entries_recovered: recovered_paths.len(),
        entries_skipped_corrupt,
        bytes_recovered: cumulative_bytes,
        rebuilt_central_directory: true,
        recovered_paths,
    })
}

type ZipScanResult = (Vec<(String, Vec<u8>)>, usize, u64);

fn scan_recovered_zip_entries(
    raw: &[u8],
    archive_len: u64,
    limits: &ResourceLimits,
    ctx: &JobContext,
) -> FileFusionResult<ZipScanResult> {
    let mut offset = 0usize;
    let mut recovered: Vec<(String, Vec<u8>)> = Vec::new();
    let mut entries_skipped_corrupt = 0usize;
    let mut cumulative_bytes: u64 = 0;

    while let Some(rel) = find_signature(&raw[offset..], ZIP_LFH_SIG) {
        ctx.check_cancelled()?;
        let lfh = offset + rel;
        if lfh + 30 > raw.len() {
            entries_skipped_corrupt += 1;
            break;
        }

        let flags = u16_le(raw, lfh + 6);
        let method = u16_le(raw, lfh + 8);
        let expected_crc = u32_le(raw, lfh + 14);
        let comp_size = u32_le(raw, lfh + 18) as usize;
        let name_len = u16_le(raw, lfh + 26) as usize;
        let extra_len = u16_le(raw, lfh + 28) as usize;

        let Some(name_start) = lfh.checked_add(30) else {
            entries_skipped_corrupt += 1;
            break;
        };
        let Some(name_end) = name_start.checked_add(name_len) else {
            entries_skipped_corrupt += 1;
            offset = lfh + 4;
            continue;
        };
        let Some(payload_start) = name_end.checked_add(extra_len) else {
            entries_skipped_corrupt += 1;
            offset = lfh + 4;
            continue;
        };
        let Some(name_bytes) = raw.get(name_start..name_end) else {
            entries_skipped_corrupt += 1;
            break;
        };
        let raw_name = String::from_utf8_lossy(name_bytes).into_owned();
        if raw_name.ends_with('/') {
            offset = payload_start.min(raw.len());
            continue;
        }
        let clean_name = validate_archive_member_path(&raw_name)?;

        let payload_end = if flags & 0x0008 == 0 && comp_size > 0 {
            payload_start.saturating_add(comp_size)
        } else {
            next_zip_boundary(raw, payload_start).unwrap_or(raw.len())
        };

        if payload_end > raw.len() || payload_start > payload_end {
            entries_skipped_corrupt += 1;
            offset = lfh + 4;
            continue;
        }

        if let Some(plain) = decode_lfh_payload(
            &raw[payload_start..payload_end],
            method,
            expected_crc,
            flags & 0x0008 != 0,
            archive_len,
            &mut cumulative_bytes,
            limits,
        )? {
            recovered.push((clean_name, plain));
            offset = payload_end;
        } else {
            entries_skipped_corrupt += 1;
            offset = lfh + 4;
        }
    }

    Ok((recovered, entries_skipped_corrupt, cumulative_bytes))
}

fn decode_lfh_payload(
    compressed_slice: &[u8],
    method: u16,
    expected_crc: u32,
    has_data_descriptor: bool,
    archive_len: u64,
    cumulative_bytes: &mut u64,
    limits: &ResourceLimits,
) -> FileFusionResult<Option<Vec<u8>>> {
    let plain = match method {
        0 => {
            *cumulative_bytes = cumulative_bytes.saturating_add(compressed_slice.len() as u64);
            validate_expansion_budget(archive_len, *cumulative_bytes, 0, limits)?;
            compressed_slice.to_vec()
        }
        8 => {
            let mut decoder = DeflateDecoder::new(Cursor::new(compressed_slice));
            let mut out = Vec::new();
            let mut chunk = vec![0u8; 32 * 1024];
            let mut entry_bytes = *cumulative_bytes;
            loop {
                match decoder.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => {
                        entry_bytes = entry_bytes.saturating_add(n as u64);
                        validate_expansion_budget(archive_len, entry_bytes, 0, limits)?;
                        out.extend_from_slice(&chunk[..n]);
                    }
                    Err(_) => return Ok(None),
                }
            }
            *cumulative_bytes = entry_bytes;
            out
        }
        _ => return Ok(None),
    };

    if !has_data_descriptor && expected_crc != 0 {
        let mut hasher = Crc32Hasher::new();
        hasher.update(&plain);
        if hasher.finalize() != expected_crc {
            return Ok(None);
        }
    }

    Ok(Some(plain))
}

fn next_zip_boundary(raw: &[u8], start: usize) -> Option<usize> {
    let tail = raw.get(start..)?;
    [ZIP_LFH_SIG, ZIP_CEN_SIG, ZIP_EOCD_SIG]
        .iter()
        .filter_map(|&sig| find_signature(tail, sig))
        .min()
        .map(|rel| start + rel)
}

fn find_signature(haystack: &[u8], sig: [u8; 4]) -> Option<usize> {
    haystack.windows(4).position(|w| w == sig)
}

fn u16_le(slice: &[u8], off: usize) -> u16 {
    slice
        .get(off..off + 2)
        .and_then(|s| s.try_into().ok())
        .map_or(0, u16::from_le_bytes)
}

fn u32_le(slice: &[u8], off: usize) -> u32 {
    slice
        .get(off..off + 4)
        .and_then(|s| s.try_into().ok())
        .map_or(0, u32::from_le_bytes)
}
