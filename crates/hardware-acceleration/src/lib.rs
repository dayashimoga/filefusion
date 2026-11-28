//! `hardware-acceleration`: L1 CPU SIMD and GPU hardware encoder discovery
//! with deterministic CPU fallback (`FR-ADP-002`, `FF-MOD-001` §3, §6).
//!
//! Uses safe runtime ISA detection (`std::arch`) and non-blocking OS library/device
//! probing (`MOD-RULE-04` for explicit `ffmpeg -hwaccels` checks), plus a clean
//! single-worker `wasm32` no-op stub when `hw-accel` is disabled.

#![forbid(unsafe_code)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use file_core::{
    CORE_SCHEMA_VERSION, ErrorCode, FileFusionError, FileFusionResult, FormatId,
    IsolatedProcessRunner, JobContext, ProcessInvocation,
};
use serde::{Deserialize, Serialize};

/// Detected CPU SIMD vector capabilities (`FR-ADP-002`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct SimdFeatures {
    /// `x86/x86_64` SSE4.2 instruction set available.
    pub sse4_2: bool,
    /// `x86/x86_64` AVX2 256-bit SIMD instruction set available.
    pub avx2: bool,
    /// `x86/x86_64` AVX-512 Foundation instruction set available.
    pub avx512f: bool,
    /// ARM64 NEON vector extension available.
    pub neon: bool,
    /// WebAssembly 128-bit SIMD baseline compiled in.
    pub wasm_simd128: bool,
}

impl SimdFeatures {
    /// Probes CPU SIMD instruction sets supported on the active execution target safely.
    #[must_use]
    pub fn detect() -> Self {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            Self {
                sse4_2: std::arch::is_x86_feature_detected!("sse4.2"),
                avx2: std::arch::is_x86_feature_detected!("avx2"),
                avx512f: std::arch::is_x86_feature_detected!("avx512f"),
                neon: false,
                wasm_simd128: false,
            }
        }
        #[cfg(target_arch = "aarch64")]
        {
            Self {
                sse4_2: false,
                avx2: false,
                avx512f: false,
                neon: std::arch::is_aarch64_feature_detected!("neon"),
                wasm_simd128: false,
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            Self {
                sse4_2: false,
                avx2: false,
                avx512f: false,
                neon: false,
                wasm_simd128: cfg!(target_feature = "simd128"),
            }
        }
        #[cfg(not(any(
            target_arch = "x86",
            target_arch = "x86_64",
            target_arch = "aarch64",
            target_arch = "wasm32"
        )))]
        {
            Self::default()
        }
    }
}

/// Supported vendor hardware media acceleration backends (`FR-ADP-002`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HardwareVendorBackend {
    /// NVIDIA NVENC hardware encoder/decoder.
    Nvenc,
    /// Intel Quick Sync Video (oneVPL / Media SDK).
    Qsv,
    /// AMD Advanced Media Framework.
    Amf,
    /// Apple `VideoToolbox` (macOS / iOS).
    VideoToolbox,
    /// Android `MediaCodec` hardware NDK bridge.
    MediaCodec,
    /// Linux VA-API DRI hardware render backend.
    Vaapi,
}

/// Probe status for an individual hardware video acceleration backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HardwareEncoderCapability {
    /// Hardware vendor API identifier.
    pub backend: HardwareVendorBackend,
    /// Canonical human/machine identifier (e.g., `"hw-nvenc"`).
    pub id: String,
    /// Whether runtime driver/device presence was detected on this host.
    pub available: bool,
    /// Video container formats accelerated by this backend.
    pub supported_formats: Vec<FormatId>,
    /// File path, sysfs node, or platform rationale explaining probe status.
    pub evidence: String,
}

/// Complete host CPU SIMD + GPU acceleration capability snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HardwareProfile {
    /// Contract schema version (`CORE_SCHEMA_VERSION`).
    pub schema_version: u32,
    /// Target CPU architecture (`std::env::consts::ARCH`).
    pub arch: String,
    /// Target operating system (`std::env::consts::OS`).
    pub os: String,
    /// Detected CPU SIMD vector capabilities.
    pub simd: SimdFeatures,
    /// Discovered or absent vendor hardware encoder capabilities.
    pub encoders: Vec<HardwareEncoderCapability>,
    /// `true` when compiled as a `wasm32` / `--features wasm` no-op stub.
    pub wasm_stub: bool,
    /// `true` when deterministic CPU encoding should be used by default.
    pub fallback_to_cpu: bool,
    /// Human-readable summary of host acceleration readiness.
    pub note: String,
}

/// Backend routing choice returned by [`select_video_backend`] (`FR-ADP-002`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HardwareRoutingDecision {
    /// `true` when a hardware encoder backend was chosen.
    pub use_hardware: bool,
    /// Selected hardware vendor backend, if `use_hardware == true`.
    pub selected_backend: Option<HardwareVendorBackend>,
    /// `true` when falling back to deterministic CPU encoders (`x264`/`x265`/`SVT-AV1`).
    pub fallback_to_cpu: bool,
    /// Human-readable rationale for auditability.
    pub reason: String,
}

const VIDEO_FORMATS: &[FormatId] = &[
    FormatId::Mp4,
    FormatId::Mov,
    FormatId::Mkv,
    FormatId::Webm,
    FormatId::Avi,
];

/// Probes CPU SIMD instructions and native hardware encoders on the active host.
///
/// Under `--features wasm` (or `wasm32-unknown-unknown` without `hw-accel`),
/// cleanly returns a deterministic no-op stub defaulting to CPU/Web-Worker execution.
#[must_use]
pub fn probe_hardware() -> HardwareProfile {
    #[cfg(all(feature = "hw-accel", not(target_arch = "wasm32")))]
    {
        let dirs = default_native_driver_dirs();
        probe_hardware_in_dirs(&dirs)
    }
    #[cfg(any(not(feature = "hw-accel"), target_arch = "wasm32"))]
    {
        HardwareProfile {
            schema_version: CORE_SCHEMA_VERSION,
            arch: std::env::consts::ARCH.to_owned(),
            os: std::env::consts::OS.to_owned(),
            simd: SimdFeatures::detect(),
            encoders: Vec::new(),
            wasm_stub: true,
            fallback_to_cpu: true,
            note: String::from(
                "Hardware encoder probe disabled under wasm no-op stub; deterministic CPU/Worker fallback active.",
            ),
        }
    }
}

#[cfg_attr(
    any(not(feature = "hw-accel"), target_arch = "wasm32"),
    allow(dead_code)
)]
fn default_native_driver_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    #[cfg(target_os = "windows")]
    {
        if let Some(root) = std::env::var_os("SystemRoot").or_else(|| std::env::var_os("WINDIR")) {
            dirs.push(PathBuf::from(root).join("System32"));
        } else {
            dirs.push(PathBuf::from("C:\\Windows\\System32"));
        }
    }
    #[cfg(target_os = "linux")]
    {
        dirs.push(PathBuf::from("/usr/lib/x86_64-linux-gnu"));
        dirs.push(PathBuf::from("/usr/lib64"));
        dirs.push(PathBuf::from("/usr/lib"));
        dirs.push(PathBuf::from("/dev/dri"));
    }
    #[cfg(target_os = "macos")]
    {
        dirs.push(PathBuf::from("/System/Library/Frameworks"));
    }
    #[cfg(target_os = "android")]
    {
        dirs.push(PathBuf::from("/system/lib64"));
        dirs.push(PathBuf::from("/system/lib"));
    }
    dirs
}

struct BackendSpec {
    backend: HardwareVendorBackend,
    id: &'static str,
    artifacts: &'static [&'static str],
}

const BACKEND_SPECS: &[BackendSpec] = &[
    BackendSpec {
        backend: HardwareVendorBackend::Nvenc,
        id: "hw-nvenc",
        artifacts: &[
            "nvEncodeAPI64.dll",
            "libnvidia-encode.so.1",
            "libnvidia-encode.so",
        ],
    },
    BackendSpec {
        backend: HardwareVendorBackend::Qsv,
        id: "hw-qsv",
        artifacts: &["libmfx64.dll", "libvpl.dll", "libmfx.so.1", "libvpl.so.2"],
    },
    BackendSpec {
        backend: HardwareVendorBackend::Amf,
        id: "hw-amf",
        artifacts: &["amfrt64.dll", "libamfrt64.so.1"],
    },
    BackendSpec {
        backend: HardwareVendorBackend::VideoToolbox,
        id: "hw-videotoolbox",
        artifacts: &["VideoToolbox.framework"],
    },
    BackendSpec {
        backend: HardwareVendorBackend::MediaCodec,
        id: "hw-mediacodec",
        artifacts: &["libmediandk.so"],
    },
    BackendSpec {
        backend: HardwareVendorBackend::Vaapi,
        id: "hw-vaapi",
        artifacts: &["renderD128", "libva.so.2"],
    },
];

fn find_artifact(dirs: &[PathBuf], names: &[&str]) -> Option<PathBuf> {
    for dir in dirs {
        for name in names {
            let candidate = dir.join(name);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Deterministic directory-parameterized hardware encoder probe useful for tests and custom setups.
#[must_use]
pub fn probe_hardware_in_dirs(driver_dirs: &[PathBuf]) -> HardwareProfile {
    let simd = SimdFeatures::detect();
    let mut encoders = Vec::with_capacity(BACKEND_SPECS.len());

    for spec in BACKEND_SPECS {
        let matched = find_artifact(driver_dirs, spec.artifacts);
        let (available, evidence) = match matched {
            Some(path) => (true, path.display().to_string()),
            None => (false, String::from("driver/device artifact not present")),
        };
        encoders.push(HardwareEncoderCapability {
            backend: spec.backend,
            id: spec.id.to_owned(),
            available,
            supported_formats: VIDEO_FORMATS.to_vec(),
            evidence,
        });
    }

    let any_hw = encoders.iter().any(|e| e.available);
    let fallback_to_cpu = !any_hw;
    let note = if any_hw {
        String::from(
            "Hardware media acceleration candidate detected alongside deterministic CPU fallback.",
        )
    } else {
        String::from("No hardware media encoder detected; deterministic CPU encoders active.")
    };

    HardwareProfile {
        schema_version: CORE_SCHEMA_VERSION,
        arch: std::env::consts::ARCH.to_owned(),
        os: std::env::consts::OS.to_owned(),
        simd,
        encoders,
        wasm_stub: false,
        fallback_to_cpu,
        note,
    }
}

/// Parses `ffmpeg -hide_banner -hwaccels` output lines into typed [`HardwareVendorBackend`]s.
#[must_use]
pub fn parse_ffmpeg_hwaccels(stdout: &str) -> Vec<HardwareVendorBackend> {
    let mut found = Vec::new();
    for line in stdout.lines() {
        let token = line.trim().to_ascii_lowercase();
        let mapped = match token.as_str() {
            "cuda" | "nvenc" => Some(HardwareVendorBackend::Nvenc),
            "qsv" => Some(HardwareVendorBackend::Qsv),
            "amf" => Some(HardwareVendorBackend::Amf),
            "videotoolbox" => Some(HardwareVendorBackend::VideoToolbox),
            "mediacodec" => Some(HardwareVendorBackend::MediaCodec),
            "vaapi" => Some(HardwareVendorBackend::Vaapi),
            _ => None,
        };
        if let Some(backend) = mapped {
            if !found.contains(&backend) {
                found.push(backend);
            }
        }
    }
    found
}

/// Queries live `ffmpeg` hardware acceleration methods strictly through [`IsolatedProcessRunner`]
/// (`MOD-RULE-04`).
///
/// # Errors
/// Returns [`FileFusionError`] if process execution fails, times out, or is cancelled.
pub fn probe_ffmpeg_hwaccels(
    ffmpeg_path: &Path,
    runner: &dyn IsolatedProcessRunner,
    ctx: &JobContext,
) -> FileFusionResult<Vec<HardwareVendorBackend>> {
    let inv = ProcessInvocation {
        executable: ffmpeg_path.to_path_buf(),
        args: vec![OsString::from("-hide_banner"), OsString::from("-hwaccels")],
        working_dir: None,
        timeout_secs: 10,
        allow_env_keys: Vec::new(),
    };
    let output = runner.run_isolated(&inv, ctx)?;
    if output.exit_code != 0 {
        return Err(FileFusionError::new(
            ErrorCode::EngMissing,
            format!(
                "ffmpeg -hwaccels probe exited with code {}",
                output.exit_code
            ),
        )
        .with_remediation("Verify Tier 2 FFmpeg installation via `filefusion doctor`."));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(parse_ffmpeg_hwaccels(&text))
}

/// Selects hardware vs. deterministic CPU video backend (`FR-ADP-002`).
#[must_use]
pub fn select_video_backend(
    profile: &HardwareProfile,
    target_format: FormatId,
    prefer_deterministic_cpu: bool,
) -> HardwareRoutingDecision {
    if profile.wasm_stub {
        return HardwareRoutingDecision {
            use_hardware: false,
            selected_backend: None,
            fallback_to_cpu: true,
            reason: String::from("WASM no-op stub falls back to deterministic CPU/Worker codecs."),
        };
    }
    if prefer_deterministic_cpu {
        return HardwareRoutingDecision {
            use_hardware: false,
            selected_backend: None,
            fallback_to_cpu: true,
            reason: String::from(
                "Deterministic CPU quality/compression target requested; bypassing hardware encoder.",
            ),
        };
    }

    let preference_order = [
        HardwareVendorBackend::VideoToolbox,
        HardwareVendorBackend::Nvenc,
        HardwareVendorBackend::Qsv,
        HardwareVendorBackend::Amf,
        HardwareVendorBackend::Vaapi,
        HardwareVendorBackend::MediaCodec,
    ];
    for candidate in preference_order {
        if let Some(cap) = profile.encoders.iter().find(|enc| {
            enc.backend == candidate
                && enc.available
                && enc.supported_formats.contains(&target_format)
        }) {
            return HardwareRoutingDecision {
                use_hardware: true,
                selected_backend: Some(cap.backend),
                fallback_to_cpu: false,
                reason: format!("Selected hardware backend {} ({})", cap.id, cap.evidence),
            };
        }
    }

    HardwareRoutingDecision {
        use_hardware: false,
        selected_backend: None,
        fallback_to_cpu: true,
        reason: String::from(
            "No compatible hardware encoder available; falling back to deterministic CPU.",
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::fs;

    use file_core::{ProcessOutput, ResourceLimits};

    use super::*;

    #[test]
    fn probe_hardware_detects_arch_and_handles_empty_dirs_safely() {
        let empty = probe_hardware_in_dirs(&[]);
        assert_eq!(empty.schema_version, CORE_SCHEMA_VERSION);
        assert!(empty.fallback_to_cpu);
        assert!(!empty.wasm_stub);
        assert_eq!(empty.encoders.len(), 6);

        let decision = select_video_backend(&empty, FormatId::Mp4, false);
        assert!(!decision.use_hardware);
        assert!(decision.fallback_to_cpu);
        assert!(decision.selected_backend.is_none());

        let live = probe_hardware();
        assert_eq!(live.schema_version, CORE_SCHEMA_VERSION);
        assert_ne!(live.arch, "");
        assert_ne!(live.os, "");
        #[cfg(not(feature = "hw-accel"))]
        {
            assert!(live.wasm_stub);
            assert!(live.fallback_to_cpu);
        }
    }

    #[test]
    fn probe_hardware_in_dirs_discovers_driver_and_respects_deterministic_cpu_flag()
    -> Result<(), Box<dyn Error>> {
        let dir = std::env::temp_dir().join(format!("filefusion_hw_probe_{}", std::process::id()));
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("nvEncodeAPI64.dll"), b"stub-nvenc")?;

        let profile = probe_hardware_in_dirs(std::slice::from_ref(&dir));
        assert!(!profile.fallback_to_cpu);

        let hw_choice = select_video_backend(&profile, FormatId::Mp4, false);
        assert!(hw_choice.use_hardware);
        assert_eq!(
            hw_choice.selected_backend,
            Some(HardwareVendorBackend::Nvenc)
        );

        let cpu_override = select_video_backend(&profile, FormatId::Mp4, true);
        assert!(!cpu_override.use_hardware);
        assert!(cpu_override.fallback_to_cpu);

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    struct MockFfmpegRunner {
        stdout: &'static str,
    }

    impl IsolatedProcessRunner for MockFfmpegRunner {
        fn run_isolated(
            &self,
            invocation: &ProcessInvocation,
            ctx: &JobContext,
        ) -> FileFusionResult<ProcessOutput> {
            ctx.check_cancelled()?;
            assert_eq!(
                invocation.args,
                vec![OsString::from("-hide_banner"), OsString::from("-hwaccels")]
            );
            Ok(ProcessOutput {
                exit_code: 0,
                stdout: self.stdout.as_bytes().to_vec(),
                stderr: Vec::new(),
                duration_ms: 4,
                timed_out: false,
            })
        }
    }

    #[test]
    fn parse_and_probe_ffmpeg_hwaccels_extracts_known_backends() -> FileFusionResult<()> {
        let sample = "Hardware acceleration methods:\ncuda\nqsv\nvideotoolbox\nvaapi\ncuda\n";
        let runner = MockFfmpegRunner { stdout: sample };
        let ctx = JobContext::new("hw_probe", ResourceLimits::default());

        let backends = probe_ffmpeg_hwaccels(Path::new("ffmpeg"), &runner, &ctx)?;
        assert_eq!(
            backends,
            vec![
                HardwareVendorBackend::Nvenc,
                HardwareVendorBackend::Qsv,
                HardwareVendorBackend::VideoToolbox,
                HardwareVendorBackend::Vaapi,
            ]
        );
        Ok(())
    }
}
