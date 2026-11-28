//! Pure-Rust audio decoding, FLAC/WAV lossless encoding, sample-rate conversion,
//! channel mapping, and ITU-R BS.1770 / EBU R128 loudness normalization (`FR-AV-002`).

use std::f64::consts::PI;
use std::fs::{self, File};
use std::path::Path;

use claxon::FlacReader;
use file_core::{CompressionProfile, ErrorCode, FileFusionError, FileFusionResult, FormatId};
use flacenc::bitsink::ByteSink;
use flacenc::component::BitRepr;
use flacenc::error::Verify;
use hound::{SampleFormat as HoundSampleFormat, WavReader, WavSpec, WavWriter};
use serde::{Deserialize, Serialize};
use symphonia::core::codecs::CodecParameters;
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;

/// Canonical interleaved integer PCM buffer used across pure-Rust audio pipelines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PcmAudioBuffer {
    /// Sample rate in Hz (e.g., `44_100` or `48_000`).
    pub sample_rate: u32,
    /// Interleaved channel count (`1` = mono, `2` = stereo, etc.).
    pub channels: u16,
    /// Valid signed sample bit depth (`16` or `24`).
    pub bits_per_sample: u16,
    /// Interleaved signed PCM samples scaled to `[-2^(bits-1), 2^(bits-1) - 1]`.
    pub samples_i32: Vec<i32>,
}

impl PcmAudioBuffer {
    /// Total interleaved sample frames (`samples_i32.len() / channels`).
    #[must_use]
    pub fn frame_count(&self) -> usize {
        let ch = usize::from(self.channels).max(1);
        self.samples_i32.len() / ch
    }

    /// Full-scale positive integer amplitude for `self.bits_per_sample`.
    #[must_use]
    pub fn full_scale(&self) -> f64 {
        let shift = u32::from(self.bits_per_sample.clamp(8, 24) - 1);
        f64::from(1i32 << shift)
    }
}

/// Measured ITU-R BS.1770 / EBU R128 loudness and sample-peak summary.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LoudnessMeasurement {
    /// Integrated gated loudness in LUFS (`-70.0..=0.0`, or `-120.0` for digital silence).
    pub integrated_lufs: f64,
    /// Maximum absolute sample peak in dBFS (`<= 0.0`).
    pub sample_peak_dbfs: f64,
}

/// Decodes a WAV file via `hound` into an interleaved [`PcmAudioBuffer`].
///
/// # Errors
/// Returns [`FileFusionError`] on corrupt headers, unsupported sample format, or I/O error.
pub fn decode_wav_path(path: &Path) -> FileFusionResult<PcmAudioBuffer> {
    let mut reader = WavReader::open(path).map_err(|err| {
        FileFusionError::new(ErrorCode::FmtCorrupt, format!("invalid WAV header: {err}"))
    })?;
    let spec = reader.spec();
    let bits = spec.bits_per_sample.clamp(8, 24);
    let mut samples_i32 = Vec::new();

    match spec.sample_format {
        HoundSampleFormat::Int => {
            for sample in reader.samples::<i32>() {
                let val = sample.map_err(|err| {
                    FileFusionError::new(ErrorCode::FmtCorrupt, format!("corrupt WAV PCM: {err}"))
                })?;
                samples_i32.push(val);
            }
        }
        HoundSampleFormat::Float => {
            let scale = f64::from(1i32 << 23) - 1.0;
            for sample in reader.samples::<f32>() {
                let val = sample.map_err(|err| {
                    FileFusionError::new(ErrorCode::FmtCorrupt, format!("corrupt WAV float: {err}"))
                })?;
                let clamped = f64::from(val).clamp(-1.0, 1.0);
                samples_i32.push(round_f64_to_i32(clamped * scale));
            }
            return Ok(PcmAudioBuffer {
                sample_rate: spec.sample_rate,
                channels: spec.channels,
                bits_per_sample: 24,
                samples_i32,
            });
        }
    }

    Ok(PcmAudioBuffer {
        sample_rate: spec.sample_rate,
        channels: spec.channels,
        bits_per_sample: bits,
        samples_i32,
    })
}

/// Writes an interleaved [`PcmAudioBuffer`] as lossless PCM WAV via `hound`.
///
/// # Errors
/// Returns [`FileFusionError`] on invalid sample parameters or I/O failure.
pub fn encode_wav_path(buf: &PcmAudioBuffer, dest: &Path) -> FileFusionResult<()> {
    let bits = if buf.bits_per_sample <= 16 { 16 } else { 24 };
    let spec = WavSpec {
        channels: buf.channels,
        sample_rate: buf.sample_rate,
        bits_per_sample: bits,
        sample_format: HoundSampleFormat::Int,
    };
    let mut writer = WavWriter::create(dest, spec).map_err(|err| {
        FileFusionError::new(
            ErrorCode::IntInvariant,
            format!("failed creating WAV writer: {err}"),
        )
    })?;
    let max_pos = (1i32 << (u32::from(bits) - 1)) - 1;
    let min_neg = -(1i32 << (u32::from(bits) - 1));
    for &sample in &buf.samples_i32 {
        writer
            .write_sample(sample.clamp(min_neg, max_pos))
            .map_err(|err| {
                FileFusionError::new(
                    ErrorCode::IntInvariant,
                    format!("WAV sample write error: {err}"),
                )
            })?;
    }
    writer.finalize().map_err(|err| {
        FileFusionError::new(
            ErrorCode::IntInvariant,
            format!("WAV finalize failed: {err}"),
        )
    })?;
    Ok(())
}

/// Decodes a FLAC file losslessly via `claxon` preserving exact integer sample bit depth.
///
/// # Errors
/// Returns [`FileFusionError`] if the FLAC stream is corrupt or unreadable.
pub fn decode_flac_path(path: &Path) -> FileFusionResult<PcmAudioBuffer> {
    let mut reader = FlacReader::open(path).map_err(|err| {
        FileFusionError::new(ErrorCode::FmtCorrupt, format!("invalid FLAC header: {err}"))
    })?;
    let info = reader.streaminfo();
    let channels = u16::try_from(info.channels).unwrap_or(2);
    let bits_per_sample = u16::try_from(info.bits_per_sample)
        .unwrap_or(16)
        .clamp(8, 24);
    let mut samples_i32 = Vec::new();
    for sample in reader.samples() {
        let val = sample.map_err(|err| {
            FileFusionError::new(
                ErrorCode::FmtCorrupt,
                format!("corrupt FLAC sample frame: {err}"),
            )
        })?;
        samples_i32.push(val);
    }
    Ok(PcmAudioBuffer {
        sample_rate: info.sample_rate,
        channels,
        bits_per_sample,
        samples_i32,
    })
}

/// Encodes an interleaved [`PcmAudioBuffer`] as a lossless FLAC stream via `flacenc`,
/// tuning block size per [`CompressionProfile`].
///
/// # Errors
/// Returns [`FileFusionError`] on invalid channel/bit configurations or I/O errors.
pub fn encode_flac_path(
    buf: &PcmAudioBuffer,
    dest: &Path,
    profile: CompressionProfile,
) -> FileFusionResult<()> {
    let mut cfg = flacenc::config::Encoder::default();
    cfg.block_size = match profile {
        CompressionProfile::LosslessMaximum | CompressionProfile::ArchivalColdStorage => 4096,
        CompressionProfile::BalancedOptimal | CompressionProfile::WebSharingReady => 2048,
        CompressionProfile::ExtremeSizeTarget => 4608,
    };
    let verified = cfg.into_verified().map_err(|(_, err)| {
        FileFusionError::new(
            ErrorCode::IntInvariant,
            format!("invalid flacenc config: {err:?}"),
        )
    })?;
    let bits = usize::from(buf.bits_per_sample.clamp(8, 24));
    let channels = usize::from(buf.channels.clamp(1, 8));
    let source = flacenc::source::MemSource::from_samples(
        &buf.samples_i32,
        channels,
        bits,
        buf.sample_rate as usize,
    );
    let stream = flacenc::encode_with_fixed_block_size(&verified, source, verified.block_size)
        .map_err(|err| {
            FileFusionError::new(
                ErrorCode::IntInvariant,
                format!("FLAC encoding failed: {err:?}"),
            )
        })?;
    let mut sink = ByteSink::new();
    stream.write(&mut sink).map_err(|err| {
        FileFusionError::new(
            ErrorCode::IntInvariant,
            format!("FLAC serialization failed: {err:?}"),
        )
    })?;
    fs::write(dest, sink.as_slice())?;
    Ok(())
}

/// Decodes any supported audio file (`WAV`, `FLAC`, `MP3`, `Ogg/Vorbis`, `AAC`/`M4A`, `ALAC`, `MKV`)
/// into a canonical [`PcmAudioBuffer`] using `hound` / `claxon` or `symphonia`.
///
/// # Errors
/// Returns [`FileFusionError`] on unsupported codecs, corrupt headers, or I/O failure.
pub fn decode_any_audio_to_pcm(path: &Path, format: FormatId) -> FileFusionResult<PcmAudioBuffer> {
    match format {
        FormatId::Wav => decode_wav_path(path),
        FormatId::Flac => decode_flac_path(path),
        _ => decode_via_symphonia(path),
    }
}

/// Decodes an audio stream via `symphonia` (`WAV`, `FLAC`, `MP3`, `Ogg/Vorbis`, `AAC`/`M4A`,
/// `ALAC`, `MKV`) into a 16-bit normalized [`PcmAudioBuffer`].
///
/// # Errors
/// Returns [`FileFusionError`] on unsupported container/codec, corrupt audio packets, or I/O error.
pub fn decode_via_symphonia(path: &Path) -> FileFusionResult<PcmAudioBuffer> {
    let file = File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let mut format_reader = symphonia::default::get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|err| {
            FileFusionError::new(
                ErrorCode::FmtUnsupported,
                format!("unable to probe audio stream via symphonia: {err}"),
            )
        })?;

    let track = format_reader
        .default_track(TrackType::Audio)
        .ok_or_else(|| {
            FileFusionError::new(ErrorCode::FmtCorrupt, "no audio track present in container")
        })?;
    let track_id = track.id;
    let Some(CodecParameters::Audio(audio_params)) = track.codec_params.as_ref() else {
        return Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            "missing audio codec parameters",
        ));
    };

    let sample_rate = audio_params.sample_rate.unwrap_or(44_100);
    let channels = audio_params
        .channels
        .as_ref()
        .map_or(2, |c| u16::try_from(c.count()).unwrap_or(2));
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(audio_params, &AudioDecoderOptions::default())
        .map_err(|err| {
            FileFusionError::new(
                ErrorCode::FmtUnsupported,
                format!("symphonia audio decoder unavailable: {err}"),
            )
        })?;

    let mut float_samples: Vec<f32> = Vec::new();
    let mut chunk: Vec<f32> = Vec::new();
    loop {
        let packet_opt = format_reader.next_packet().map_err(|err| {
            FileFusionError::new(
                ErrorCode::FmtCorrupt,
                format!("corrupt container packet: {err}"),
            )
        })?;
        let Some(packet) = packet_opt else {
            break;
        };
        if packet.track_id != track_id {
            continue;
        }
        let audio_buf = decoder.decode(&packet).map_err(|err| {
            FileFusionError::new(
                ErrorCode::FmtCorrupt,
                format!("corrupt audio frame packet: {err}"),
            )
        })?;
        chunk.clear();
        audio_buf.copy_to_vec_interleaved(&mut chunk);
        float_samples.extend_from_slice(&chunk);
    }

    let scale = 32_768.0_f64;
    let samples_i32 = float_samples
        .into_iter()
        .map(|s| {
            round_f64_to_i32((f64::from(s) * scale).clamp(f64::from(i16::MIN), f64::from(i16::MAX)))
        })
        .collect();

    Ok(PcmAudioBuffer {
        sample_rate,
        channels,
        bits_per_sample: 16,
        samples_i32,
    })
}

/// Resamples an interleaved [`PcmAudioBuffer`] to `target_rate` using linear phase interpolation.
///
/// # Errors
/// Returns [`FileFusionError`] if `target_rate == 0` or input buffer has zero channels/rate.
pub fn resample_pcm(buf: &PcmAudioBuffer, target_rate: u32) -> FileFusionResult<PcmAudioBuffer> {
    if target_rate == 0 || buf.sample_rate == 0 || buf.channels == 0 {
        return Err(FileFusionError::new(
            ErrorCode::FmtUnsupported,
            "sample rate and channel count must be positive",
        ));
    }
    if target_rate == buf.sample_rate || buf.samples_i32.is_empty() {
        return Ok(buf.clone());
    }

    let ch = usize::from(buf.channels);
    let src_frames = buf.frame_count();
    let dst_frames_f =
        usize_to_f64(src_frames) * f64::from(target_rate) / f64::from(buf.sample_rate);
    let dst_frames = usize::try_from(round_f64_to_i32(dst_frames_f.ceil()).max(1)).unwrap_or(1);
    let step = f64::from(buf.sample_rate) / f64::from(target_rate);

    let mut out = Vec::with_capacity(dst_frames * ch);
    for out_idx in 0..dst_frames {
        let src_pos = usize_to_f64(out_idx) * step;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let i0 = (src_pos.floor() as usize).min(src_frames.saturating_sub(1));
        let i1 = (i0 + 1).min(src_frames.saturating_sub(1));
        let frac = src_pos - usize_to_f64(i0);

        for c in 0..ch {
            let s0 = f64::from(buf.samples_i32[i0 * ch + c]);
            let s1 = f64::from(buf.samples_i32[i1 * ch + c]);
            let interp = s0 + (s1 - s0) * frac;
            out.push(round_f64_to_i32(interp));
        }
    }

    Ok(PcmAudioBuffer {
        sample_rate: target_rate,
        channels: buf.channels,
        bits_per_sample: buf.bits_per_sample,
        samples_i32: out,
    })
}

/// Remaps channel layout (mono <-> stereo or multi-channel downmix to mono/stereo).
///
/// # Errors
/// Returns [`FileFusionError`] if `target_channels` is `0` or unsupported (`> 2` downmix/upmix).
pub fn remap_channels(
    buf: &PcmAudioBuffer,
    target_channels: u16,
) -> FileFusionResult<PcmAudioBuffer> {
    if target_channels == 0 || buf.channels == 0 {
        return Err(FileFusionError::new(
            ErrorCode::FmtUnsupported,
            "channel count cannot be zero",
        ));
    }
    if target_channels == buf.channels {
        return Ok(buf.clone());
    }

    let src_ch = usize::from(buf.channels);
    let frames = buf.frame_count();
    let mut out = Vec::with_capacity(frames * usize::from(target_channels));

    match target_channels {
        1 => {
            for f in 0..frames {
                let mut acc: i64 = 0;
                for c in 0..src_ch {
                    acc += i64::from(buf.samples_i32[f * src_ch + c]);
                }
                let mean = i32::try_from(acc / i64::try_from(src_ch).unwrap_or(1)).unwrap_or(0);
                out.push(mean);
            }
        }
        2 if buf.channels == 1 => {
            for &s in &buf.samples_i32 {
                out.push(s);
                out.push(s);
            }
        }
        2 => {
            for f in 0..frames {
                out.push(buf.samples_i32[f * src_ch]);
                out.push(buf.samples_i32[f * src_ch + 1]);
            }
        }
        other => {
            return Err(FileFusionError::new(
                ErrorCode::FmtUnsupported,
                format!("unsupported channel target count {other}"),
            ));
        }
    }

    Ok(PcmAudioBuffer {
        sample_rate: buf.sample_rate,
        channels: target_channels,
        bits_per_sample: buf.bits_per_sample,
        samples_i32: out,
    })
}

/// Measures ITU-R BS.1770 / EBU R128 integrated loudness (LUFS) with K-weighting filters and
/// `-70 LUFS` / `-10 LU` gating (`FR-AV-002`).
#[must_use]
pub fn measure_integrated_lufs(buf: &PcmAudioBuffer) -> LoudnessMeasurement {
    let ch = usize::from(buf.channels).max(1);
    let frames = buf.frame_count();
    if frames == 0 || buf.sample_rate == 0 {
        return LoudnessMeasurement {
            integrated_lufs: -120.0,
            sample_peak_dbfs: -120.0,
        };
    }

    let full_scale = buf.full_scale();
    let mut peak_abs = 0.0_f64;
    for &sample in &buf.samples_i32 {
        let norm = (f64::from(sample) / full_scale).abs();
        if norm > peak_abs {
            peak_abs = norm;
        }
    }
    let sample_peak_dbfs = if peak_abs > 1e-12 {
        20.0 * peak_abs.log10()
    } else {
        -120.0
    };

    let (shelf, rlb) = k_weight_biquads(f64::from(buf.sample_rate));
    let mut ch_filtered = vec![Vec::with_capacity(frames); ch];

    for (c, chan_buf) in ch_filtered.iter_mut().enumerate().take(ch) {
        let mut s1 = BiquadState::default();
        let mut s2 = BiquadState::default();
        for f in 0..frames {
            let x = f64::from(buf.samples_i32[f * ch + c]) / full_scale;
            let y1 = s1.process(x, shelf);
            let y2 = s2.process(y1, rlb);
            chan_buf.push(y2);
        }
    }

    let block_frames = (usize::try_from(buf.sample_rate).unwrap_or(48_000) * 400 / 1000).max(1);
    let step_frames = (block_frames / 4).max(1);
    let mut block_powers = Vec::new();

    if frames <= block_frames {
        let power = block_weighted_power(&ch_filtered, 0, frames);
        if power > 0.0 {
            block_powers.push(power);
        }
    } else {
        let mut start = 0usize;
        while start + block_frames <= frames {
            let power = block_weighted_power(&ch_filtered, start, start + block_frames);
            if power > 0.0 {
                block_powers.push(power);
            }
            start += step_frames;
        }
    }

    // Absolute gate: >= -70.0 LUFS.
    let abs_gated: Vec<f64> = block_powers
        .into_iter()
        .filter(|&p| -0.691 + 10.0 * p.max(1e-15).log10() >= -70.0)
        .collect();
    if abs_gated.is_empty() {
        return LoudnessMeasurement {
            integrated_lufs: -120.0,
            sample_peak_dbfs,
        };
    }

    let ungated_mean = abs_gated.iter().sum::<f64>() / usize_to_f64(abs_gated.len());
    let rel_threshold = (-0.691 + 10.0 * ungated_mean.max(1e-15).log10()) - 10.0;
    let rel_gated: Vec<f64> = abs_gated
        .into_iter()
        .filter(|&p| -0.691 + 10.0 * p.max(1e-15).log10() >= rel_threshold)
        .collect();
    let final_mean = if rel_gated.is_empty() {
        ungated_mean
    } else {
        rel_gated.iter().sum::<f64>() / usize_to_f64(rel_gated.len())
    };
    let integrated_lufs = -0.691 + 10.0 * final_mean.max(1e-15).log10();

    LoudnessMeasurement {
        integrated_lufs,
        sample_peak_dbfs,
    }
}

/// Applies ITU-R BS.1770 / EBU R128 loudness normalization to `buf` toward `target_lufs`
/// (e.g., `-16.0 LUFS`), clamped by `true_peak_ceiling_dbfs` (e.g., `-1.0 dBFS`) to prevent
/// digital clipping (`FR-AV-002`).
///
/// # Errors
/// Returns [`FileFusionError`] if `target_lufs` or peak ceiling are invalid non-finite values.
pub fn normalize_loudness_lufs(
    buf: &mut PcmAudioBuffer,
    target_lufs: f64,
    true_peak_ceiling_dbfs: f64,
) -> FileFusionResult<LoudnessMeasurement> {
    if !target_lufs.is_finite() || !true_peak_ceiling_dbfs.is_finite() {
        return Err(FileFusionError::new(
            ErrorCode::FmtUnsupported,
            "loudness normalization parameters must be finite",
        ));
    }
    let measured = measure_integrated_lufs(buf);
    if measured.integrated_lufs <= -100.0 {
        return Ok(measured);
    }

    let desired_gain_db = target_lufs - measured.integrated_lufs;
    let max_headroom_db = true_peak_ceiling_dbfs - measured.sample_peak_dbfs;
    let applied_gain_db = desired_gain_db.min(max_headroom_db);
    let linear_gain = 10.0_f64.powf(applied_gain_db / 20.0);

    let full_scale = buf.full_scale();
    let max_pos = (1i32 << (u32::from(buf.bits_per_sample.clamp(8, 24)) - 1)) - 1;
    let min_neg = -(1i32 << (u32::from(buf.bits_per_sample.clamp(8, 24)) - 1));
    let ceiling_linear = 10.0_f64.powf(true_peak_ceiling_dbfs.min(0.0) / 20.0) * full_scale;

    for sample in &mut buf.samples_i32 {
        let scaled = (f64::from(*sample) * linear_gain).clamp(-ceiling_linear, ceiling_linear);
        *sample = round_f64_to_i32(scaled).clamp(min_neg, max_pos);
    }

    Ok(measure_integrated_lufs(buf))
}

/// Evaluates bit-exact equality and time-domain PCM waveform PSNR (dB) between two decoded buffers.
#[must_use]
pub fn evaluate_pcm_equivalence(orig: &PcmAudioBuffer, cand: &PcmAudioBuffer) -> (bool, f64) {
    if orig.sample_rate != cand.sample_rate
        || orig.channels != cand.channels
        || orig.samples_i32.len() != cand.samples_i32.len()
    {
        return (false, 0.0);
    }
    if orig.samples_i32.is_empty() {
        return (true, 99.9);
    }

    let mut sq_err_sum = 0.0_f64;
    let full_scale = orig.full_scale();
    let cand_scale = cand.full_scale();
    let mut exact = orig.bits_per_sample == cand.bits_per_sample;

    for (&a, &b) in orig.samples_i32.iter().zip(cand.samples_i32.iter()) {
        if a != b {
            exact = false;
        }
        let diff = (f64::from(a) / full_scale) - (f64::from(b) / cand_scale);
        sq_err_sum += diff * diff;
    }

    if sq_err_sum == 0.0 {
        return (true, 99.9);
    }
    let mse = sq_err_sum / usize_to_f64(orig.samples_i32.len());
    let psnr_db = (10.0 * (1.0 / mse).log10()).min(99.9);
    (exact, psnr_db)
}

#[derive(Clone, Copy)]
struct BiquadCoeffs {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
}

#[derive(Default)]
struct BiquadState {
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl BiquadState {
    fn process(&mut self, x: f64, c: BiquadCoeffs) -> f64 {
        let y = c.b0 * x + c.b1 * self.x1 + c.b2 * self.x2 - c.a1 * self.y1 - c.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

fn k_weight_biquads(sample_rate: f64) -> (BiquadCoeffs, BiquadCoeffs) {
    // ITU-R BS.1770-4 analytical bilinear-transformed K-weighting stages.
    // Stage 1: high-shelf filter (+4 dB @ ~1681.97 Hz, Q = 0.7071752369554196).
    let db = 3.999_843_853_973_347;
    let f0 = 1_681.974_450_955_533;
    let q = 0.707_175_236_955_419_6;
    let k = (PI * f0 / sample_rate).tan();
    let vh = 10.0_f64.powf(db / 20.0);
    let vb = vh.powf(0.499_666_774_154_541_6);
    let a0 = 1.0 + k / q + k * k;
    let shelf = BiquadCoeffs {
        b0: (vh + vb * k / q + k * k) / a0,
        b1: 2.0 * (k * k - vh) / a0,
        b2: (vh - vb * k / q + k * k) / a0,
        a1: 2.0 * (k * k - 1.0) / a0,
        a2: (1.0 - k / q + k * k) / a0,
    };

    // Stage 2: RLB second-order high-pass (~38.135 Hz, Q = 0.5003270373238773).
    let f0_hp = 38.135_470_876_024_44;
    let q_hp = 0.500_327_037_323_877_3;
    let k_hp = (PI * f0_hp / sample_rate).tan();
    let hp_a0 = 1.0 + k_hp / q_hp + k_hp * k_hp;
    let rlb = BiquadCoeffs {
        b0: 1.0 / hp_a0,
        b1: -2.0 / hp_a0,
        b2: 1.0 / hp_a0,
        a1: 2.0 * (k_hp * k_hp - 1.0) / hp_a0,
        a2: (1.0 - k_hp / q_hp + k_hp * k_hp) / hp_a0,
    };

    (shelf, rlb)
}

fn block_weighted_power(ch_filtered: &[Vec<f64>], start: usize, end: usize) -> f64 {
    let len = end.saturating_sub(start);
    if len == 0 {
        return 0.0;
    }
    let mut weighted_sum = 0.0;
    for (idx, chan) in ch_filtered.iter().enumerate() {
        // BS.1770 channel weights: 1.0 for L/R/C, 1.41 for surround (idx >= 4).
        let g = if idx == 4 || idx == 5 { 1.41 } else { 1.0 };
        let mean_sq = chan[start..end].iter().map(|&v| v * v).sum::<f64>() / usize_to_f64(len);
        weighted_sum += g * mean_sq;
    }
    weighted_sum
}

#[allow(clippy::cast_possible_truncation)]
fn round_f64_to_i32(val: f64) -> i32 {
    val.round() as i32
}

#[allow(clippy::cast_precision_loss)]
fn usize_to_f64(val: usize) -> f64 {
    val as f64
}
