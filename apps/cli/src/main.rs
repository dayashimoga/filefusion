//! `filefusion` CLI entrypoint (`apps/cli`).
//!
//! Thin Plane 1/2 adapter over `job-manager` (`MOD-RULE-03`) with versioned
//! configuration resolution and structured `tracing` diagnostics routed strictly
//! to `stderr`.

#![forbid(unsafe_code)]

use std::fmt::Write as _;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use file_core::{
    CORE_SCHEMA_VERSION, CliConfigOverrides, CliExitCode, ErrorCode, FileFusionError, LogFormat,
    LogLevel, init_logging, resolve_default_config_path, save_config_atomic, workspace_identity,
};
use job_manager::{DoctorReport, load_runtime_config, run_doctor_probe_with_config};
use serde::Serialize;

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
    /// Path to an explicit TOML configuration file (`FILEFUSION_CONFIG`).
    #[arg(long, global = true, value_name = "PATH")]
    config: Option<PathBuf>,

    /// Maximum concurrent CPU workers (`FILEFUSION_WORKERS`).
    #[arg(long, global = true, value_name = "N")]
    workers: Option<usize>,

    /// Global working-set RAM budget in MB (`FILEFUSION_MAX_MEMORY_MB`).
    #[arg(long, global = true, value_name = "MB")]
    max_memory_mb: Option<u64>,

    /// Suppress non-error stderr diagnostics.
    #[arg(short, long, global = true)]
    quiet: bool,

    /// Disable ANSI color sequences in stderr logs and terminal output.
    #[arg(long, global = true)]
    no_color: bool,

    /// Minimum log level emitted to stderr.
    #[arg(long, global = true, value_enum)]
    log_level: Option<CliLogLevel>,

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
    /// Manage `FileFusion` TOML configuration, defaults, and precedence resolution.
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Debug, Subcommand)]
enum ConfigAction {
    /// Display the resolved effective configuration (defaults -> TOML -> env -> CLI).
    Show {
        /// Emit machine-readable JSON output including provenance metadata.
        #[arg(long)]
        json: bool,
    },
    /// Print the resolved default configuration file path for this platform.
    Path {
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Write an initial or updated `config.toml` file atomically.
    Init {
        /// Overwrite an existing configuration file.
        #[arg(long)]
        force: bool,
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Validate active configuration syntax, limits, and privacy/security invariants.
    Validate {
        /// Emit machine-readable JSON output.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Serialize)]
struct ConfigStatusOutput {
    schema_version: u32,
    ok: bool,
    path: Option<String>,
    action: &'static str,
}

fn format_doctor_human(report: &DoctorReport) -> String {
    let mut out = format!(
        "FileFusion ({}) — Doctor Diagnostics\nHost: {} ({}) | Logical CPUs: {}\nDefault ResourceLimits: max_workers={}, max_memory_mb={} MB, max_expansion_ratio={}:1, max_archive_depth={}\nEffective ResourceLimits: max_workers={}, max_memory_mb={} MB, max_expansion_ratio={}:1, max_archive_depth={}\nCapabilities:\n",
        workspace_identity(),
        report.host_os,
        report.host_arch,
        report.logical_cpus,
        report.default_limits.max_workers,
        report.default_limits.max_memory_mb,
        report.default_limits.max_expansion_ratio,
        report.default_limits.max_archive_depth,
        report.effective_limits.max_workers,
        report.effective_limits.max_memory_mb,
        report.effective_limits.max_expansion_ratio,
        report.effective_limits.max_archive_depth,
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

fn emit_text_stdout(content: &str, json_err_mode: bool) -> CliExitCode {
    let mut stdout = io::stdout().lock();
    if let Err(err) = write_stream(&mut stdout, content) {
        let ff_err = FileFusionError::from(err);
        return report_cli_error(&ff_err, json_err_mode);
    }
    CliExitCode::Success
}

fn emit_json_stdout<T: Serialize>(value: &T) -> CliExitCode {
    match serde_json::to_string_pretty(value) {
        Ok(rendered) => emit_text_stdout(&rendered, true),
        Err(err) => {
            let ff_err = FileFusionError::new(
                ErrorCode::IntInvariant,
                format!("failed to serialize JSON output: {err}"),
            );
            report_cli_error(&ff_err, true)
        }
    }
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

const fn command_requests_json(command: &Commands) -> bool {
    match command {
        Commands::Doctor { json } => *json,
        Commands::Config { action } => match action {
            ConfigAction::Show { json }
            | ConfigAction::Path { json }
            | ConfigAction::Init { json, .. }
            | ConfigAction::Validate { json } => *json,
        },
    }
}

fn handle_config_command(action: &ConfigAction, overrides: &CliConfigOverrides) -> CliExitCode {
    match action {
        ConfigAction::Path { json } => {
            let target = overrides
                .config_path
                .clone()
                .or_else(resolve_default_config_path);
            let path_str = target.map(|p| p.to_string_lossy().into_owned());
            if *json {
                emit_json_stdout(&ConfigStatusOutput {
                    schema_version: CORE_SCHEMA_VERSION,
                    ok: path_str.is_some(),
                    path: path_str,
                    action: "path",
                })
            } else {
                let text = path_str.unwrap_or_else(|| String::from("<unresolved>"));
                emit_text_stdout(&text, false)
            }
        }
        ConfigAction::Show { json } => match load_runtime_config(overrides) {
            Ok(loaded) => {
                if *json {
                    emit_json_stdout(&loaded)
                } else {
                    match loaded.config.to_toml_string_pretty() {
                        Ok(toml_str) => emit_text_stdout(&toml_str, false),
                        Err(err) => report_cli_error(&err, false),
                    }
                }
            }
            Err(err) => report_cli_error(&err, *json),
        },
        ConfigAction::Init { force, json } => {
            let Some(target_path) = overrides
                .config_path
                .clone()
                .or_else(resolve_default_config_path)
            else {
                let err = FileFusionError::new(
                    ErrorCode::IoNotFound,
                    "unable to determine platform configuration path",
                )
                .with_exit_code(CliExitCode::InvalidUsage)
                .with_remediation("Pass an explicit path using `--config <PATH>`.");
                return report_cli_error(&err, *json);
            };
            let mut cfg = file_core::FileFusionConfig::default();
            cfg.apply_cli_overrides(overrides);
            if let Err(err) = save_config_atomic(&target_path, &cfg, *force) {
                return report_cli_error(&err, *json);
            }
            let rendered_path = target_path.to_string_lossy().into_owned();
            if *json {
                emit_json_stdout(&ConfigStatusOutput {
                    schema_version: CORE_SCHEMA_VERSION,
                    ok: true,
                    path: Some(rendered_path),
                    action: "init",
                })
            } else {
                emit_text_stdout(
                    &format!("Initialized configuration at {rendered_path}"),
                    false,
                )
            }
        }
        ConfigAction::Validate { json } => match load_runtime_config(overrides) {
            Ok(loaded) => {
                let source_path = match loaded.source {
                    file_core::ConfigSource::Default => None,
                    file_core::ConfigSource::UserConfigFile(path)
                    | file_core::ConfigSource::ExplicitConfigFile(path) => Some(path),
                };
                if *json {
                    emit_json_stdout(&ConfigStatusOutput {
                        schema_version: CORE_SCHEMA_VERSION,
                        ok: true,
                        path: source_path,
                        action: "validate",
                    })
                } else {
                    emit_text_stdout("Configuration is valid.", false)
                }
            }
            Err(err) => report_cli_error(&err, *json),
        },
    }
}

fn run() -> CliExitCode {
    let cli = Cli::parse();
    let wants_json = command_requests_json(&cli.command);

    let overrides = CliConfigOverrides {
        config_path: cli.config.clone(),
        workers: cli.workers,
        max_memory_mb: cli.max_memory_mb,
        log_level: cli.log_level.map(LogLevel::from),
        log_json: cli.log_json.then_some(true),
        quiet: cli.quiet,
        no_color: cli.no_color,
        offline_only: None,
        redact_paths_in_logs: None,
    };

    if let Commands::Config { action } = &cli.command {
        // Initialize logging best-effort for config subcommands before handling validation errors.
        let fallback_logging = file_core::LoggingConfig {
            level: overrides.log_level.unwrap_or(if overrides.quiet {
                LogLevel::Error
            } else {
                LogLevel::Warn
            }),
            format: if cli.log_json {
                LogFormat::Json
            } else {
                LogFormat::Pretty
            },
            respect_env: true,
            include_target: false,
            ansi_color: !cli.no_color,
        };
        let _ = init_logging(&fallback_logging);
        return handle_config_command(action, &overrides);
    }

    let loaded = match load_runtime_config(&overrides) {
        Ok(loaded) => loaded,
        Err(err) => return report_cli_error(&err, wants_json || cli.log_json),
    };
    if let Err(err) = init_logging(&loaded.config.logging) {
        return report_cli_error(&err, wants_json || cli.log_json);
    }

    match cli.command {
        Commands::Doctor { json } => {
            let report = run_doctor_probe_with_config(&loaded.config);
            if json {
                emit_json_stdout(&report)
            } else {
                emit_text_stdout(&format_doctor_human(&report), false)
            }
        }
        Commands::Config { .. } => CliExitCode::Success,
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
        let report = job_manager::run_doctor_probe();
        let rendered = format_doctor_human(&report);
        assert!(rendered.contains("file-core"));
        assert!(rendered.contains("file-core-workspace"));
        assert!(rendered.contains("Effective ResourceLimits"));
        assert!(rendered.contains("READY"));
    }
}
