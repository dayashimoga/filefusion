//! Permanent Stage 2 Criterion benchmarks (`FF-PERF-001`, `FF-ARCH-001` §7).
//!
//! Measures pure-Rust Stage 2 compression & intelligence hot paths on the host CPU:
//! - `FastCDC` Gear-hash chunking throughput
//! - Zstandard stream compression throughput (`StreamCodec::Zstd`)
//! - Lossless PNG (`oxipng`) optimization via `compression-image::optimize_raster_in_memory`
//! - Quality-constrained JPEG binary solver (`solve_jpeg_quality_target`)

#![allow(missing_docs)]

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use file_core::{CompressionProfile, FormatId, JobContext, ResourceLimits};
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ColorType, ImageEncoder};
use optimization_engine::{
    FastCdcConfig, QualityTargetConstraint, StreamCodec, chunk_bytes_fastcdc,
    compress_stream_bytes, solve_jpeg_quality_target,
};

fn deterministic_payload(len: usize) -> Vec<u8> {
    let mut out = vec![0u8; len];
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    for (idx, b) in out.iter_mut().enumerate() {
        if idx % 3 == 0 {
            state = state.wrapping_add(0xBF58_476D_1CE4_E5B9);
            *b = u8::try_from((state >> 16) & 0x7F).unwrap_or(0);
        } else {
            *b = u8::try_from(idx % 64).unwrap_or(0);
        }
    }
    out
}

fn sample_unoptimized_png(dim: u32) -> Vec<u8> {
    let side = usize::try_from(dim).unwrap_or(64);
    let mut rgba = Vec::with_capacity(side * side * 4);
    for y in 0..side {
        for x in 0..side {
            let r = u8::try_from((x * 3) % 255).unwrap_or(0);
            let g = u8::try_from((y * 5) % 255).unwrap_or(0);
            rgba.extend_from_slice(&[r, g, 160, 255]);
        }
    }
    let mut out = Vec::new();
    let _ = PngEncoder::new_with_quality(&mut out, CompressionType::Fast, FilterType::NoFilter)
        .write_image(&rgba, dim, dim, ColorType::Rgba8.into());
    out
}

fn bench_stage2_compression(c: &mut Criterion) {
    let payload_64k = deterministic_payload(64 * 1024);
    let mut cdc_group = c.benchmark_group("fastcdc_64kb");
    cdc_group.throughput(Throughput::Bytes(64 * 1024));
    cdc_group.bench_function(BenchmarkId::from_parameter("gear_hash_16kb"), |b| {
        b.iter(|| chunk_bytes_fastcdc(black_box(&payload_64k), FastCdcConfig::default()));
    });
    cdc_group.finish();

    let mut zstd_group = c.benchmark_group("zstd_stream_64kb");
    zstd_group.throughput(Throughput::Bytes(64 * 1024));
    zstd_group.bench_function(BenchmarkId::from_parameter("balanced_optimal"), |b| {
        b.iter(|| {
            compress_stream_bytes(
                black_box(&payload_64k),
                StreamCodec::Zstd,
                CompressionProfile::BalancedOptimal,
            )
        });
    });
    zstd_group.finish();

    let png_96 = sample_unoptimized_png(96);
    let mut img_group = c.benchmark_group("image_png_oxipng_96x96");
    img_group.throughput(Throughput::Bytes(u64::try_from(png_96.len()).unwrap_or(1)));
    img_group.bench_function(BenchmarkId::from_parameter("balanced"), |b| {
        b.iter(|| {
            compression_image::optimize_raster_in_memory(
                black_box(&png_96),
                FormatId::Png,
                FormatId::Png,
                CompressionProfile::BalancedOptimal,
                true,
                None,
            )
        });
    });
    img_group.finish();

    let ctx = JobContext::new("bench-solver", ResourceLimits::default());
    let constraint = QualityTargetConstraint {
        min_ssim: Some(0.96),
        min_psnr_db: None,
        target_max_bytes: None,
        min_param: 40,
        max_param: 95,
        max_iterations: 6,
    };
    let mut solver_group = c.benchmark_group("jpeg_quality_solver_96x96");
    solver_group.bench_function(BenchmarkId::from_parameter("ssim_0_96"), |b| {
        b.iter(|| solve_jpeg_quality_target(black_box(&png_96), &constraint, &ctx));
    });
    solver_group.finish();
}

criterion_group!(benches, bench_stage2_compression);
criterion_main!(benches);
