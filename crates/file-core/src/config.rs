//! Versioned configuration management, precedence resolution, validation,
//! and atomic TOML persistence (`FF-CTR-001`, `FR-LOC-001`, `FR-AI-002`).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{ErrorCode, FileFusionError, FileFusionResult};
use crate::logging::{LogLevel, LoggingConfig};
use crate::{CORE_SCHEMA_VERSION, CliExitCode, ResourceLimits};

/// Destination collision handling policy for batch jobs (`FF-CTR-001` §4.1).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollisionPolicy {
    /// Leave existing destination files untouched and skip conflicting items.
    Skip,
    /// Append a deterministic numeric suffix before overwriting.
    #[default]
    RenameWithSuffix,
    /// Back up the existing destination before promoting staged output.
    OverwriteWithBackup,
}

/// Privacy and local-first enforcement defaults (`FR-LOC-001`, `FR-AI-002`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[allow(clippy::struct_excessive_bools)]
pub struct PrivacyConfig {
    /// Enforce local-only execution with zero outbound network or offload calls.
    pub offline_only: bool,
    /// Background telemetry toggle (disabled by default per `FR-AI-002`).
    pub telemetry_enabled: bool,
    /// Strip sensitive metadata (GPS, author, serials) during optimization by default.
    pub strip_metadata_by_default: bool,
    /// Redact parent directory paths from diagnostic log events.
    pub redact_paths_in_logs: bool,
    /// Explicit opt-in gate for external AI API usage (cannot be active while offline).
    pub external_ai_opt_in: bool,
}

impl Default for PrivacyConfig {
    fn default() -> Self {
        Self {
            offline_only: true,
            telemetry_enabled: false,
            strip_metadata_by_default: false,
            redact_paths_in_logs: true,
            external_ai_opt_in: false,
        }
    }
}

/// Default batch execution and naming policy (`FF-CTR-001` §4.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BatchDefaults {
    /// Behavior when a target output file already exists.
    pub collision_policy: CollisionPolicy,
    /// Output filename template supporting `{stem}`, `{ext}`, `{date}`, `{hash8}`, `{profile}`, `{index}`.
    pub naming_template: String,
    /// Preserve source modification timestamps when writing outputs.
    pub preserve_timestamps: bool,
    /// Retain original file untouched when same-format optimization does not shrink size (`FR-CMP-002`).
    pub no_op_if_larger: bool,
}

impl Default for BatchDefaults {
    fn default() -> Self {
        Self {
            collision_policy: CollisionPolicy::RenameWithSuffix,
            naming_template: String::from("{stem}.{ext}"),
            preserve_timestamps: true,
            no_op_if_larger: true,
        }
    }
}

/// Optional filesystem path overrides for local storage and staging.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PathConfig {
    /// Custom application data root directory.
    pub data_dir: Option<String>,
    /// Custom temporary staging directory.
    pub temp_dir: Option<String>,
    /// Custom local `SQLite` WAL database path.
    pub sqlite_path: Option<String>,
}

/// Canonical `FileFusion` runtime configuration (`schema_version = 1`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FileFusionConfig {
    /// Monotonic configuration schema version (`CORE_SCHEMA_VERSION`).
    pub schema_version: u32,
    /// Host resource and untrusted-input security bounds.
    pub limits: ResourceLimits,
    /// Structured logging configuration.
    pub logging: LoggingConfig,
    /// Local-first privacy and opt-in telemetry/AI gates.
    pub privacy: PrivacyConfig,
    /// Default batch collision, naming, and retention rules.
    pub batch: BatchDefaults,
    /// Optional data, staging, and database path overrides.
    pub paths: PathConfig,
}

impl Default for FileFusionConfig {
    fn default() -> Self {
        Self {
            schema_version: CORE_SCHEMA_VERSION,
            limits: ResourceLimits::default(),
            logging: LoggingConfig::default(),
            privacy: PrivacyConfig::default(),
            batch: BatchDefaults::default(),
            paths: PathConfig::default(),
        }
    }
}

/// Provenance describing where the base [`FileFusionConfig`] originated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigSource {
    /// Built-in host-aware defaults (no TOML file present).
    Default,
    /// Auto-discovered OS user configuration file.
    UserConfigFile(String),
    /// Explicit `--config` or `FILEFUSION_CONFIG` TOML path.
    ExplicitConfigFile(String),
}

/// Higher-precedence overrides typically supplied by CLI flags.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CliConfigOverrides {
    /// Explicit TOML file path (`--config`).
    pub config_path: Option<PathBuf>,
    /// Worker pool cap (`--workers`).
    pub workers: Option<usize>,
    /// Memory budget cap in megabytes (`--max-memory-mb`).
    pub max_memory_mb: Option<u64>,
    /// Explicit CLI verbosity override (`--log-level`).
    pub log_level: Option<LogLevel>,
    /// Structured JSON log toggle (`--log-json`).
    pub log_json: Option<bool>,
    /// Suppress non-error stderr diagnostics (`--quiet`).
    pub quiet: bool,
    /// Disable ANSI color output (`--no-color`).
    pub no_color: bool,
    /// Optional override for local-only mode.
    pub offline_only: Option<bool>,
    /// Optional override for path redaction in logs.
    pub redact_paths_in_logs: Option<bool>,
}

/// Resolved effective configuration along with provenance metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoadedConfig {
    /// Effective validated configuration after layering defaults, TOML, env, and CLI overrides.
    pub config: FileFusionConfig,
    /// Origin of the base TOML file or built-in defaults.
    pub source: ConfigSource,
    /// Names of `FILEFUSION_*` environment variables applied during resolution.
    pub applied_env_vars: Vec<String>,
    /// Names of CLI overrides applied on top of file/env settings.
    pub applied_cli_overrides: Vec<String>,
}

fn invalid_config_error(
    message: impl Into<String>,
    remediation: impl Into<String>,
) -> FileFusionError {
    FileFusionError::new(ErrorCode::IntInvariant, message)
        .with_exit_code(CliExitCode::InvalidUsage)
        .with_remediation(remediation)
}

fn parse_bool_flag(name: &str, raw: &str) -> FileFusionResult<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => Err(invalid_config_error(
            format!("invalid boolean value '{raw}' for {name}"),
            "Use one of: true, false, 1, 0.",
        )),
    }
}

fn parse_log_level(name: &str, raw: &str) -> FileFusionResult<LogLevel> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "error" => Ok(LogLevel::Error),
        "warn" => Ok(LogLevel::Warn),
        "info" => Ok(LogLevel::Info),
        "debug" => Ok(LogLevel::Debug),
        "trace" => Ok(LogLevel::Trace),
        _ => Err(invalid_config_error(
            format!("invalid log level '{raw}' for {name}"),
            "Use one of: error, warn, info, debug, trace.",
        )),
    }
}

fn validate_naming_template(template: &str) -> FileFusionResult<()> {
    let trimmed = template.trim();
    if trimmed.is_empty() {
        return Err(invalid_config_error(
            "batch.naming_template cannot be empty",
            "Provide a template such as '{stem}.{ext}'.",
        ));
    }
    if trimmed.contains('\0')
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed.split(['.', '/']).any(|seg| seg == "..")
        || trimmed.contains("..")
    {
        return Err(FileFusionError::new(
            ErrorCode::SecTraversal,
            "batch.naming_template cannot contain path separators, '..', or null bytes",
        )
        .with_remediation(
            "Keep naming_template as a filename pattern like '{stem}_{profile}.{ext}'.",
        ));
    }
    Ok(())
}

impl FileFusionConfig {
    /// Parses and validates a [`FileFusionConfig`] from a TOML string.
    ///
    /// # Errors
    /// Returns [`FileFusionError`] (with `CliExitCode::InvalidUsage`) if TOML syntax
    /// is malformed or semantic configuration checks fail.
    pub fn from_toml_str(raw: &str) -> FileFusionResult<Self> {
        let parsed: Self = toml::from_str(raw).map_err(|err| {
            FileFusionError::new(
                ErrorCode::FmtCorrupt,
                format!("failed to parse TOML configuration: {err}"),
            )
            .with_exit_code(CliExitCode::InvalidUsage)
            .with_remediation(
                "Check config.toml syntax or regenerate defaults with `filefusion config init`.",
            )
            .with_source(err)
        })?;
        parsed.validate()?;
        Ok(parsed)
    }

    /// Validates and serializes this configuration to a formatted TOML document.
    ///
    /// # Errors
    /// Returns [`FileFusionError`] if configuration validation or TOML serialization fails.
    pub fn to_toml_string_pretty(&self) -> FileFusionResult<String> {
        self.validate()?;
        toml::to_string_pretty(self).map_err(|err| {
            invalid_config_error(
                format!("failed to serialize configuration to TOML: {err}"),
                "Verify all configured fields are valid TOML values.",
            )
            .with_source(err)
        })
    }

    /// Validates resource bounds, privacy gates, and naming safety invariants.
    ///
    /// # Errors
    /// Returns [`FileFusionError`] when any configuration invariant is violated.
    pub fn validate(&self) -> FileFusionResult<()> {
        if self.schema_version != CORE_SCHEMA_VERSION {
            return Err(invalid_config_error(
                format!(
                    "unsupported configuration schema_version {} (expected {})",
                    self.schema_version, CORE_SCHEMA_VERSION
                ),
                "Update schema_version to 1 or run `filefusion config init --force`.",
            ));
        }
        if self.limits.max_workers == 0 {
            return Err(invalid_config_error(
                "limits.max_workers must be at least 1",
                "Set max_workers >= 1 via config.toml or --workers.",
            ));
        }
        if self.limits.max_memory_mb < 64 {
            return Err(invalid_config_error(
                format!(
                    "limits.max_memory_mb ({}) must be at least 64 MB",
                    self.limits.max_memory_mb
                ),
                "Set max_memory_mb >= 64 via config.toml or --max-memory-mb.",
            ));
        }
        if self.limits.max_expansion_ratio == 0 {
            return Err(invalid_config_error(
                "limits.max_expansion_ratio must be at least 1",
                "Use default 100 or higher positive integer ratio.",
            ));
        }
        if self.limits.max_uncompressed_bytes < 1024 {
            return Err(invalid_config_error(
                "limits.max_uncompressed_bytes must be at least 1024 bytes",
                "Set a larger byte limit or use the 4 GiB default.",
            ));
        }
        if self.limits.max_archive_depth == 0 {
            return Err(invalid_config_error(
                "limits.max_archive_depth must be at least 1",
                "Set max_archive_depth >= 1 (default 3).",
            ));
        }
        if self.limits.subprocess_timeout_secs == 0 {
            return Err(invalid_config_error(
                "limits.subprocess_timeout_secs must be at least 1 second",
                "Set a positive timeout such as 1800 seconds.",
            ));
        }
        if self.privacy.offline_only && self.privacy.external_ai_opt_in {
            return Err(invalid_config_error(
                "privacy.external_ai_opt_in cannot be true while privacy.offline_only is enabled",
                "Keep external_ai_opt_in = false or set offline_only = false explicitly.",
            ));
        }
        if self.privacy.offline_only && self.privacy.telemetry_enabled {
            return Err(invalid_config_error(
                "privacy.telemetry_enabled cannot be true while privacy.offline_only is enabled",
                "Set telemetry_enabled = false to preserve offline-by-default privacy.",
            ));
        }
        validate_naming_template(&self.batch.naming_template)?;
        Ok(())
    }

    /// Applies `FILEFUSION_*` environment overrides using an injected variable lookup.
    ///
    /// # Errors
    /// Returns [`FileFusionError`] if any supplied environment variable cannot be parsed.
    pub fn apply_env_lookup<F>(&mut self, mut get_var: F) -> FileFusionResult<Vec<String>>
    where
        F: FnMut(&str) -> Option<String>,
    {
        let mut applied = Vec::new();
        self.apply_env_limits(&mut get_var, &mut applied)?;
        self.apply_env_logging_and_privacy(&mut get_var, &mut applied)?;
        self.apply_env_paths(&mut get_var, &mut applied);
        Ok(applied)
    }

    fn apply_env_limits<F>(
        &mut self,
        get_var: &mut F,
        applied: &mut Vec<String>,
    ) -> FileFusionResult<()>
    where
        F: FnMut(&str) -> Option<String>,
    {
        if let Some(raw) = get_var("FILEFUSION_WORKERS") {
            let workers = raw.trim().parse::<usize>().map_err(|err| {
                invalid_config_error(
                    format!("invalid integer '{raw}' for FILEFUSION_WORKERS: {err}"),
                    "Provide a positive integer worker count such as 4.",
                )
            })?;
            self.limits.max_workers = workers;
            applied.push(String::from("FILEFUSION_WORKERS"));
        }
        if let Some(raw) = get_var("FILEFUSION_MAX_MEMORY_MB") {
            let mb = raw.trim().parse::<u64>().map_err(|err| {
                invalid_config_error(
                    format!("invalid integer '{raw}' for FILEFUSION_MAX_MEMORY_MB: {err}"),
                    "Provide a memory limit in MB such as 1024.",
                )
            })?;
            self.limits.max_memory_mb = mb;
            applied.push(String::from("FILEFUSION_MAX_MEMORY_MB"));
        }
        if let Some(raw) = get_var("FILEFUSION_MAX_EXPANSION_RATIO") {
            let ratio = raw.trim().parse::<u32>().map_err(|err| {
                invalid_config_error(
                    format!("invalid integer '{raw}' for FILEFUSION_MAX_EXPANSION_RATIO: {err}"),
                    "Provide a positive integer ratio such as 100.",
                )
            })?;
            self.limits.max_expansion_ratio = ratio;
            applied.push(String::from("FILEFUSION_MAX_EXPANSION_RATIO"));
        }
        if let Some(raw) = get_var("FILEFUSION_MAX_ARCHIVE_DEPTH") {
            let depth = raw.trim().parse::<u8>().map_err(|err| {
                invalid_config_error(
                    format!("invalid integer '{raw}' for FILEFUSION_MAX_ARCHIVE_DEPTH: {err}"),
                    "Provide an integer depth between 1 and 255.",
                )
            })?;
            self.limits.max_archive_depth = depth;
            applied.push(String::from("FILEFUSION_MAX_ARCHIVE_DEPTH"));
        }
        Ok(())
    }

    fn apply_env_logging_and_privacy<F>(
        &mut self,
        get_var: &mut F,
        applied: &mut Vec<String>,
    ) -> FileFusionResult<()>
    where
        F: FnMut(&str) -> Option<String>,
    {
        if let Some(raw) = get_var("FILEFUSION_LOG_LEVEL") {
            self.logging.level = parse_log_level("FILEFUSION_LOG_LEVEL", &raw)?;
            applied.push(String::from("FILEFUSION_LOG_LEVEL"));
        }
        if let Some(raw) = get_var("FILEFUSION_LOG_JSON") {
            self.logging.format = if parse_bool_flag("FILEFUSION_LOG_JSON", &raw)? {
                crate::logging::LogFormat::Json
            } else {
                crate::logging::LogFormat::Pretty
            };
            applied.push(String::from("FILEFUSION_LOG_JSON"));
        }
        if let Some(raw) = get_var("NO_COLOR")
            && !raw.is_empty()
        {
            self.logging.ansi_color = false;
            applied.push(String::from("NO_COLOR"));
        }
        if let Some(raw) = get_var("FILEFUSION_OFFLINE_ONLY") {
            self.privacy.offline_only = parse_bool_flag("FILEFUSION_OFFLINE_ONLY", &raw)?;
            applied.push(String::from("FILEFUSION_OFFLINE_ONLY"));
        }
        if let Some(raw) = get_var("FILEFUSION_REDACT_PATHS") {
            self.privacy.redact_paths_in_logs = parse_bool_flag("FILEFUSION_REDACT_PATHS", &raw)?;
            applied.push(String::from("FILEFUSION_REDACT_PATHS"));
        }
        Ok(())
    }

    fn apply_env_paths<F>(&mut self, get_var: &mut F, applied: &mut Vec<String>)
    where
        F: FnMut(&str) -> Option<String>,
    {
        if let Some(raw) = get_var("FILEFUSION_DATA_DIR") {
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                self.paths.data_dir = Some(trimmed.to_owned());
                applied.push(String::from("FILEFUSION_DATA_DIR"));
            }
        }
        if let Some(raw) = get_var("FILEFUSION_TEMP_DIR") {
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                self.paths.temp_dir = Some(trimmed.to_owned());
                applied.push(String::from("FILEFUSION_TEMP_DIR"));
            }
        }
    }

    /// Applies highest-precedence CLI/caller overrides cleanly onto this configuration.
    pub fn apply_cli_overrides(&mut self, overrides: &CliConfigOverrides) -> Vec<String> {
        let mut applied = Vec::new();
        if let Some(workers) = overrides.workers {
            self.limits.max_workers = workers;
            applied.push(String::from("workers"));
        }
        if let Some(max_memory_mb) = overrides.max_memory_mb {
            self.limits.max_memory_mb = max_memory_mb;
            applied.push(String::from("max_memory_mb"));
        }
        if let Some(level) = overrides.log_level {
            self.logging.level = level;
            applied.push(String::from("log_level"));
        } else if overrides.quiet {
            self.logging.level = LogLevel::Error;
            applied.push(String::from("quiet"));
        }
        if let Some(log_json) = overrides.log_json {
            self.logging.format = if log_json {
                crate::logging::LogFormat::Json
            } else {
                crate::logging::LogFormat::Pretty
            };
            applied.push(String::from("log_json"));
        }
        if overrides.no_color {
            self.logging.ansi_color = false;
            applied.push(String::from("no_color"));
        }
        if let Some(offline_only) = overrides.offline_only {
            self.privacy.offline_only = offline_only;
            applied.push(String::from("offline_only"));
        }
        if let Some(redact_paths) = overrides.redact_paths_in_logs {
            self.privacy.redact_paths_in_logs = redact_paths;
            applied.push(String::from("redact_paths_in_logs"));
        }
        applied
    }
}

/// Resolves the default platform `config.toml` path using an injected env lookup.
#[must_use]
pub fn resolve_default_config_path_with<F>(mut get_var: F) -> Option<PathBuf>
where
    F: FnMut(&str) -> Option<String>,
{
    if let Some(explicit) = get_var("FILEFUSION_CONFIG") {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }

    if cfg!(target_os = "windows") {
        get_var("APPDATA")
            .filter(|v| !v.trim().is_empty())
            .map(|appdata| {
                PathBuf::from(appdata)
                    .join("FileFusion")
                    .join("config.toml")
            })
    } else if cfg!(target_os = "macos") {
        get_var("HOME")
            .filter(|v| !v.trim().is_empty())
            .map(|home| {
                PathBuf::from(home)
                    .join("Library")
                    .join("Application Support")
                    .join("FileFusion")
                    .join("config.toml")
            })
    } else {
        get_var("XDG_CONFIG_HOME")
            .filter(|v| !v.trim().is_empty())
            .map(|xdg| PathBuf::from(xdg).join("filefusion").join("config.toml"))
            .or_else(|| {
                get_var("HOME")
                    .filter(|v| !v.trim().is_empty())
                    .map(|home| {
                        PathBuf::from(home)
                            .join(".config")
                            .join("filefusion")
                            .join("config.toml")
                    })
            })
    }
}

/// Resolves the default platform `config.toml` path from host environment variables.
#[must_use]
pub fn resolve_default_config_path() -> Option<PathBuf> {
    resolve_default_config_path_with(|key| std::env::var(key).ok())
}

/// Loads and validates a [`FileFusionConfig`] directly from a TOML file path.
///
/// # Errors
/// Returns [`FileFusionError`] if the file cannot be read or fails TOML/schema validation.
pub fn load_config_from_path(path: &Path) -> FileFusionResult<FileFusionConfig> {
    let raw = fs::read_to_string(path).map_err(|err| {
        let ff_err = FileFusionError::from(err);
        if ff_err.diagnostic.code == ErrorCode::IoNotFound {
            ff_err.with_exit_code(CliExitCode::InvalidUsage)
        } else {
            ff_err
        }
    })?;
    FileFusionConfig::from_toml_str(&raw)
}

/// Atomically writes a validated [`FileFusionConfig`] to disk using a sibling staging file.
///
/// # Errors
/// Returns [`FileFusionError`] if validation fails, `overwrite` is `false` and the target
/// exists, or filesystem write/rename fails.
pub fn save_config_atomic(
    path: &Path,
    config: &FileFusionConfig,
    overwrite: bool,
) -> FileFusionResult<()> {
    config.validate()?;
    if !overwrite && path.exists() {
        return Err(invalid_config_error(
            "configuration file already exists",
            "Pass --force to overwrite the existing configuration file.",
        ));
    }

    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;

    let toml_text = config.to_toml_string_pretty()?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("config.toml");
    let temp_path = parent.join(format!(".{file_name}.tmp.{}", std::process::id()));

    let write_result = (|| -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&temp_path)?;
        file.write_all(toml_text.as_bytes())?;
        file.sync_all()?;
        Ok(())
    })();

    if let Err(err) = write_result {
        let _ = fs::remove_file(&temp_path);
        return Err(FileFusionError::from(err));
    }

    if let Err(err) = fs::rename(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(FileFusionError::from(err));
    }

    Ok(())
}

/// Resolves the effective configuration using host defaults, TOML file, `FILEFUSION_*`
/// environment variables (via injected lookup), and CLI overrides.
///
/// # Errors
/// Returns [`FileFusionError`] if an explicit config path is missing/malformed or
/// resulting values violate configuration invariants.
pub fn load_effective_config_with<F>(
    overrides: &CliConfigOverrides,
    mut get_var: F,
) -> FileFusionResult<LoadedConfig>
where
    F: FnMut(&str) -> Option<String>,
{
    let (mut config, source) = if let Some(explicit_path) = &overrides.config_path {
        let loaded = load_config_from_path(explicit_path)?;
        (
            loaded,
            ConfigSource::ExplicitConfigFile(explicit_path.to_string_lossy().into_owned()),
        )
    } else if let Some(env_config) = get_var("FILEFUSION_CONFIG").filter(|v| !v.trim().is_empty()) {
        let path = PathBuf::from(&env_config);
        let loaded = load_config_from_path(&path)?;
        (loaded, ConfigSource::ExplicitConfigFile(env_config))
    } else if let Some(default_path) = resolve_default_config_path_with(&mut get_var) {
        if default_path.exists() {
            let loaded = load_config_from_path(&default_path)?;
            (
                loaded,
                ConfigSource::UserConfigFile(default_path.to_string_lossy().into_owned()),
            )
        } else {
            (FileFusionConfig::default(), ConfigSource::Default)
        }
    } else {
        (FileFusionConfig::default(), ConfigSource::Default)
    };

    let applied_env_vars = config.apply_env_lookup(&mut get_var)?;
    let applied_cli_overrides = config.apply_cli_overrides(overrides);
    config.validate()?;

    Ok(LoadedConfig {
        config,
        source,
        applied_env_vars,
        applied_cli_overrides,
    })
}

/// Resolves the effective configuration using the host process environment.
///
/// # Errors
/// Returns [`FileFusionError`] if an explicit config path fails to load or validation fails.
pub fn load_effective_config(overrides: &CliConfigOverrides) -> FileFusionResult<LoadedConfig> {
    load_effective_config_with(overrides, |key| std::env::var(key).ok())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn unique_temp_dir(label: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let dir = std::env::temp_dir().join(format!(
            "filefusion_cfg_test_{label}_{}",
            std::process::id()
        ));
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    #[test]
    fn partial_toml_merges_with_secure_defaults() -> Result<(), Box<dyn std::error::Error>> {
        let raw = r#"
[limits]
max_workers = 5
max_memory_mb = 2048

[batch]
collision_policy = "skip"
"#;
        let cfg = FileFusionConfig::from_toml_str(raw)?;
        assert_eq!(cfg.schema_version, CORE_SCHEMA_VERSION);
        assert_eq!(cfg.limits.max_workers, 5);
        assert_eq!(cfg.limits.max_memory_mb, 2048);
        assert_eq!(cfg.limits.max_expansion_ratio, 100);
        assert_eq!(cfg.batch.collision_policy, CollisionPolicy::Skip);
        assert!(cfg.privacy.offline_only);
        assert!(!cfg.privacy.telemetry_enabled);
        Ok(())
    }

    #[test]
    fn precedence_layers_toml_env_and_cli_overrides() -> Result<(), Box<dyn std::error::Error>> {
        let dir = unique_temp_dir("precedence")?;
        let config_path = dir.join("config.toml");

        let mut base = FileFusionConfig::default();
        base.limits.max_workers = 2;
        base.limits.max_memory_mb = 1024;
        save_config_atomic(&config_path, &base, false)?;

        let env_map = BTreeMap::from([
            ("FILEFUSION_WORKERS", "6"),
            ("FILEFUSION_MAX_MEMORY_MB", "1536"),
            ("FILEFUSION_LOG_JSON", "true"),
        ]);
        let overrides = CliConfigOverrides {
            config_path: Some(config_path.clone()),
            workers: Some(4),
            ..CliConfigOverrides::default()
        };

        let loaded =
            load_effective_config_with(&overrides, |k| env_map.get(k).map(|v| (*v).to_owned()))?;

        assert_eq!(loaded.config.limits.max_workers, 4);
        assert_eq!(loaded.config.limits.max_memory_mb, 1536);
        assert_eq!(
            loaded.config.logging.format,
            crate::logging::LogFormat::Json
        );
        assert!(matches!(loaded.source, ConfigSource::ExplicitConfigFile(_)));
        assert_eq!(loaded.applied_cli_overrides, vec!["workers"]);

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn validation_rejects_unsafe_template_offline_ai_and_zero_workers() {
        let mut bad_workers = FileFusionConfig::default();
        bad_workers.limits.max_workers = 0;
        let workers_err = bad_workers.validate();
        assert!(workers_err.is_err());
        if let Err(err) = workers_err {
            assert_eq!(err.exit_code(), CliExitCode::InvalidUsage);
        }

        let mut bad_template = FileFusionConfig::default();
        bad_template.batch.naming_template = String::from("../escape/{stem}.{ext}");
        let tpl_err = bad_template.validate();
        assert!(tpl_err.is_err());
        if let Err(err) = tpl_err {
            assert_eq!(err.diagnostic.code, ErrorCode::SecTraversal);
            assert_eq!(err.exit_code(), CliExitCode::SecurityRejection);
        }

        let mut conflicting_ai = FileFusionConfig::default();
        conflicting_ai.privacy.offline_only = true;
        conflicting_ai.privacy.external_ai_opt_in = true;
        let ai_err = conflicting_ai.validate();
        assert!(ai_err.is_err());
        if let Err(err) = ai_err {
            assert_eq!(err.exit_code(), CliExitCode::InvalidUsage);
        }
    }

    #[test]
    fn atomic_save_enforces_overwrite_flag() -> Result<(), Box<dyn std::error::Error>> {
        let dir = unique_temp_dir("atomic")?;
        let path = dir.join("nested").join("config.toml");
        let mut cfg = FileFusionConfig::default();
        cfg.limits.max_workers = 3;

        save_config_atomic(&path, &cfg, false)?;
        let second = save_config_atomic(&path, &cfg, false);
        assert!(second.is_err());

        cfg.limits.max_workers = 4;
        save_config_atomic(&path, &cfg, true)?;
        let reloaded = load_config_from_path(&path)?;
        assert_eq!(reloaded.limits.max_workers, 4);

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }
}
