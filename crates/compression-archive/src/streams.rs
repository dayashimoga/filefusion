//! Standalone high-ratio stream compressors and bomb-bounded decoders (`FR-ARC-002`).

use std::io::{Cursor, Read, Write};

use compression_core::CompressionProfileConfig;
use file_core::{
    CompressionProfile, ErrorCode, FileFusionError, FileFusionResult, FormatId, ResourceLimits,
};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use lz4_flex::frame::{FrameDecoder, FrameEncoder};
use lzma_rust2::{XzOptions, XzReader, XzWriter};
use oxiarc_bzip2::{CompressionLevel as BzLevel, compress as bz2_compress, decompress_with_limit};
use serde::{Deserialize, Serialize};
use validation_engine::validate_expansion_budget;

/// Supported lossless byte-stream compression algorithms (`FR-ARC-002`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamCodec {
    /// Zstandard (`zstd`).
    Zstd,
    /// LZ4 frame (`lz4_flex`).
    Lz4,
    /// Brotli RFC 7932 (`brotli`).
    Brotli,
    /// Gzip RFC 1952 (`flate2` pure-Rust backend).
    Gzip,
    /// XZ / LZMA2 (`lzma-rust2`).
    Xz,
    /// Bzip2 (`oxiarc-bzip2`).
    Bzip2,
}

impl StreamCodec {
    /// Canonical lowercase filename extension for standalone streams.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Zstd => "zst",
            Self::Lz4 => "lz4",
            Self::Brotli => "br",
            Self::Gzip => "gz",
            Self::Xz => "xz",
            Self::Bzip2 => "bz2",
        }
    }

    /// Canonical MIME label or codec string for audit logs.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Zstd => "archive-zstd",
            Self::Lz4 => "archive-lz4",
            Self::Brotli => "archive-brotli",
            Self::Gzip => "archive-gzip",
            Self::Xz => "archive-xz",
            Self::Bzip2 => "archive-bzip2",
        }
    }

    /// Maps a [`FormatId`] stream format to [`StreamCodec`], if applicable.
    #[must_use]
    pub const fn from_format(format: FormatId) -> Option<Self> {
        match format {
            FormatId::Zstd => Some(Self::Zstd),
            FormatId::Lz4 => Some(Self::Lz4),
            FormatId::Gzip => Some(Self::Gzip),
            FormatId::Xz => Some(Self::Xz),
            FormatId::Bzip2 => Some(Self::Bzip2),
            _ => None,
        }
    }

    /// Maps this codec to canonical [`FormatId`] (uses `Unknown` for raw headerless Brotli).
    #[must_use]
    pub const fn format_id(self) -> FormatId {
        match self {
            Self::Zstd => FormatId::Zstd,
            Self::Lz4 => FormatId::Lz4,
            Self::Brotli => FormatId::Unknown,
            Self::Gzip => FormatId::Gzip,
            Self::Xz => FormatId::Xz,
            Self::Bzip2 => FormatId::Bzip2,
        }
    }
}

/// Copies bytes from `reader` into a `Vec<u8>` while enforcing [`ResourceLimits`] on
/// cumulative uncompressed output size and expansion ratio (`FR-SEC-001`).
///
/// # Errors
/// Returns [`FileFusionError`] (`SecBomb` or `FmtCorrupt`) if limits are exceeded or reading fails.
pub fn read_stream_bounded<R: Read>(
    reader: &mut R,
    compressed_len_hint: u64,
    limits: &ResourceLimits,
    depth: u8,
) -> FileFusionResult<Vec<u8>> {
    let mut out = Vec::new();
    let mut chunk = vec![0u8; 64 * 1024];
    let mut total: u64 = 0;

    loop {
        let n = reader.read(&mut chunk).map_err(|err| {
            FileFusionError::new(
                ErrorCode::FmtCorrupt,
                format!("corrupt compressed stream: {err}"),
            )
        })?;
        if n == 0 {
            break;
        }
        total = total.saturating_add(n as u64);
        validate_expansion_budget(compressed_len_hint.max(1), total, depth, limits)?;
        out.extend_from_slice(&chunk[..n]);
    }

    Ok(out)
}

/// Compresses raw `data` using `codec` tuned to `profile` (`FR-ARC-002`, `FR-CMP-001`).
///
/// # Errors
/// Returns [`FileFusionError`] if the underlying encoder fails.
pub fn compress_stream_bytes(
    data: &[u8],
    codec: StreamCodec,
    profile: CompressionProfile,
) -> FileFusionResult<Vec<u8>> {
    let cfg = CompressionProfileConfig::for_profile(profile);
    match codec {
        StreamCodec::Zstd => {
            let level = zstd_level_for(cfg.effort_level);
            zstd::stream::encode_all(Cursor::new(data), level).map_err(|err| {
                FileFusionError::new(
                    ErrorCode::IntInvariant,
                    format!("zstd encoding failed: {err}"),
                )
            })
        }
        StreamCodec::Lz4 => {
            let mut encoder = FrameEncoder::new(Vec::new());
            encoder.write_all(data)?;
            encoder.finish().map_err(|err| {
                FileFusionError::new(
                    ErrorCode::IntInvariant,
                    format!("lz4 frame finalization failed: {err}"),
                )
            })
        }
        StreamCodec::Brotli => {
            let quality = u32::from(cfg.effort_level.clamp(1, 11));
            let mut out = Vec::new();
            {
                let mut writer = brotli::CompressorWriter::new(&mut out, 4096, quality, 22);
                writer.write_all(data)?;
                writer.flush()?;
            }
            Ok(out)
        }
        StreamCodec::Gzip => {
            let level = Compression::new(u32::from(cfg.effort_level.clamp(1, 9)));
            let mut encoder = GzEncoder::new(Vec::new(), level);
            encoder.write_all(data)?;
            encoder.finish().map_err(FileFusionError::from)
        }
        StreamCodec::Xz => {
            let preset = u32::from(cfg.effort_level.clamp(0, 6));
            let mut writer =
                XzWriter::new(Vec::new(), XzOptions::with_preset(preset)).map_err(|err| {
                    FileFusionError::new(ErrorCode::IntInvariant, format!("xz init failed: {err}"))
                })?;
            writer.write_all(data)?;
            writer.finish().map_err(|err| {
                FileFusionError::new(
                    ErrorCode::IntInvariant,
                    format!("xz encoding failed: {err}"),
                )
            })
        }
        StreamCodec::Bzip2 => {
            let level = BzLevel::new(cfg.effort_level.clamp(1, 9));
            bz2_compress(data, level).map_err(|err| {
                FileFusionError::new(
                    ErrorCode::IntInvariant,
                    format!("bzip2 encoding failed: {err}"),
                )
            })
        }
    }
}

/// Decompresses `data` encoded with `codec` while strictly enforcing [`ResourceLimits`]
/// decompression-bomb ceilings (`FR-SEC-001`).
///
/// # Errors
/// Returns [`FileFusionError`] (`SecBomb` if ratio/byte limit exceeded, `FmtCorrupt` on invalid
/// stream syntax).
pub fn decompress_stream_bounded(
    data: &[u8],
    codec: StreamCodec,
    limits: &ResourceLimits,
    depth: u8,
) -> FileFusionResult<Vec<u8>> {
    let compressed_len = data.len() as u64;
    validate_expansion_budget(compressed_len.max(1), 0, depth, limits)?;

    match codec {
        StreamCodec::Zstd => {
            let mut decoder =
                zstd::stream::read::Decoder::new(Cursor::new(data)).map_err(|err| {
                    FileFusionError::new(
                        ErrorCode::FmtCorrupt,
                        format!("invalid zstd stream header: {err}"),
                    )
                })?;
            read_stream_bounded(&mut decoder, compressed_len, limits, depth)
        }
        StreamCodec::Lz4 => {
            let mut decoder = FrameDecoder::new(Cursor::new(data));
            read_stream_bounded(&mut decoder, compressed_len, limits, depth)
        }
        StreamCodec::Brotli => {
            let mut decoder = brotli::Decompressor::new(Cursor::new(data), 4096);
            read_stream_bounded(&mut decoder, compressed_len, limits, depth)
        }
        StreamCodec::Gzip => {
            let mut decoder = GzDecoder::new(Cursor::new(data));
            read_stream_bounded(&mut decoder, compressed_len, limits, depth)
        }
        StreamCodec::Xz => {
            let mem_limit_kb =
                u32::try_from(limits.max_memory_mb.saturating_mul(1024)).unwrap_or(u32::MAX);
            let mut decoder = XzReader::new_mem_limit(Cursor::new(data), true, mem_limit_kb);
            read_stream_bounded(&mut decoder, compressed_len, limits, depth)
        }
        StreamCodec::Bzip2 => {
            let max_out = usize::try_from(limits.max_uncompressed_bytes).unwrap_or(usize::MAX);
            let out = decompress_with_limit(Cursor::new(data), max_out).map_err(|err| {
                FileFusionError::new(
                    ErrorCode::FmtCorrupt,
                    format!("invalid or oversized bzip2 stream: {err}"),
                )
            })?;
            validate_expansion_budget(compressed_len, out.len() as u64, depth, limits)?;
            Ok(out)
        }
    }
}

fn zstd_level_for(effort_level: u8) -> i32 {
    match effort_level {
        0..=3 => 3,
        4..=6 => 7,
        7..=8 => 13,
        9 => 17,
        _ => 19,
    }
}
