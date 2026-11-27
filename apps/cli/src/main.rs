//! `filefusion` CLI entrypoint (`apps/cli`).
//!
//! Thin Plane 1/2 adapter over `job-manager` (`MOD-RULE-03`).

#![forbid(unsafe_code)]

use std::fmt::Write as _;
use std::io::{self, Write};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use file_core::{CliExitCode, workspace_identity};
use job_manager::{DoctorReport, collect_baseline_doctor_report};

#[derive(Debug, Parser)]
#[command(
    name = "filefusion",
    version,
    about = "FileFusion — Universal File Intelligence, Transformation & Optimization CLI"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Inspect host environment, resource ceilings, and Tier 1/Tier 2 engine readiness.
    Doctor {
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
}

fn format_doctor_human(report: &DoctorReport) -> String {
    let mut out = format!(
        "FileFusion ({}) — Doctor Diagnostics\nHost: {} ({}) | Logical CPUs: {}\nDefault ResourceLimits: max_workers={}, max_memory_mb={} MB, max_expansion_ratio={}:1, max_archive_depth={}\nCapabilities:\n",
        workspace_identity(),
        report.host_os,
        report.host_arch,
        report.logical_cpus,
        report.default_limits.max_workers,
        report.default_limits.max_memory_mb,
        report.default_limits.max_expansion_ratio,
        report.default_limits.max_archive_depth,
    );
    for cap in &report.capabilities {
        let state = if cap.available { "READY" } else { "MISSING" };
        let _ = writeln!(
            out,
            "  - [{}] {} ({:?}, {}) — {}",
            state, cap.id, cap.tier, cap.spdx, cap.note
        );
    }
    out
}

fn write_stdout_line(content: &str) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    stdout.write_all(content.as_bytes())?;
    if !content.ends_with('\n') {
        stdout.write_all(b"\n")?;
    }
    stdout.flush()
}

fn run() -> CliExitCode {
    let cli = Cli::parse();
    match cli.command {
        Commands::Doctor { json } => {
            let report = collect_baseline_doctor_report();
            if json {
                match serde_json::to_string_pretty(&report) {
                    Ok(rendered) => {
                        if write_stdout_line(&rendered).is_err() {
                            return CliExitCode::OperationalFailure;
                        }
                        CliExitCode::Success
                    }
                    Err(_) => CliExitCode::OperationalFailure,
                }
            } else if write_stdout_line(&format_doctor_human(&report)).is_err() {
                CliExitCode::OperationalFailure
            } else {
                CliExitCode::Success
            }
        }
    }
}

fn main() -> ExitCode {
    ExitCode::from(run())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_doctor_output_includes_workspace_and_tier1_capability() {
        let report = collect_baseline_doctor_report();
        let rendered = format_doctor_human(&report);
        assert!(rendered.contains("file-core"));
        assert!(rendered.contains("file-core-workspace"));
        assert!(rendered.contains("READY"));
    }
}
