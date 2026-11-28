//! `FastCDC` content-defined chunking and perceptual image similarity hashing
//! (`aHash` / `dHash` / `pHash`) for storage deduplication analytics (`FR-DUP-002`, `FR-DET-004`).

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use file_core::{FileFusionResult, JobContext};
use image::DynamicImage;
use image::imageops::FilterType;
use serde::{Deserialize, Serialize};

const STREAM_READ_WINDOW_BYTES: usize = 65_536;

/// `FastCDC` chunk-size bounds (defaults: `min=4 KB`, `avg=16 KB`, `max=64 KB`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FastCdcConfig {
    /// Minimum chunk length in bytes.
    pub min_size: usize,
    /// Target average chunk length in bytes (must be a power of 2 mask alignment).
    pub avg_size: usize,
    /// Maximum chunk length in bytes.
    pub max_size: usize,
}

impl Default for FastCdcConfig {
    fn default() -> Self {
        Self {
            min_size: 4 * 1024,
            avg_size: 16 * 1024,
            max_size: 64 * 1024,
        }
    }
}

/// Content-defined chunk metadata emitted by `FastCDC` scanning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CdcChunk {
    /// Byte offset inside the scanned stream.
    pub offset: u64,
    /// Chunk byte length.
    pub length: usize,
    /// Hex-encoded `BLAKE3` digest of chunk bytes.
    pub blake3_hex: String,
}

/// Aggregate sub-file deduplication report computed via `FastCDC` (`FR-DUP-002`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CdcDeduplicationReport {
    /// Total logical bytes scanned across all inputs.
    pub total_bytes: u64,
    /// Sum of unique chunk byte lengths (`BLAKE3`-deduplicated).
    pub unique_bytes: u64,
    /// Recoverable duplicate sub-file byte savings (`total_bytes - unique_bytes`).
    pub duplicate_bytes: u64,
    /// Total chunks emitted.
    pub chunk_count: usize,
    /// Distinct content chunks encountered.
    pub unique_chunk_count: usize,
}

/// Combined 64-bit perceptual image fingerprint bundle (`aHash`, `dHash`, `pHash`) (`FR-DET-004`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerceptualHashSet {
    /// 64-bit average luminance hash.
    pub ahash: u64,
    /// 64-bit horizontal gradient difference hash.
    pub dhash: u64,
    /// 64-bit 2D DCT-II perceptual hash.
    pub phash: u64,
}

/// Deterministic 256-entry 64-bit Gear hash lookup table generated cleanly at compile time.
const GEAR_TABLE: [u64; 256] = {
    let mut table = [0u64; 256];
    let mut i = 0usize;
    let mut state = 0x5A17_9C2D_84B3_E6F1u64;
    while i < 256 {
        // SplitMix64 avalanche guarantees distinct full-width bit patterns without RNG deps.
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        table[i] = z ^ (z >> 31);
        i += 1;
    }
    table
};

fn normalize_cdc_params(cfg: FastCdcConfig) -> (usize, usize, u64) {
    let min_sz = cfg.min_size.max(64);
    let avg_sz = cfg.avg_size.max(min_sz);
    let max_sz = cfg.max_size.max(avg_sz);
    let mask = avg_sz.next_power_of_two().saturating_sub(1) as u64;
    (min_sz, max_sz, mask)
}

fn find_cdc_cut(slice: &[u8], min_sz: usize, max_sz: usize, mask: u64) -> usize {
    if slice.len() <= min_sz {
        return slice.len();
    }
    let cap = slice.len().min(max_sz);
    let mut hash = 0u64;
    for (rel, &byte) in slice[..cap].iter().enumerate() {
        hash = (hash << 1).wrapping_add(GEAR_TABLE[usize::from(byte)]);
        if rel + 1 >= min_sz && (hash & mask) == 0 {
            return rel + 1;
        }
    }
    cap
}

/// Slices `data` into content-defined [`CdcChunk`] records using normalized `FastCDC` Gear hashing.
///
/// Resets rolling hash on chunk boundaries so identical regions shifted by prefix insertions/deletions
/// align to identical `BLAKE3` chunks (`FR-DUP-002`).
#[must_use]
pub fn chunk_bytes_fastcdc(data: &[u8], cfg: FastCdcConfig) -> Vec<CdcChunk> {
    if data.is_empty() {
        return Vec::new();
    }
    let (min_sz, max_sz, mask) = normalize_cdc_params(cfg);
    let mut chunks = Vec::new();
    let mut start = 0usize;

    while start < data.len() {
        let cut = find_cdc_cut(&data[start..], min_sz, max_sz, mask);
        let slice = &data[start..start + cut];
        let digest = blake3::hash(slice);
        chunks.push(CdcChunk {
            offset: u64::try_from(start).unwrap_or(u64::MAX),
            length: slice.len(),
            blake3_hex: digest.to_hex().to_string(),
        });
        start += cut;
    }

    chunks
}

/// Streams any `Read` source through a bounded sliding window emitting exact `FastCDC` chunks
/// equivalent to [`chunk_bytes_fastcdc`] without loading the whole stream into memory (`NFR-PERF-001`).
///
/// # Errors
/// Returns [`file_core::FileFusionError`] on I/O failure, callback error, or cancellation.
pub fn chunk_stream_fastcdc<R, F>(
    reader: &mut R,
    cfg: FastCdcConfig,
    ctx: &JobContext,
    mut on_chunk: F,
) -> FileFusionResult<()>
where
    R: Read,
    F: FnMut(CdcChunk) -> FileFusionResult<()>,
{
    let (min_sz, max_sz, mask) = normalize_cdc_params(cfg);
    let target_buf_len = max_sz.saturating_mul(2).max(STREAM_READ_WINDOW_BYTES);
    let mut buf = Vec::with_capacity(target_buf_len);
    let mut scratch = vec![0u8; STREAM_READ_WINDOW_BYTES];
    let mut eof = false;
    let mut offset = 0u64;

    loop {
        while !eof && buf.len() < max_sz {
            ctx.check_cancelled()?;
            let to_read = (target_buf_len - buf.len()).min(scratch.len()).max(1);
            let n = reader.read(&mut scratch[..to_read])?;
            if n == 0 {
                eof = true;
            } else {
                buf.extend_from_slice(&scratch[..n]);
            }
        }
        if buf.is_empty() {
            break;
        }
        ctx.check_cancelled()?;

        let cut = find_cdc_cut(&buf, min_sz, max_sz, mask);
        let digest = blake3::hash(&buf[..cut]);
        on_chunk(CdcChunk {
            offset,
            length: cut,
            blake3_hex: digest.to_hex().to_string(),
        })?;
        buf.drain(..cut);
        offset = offset.saturating_add(u64::try_from(cut).unwrap_or(0));
    }

    Ok(())
}

/// Scans one or more file paths with bounded streaming reads and aggregates sub-file
/// `FastCDC` deduplication statistics (`FR-DUP-002`, `NFR-PERF-001`).
///
/// # Errors
/// Returns [`file_core::FileFusionError`] on I/O failures or job cancellation.
pub fn analyze_files_fastcdc(
    paths: &[&Path],
    cfg: FastCdcConfig,
    ctx: &JobContext,
) -> FileFusionResult<CdcDeduplicationReport> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut report = CdcDeduplicationReport::default();

    for path in paths {
        ctx.check_cancelled()?;
        let mut file = File::open(path)?;
        chunk_stream_fastcdc(&mut file, cfg, ctx, |chunk| {
            report.total_bytes = report
                .total_bytes
                .saturating_add(u64::try_from(chunk.length).unwrap_or(0));
            report.chunk_count = report.chunk_count.saturating_add(1);
            if let std::collections::hash_map::Entry::Vacant(entry) = seen.entry(chunk.blake3_hex) {
                report.unique_bytes = report
                    .unique_bytes
                    .saturating_add(u64::try_from(chunk.length).unwrap_or(0));
                report.unique_chunk_count = report.unique_chunk_count.saturating_add(1);
                entry.insert(chunk.length);
            }
            Ok(())
        })?;
    }

    report.duplicate_bytes = report.total_bytes.saturating_sub(report.unique_bytes);
    Ok(report)
}

/// Computes a 64-bit average luminance hash (`aHash`) over an `8x8` grayscale thumbnail
/// (`FR-DET-004`, `FR-DUP-002`).
#[must_use]
pub fn compute_image_ahash(img: &DynamicImage) -> u64 {
    let small = img
        .grayscale()
        .resize_exact(8, 8, FilterType::Triangle)
        .to_luma8();
    let sum: u64 = small.pixels().map(|p| u64::from(p[0])).sum();
    let mean = sum / 64;
    let mut hash = 0u64;
    for (bit, pixel) in small.pixels().enumerate() {
        if bit < 64 && u64::from(pixel[0]) >= mean {
            hash |= 1u64 << bit;
        }
    }
    hash
}

/// Computes a 64-bit horizontal difference hash (`dHash`) resilient to resizing and mild
/// recompression (`FR-DET-004`, `FR-DUP-002`).
#[must_use]
pub fn compute_image_dhash(img: &DynamicImage) -> u64 {
    let small = img
        .grayscale()
        .resize_exact(9, 8, FilterType::Triangle)
        .to_luma8();
    let mut hash = 0u64;
    let mut bit = 0u32;
    for y in 0..8 {
        for x in 0..8 {
            let left = small.get_pixel(x, y)[0];
            let right = small.get_pixel(x + 1, y)[0];
            if left > right {
                hash |= 1u64 << bit;
            }
            bit += 1;
        }
    }
    hash
}

/// Computes a 64-bit 2D Type-II DCT perceptual hash (`pHash`) over a `32x32` grayscale grid,
/// comparing the upper-left `8x8` low-frequency coefficients against their median (`FR-DET-004`).
#[must_use]
pub fn compute_image_phash(img: &DynamicImage) -> u64 {
    const N: usize = 32;
    let gray = img
        .grayscale()
        .resize_exact(32, 32, FilterType::Lanczos3)
        .to_luma8();

    let mut pixels = [[0.0f64; N]; N];
    for (y, row) in pixels.iter_mut().enumerate() {
        for (x, cell) in row.iter_mut().enumerate() {
            let px = u32::try_from(x).unwrap_or(0);
            let py = u32::try_from(y).unwrap_or(0);
            *cell = f64::from(gray.get_pixel(px, py)[0]);
        }
    }

    // Compute separable top-left 8x8 low-frequency DCT-II coefficients cleanly without heap allocs.
    let mut low_freq = [0.0f64; 64];
    let pi = std::f64::consts::PI;
    for u in 0..8usize {
        for v in 0..8usize {
            let mut sum = 0.0f64;
            for (x, row) in pixels.iter().enumerate() {
                #[allow(clippy::cast_precision_loss)]
                let cx = ((pi * (2.0 * (x as f64) + 1.0) * (u as f64)) / 64.0).cos();
                for (y, &val) in row.iter().enumerate() {
                    #[allow(clippy::cast_precision_loss)]
                    let cy = ((pi * (2.0 * (y as f64) + 1.0) * (v as f64)) / 64.0).cos();
                    sum += val * cx * cy;
                }
            }
            low_freq[u * 8 + v] = sum;
        }
    }

    // Exclude DC component `[0]` from median threshold to preserve luminance invariance.
    let mut ac_sorted: Vec<f64> = low_freq[1..].to_vec();
    ac_sorted.sort_by(f64::total_cmp);
    let median = ac_sorted[ac_sorted.len() / 2];

    let mut hash = 0u64;
    for (bit, &coeff) in low_freq.iter().enumerate() {
        if bit > 0 && coeff > median {
            hash |= 1u64 << bit;
        }
    }
    hash
}

/// Computes `aHash`, `dHash`, and `pHash` together for a decoded raster image (`FR-DET-004`).
#[must_use]
pub fn compute_perceptual_hash_set(img: &DynamicImage) -> PerceptualHashSet {
    PerceptualHashSet {
        ahash: compute_image_ahash(img),
        dhash: compute_image_dhash(img),
        phash: compute_image_phash(img),
    }
}

/// Returns the Hamming bit distance (`0..=64`) between two perceptual hashes.
#[must_use]
pub const fn hamming_distance_u64(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

/// Returns `true` when two perceptual hashes fall within `max_distance` bits (e.g., `<= 10`).
#[must_use]
pub const fn are_perceptually_similar(a: u64, b: u64, max_distance: u32) -> bool {
    hamming_distance_u64(a, b) <= max_distance
}
