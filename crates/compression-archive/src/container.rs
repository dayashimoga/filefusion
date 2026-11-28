//! Multi-format container creation, listing, and bomb/traversal-safe extraction
//! (`FR-ARC-001`, `FR-SEC-001`).

use std::fs::{self, File};
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

use file_core::{
    CompressionProfile, ErrorCode, FileFusionError, FileFusionResult, FormatId, JobContext,
    ResourceLimits, detect_signature_from_path,
};
use serde::{Deserialize, Serialize};
use sevenz_rust::{SevenZArchiveEntry, SevenZReader, SevenZWriter};
use tar::{Archive as TarArchive, Builder as TarBuilder, Header as TarHeader};
use validation_engine::{validate_archive_member_path, validate_expansion_budget};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::streams::{StreamCodec, compress_stream_bytes, decompress_stream_bounded};

/// Archive container and compressed-tar formats supported natively (`FR-ARC-001`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveContainerKind {
    /// PKZIP container (`.zip`).
    Zip,
    /// Uncompressed USTAR TAR container (`.tar`).
    Tar,
    /// Gzip-compressed TAR container (`.tar.gz`, `.tgz`).
    TarGz,
    /// Zstandard-compressed TAR container (`.tar.zst`, `.tzst`).
    TarZst,
    /// XZ-compressed TAR container (`.tar.xz`, `.txz`).
    TarXz,
    /// Bzip2-compressed TAR container (`.tar.bz2`, `.tbz2`).
    TarBz2,
    /// LZ4-compressed TAR container (`.tar.lz4`).
    TarLz4,
    /// Brotli-compressed TAR container (`.tar.br`).
    TarBrotli,
    /// 7-Zip archive container (`.7z`).
    SevenZ,
}

impl ArchiveContainerKind {
    /// Canonical lowercase extension (without leading dot).
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::Tar => "tar",
            Self::TarGz => "tar.gz",
            Self::TarZst => "tar.zst",
            Self::TarXz => "tar.xz",
            Self::TarBz2 => "tar.bz2",
            Self::TarLz4 => "tar.lz4",
            Self::TarBrotli => "tar.br",
            Self::SevenZ => "7z",
        }
    }

    /// Canonical [`FormatId`] describing the outer serialized bytes of this container.
    #[must_use]
    pub const fn outer_format_id(self) -> FormatId {
        match self {
            Self::Zip => FormatId::Zip,
            Self::Tar => FormatId::Tar,
            Self::TarGz => FormatId::Gzip,
            Self::TarZst => FormatId::Zstd,
            Self::TarXz => FormatId::Xz,
            Self::TarBz2 => FormatId::Bzip2,
            Self::TarLz4 => FormatId::Lz4,
            Self::TarBrotli => FormatId::Unknown,
            Self::SevenZ => FormatId::SevenZ,
        }
    }

    /// Inner wrapper [`StreamCodec`] when this format wraps a `.tar` archive.
    #[must_use]
    pub const fn wrapped_stream_codec(self) -> Option<StreamCodec> {
        match self {
            Self::TarGz => Some(StreamCodec::Gzip),
            Self::TarZst => Some(StreamCodec::Zstd),
            Self::TarXz => Some(StreamCodec::Xz),
            Self::TarBz2 => Some(StreamCodec::Bzip2),
            Self::TarLz4 => Some(StreamCodec::Lz4),
            Self::TarBrotli => Some(StreamCodec::Brotli),
            _ => None,
        }
    }

    /// Canonical codec label for telemetry and `CompressionOutcome`.
    #[must_use]
    pub const fn codec_label(self) -> &'static str {
        match self {
            Self::Zip => "archive-zip-deflate",
            Self::Tar => "archive-tar",
            Self::TarGz => "archive-tar-gzip",
            Self::TarZst => "archive-tar-zstd",
            Self::TarXz => "archive-tar-xz",
            Self::TarBz2 => "archive-tar-bzip2",
            Self::TarLz4 => "archive-tar-lz4",
            Self::TarBrotli => "archive-tar-brotli",
            Self::SevenZ => "archive-7z-lzma2",
        }
    }

    /// Resolves container kind from a source path's magic signature and compound extension
    /// using default [`ResourceLimits`].
    ///
    /// # Errors
    /// Returns [`FileFusionError`] if reading fails or format is not an archive container.
    pub fn detect_from_path(path: &Path) -> FileFusionResult<Self> {
        Self::detect_from_path_with_limits(path, &ResourceLimits::default())
    }

    /// Resolves container kind from a source path's magic signature, compound extension,
    /// or bounded stream header inspection under custom [`ResourceLimits`].
    ///
    /// # Errors
    /// Returns [`FileFusionError`] on I/O failure, bomb limit breach, or unsupported container.
    pub fn detect_from_path_with_limits(
        path: &Path,
        limits: &ResourceLimits,
    ) -> FileFusionResult<Self> {
        let sig = detect_signature_from_path(path)?;
        let lower_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();

        match sig.detected_format {
            FormatId::Zip => Ok(Self::Zip),
            FormatId::SevenZ => Ok(Self::SevenZ),
            FormatId::Tar => Ok(Self::Tar),
            FormatId::Gzip => {
                if has_tar_ext(path, &lower_name, ".tar.gz", "tgz") {
                    Ok(Self::TarGz)
                } else {
                    inspect_compressed_tar_or_err(path, StreamCodec::Gzip, limits, Self::TarGz)
                }
            }
            FormatId::Zstd => {
                if has_tar_ext(path, &lower_name, ".tar.zst", "tzst") {
                    Ok(Self::TarZst)
                } else {
                    inspect_compressed_tar_or_err(path, StreamCodec::Zstd, limits, Self::TarZst)
                }
            }
            FormatId::Xz => {
                if has_tar_ext(path, &lower_name, ".tar.xz", "txz") {
                    Ok(Self::TarXz)
                } else {
                    inspect_compressed_tar_or_err(path, StreamCodec::Xz, limits, Self::TarXz)
                }
            }
            FormatId::Bzip2 => {
                if has_tar_ext(path, &lower_name, ".tar.bz2", "tbz2") {
                    Ok(Self::TarBz2)
                } else {
                    inspect_compressed_tar_or_err(path, StreamCodec::Bzip2, limits, Self::TarBz2)
                }
            }
            FormatId::Lz4 => {
                if lower_name.ends_with(".tar.lz4") {
                    Ok(Self::TarLz4)
                } else {
                    inspect_compressed_tar_or_err(path, StreamCodec::Lz4, limits, Self::TarLz4)
                }
            }
            FormatId::Unknown => {
                if lower_name.ends_with(".tar.br") {
                    Ok(Self::TarBrotli)
                } else {
                    inspect_compressed_tar_or_err(
                        path,
                        StreamCodec::Brotli,
                        limits,
                        Self::TarBrotli,
                    )
                }
            }
            other => Err(FileFusionError::new(
                ErrorCode::FmtUnsupported,
                format!("format {other:?} is not a supported multi-file archive container"),
            )),
        }
    }
}

fn has_tar_ext(path: &Path, lower_name: &str, compound: &str, short_ext: &str) -> bool {
    lower_name.ends_with(compound)
        || path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case(short_ext))
}

fn inspect_compressed_tar_or_err(
    path: &Path,
    codec: StreamCodec,
    limits: &ResourceLimits,
    if_tar: ArchiveContainerKind,
) -> FileFusionResult<ArchiveContainerKind> {
    let raw = fs::read(path)?;
    let inner = decompress_stream_bounded(&raw, codec, limits, 0)?;
    if inner.get(257..262) == Some(b"ustar") {
        Ok(if_tar)
    } else {
        Err(FileFusionError::new(
            ErrorCode::FmtUnsupported,
            format!("standalone {codec:?} stream does not contain a TAR archive"),
        ))
    }
}

/// Safe in-memory input entry for deterministic archive construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveInputEntry {
    /// Relative entry path (validated via `validation_engine::validate_archive_member_path`).
    pub path: String,
    /// Uncompressed file payload bytes.
    pub data: Vec<u8>,
}

/// Metadata summary describing an entry inside an archive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveMemberMetadata {
    /// Sanitized relative path inside the archive.
    pub path: String,
    /// Compressed bytes if known.
    pub compressed_bytes: Option<u64>,
    /// Uncompressed byte length.
    pub uncompressed_bytes: u64,
    /// True if entry represents a directory.
    pub is_directory: bool,
}

/// Summary returned after writing an archive container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveWriteSummary {
    /// Container kind written.
    pub kind: ArchiveContainerKind,
    /// Number of file entries written.
    pub entry_count: usize,
    /// Total uncompressed input bytes across entries.
    pub total_input_bytes: u64,
    /// Total serialized archive output bytes.
    pub total_output_bytes: u64,
}

/// Summary returned after safe archive extraction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveExtractionReport {
    /// Container kind extracted.
    pub kind: ArchiveContainerKind,
    /// Files extracted safely into the target directory.
    pub extracted_files: Vec<PathBuf>,
    /// Total uncompressed bytes written.
    pub total_bytes_written: u64,
}

/// Lists archive entries cleanly while rejecting traversal paths (`FR-ARC-001`, `FR-SEC-001`).
///
/// # Errors
/// Returns [`FileFusionError`] on corrupt containers or path traversal attempts.
pub fn list_archive_members(
    archive_path: &Path,
    limits: &ResourceLimits,
) -> FileFusionResult<Vec<ArchiveMemberMetadata>> {
    let kind = ArchiveContainerKind::detect_from_path_with_limits(archive_path, limits)?;
    let archive_len = fs::metadata(archive_path)?.len().max(1);

    match kind {
        ArchiveContainerKind::Zip => list_zip_members(archive_path, limits, archive_len),
        ArchiveContainerKind::SevenZ => list_7z_members(archive_path, limits, archive_len),
        ArchiveContainerKind::Tar => {
            let file = File::open(archive_path)?;
            list_tar_members(file, limits, archive_len)
        }
        wrapped => {
            let codec = wrapped.wrapped_stream_codec().ok_or_else(|| {
                FileFusionError::new(ErrorCode::FmtUnsupported, "missing tar stream codec")
            })?;
            let raw = fs::read(archive_path)?;
            let tar_bytes = decompress_stream_bounded(&raw, codec, limits, 0)?;
            list_tar_members(Cursor::new(tar_bytes), limits, archive_len)
        }
    }
}

/// Decodes and validates all regular file entries of an archive in memory under [`ResourceLimits`].
///
/// # Errors
/// Returns [`FileFusionError`] on path traversal (`SecTraversal`), expansion bomb (`SecBomb`),
/// cancellation, or malformed container structure.
pub fn read_archive_entries_checked(
    archive_path: &Path,
    kind: ArchiveContainerKind,
    limits: &ResourceLimits,
    ctx: &JobContext,
) -> FileFusionResult<Vec<ArchiveInputEntry>> {
    ctx.check_cancelled()?;
    let archive_len = fs::metadata(archive_path)?.len().max(1);

    let entries = match kind {
        ArchiveContainerKind::Zip => {
            read_zip_entries_checked(archive_path, limits, archive_len, ctx)?
        }
        ArchiveContainerKind::SevenZ => {
            read_7z_entries_checked(archive_path, limits, archive_len, ctx)?
        }
        ArchiveContainerKind::Tar => {
            let file = File::open(archive_path)?;
            read_tar_entries_checked(file, limits, archive_len, ctx)?
        }
        wrapped => {
            let codec = wrapped.wrapped_stream_codec().ok_or_else(|| {
                FileFusionError::new(ErrorCode::FmtUnsupported, "missing tar stream codec")
            })?;
            let raw = fs::read(archive_path)?;
            ctx.check_cancelled()?;
            let tar_bytes = decompress_stream_bounded(&raw, codec, limits, 0)?;
            read_tar_entries_checked(Cursor::new(tar_bytes), limits, archive_len, ctx)?
        }
    };

    Ok(entries)
}

/// Creates a multi-format archive from validated in-memory entries (`FR-ARC-001`).
///
/// # Errors
/// Returns [`FileFusionError`] on unsafe member paths, cancellation, memory budget exhaustion, or I/O errors.
pub fn create_archive_from_entries(
    entries: &[ArchiveInputEntry],
    dest_path: &Path,
    kind: ArchiveContainerKind,
    profile: CompressionProfile,
    ctx: &JobContext,
) -> FileFusionResult<ArchiveWriteSummary> {
    ctx.check_cancelled()?;
    let total_input_bytes: u64 = entries.iter().map(|e| e.data.len() as u64).sum();
    let est_mb = (total_input_bytes / (1024 * 1024)).clamp(1, 512);
    let _mem = ctx.reserve_working_set_mb(est_mb, "archive_create")?;

    let mut sanitized = Vec::with_capacity(entries.len());
    for entry in entries {
        ctx.check_cancelled()?;
        let clean_path = validate_archive_member_path(&entry.path)?;
        sanitized.push((clean_path, entry.data.as_slice()));
    }

    if let Some(parent) = dest_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    match kind {
        ArchiveContainerKind::Zip => write_zip_entries(&sanitized, dest_path, profile, ctx)?,
        ArchiveContainerKind::SevenZ => write_7z_entries(&sanitized, dest_path, ctx)?,
        ArchiveContainerKind::Tar => {
            let file = File::create(dest_path)?;
            write_tar_entries(&sanitized, file, ctx)?;
        }
        wrapped => {
            let codec = wrapped.wrapped_stream_codec().ok_or_else(|| {
                FileFusionError::new(ErrorCode::FmtUnsupported, "missing tar stream codec")
            })?;
            let mut tar_buf = Vec::new();
            write_tar_entries(&sanitized, &mut tar_buf, ctx)?;
            ctx.check_cancelled()?;
            let compressed = compress_stream_bytes(&tar_buf, codec, profile)?;
            fs::write(dest_path, compressed)?;
        }
    }

    let total_output_bytes = fs::metadata(dest_path)?.len();
    Ok(ArchiveWriteSummary {
        kind,
        entry_count: sanitized.len(),
        total_input_bytes,
        total_output_bytes,
    })
}

/// Safely extracts an archive into `dest_dir`, strictly enforcing Zip-Slip traversal
/// rejection and decompression-bomb ceilings (`FR-SEC-001`). Any partial files created
/// before an error are rolled back automatically.
///
/// # Errors
/// Returns [`FileFusionError`] on traversal attempt, bomb threshold breach, cancellation, or I/O error.
pub fn extract_archive_safely(
    archive_path: &Path,
    dest_dir: &Path,
    limits: &ResourceLimits,
    ctx: &JobContext,
) -> FileFusionResult<ArchiveExtractionReport> {
    ctx.check_cancelled()?;
    let kind = ArchiveContainerKind::detect_from_path_with_limits(archive_path, limits)?;
    let created_dest = !dest_dir.exists();
    fs::create_dir_all(dest_dir)?;

    let mut written_paths: Vec<PathBuf> = Vec::new();
    let result = (|| -> FileFusionResult<ArchiveExtractionReport> {
        let entries = read_archive_entries_checked(archive_path, kind, limits, ctx)?;
        let mut total_bytes_written: u64 = 0;

        for entry in entries {
            ctx.check_cancelled()?;
            let clean_rel = validate_archive_member_path(&entry.path)?;
            let target = dest_dir.join(&clean_rel);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&target, &entry.data)?;
            total_bytes_written = total_bytes_written.saturating_add(entry.data.len() as u64);
            written_paths.push(target);
        }

        Ok(ArchiveExtractionReport {
            kind,
            extracted_files: written_paths.clone(),
            total_bytes_written,
        })
    })();

    if result.is_err() {
        for path in written_paths.iter().rev() {
            let _ = fs::remove_file(path);
        }
        if created_dest {
            let _ = fs::remove_dir_all(dest_dir);
        }
    }

    result
}

fn list_zip_members(
    archive_path: &Path,
    limits: &ResourceLimits,
    archive_len: u64,
) -> FileFusionResult<Vec<ArchiveMemberMetadata>> {
    let file = File::open(archive_path)?;
    let mut archive = ZipArchive::new(file).map_err(|err| {
        FileFusionError::new(ErrorCode::FmtCorrupt, format!("corrupt zip archive: {err}"))
    })?;
    let mut out = Vec::with_capacity(archive.len());
    let mut cumulative_uncompressed: u64 = 0;

    for idx in 0..archive.len() {
        let entry = archive.by_index(idx).map_err(|err| {
            FileFusionError::new(ErrorCode::FmtCorrupt, format!("zip entry error: {err}"))
        })?;
        if entry.is_symlink() {
            return Err(FileFusionError::new(
                ErrorCode::SecTraversal,
                format!(
                    "zip archive contains forbidden symlink entry `{}`",
                    entry.name()
                ),
            ));
        }
        let clean = validate_archive_member_path(entry.name())?;
        cumulative_uncompressed = cumulative_uncompressed.saturating_add(entry.size());
        validate_expansion_budget(archive_len, cumulative_uncompressed, 0, limits)?;
        out.push(ArchiveMemberMetadata {
            path: clean,
            compressed_bytes: Some(entry.compressed_size()),
            uncompressed_bytes: entry.size(),
            is_directory: entry.is_dir(),
        });
    }
    Ok(out)
}

fn read_zip_entries_checked(
    archive_path: &Path,
    limits: &ResourceLimits,
    archive_len: u64,
    ctx: &JobContext,
) -> FileFusionResult<Vec<ArchiveInputEntry>> {
    let file = File::open(archive_path)?;
    let mut archive = ZipArchive::new(file).map_err(|err| {
        FileFusionError::new(ErrorCode::FmtCorrupt, format!("corrupt zip archive: {err}"))
    })?;
    let mut out = Vec::new();
    let mut cumulative: u64 = 0;

    for idx in 0..archive.len() {
        ctx.check_cancelled()?;
        let mut entry = archive.by_index(idx).map_err(|err| {
            FileFusionError::new(ErrorCode::FmtCorrupt, format!("zip entry error: {err}"))
        })?;
        if entry.is_symlink() {
            return Err(FileFusionError::new(
                ErrorCode::SecTraversal,
                format!("zip archive contains forbidden symlink `{}`", entry.name()),
            ));
        }
        let clean = validate_archive_member_path(entry.name())?;
        if entry.is_dir() {
            continue;
        }
        validate_expansion_budget(
            archive_len,
            cumulative.saturating_add(entry.size()),
            0,
            limits,
        )?;
        let data = read_entry_stream_checked(&mut entry, &mut cumulative, archive_len, limits)?;
        out.push(ArchiveInputEntry { path: clean, data });
    }

    Ok(out)
}

fn list_tar_members<R: Read>(
    reader: R,
    limits: &ResourceLimits,
    archive_len: u64,
) -> FileFusionResult<Vec<ArchiveMemberMetadata>> {
    let mut archive = TarArchive::new(reader);
    let mut out = Vec::new();
    let mut cumulative: u64 = 0;

    let entries = archive.entries().map_err(|err| {
        FileFusionError::new(ErrorCode::FmtCorrupt, format!("corrupt tar archive: {err}"))
    })?;
    for entry_res in entries {
        let entry = entry_res.map_err(|err| {
            FileFusionError::new(
                ErrorCode::FmtCorrupt,
                format!("tar entry header error: {err}"),
            )
        })?;
        let entry_type = entry.header().entry_type();
        let raw_path = entry.path().map_err(|err| {
            FileFusionError::new(
                ErrorCode::FmtCorrupt,
                format!("invalid tar path bytes: {err}"),
            )
        })?;
        let path_str = raw_path.to_string_lossy();
        if entry_type.is_symlink() || entry_type.is_hard_link() {
            return Err(FileFusionError::new(
                ErrorCode::SecTraversal,
                format!("tar archive contains forbidden link entry `{path_str}`"),
            ));
        }
        let clean = validate_archive_member_path(&path_str)?;
        let size = entry.header().size().map_err(|err| {
            FileFusionError::new(
                ErrorCode::FmtCorrupt,
                format!("invalid tar entry size: {err}"),
            )
        })?;
        cumulative = cumulative.saturating_add(size);
        validate_expansion_budget(archive_len, cumulative, 0, limits)?;
        out.push(ArchiveMemberMetadata {
            path: clean,
            compressed_bytes: None,
            uncompressed_bytes: size,
            is_directory: entry_type.is_dir(),
        });
    }
    Ok(out)
}

fn read_tar_entries_checked<R: Read>(
    reader: R,
    limits: &ResourceLimits,
    archive_len: u64,
    ctx: &JobContext,
) -> FileFusionResult<Vec<ArchiveInputEntry>> {
    let mut archive = TarArchive::new(reader);
    let mut out = Vec::new();
    let mut cumulative: u64 = 0;

    let entries = archive.entries().map_err(|err| {
        FileFusionError::new(ErrorCode::FmtCorrupt, format!("corrupt tar archive: {err}"))
    })?;
    for entry_res in entries {
        ctx.check_cancelled()?;
        let mut entry = entry_res.map_err(|err| {
            FileFusionError::new(ErrorCode::FmtCorrupt, format!("tar entry error: {err}"))
        })?;
        let entry_type = entry.header().entry_type();
        let path_cow = entry.path().map_err(|err| {
            FileFusionError::new(ErrorCode::FmtCorrupt, format!("invalid tar path: {err}"))
        })?;
        let path_str = path_cow.to_string_lossy().into_owned();
        if entry_type.is_symlink() || entry_type.is_hard_link() {
            return Err(FileFusionError::new(
                ErrorCode::SecTraversal,
                format!("tar archive contains forbidden link `{path_str}`"),
            ));
        }
        let clean = validate_archive_member_path(&path_str)?;
        if entry_type.is_dir() {
            continue;
        }
        if !entry_type.is_file() {
            return Err(FileFusionError::new(
                ErrorCode::SecTraversal,
                format!("unsupported special tar entry type for `{clean}`"),
            ));
        }
        let data = read_entry_stream_checked(&mut entry, &mut cumulative, archive_len, limits)?;
        out.push(ArchiveInputEntry { path: clean, data });
    }
    Ok(out)
}

fn list_7z_members(
    archive_path: &Path,
    limits: &ResourceLimits,
    archive_len: u64,
) -> FileFusionResult<Vec<ArchiveMemberMetadata>> {
    let reader =
        SevenZReader::open(archive_path, sevenz_rust::Password::empty()).map_err(|err| {
            FileFusionError::new(ErrorCode::FmtCorrupt, format!("corrupt 7z archive: {err}"))
        })?;
    let mut out = Vec::with_capacity(reader.archive().files.len());
    let mut cumulative: u64 = 0;

    for entry in &reader.archive().files {
        let clean = validate_archive_member_path(entry.name())?;
        cumulative = cumulative.saturating_add(entry.size());
        validate_expansion_budget(archive_len, cumulative, 0, limits)?;
        out.push(ArchiveMemberMetadata {
            path: clean,
            compressed_bytes: Some(entry.compressed_size),
            uncompressed_bytes: entry.size(),
            is_directory: entry.is_directory(),
        });
    }
    Ok(out)
}

fn read_7z_entries_checked(
    archive_path: &Path,
    limits: &ResourceLimits,
    archive_len: u64,
    ctx: &JobContext,
) -> FileFusionResult<Vec<ArchiveInputEntry>> {
    let mut reader =
        SevenZReader::open(archive_path, sevenz_rust::Password::empty()).map_err(|err| {
            FileFusionError::new(ErrorCode::FmtCorrupt, format!("corrupt 7z archive: {err}"))
        })?;
    let mut out = Vec::new();
    let mut cumulative: u64 = 0;

    let res = reader.for_each_entries(|entry, reader| {
        if ctx.cancellation.is_cancelled() {
            return Err(sevenz_rust::Error::other("cancelled"));
        }
        let clean = validate_archive_member_path(entry.name())
            .map_err(|e| sevenz_rust::Error::other(e.to_string()))?;
        if entry.is_directory() {
            return Ok(true);
        }
        let data = read_entry_stream_checked(reader, &mut cumulative, archive_len, limits)
            .map_err(|e| sevenz_rust::Error::other(e.to_string()))?;
        out.push(ArchiveInputEntry { path: clean, data });
        Ok(true)
    });

    match res {
        Ok(()) => Ok(out),
        Err(err) => {
            ctx.check_cancelled()?;
            // Re-run path/budget classification for typed error fidelity if tripped inside closure.
            let _ = list_7z_members(archive_path, limits, archive_len)?;
            validate_expansion_budget(archive_len, cumulative, 0, limits)?;
            Err(FileFusionError::new(
                ErrorCode::FmtCorrupt,
                format!("failed decoding 7z archive: {err}"),
            ))
        }
    }
}

fn read_entry_stream_checked<R: Read + ?Sized>(
    reader: &mut R,
    cumulative: &mut u64,
    archive_len: u64,
    limits: &ResourceLimits,
) -> FileFusionResult<Vec<u8>> {
    let mut data = Vec::new();
    let mut chunk = vec![0u8; 32 * 1024];
    loop {
        let n = reader.read(&mut chunk).map_err(|err| {
            FileFusionError::new(
                ErrorCode::FmtCorrupt,
                format!("failed reading archive entry payload: {err}"),
            )
        })?;
        if n == 0 {
            break;
        }
        *cumulative = cumulative.saturating_add(n as u64);
        validate_expansion_budget(archive_len, *cumulative, 0, limits)?;
        data.extend_from_slice(&chunk[..n]);
    }
    Ok(data)
}

fn write_zip_entries(
    entries: &[(String, &[u8])],
    dest_path: &Path,
    profile: CompressionProfile,
    ctx: &JobContext,
) -> FileFusionResult<()> {
    let file = File::create(dest_path)?;
    let mut writer = ZipWriter::new(file);
    let level = match profile {
        CompressionProfile::LosslessMaximum | CompressionProfile::ArchivalColdStorage => 9,
        CompressionProfile::ExtremeSizeTarget => 8,
        CompressionProfile::BalancedOptimal | CompressionProfile::WebSharingReady => 6,
    };
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(i64::from(level)));

    for (name, data) in entries {
        ctx.check_cancelled()?;
        writer.start_file(name, options).map_err(|err| {
            FileFusionError::new(
                ErrorCode::IntInvariant,
                format!("zip start_file failed: {err}"),
            )
        })?;
        writer.write_all(data)?;
    }

    writer.finish().map_err(|err| {
        FileFusionError::new(ErrorCode::IntInvariant, format!("zip finish failed: {err}"))
    })?;
    Ok(())
}

fn write_tar_entries<W: Write>(
    entries: &[(String, &[u8])],
    dest: W,
    ctx: &JobContext,
) -> FileFusionResult<()> {
    let mut builder = TarBuilder::new(dest);
    for (name, data) in entries {
        ctx.check_cancelled()?;
        let mut header = TarHeader::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, name, Cursor::new(data))?;
    }
    builder.finish()?;
    Ok(())
}

fn write_7z_entries(
    entries: &[(String, &[u8])],
    dest_path: &Path,
    ctx: &JobContext,
) -> FileFusionResult<()> {
    let mut writer = SevenZWriter::create(dest_path).map_err(|err| {
        FileFusionError::new(ErrorCode::IntInvariant, format!("7z create failed: {err}"))
    })?;
    for (name, data) in entries {
        ctx.check_cancelled()?;
        let mut entry = SevenZArchiveEntry::new();
        entry.name.clone_from(name);
        entry.has_stream = true;
        entry.is_directory = false;
        entry.size = data.len() as u64;
        writer
            .push_archive_entry(entry, Some(Cursor::new(*data)))
            .map_err(|err| {
                FileFusionError::new(ErrorCode::IntInvariant, format!("7z push failed: {err}"))
            })?;
    }
    writer.finish().map_err(|err| {
        FileFusionError::new(ErrorCode::IntInvariant, format!("7z finish failed: {err}"))
    })?;
    Ok(())
}
