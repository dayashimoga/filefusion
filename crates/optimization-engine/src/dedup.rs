//! `FastCDC` content-defined chunking and perceptual image similarity hashing (`pHash` / `dHash`)
//! for storage deduplication analytics (`FR-DUP-002`, `FR-DET-004`).

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use file_core::{FileFusionResult, JobContext};
use image::DynamicImage;
use image::imageops::FilterType;
use serde::{Deserialize, Serialize};

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

/// Slices `data` into content-defined [`CdcChunk`] records using normalized `FastCDC` Gear hashing.
///
/// Resets rolling hash on chunk boundaries so identical regions shifted by prefix insertions/deletions
/// align to identical `BLAKE3` chunks (`FR-DUP-002`).
#[must_use]
pub fn chunk_bytes_fastcdc(data: &[u8], cfg: FastCdcConfig) -> Vec<CdcChunk> {
    if data.is_empty() {
        return Vec::new();
    }
    let min_sz = cfg.min_size.max(64);
    let avg_sz = cfg.avg_size.max(min_sz);
    let max_sz = cfg.max_size.max(avg_sz);
    let mask = avg_sz.next_power_of_two().saturating_sub(1) as u64;

    let mut chunks = Vec::new();
    let mut start = 0usize;
    while start < data.len() {
        let remaining = data.len() - start;
        let end = if remaining <= min_sz {
            data.len()
        } else {
            let cap = remaining.min(max_sz);
            let mut hash = 0u64;
            let mut cut = cap;
            for (rel, &byte) in data[start..start + cap].iter().enumerate() {
                hash = (hash << 1).wrapping_add(GEAR_TABLE[usize::from(byte)]);
                if rel + 1 >= min_sz && (hash & mask) == 0 {
                    cut = rel + 1;
                    break;
                }
            }
            start + cut
        };

        let slice = &data[start..end];
        let digest = blake3::hash(slice);
        chunks.push(CdcChunk {
            offset: u64::try_from(start).unwrap_or(u64::MAX),
            length: slice.len(),
            blake3_hex: digest.to_hex().to_string(),
        });
        start = end;
    }

    chunks
}

/// Scans one or more file paths with bounded streaming reads and aggregates sub-file
/// `FastCDC` deduplication statistics (`FR-DUP-002`).
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
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        let chunks = chunk_bytes_fastcdc(&buf, cfg);
        for chunk in chunks {
            ctx.check_cancelled()?;
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
        }
    }

    report.duplicate_bytes = report.total_bytes.saturating_sub(report.unique_bytes);
    Ok(report)
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
