//! Privacy-aware structured `tracing` initialization for `FileFusion`.
//!
//! Logs are routed strictly to `stderr` so `stdout` remains clean for `--json`
//! machine output and piped streams.

use serde::{Deserialize, Serialize};

use crate::error::FileFusionResult;
#[cfg(feature = "native-logging")]
use crate::error::{ErrorCode, FileFusionError};

/// Supported log verbosity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    /// Only fatal/error events.
    Error,
    /// Warnings and errors.
    Warn,
    /// Operational lifecycle info (default).
    Info,
    /// Detailed pipeline and codec diagnostics.
    Debug,
    /// Verbose low-level tracing.
    Trace,
}

impl LogLevel {
    /// Returns the `tracing_subscriber::EnvFilter` directive string.
    #[must_use]
    pub const fn as_directive(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }
}

/// Output formatting mode for structured diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    /// Human-readable compact lines on `stderr`.
    Pretty,
    /// Machine-parseable JSON lines on `stderr`.
    Json,
}

/// Configuration for initializing the workspace `tracing` subscriber.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Minimum verbosity level (overridable via `RUST_LOG` when `respect_env` is `true`).
    pub level: LogLevel,
    /// Log formatting mode.
    pub format: LogFormat,
    /// Whether to honor `RUST_LOG` environment filter overrides.
    pub respect_env: bool,
    /// Whether to include target module paths in log entries.
    pub include_target: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: LogLevel::Warn,
            format: LogFormat::Pretty,
            respect_env: true,
            include_target: false,
        }
    }
}

/// Redacts sensitive directory prefixes from paths before logging (`SECURITY.md`).
///
/// Retains only the trailing filename component to avoid leaking user home folders
/// or confidential directory structures into diagnostic logs.
#[must_use]
pub fn redact_path_for_log(raw_path: &str) -> String {
    let trimmed = raw_path.trim();
    if trimmed.is_empty() {
        return String::from("<empty>");
    }
    let normalized = trimmed.replace('\\', "/");
    match normalized.rsplit('/').find(|seg| !seg.is_empty()) {
        Some(file_name) if normalized.contains('/') => format!("<redacted>/{file_name}"),
        Some(file_name) => file_name.to_owned(),
        None => String::from("<redacted>"),
    }
}

/// Initializes the global `tracing` subscriber on `stderr`.
///
/// Idempotent: if a global subscriber is already installed (e.g. in test harnesses),
/// subsequent calls return `Ok(())` without panicking.
///
/// # Errors
/// Returns [`FileFusionError`] if an invalid `RUST_LOG` filter directive cannot be parsed.
#[cfg(feature = "native-logging")]
pub fn init_logging(config: &LoggingConfig) -> FileFusionResult<()> {
    use tracing_subscriber::EnvFilter;

    let filter = if config.respect_env {
        match std::env::var("RUST_LOG") {
            Ok(raw) => EnvFilter::try_new(&raw).map_err(|err| {
                FileFusionError::new(
                    ErrorCode::IntInvariant,
                    format!("invalid RUST_LOG filter '{raw}': {err}"),
                )
                .with_remediation(
                    "Set RUST_LOG to a valid level such as info, debug, or file_core=debug.",
                )
            })?,
            Err(std::env::VarError::NotPresent) => EnvFilter::new(config.level.as_directive()),
            Err(err) => {
                return Err(FileFusionError::new(
                    ErrorCode::IntInvariant,
                    format!("invalid RUST_LOG environment value: {err}"),
                )
                .with_remediation(
                    "Set RUST_LOG to a valid UTF-8 level such as info, debug, or file_core=debug.",
                ));
            }
        }
    } else {
        EnvFilter::new(config.level.as_directive())
    };

    let init_res = match config.format {
        LogFormat::Pretty => tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(config.include_target)
            .with_writer(std::io::stderr)
            .try_init(),
        LogFormat::Json => tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(config.include_target)
            .json()
            .with_writer(std::io::stderr)
            .try_init(),
    };

    // Ignore "already initialized" errors so repeated library/test setups stay idempotent.
    let _ = init_res;
    Ok(())
}

/// No-op fallback for `wasm32` targets compiled without `native-logging`.
///
/// # Errors
/// Always returns `Ok(())`.
#[cfg(not(feature = "native-logging"))]
pub fn init_logging(_config: &LoggingConfig) -> FileFusionResult<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_path_hides_user_directories_while_keeping_filename() {
        assert_eq!(
            redact_path_for_log(r"C:\Users\alice\Documents\secret-contract.pdf"),
            "<redacted>/secret-contract.pdf"
        );
        assert_eq!(
            redact_path_for_log("/home/alice/photos/img_01.png"),
            "<redacted>/img_01.png"
        );
        assert_eq!(redact_path_for_log("report.docx"), "report.docx");
        assert_eq!(redact_path_for_log("   "), "<empty>");
    }

    #[test]
    fn init_logging_is_idempotent() -> FileFusionResult<()> {
        let cfg = LoggingConfig {
            level: LogLevel::Info,
            format: LogFormat::Json,
            respect_env: false,
            include_target: true,
        };
        init_logging(&cfg)?;
        init_logging(&cfg)?;
        Ok(())
    }
}
