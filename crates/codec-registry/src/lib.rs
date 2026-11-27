//! `codec-registry`: L1 capability discovery and license metadata crate.
//!
//! Separates Tier 1 built-in permissive Rust engines from Tier 2 out-of-process
//! external adapters (`ADR-002`, `MOD-RULE-05`).

#![forbid(unsafe_code)]

use file_core::{CORE_SCHEMA_VERSION, ResourceLimits, WORKSPACE_VERSION};
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
    /// Permissive (`MIT`, `Apache-2.0`, `BSD`, `Zlib`, `ISC`).
    Permissive,
    /// Weak file-level copyleft (`MPL-2.0`).
    WeakCopyleft,
    /// Strong copyleft (`GPL`/`LGPL`/`AGPL`) strictly isolated out-of-process.
    CopyleftIsolated,
}

/// Capability entry returned in [`DoctorReport`].
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
    /// Human-readable status note.
    pub note: String,
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
}

/// Builds a [`DoctorReport`] using explicitly resolved [`ResourceLimits`].
#[must_use]
pub fn collect_doctor_report(effective_limits: ResourceLimits) -> DoctorReport {
    let logical_cpus = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);

    let capabilities = vec![CapabilityEntry {
        id: String::from("file-core-workspace"),
        tier: EngineTier::Tier1BuiltIn,
        license_class: LicenseClass::Permissive,
        spdx: String::from("Apache-2.0 OR MIT"),
        available: true,
        note: String::from("Stage 1.1.1 Cargo workspace anchor active"),
    }];

    DoctorReport {
        schema_version: CORE_SCHEMA_VERSION,
        workspace_version: WORKSPACE_VERSION.to_owned(),
        host_os: std::env::consts::OS.to_owned(),
        host_arch: std::env::consts::ARCH.to_owned(),
        logical_cpus,
        default_limits: ResourceLimits::default(),
        effective_limits,
        capabilities,
    }
}

/// Builds a baseline [`DoctorReport`] reflecting currently active workspace engines.
#[must_use]
pub fn collect_baseline_doctor_report() -> DoctorReport {
    collect_doctor_report(ResourceLimits::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_doctor_report_contains_tier1_core_and_host_info() {
        let report = collect_baseline_doctor_report();
        assert_eq!(report.schema_version, CORE_SCHEMA_VERSION);
        assert_ne!(report.workspace_version, "");
        assert_ne!(report.host_os, "");
        assert_ne!(report.host_arch, "");
        assert!(report.logical_cpus >= 1);
        assert_eq!(report.capabilities.len(), 1);
        assert_eq!(report.capabilities[0].tier, EngineTier::Tier1BuiltIn);
        assert!(report.capabilities[0].available);
    }
}
