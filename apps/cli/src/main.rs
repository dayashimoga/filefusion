//! `filefusion` CLI entrypoint (`apps/cli`).
//!
//! Thin Plane 1/2 adapter over `job-manager` (`MOD-RULE-03`) with structured
//! `tracing` diagnostics routed strictly to `stderr`.

#![forbid(unsafe_code)]

use std::fmt::Write as _;
use std::io::{self, Write};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use file_core::{
    CliExitCode, FileFusionError, LogFormat, LogLevel, LoggingConfig, init_logging,
    workspace_identity,
};
use job_manager::{DoctorReport, run_doctor_probe};

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliLogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl From<CliLogLevel> for LogLevel {
    fn from(value: CliLogLevel) -> Self {
        match value {
            CliLogLevel::Error => Self::Error,
            CliLogLevel::Warn => Self::Warn,
            CliLogLevel::Info => Self::Info,
            CliLogLevel::Debug => Self::Debug,
            CliLogLevel::Trace => Self::Trace,
        }
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "filefusion",
    version,
    about = "FileFusion — Universal File Intelligence, Transformation & Optimization CLI"
)]
struct Cli {
    /// Minimum log level emitted to stderr.
    #[arg(long, global = true, value_enum, default_value_t = CliLogLevel::Warn)]
    log_level: CliLogLevel,

    /// Emit structured JSON log lines to stderr.
    #[arg(long, global = true)]
    log_json: bool,

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

fn write_stream(stream: &mut dyn Write, content: &str) -> io::Result<()> {
    stream.write_all(content.as_bytes())?;
    if !content.ends_with('\n') {
        stream.write_all(b"\n")?;
    }
    stream.flush()
}

fn report_cli_error(err: &FileFusionError, json_mode: bool) -> CliExitCode {
    tracing::error!(
        code = %err.diagnostic.code,
        category = ?err.diagnostic.category,
        "{}",
        err.diagnostic.message
    );
    let rendered = if json_mode {
        serde_json::to_string_pretty(&err.diagnostic)
            .unwrap_or_else(|_| format!("{{\"error\":\"{}\"}}", err.diagnostic.code))
    } else if let Some(hint) = &err.diagnostic.remediation {
        format!(
            "Error [{}]: {}\nHint: {}",
            err.diagnostic.code, err.diagnostic.message, hint
        )
    } else {
        format!(
            "Error [{}]: {}",
            err.diagnostic.code, err.diagnostic.message
        )
    };
    let mut stderr = io::stderr().lock();
    let _ = write_stream(&mut stderr, &rendered);
    err.exit_code()
}

fn run() -> CliExitCode {
    let cli = Cli::parse();
    let logging_cfg = LoggingConfig {
        level: LogLevel::from(cli.log_level),
        format: if cli.log_json {
            LogFormat::Json
        } else {
            LogFormat::Pretty
        },
        respect_env: true,
        include_target: false,
    };
    if let Err(err) = init_logging(&logging_cfg) {
        return report_cli_error(&err, cli.log_json);
    }

    match cli.command {
        Commands::Doctor { json } => {
            let report = run_doctor_probe();
            let mut stdout = io::stdout().lock();
            if json {
                match serde_json::to_string_pretty(&report) {
                    Ok(rendered) => {
                        if let Err(err) = write_stream(&mut stdout, &rendered) {
                            let ff_err = FileFusionError::from(err);
                            return report_cli_error(&ff_err, true);
                        }
                        CliExitCode::Success
                    }
                    Err(err) => {
                        let ff_err = FileFusionError::new(
                            file_core::ErrorCode::IntInvariant,
                            format!("failed to serialize doctor report: {err}"),
                        );
                        report_cli_error(&ff_err, true)
                    }
                }
            } else if let Err(err) = write_stream(&mut stdout, &format_doctor_human(&report)) {
                let ff_err = FileFusionError::from(err);
                report_cli_error(&ff_err, false)
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
        let report = run_doctor_probe();
        let rendered = format_doctor_human(&report);
        assert!(rendered.contains("file-core"));
        assert!(rendered.contains("file-core-workspace"));
        assert!(rendered.contains("READY"));
    }
}
