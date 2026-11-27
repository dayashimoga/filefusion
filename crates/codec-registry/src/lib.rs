//! `codec-registry`: L1 capability discovery, license hygiene, and Tier 2 tool probes.
//!
//! Separates Tier 1 built-in permissive Rust engines from Tier 2 out-of-process
//! external adapters (`ADR-002`, `MOD-RULE-05`, `FF-CTR-001` §3.3).

#![forbid(unsafe_code)]

use std::path::PathBuf;

use file_core::{
    CORE_SCHEMA_VERSION, ErrorCode, FileFusionError, FileFusionResult, FormatId, ResourceLimits,
    WORKSPACE_VERSION,
};
use serde::{Deserialize, Serialize};

/// Engine execution tier per `ADR-002`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineTier {
    /// Compiled directly into `FileFusion` using permissive licenses.
    Tier1BuiltIn,
    /// External binary invoked out-of-process under `IsolatedProcessRunner`.
    Tier2External,
}

/// License classification tracked for SBOM and redistribution compliance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LicenseClass {
    /// Permissive (`MIT`, `Apache-2.0`, `BSD`, `Zlib`, `ISC`, `CC0-1.0`, `BSL-1.0`).
    Permissive,
    /// Weak file-level copyleft (`MPL-2.0`).
    WeakCopyleft,
    /// Strong copyleft (`GPL`/`LGPL`/`AGPL`) strictly isolated out-of-process.
    CopyleftIsolated,
}

/// Availability metadata for an optional Tier 2 external binary (`FF-CTR-001` §3.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolAvailability {
    /// Registry capability identifier (e.g. `tier2-ffmpeg`).
    pub tool_id: String,
    /// Candidate executable names probed on host `PATH`.
    pub executable_names: Vec<String>,
    /// Whether an executable was found on this host.
    pub available: bool,
    /// Resolved executable path when available.
    pub binary_path: Option<String>,
    /// Parsed tool version string when obtainable.
    pub version: Option<String>,
    /// Redistribution license category.
    pub license_class: LicenseClass,
    /// Canonical SPDX license expression.
    pub spdx: String,
    /// Primary formats unlocked or accelerated by this tool.
    pub supported_formats: Vec<FormatId>,
    /// Actionable installation hint when the binary is absent.
    pub install_hint: String,
}

/// Capability entry returned in [`CapabilityRegistry`] and [`DoctorReport`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityEntry {
    /// Machine identifier of the engine or capability.
    pub id: String,
    /// Execution tier (`Tier1BuiltIn` or `Tier2External`).
    pub tier: EngineTier,
    /// License classification.
    pub license_class: LicenseClass,
    /// SPDX license expression.
    pub spdx: String,
    /// Whether the capability is currently ready on this host.
    pub available: bool,
    /// Formats serviced by this capability (`[]` indicates cross-cutting foundation).
    pub supported_formats: Vec<FormatId>,
    /// Resolved executable path for Tier 2 tools when available.
    pub binary_path: Option<String>,
    /// Reported version string for Tier 2 tools when available.
    pub version: Option<String>,
    /// Human-readable status note.
    pub note: String,
}

/// Runtime-discovered catalog of Tier 1 built-in and Tier 2 external engines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityRegistry {
    /// All registered Tier 1 and Tier 2 capabilities (`capabilities[0]` is `file-core-workspace`).
    pub entries: Vec<CapabilityEntry>,
    /// Detailed Tier 2 external binary availability snapshots.
    pub tools: Vec<ToolAvailability>,
}

/// Diagnostic snapshot produced by `filefusion doctor`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorReport {
    /// Schema version matching `CORE_SCHEMA_VERSION`.
    pub schema_version: u32,
    /// Workspace semantic version.
    pub workspace_version: String,
    /// Host operating system (`std::env::consts::OS`).
    pub host_os: String,
    /// Host CPU architecture (`std::env::consts::ARCH`).
    pub host_arch: String,
    /// Detected logical processor count.
    pub logical_cpus: usize,
    /// Conservative default resource limits computed for host.
    pub default_limits: ResourceLimits,
    /// Effective active resource limits after TOML, env, and CLI overrides.
    pub effective_limits: ResourceLimits,
    /// Registered capabilities and current readiness.
    pub capabilities: Vec<CapabilityEntry>,
    /// Detailed Tier 2 external binary probe snapshots.
    pub tool_availability: Vec<ToolAvailability>,
}

struct Tier2ToolSpec {
    tool_id: &'static str,
    executable_names: &'static [&'static str],
    #[cfg_attr(not(feature = "native-probe"), allow(dead_code))]
    version_flag: &'static str,
    license_class: LicenseClass,
    spdx: &'static str,
    supported_formats: &'static [FormatId],
    install_hint: &'static str,
}

const TIER2_TOOL_SPECS: &[Tier2ToolSpec] = &[
    Tier2ToolSpec {
        tool_id: "tier2-ffmpeg",
        executable_names: &["ffmpeg"],
        version_flag: "-version",
        license_class: LicenseClass::CopyleftIsolated,
        spdx: "LGPL-2.1-or-later OR GPL-2.0-or-later",
        supported_formats: &[
            FormatId::Mp4,
            FormatId::Mov,
            FormatId::Mkv,
            FormatId::Webm,
            FormatId::Avi,
            FormatId::Flac,
            FormatId::Wav,
            FormatId::Mp3,
            FormatId::Ogg,
            FormatId::Opus,
            FormatId::Aac,
            FormatId::M4a,
        ],
        install_hint: "Install FFmpeg on PATH for video transcoding, muxing, and extended audio codecs.",
    },
    Tier2ToolSpec {
        tool_id: "tier2-qpdf",
        executable_names: &["qpdf"],
        version_flag: "--version",
        license_class: LicenseClass::Permissive,
        spdx: "Apache-2.0",
        supported_formats: &[FormatId::Pdf],
        install_hint: "Install QPDF on PATH for linearized PDF optimization and cross-reference recovery.",
    },
    Tier2ToolSpec {
        tool_id: "tier2-tesseract",
        executable_names: &["tesseract"],
        version_flag: "--version",
        license_class: LicenseClass::Permissive,
        spdx: "Apache-2.0",
        supported_formats: &[FormatId::Pdf, FormatId::Png, FormatId::Jpeg, FormatId::Tiff],
        install_hint: "Install Tesseract OCR on PATH for local scanned-PDF and image OCR layers.",
    },
    Tier2ToolSpec {
        tool_id: "tier2-libreoffice",
        executable_names: &["soffice", "libreoffice"],
        version_flag: "--version",
        license_class: LicenseClass::WeakCopyleft,
        spdx: "MPL-2.0",
        supported_formats: &[
            FormatId::Docx,
            FormatId::Xlsx,
            FormatId::Pptx,
            FormatId::Odt,
            FormatId::Ods,
            FormatId::Odp,
            FormatId::Rtf,
            FormatId::Pdf,
        ],
        install_hint: "Install LibreOffice (`soffice`) on PATH for headless high-fidelity Office-to-PDF rendering.",
    },
    Tier2ToolSpec {
        tool_id: "tier2-pandoc",
        executable_names: &["pandoc"],
        version_flag: "--version",
        license_class: LicenseClass::CopyleftIsolated,
        spdx: "GPL-2.0-or-later",
        supported_formats: &[
            FormatId::Markdown,
            FormatId::Html,
            FormatId::Docx,
            FormatId::Epub,
            FormatId::Odt,
            FormatId::Rtf,
        ],
        install_hint: "Install Pandoc on PATH for complex academic and markup document conversions.",
    },
    Tier2ToolSpec {
        tool_id: "tier2-calibre",
        executable_names: &["ebook-convert"],
        version_flag: "--version",
        license_class: LicenseClass::CopyleftIsolated,
        spdx: "GPL-3.0-only",
        supported_formats: &[
            FormatId::Epub,
            FormatId::Mobi,
            FormatId::Pdf,
            FormatId::Html,
        ],
        install_hint: "Install Calibre (`ebook-convert`) on PATH for legacy MOBI/AZW3 ebook conversions.",
    },
    Tier2ToolSpec {
        tool_id: "tier2-7z",
        executable_names: &["7z", "7za", "7zz"],
        version_flag: "--help",
        license_class: LicenseClass::CopyleftIsolated,
        spdx: "LGPL-2.1-or-later",
        supported_formats: &[
            FormatId::SevenZ,
            FormatId::Zip,
            FormatId::Tar,
            FormatId::Gzip,
            FormatId::Bzip2,
            FormatId::Xz,
        ],
        install_hint: "Install 7-Zip (`7z`) on PATH for encrypted 7z volumes and solid LZMA2 packing.",
    },
];

fn tier1_entries() -> Vec<CapabilityEntry> {
    vec![
        CapabilityEntry {
            id: String::from("file-core-workspace"),
            tier: EngineTier::Tier1BuiltIn,
            license_class: LicenseClass::Permissive,
            spdx: String::from("Apache-2.0 OR MIT"),
            available: true,
            supported_formats: Vec::new(),
            binary_path: None,
            version: Some(WORKSPACE_VERSION.to_owned()),
            note: String::from("Stage 1 core domain contracts, errors, config, and logging active"),
        },
        CapabilityEntry {
            id: String::from("file-core-inspection"),
            tier: EngineTier::Tier1BuiltIn,
            license_class: LicenseClass::Permissive,
            spdx: String::from("Apache-2.0 OR MIT"),
            available: true,
            supported_formats: vec![
                FormatId::Jpeg,
                FormatId::Png,
                FormatId::Webp,
                FormatId::Avif,
                FormatId::Gif,
                FormatId::Bmp,
                FormatId::Pdf,
                FormatId::Docx,
                FormatId::Xlsx,
                FormatId::Pptx,
                FormatId::Epub,
                FormatId::Zip,
                FormatId::Tar,
                FormatId::Zstd,
                FormatId::Wav,
                FormatId::Flac,
                FormatId::Mp3,
            ],
            binary_path: None,
            version: Some(WORKSPACE_VERSION.to_owned()),
            note: String::from(
                "Magic-byte probing, 64 KB BLAKE3/SHA-256/xxHash64 digests, metadata, and classification active",
            ),
        },
        CapabilityEntry {
            id: String::from("validation-engine-security"),
            tier: EngineTier::Tier1BuiltIn,
            license_class: LicenseClass::Permissive,
            spdx: String::from("Apache-2.0 OR MIT"),
            available: true,
            supported_formats: vec![FormatId::Zip, FormatId::Tar, FormatId::Docx, FormatId::Epub],
            binary_path: None,
            version: Some(WORKSPACE_VERSION.to_owned()),
            note: String::from(
                "Zip-slip traversal protection and expansion-bomb guardrails active",
            ),
        },
    ]
}

#[cfg(feature = "native-probe")]
fn find_executable_in_dirs(names: &[&str], search_dirs: &[PathBuf]) -> Option<PathBuf> {
    let extensions: Vec<String> = if cfg!(target_os = "windows") {
        vec![
            String::new(),
            String::from(".exe"),
            String::from(".cmd"),
            String::from(".bat"),
        ]
    } else {
        vec![String::new()]
    };

    for dir in search_dirs {
        for base in names {
            for ext in &extensions {
                let candidate = dir.join(format!("{base}{ext}"));
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

#[cfg(feature = "native-probe")]
fn probe_version_line(path: &std::path::Path, version_flag: &str) -> Option<String> {
    use std::process::{Command, Stdio};

    let output = Command::new(path)
        .arg(version_flag)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    stdout
        .lines()
        .chain(stderr.lines())
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.chars().take(120).collect())
}

#[cfg(feature = "native-probe")]
fn probe_tier2_tool(
    spec: &Tier2ToolSpec,
    search_dirs: &[PathBuf],
    run_version_probe: bool,
) -> ToolAvailability {
    let found = find_executable_in_dirs(spec.executable_names, search_dirs);
    let (available, binary_path, version) = match found {
        Some(path) => {
            let ver = if run_version_probe {
                probe_version_line(&path, spec.version_flag)
            } else {
                None
            };
            (true, Some(path.to_string_lossy().into_owned()), ver)
        }
        None => (false, None, None),
    };

    ToolAvailability {
        tool_id: String::from(spec.tool_id),
        executable_names: spec
            .executable_names
            .iter()
            .map(|s| (*s).to_owned())
            .collect(),
        available,
        binary_path,
        version,
        license_class: spec.license_class,
        spdx: String::from(spec.spdx),
        supported_formats: spec.supported_formats.to_vec(),
        install_hint: String::from(spec.install_hint),
    }
}

#[cfg(not(feature = "native-probe"))]
fn probe_tier2_tool(
    spec: &Tier2ToolSpec,
    _search_dirs: &[PathBuf],
    _run_version_probe: bool,
) -> ToolAvailability {
    ToolAvailability {
        tool_id: String::from(spec.tool_id),
        executable_names: spec
            .executable_names
            .iter()
            .map(|s| (*s).to_owned())
            .collect(),
        available: false,
        binary_path: None,
        version: None,
        license_class: spec.license_class,
        spdx: String::from(spec.spdx),
        supported_formats: spec.supported_formats.to_vec(),
        install_hint: String::from(spec.install_hint),
    }
}

fn tool_to_capability_entry(tool: &ToolAvailability) -> CapabilityEntry {
    let note = if tool.available {
        match (&tool.version, &tool.binary_path) {
            (Some(ver), _) => format!("Detected ({ver})"),
            (None, Some(path)) => format!("Detected at {path}"),
            (None, None) => String::from("Detected on PATH"),
        }
    } else if cfg!(feature = "native-probe") {
        tool.install_hint.clone()
    } else {
        String::from("Tier 2 external binaries disabled in wasm32 build")
    };

    CapabilityEntry {
        id: tool.tool_id.clone(),
        tier: EngineTier::Tier2External,
        license_class: tool.license_class,
        spdx: tool.spdx.clone(),
        available: tool.available,
        supported_formats: tool.supported_formats.clone(),
        binary_path: tool.binary_path.clone(),
        version: tool.version.clone(),
        note,
    }
}

impl CapabilityRegistry {
    /// Probes all Tier 1 built-in engines and Tier 2 external binaries on the host `PATH`.
    #[must_use]
    pub fn discover() -> Self {
        let search_dirs: Vec<PathBuf> = std::env::var_os("PATH")
            .map(|paths| std::env::split_paths(&paths).collect())
            .unwrap_or_default();
        Self::discover_with_options(&search_dirs, true)
    }

    /// Probes capabilities against explicit search directories (useful for deterministic tests).
    #[must_use]
    pub fn discover_in_dirs(search_dirs: &[PathBuf], run_version_probe: bool) -> Self {
        Self::discover_with_options(search_dirs, run_version_probe)
    }

    fn discover_with_options(search_dirs: &[PathBuf], run_version_probe: bool) -> Self {
        let mut entries = tier1_entries();
        let mut tools = Vec::with_capacity(TIER2_TOOL_SPECS.len());
        for spec in TIER2_TOOL_SPECS {
            let tool = probe_tier2_tool(spec, search_dirs, run_version_probe);
            entries.push(tool_to_capability_entry(&tool));
            tools.push(tool);
        }
        Self { entries, tools }
    }

    /// Looks up a capability entry by its machine ID.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&CapabilityEntry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    /// Returns all registered capabilities that support `format`.
    #[must_use]
    pub fn capabilities_for_format(&self, format: FormatId) -> Vec<&CapabilityEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.supported_formats.contains(&format))
            .collect()
    }

    /// Ensures capability `id` is registered and available, or returns `FF-ERR-ENG-MISSING` (`CliExitCode::MissingEngine = 5`).
    ///
    /// # Errors
    /// Returns [`FileFusionError`] (`ErrorCode::EngMissing`) if the capability is unknown or unavailable.
    pub fn require_available(&self, id: &str) -> FileFusionResult<&CapabilityEntry> {
        let Some(entry) = self.get(id) else {
            return Err(FileFusionError::new(
                ErrorCode::EngMissing,
                format!("unknown engine capability '{id}'"),
            )
            .with_remediation("Run `filefusion doctor` to list supported engine identifiers."));
        };
        if !entry.available {
            return Err(FileFusionError::new(
                ErrorCode::EngMissing,
                format!(
                    "required engine '{id}' ({}) is unavailable on this host",
                    entry.spdx
                ),
            )
            .with_remediation(entry.note.clone()));
        }
        Ok(entry)
    }

    /// Counts how many registered capabilities are currently available.
    #[must_use]
    pub fn available_count(&self) -> usize {
        self.entries.iter().filter(|e| e.available).count()
    }

    /// Counts how many registered capabilities are currently unavailable.
    #[must_use]
    pub fn missing_count(&self) -> usize {
        self.entries.iter().filter(|e| !e.available).count()
    }
}

/// Builds a [`DoctorReport`] using explicitly resolved [`ResourceLimits`].
#[must_use]
pub fn collect_doctor_report(effective_limits: ResourceLimits) -> DoctorReport {
    let logical_cpus = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    let registry = CapabilityRegistry::discover();

    DoctorReport {
        schema_version: CORE_SCHEMA_VERSION,
        workspace_version: WORKSPACE_VERSION.to_owned(),
        host_os: std::env::consts::OS.to_owned(),
        host_arch: std::env::consts::ARCH.to_owned(),
        logical_cpus,
        default_limits: ResourceLimits::default(),
        effective_limits,
        capabilities: registry.entries,
        tool_availability: registry.tools,
    }
}

/// Builds a baseline [`DoctorReport`] reflecting currently active workspace engines.
#[must_use]
pub fn collect_baseline_doctor_report() -> DoctorReport {
    collect_doctor_report(ResourceLimits::default())
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::fs;

    use file_core::CliExitCode;

    use super::*;

    #[test]
    fn baseline_doctor_report_contains_tier1_core_and_tier2_specs() {
        let report = collect_baseline_doctor_report();
        assert_eq!(report.schema_version, CORE_SCHEMA_VERSION);
        assert_ne!(report.workspace_version, "");
        assert_ne!(report.host_os, "");
        assert_ne!(report.host_arch, "");
        assert!(report.logical_cpus >= 1);
        assert_eq!(report.capabilities[0].id, "file-core-workspace");
        assert_eq!(report.capabilities[0].tier, EngineTier::Tier1BuiltIn);
        assert!(report.capabilities[0].available);
        assert_eq!(report.tool_availability.len(), 7);
        assert_eq!(report.capabilities.len(), 10);
    }

    #[test]
    fn discover_in_empty_dirs_reports_missing_tier2_with_exit_code_five() {
        let registry = CapabilityRegistry::discover_in_dirs(&[], false);
        assert!(registry.require_available("file-core-workspace").is_ok());
        assert!(registry.require_available("file-core-inspection").is_ok());

        let missing = registry.require_available("tier2-ffmpeg");
        assert!(missing.is_err());
        if let Err(err) = missing {
            assert_eq!(err.diagnostic.code, ErrorCode::EngMissing);
            assert_eq!(err.exit_code(), CliExitCode::MissingEngine);
        }

        let pdf_caps = registry.capabilities_for_format(FormatId::Pdf);
        assert!(pdf_caps.iter().any(|c| c.id == "tier2-qpdf"));
        assert!(pdf_caps.iter().any(|c| c.id == "file-core-inspection"));
    }

    #[test]
    fn discover_in_dirs_locates_present_tier2_executable() -> Result<(), Box<dyn Error>> {
        let dir =
            std::env::temp_dir().join(format!("filefusion_registry_test_{}", std::process::id()));
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }
        fs::create_dir_all(&dir)?;

        let exe_name = if cfg!(target_os = "windows") {
            "qpdf.exe"
        } else {
            "qpdf"
        };
        fs::write(dir.join(exe_name), b"stub")?;

        let registry = CapabilityRegistry::discover_in_dirs(std::slice::from_ref(&dir), false);
        let qpdf = registry.require_available("tier2-qpdf")?;
        assert!(qpdf.available);
        assert_eq!(qpdf.spdx, "Apache-2.0");
        assert!(qpdf.binary_path.is_some());

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }
}
