//! Stage 6 reusable optimization presets and local workflow schedule registry
//! (`FR-LOC-002`, `FR-BAT-002`, `FF-SCP-001` Stage 6).
//!
//! Stores versioned presets and local scheduled workflow definitions atomically in JSON/YAML
//! via `StagedPath`, and evaluates 5-field UTC cron expressions plus fixed-interval triggers
//! deterministically offline (`FR-LOC-001`).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use conversion_core::tabular::{convert_structured_data, parse_structured_value};
use file_core::{
    CORE_SCHEMA_VERSION, CollisionPolicy, CompressionProfile, ErrorCode, FileFusionError,
    FileFusionResult, FormatId, StagedPath,
};
use serde::{Deserialize, Serialize};

use crate::JobManager;

/// High-level processing domain targeted by an [`OptimizationPreset`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresetDomain {
    /// Image compression/transcoding preset.
    Image,
    /// Archive packing/recompression preset.
    Archive,
    /// PDF optimization/security/OCR preset.
    Pdf,
    /// Cross-format document/media conversion preset.
    Conversion,
    /// Audio normalization/transcoding preset.
    Audio,
    /// Video transcoding/remuxing preset.
    Video,
    /// Metadata inspection/privacy-scrubbing preset.
    Metadata,
    /// Storage duplicate/quarantine cleanup preset.
    Storage,
}

/// Reusable parameter configuration stored inside an [`OptimizationPreset`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PresetParameters {
    /// Standard compression profile tier.
    pub profile: CompressionProfile,
    /// Optional target output format override.
    pub target_format: Option<FormatId>,
    /// Strip non-essential privacy metadata before output commit.
    pub strip_metadata: bool,
    /// Enforce no-op retention if same-format output does not shrink (`FR-CMP-002`).
    pub no_op_if_larger: bool,
    /// Optional target maximum output size in bytes.
    pub target_max_bytes: Option<u64>,
    /// Optional perceptual minimum SSIM floor (`0.0..=1.0`).
    pub min_ssim: Option<f64>,
    /// Output naming template (`{stem}`, `{ext}`, `{date}`, `{hash8}`, `{profile}`, `{index}`).
    pub naming_template: String,
    /// Preserve source subdirectories relative to batch root (`FR-BAT-002`).
    pub mirror_directories: bool,
    /// Collision resolution strategy for existing output files.
    pub collision_policy: CollisionPolicy,
}

impl Default for PresetParameters {
    fn default() -> Self {
        Self {
            profile: CompressionProfile::BalancedOptimal,
            target_format: None,
            strip_metadata: false,
            no_op_if_larger: true,
            target_max_bytes: None,
            min_ssim: None,
            naming_template: String::from("{stem}.{ext}"),
            mirror_directories: false,
            collision_policy: CollisionPolicy::RenameWithSuffix,
        }
    }
}

/// Portable, versioned reusable preset record (`docs/CONTRACTS.md` §4.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OptimizationPreset {
    /// Machine-readable JSON/YAML schema version.
    pub schema_version: u32,
    /// Stable slug identifier (ASCII alphanumeric, `-`, `_`).
    pub preset_id: String,
    /// Human-readable preset title.
    pub name: String,
    /// Concise summary of the preset's intent and tradeoffs.
    pub description: String,
    /// Target processing domain.
    pub domain: PresetDomain,
    /// `true` for factory-shipped immutable presets.
    pub is_builtin: bool,
    /// Preset configuration parameters.
    pub config: PresetParameters,
    /// UNIX creation timestamp in seconds.
    pub created_unix_s: u64,
    /// UNIX last-updated timestamp in seconds.
    pub updated_unix_s: u64,
}

/// Execution cadence trigger for a local scheduled workflow task (`FF-CTR-001` §4.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScheduleTrigger {
    /// Standard 5-field UTC cron expression (`minute hour day-of-month month day-of-week`).
    CronUtc {
        /// 5-field space-separated UTC cron expression (e.g. `"0 2 * * *"`).
        expression: String,
    },
    /// Fixed interval trigger firing every `every_seconds` seconds (`>= 1`).
    IntervalSeconds {
        /// Minimum seconds elapsed between runs.
        every_seconds: u64,
    },
}

/// Persisted scheduled local automation task referencing a workflow definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduledWorkflowTask {
    /// Machine-readable JSON schema version.
    pub schema_version: u32,
    /// Unique task identifier (ASCII alphanumeric, `-`, `_`).
    pub task_id: String,
    /// Workflow definition identifier or file path executed when due.
    pub workflow_id: String,
    /// Human-readable schedule summary.
    pub description: String,
    /// Cadence trigger (UTC cron or interval).
    pub trigger: ScheduleTrigger,
    /// Input files/directories supplied to the workflow when triggered.
    pub input_roots: Vec<PathBuf>,
    /// Destination directory where mirrored/templated outputs are written.
    pub output_dir: Option<PathBuf>,
    /// Whether this task is eligible to fire.
    pub enabled: bool,
    /// UNIX timestamp (seconds) of the last completed execution, if any.
    pub last_run_unix_s: Option<u64>,
}

/// Evaluation entry describing a due scheduled task at evaluation time `now_unix_s`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduledTaskDue {
    /// Task identifier.
    pub task_id: String,
    /// Referenced workflow identifier.
    pub workflow_id: String,
    /// Trigger reason explanation.
    pub reason: String,
}

fn make_builtin(
    preset_id: &str,
    name: &str,
    description: &str,
    domain: PresetDomain,
    config: PresetParameters,
) -> OptimizationPreset {
    OptimizationPreset {
        schema_version: CORE_SCHEMA_VERSION,
        preset_id: String::from(preset_id),
        name: String::from(name),
        description: String::from(description),
        domain,
        is_builtin: true,
        config,
        created_unix_s: 0,
        updated_unix_s: 0,
    }
}

/// Returns the factory built-in [`OptimizationPreset`] catalog covering core domains.
#[must_use]
pub fn builtin_presets() -> Vec<OptimizationPreset> {
    vec![
        make_builtin(
            "web-image-webp",
            "Web Image (WebP + EXIF Scrub)",
            "Transcodes images to lossy/balanced WebP with privacy metadata scrubbed and SSIM >= 0.96 guard.",
            PresetDomain::Image,
            PresetParameters {
                profile: CompressionProfile::WebSharingReady,
                target_format: Some(FormatId::Webp),
                strip_metadata: true,
                no_op_if_larger: false,
                min_ssim: Some(0.96),
                naming_template: String::from("{stem}_{date}.{ext}"),
                mirror_directories: true,
                ..PresetParameters::default()
            },
        ),
        make_builtin(
            "lossless-archive-zst",
            "High-Density Archival (.tar.zst)",
            "Packs files or directories into compressed tar.zst with maximum lossless Zstandard density.",
            PresetDomain::Archive,
            PresetParameters {
                profile: CompressionProfile::LosslessMaximum,
                target_format: Some(FormatId::Zstd),
                naming_template: String::from("{stem}_{hash8}.{ext}"),
                ..PresetParameters::default()
            },
        ),
        make_builtin(
            "pdf-archival-clean",
            "PDF Balanced & Privacy Scrub",
            "Recompresses PDF streams cleanly while stripping author/creator PII tags.",
            PresetDomain::Pdf,
            PresetParameters {
                profile: CompressionProfile::BalancedOptimal,
                target_format: Some(FormatId::Pdf),
                strip_metadata: true,
                naming_template: String::from("{stem}_clean.{ext}"),
                mirror_directories: true,
                ..PresetParameters::default()
            },
        ),
        make_builtin(
            "privacy-metadata-scrub",
            "One-Click Privacy Sanitizer",
            "Removes GPS coordinates, camera serials, and author tags while preserving ICC color profiles and orientation.",
            PresetDomain::Metadata,
            PresetParameters {
                profile: CompressionProfile::LosslessMaximum,
                strip_metadata: true,
                no_op_if_larger: false,
                naming_template: String::from("{stem}_scrubbed.{ext}"),
                mirror_directories: true,
                ..PresetParameters::default()
            },
        ),
        make_builtin(
            "dedup-safe-quarantine",
            "Safe Duplicate Quarantine",
            "Identifies bit-exact duplicates via 3-stage BLAKE3 scan and isolates redundant copies reversibly.",
            PresetDomain::Storage,
            PresetParameters {
                collision_policy: CollisionPolicy::Skip,
                ..PresetParameters::default()
            },
        ),
    ]
}

/// Exports an [`OptimizationPreset`] as canonical YAML (`FR-LOC-002`).
///
/// # Errors
/// Returns [`FileFusionError`] if JSON serialization or YAML rendering fails.
pub fn export_preset_yaml(preset: &OptimizationPreset) -> FileFusionResult<String> {
    let json = serde_json::to_string(preset).map_err(|err| {
        FileFusionError::new(
            ErrorCode::IntInvariant,
            format!("failed to serialize preset `{}`: {err}", preset.preset_id),
        )
    })?;
    convert_structured_data(&json, FormatId::Json, FormatId::Yaml, false)
}

/// Parses an [`OptimizationPreset`] from either JSON or YAML text.
///
/// # Errors
/// Returns [`FileFusionError`] on malformed syntax or invalid preset schema.
pub fn parse_preset_str(raw: &str) -> FileFusionResult<OptimizationPreset> {
    let trimmed = raw.trim();
    if trimmed.starts_with('{') {
        let preset: OptimizationPreset = serde_json::from_str(trimmed).map_err(|err| {
            FileFusionError::new(ErrorCode::FmtCorrupt, format!("invalid preset JSON: {err}"))
        })?;
        validate_slug_id(&preset.preset_id, "preset_id")?;
        return Ok(preset);
    }
    let val = parse_structured_value(raw, FormatId::Yaml)?;
    let preset: OptimizationPreset = serde_json::from_value(val).map_err(|err| {
        FileFusionError::new(ErrorCode::FmtCorrupt, format!("invalid preset YAML: {err}"))
    })?;
    validate_slug_id(&preset.preset_id, "preset_id")?;
    Ok(preset)
}

/// Evaluates whether a 5-field UTC cron expression (`min hour dom mon dow`) matches `unix_secs`.
///
/// # Errors
/// Returns [`FileFusionError`] (`ErrorCode::FmtCorrupt`) if `expr` has invalid syntax or field bounds.
pub fn is_cron_matching_utc(expr: &str, unix_secs: u64) -> FileFusionResult<bool> {
    let fields: Vec<&str> = expr.split_whitespace().collect();
    if fields.len() != 5 {
        return Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            format!("cron expression `{expr}` must have exactly 5 fields (`min hour dom mon dow`)"),
        ));
    }
    let minute = u32::try_from((unix_secs / 60) % 60).unwrap_or(0);
    let hour = u32::try_from((unix_secs / 3_600) % 24).unwrap_or(0);
    let (year, month, day, weekday) = utc_calendar_components(unix_secs);
    let _ = year;

    let minute_ok = cron_field_matches(fields[0], minute, 0, 59, false)?;
    let hour_ok = cron_field_matches(fields[1], hour, 0, 23, false)?;
    let day_ok = cron_field_matches(fields[2], day, 1, 31, false)?;
    let month_ok = cron_field_matches(fields[3], month, 1, 12, false)?;
    let weekday_ok = cron_field_matches(fields[4], weekday, 0, 7, true)?;

    Ok(minute_ok && hour_ok && day_ok && month_ok && weekday_ok)
}

/// Evaluates enabled tasks against `now_unix_s` and returns those ready for execution.
///
/// # Errors
/// Returns [`FileFusionError`] if any enabled cron expression fails validation.
pub fn evaluate_due_tasks(
    tasks: &[ScheduledWorkflowTask],
    now_unix_s: u64,
) -> FileFusionResult<Vec<ScheduledTaskDue>> {
    let mut due = Vec::new();
    for task in tasks {
        if !task.enabled {
            continue;
        }
        match &task.trigger {
            ScheduleTrigger::CronUtc { expression } => {
                if !is_cron_matching_utc(expression, now_unix_s)? {
                    continue;
                }
                let already_ran_this_minute = task
                    .last_run_unix_s
                    .is_some_and(|prev| (prev / 60) == (now_unix_s / 60));
                if !already_ran_this_minute {
                    due.push(ScheduledTaskDue {
                        task_id: task.task_id.clone(),
                        workflow_id: task.workflow_id.clone(),
                        reason: format!("matched UTC cron `{expression}`"),
                    });
                }
            }
            ScheduleTrigger::IntervalSeconds { every_seconds } => {
                if *every_seconds == 0 {
                    return Err(FileFusionError::new(
                        ErrorCode::FmtCorrupt,
                        format!(
                            "schedule task `{}` interval must be >= 1 second",
                            task.task_id
                        ),
                    ));
                }
                let elapsed = task
                    .last_run_unix_s
                    .map_or(u64::MAX, |prev| now_unix_s.saturating_sub(prev));
                if elapsed >= *every_seconds {
                    due.push(ScheduledTaskDue {
                        task_id: task.task_id.clone(),
                        workflow_id: task.workflow_id.clone(),
                        reason: format!("interval {every_seconds}s elapsed"),
                    });
                }
            }
        }
    }
    Ok(due)
}

impl JobManager {
    /// Returns factory built-in optimization presets.
    #[must_use]
    pub fn builtin_presets(&self) -> Vec<OptimizationPreset> {
        let _ = &self.defaults;
        builtin_presets()
    }

    /// Saves a custom [`OptimizationPreset`] atomically into `store_dir/<preset_id>.json`.
    ///
    /// # Errors
    /// Returns [`FileFusionError`] if `preset_id` violates safe slug rules or on I/O errors.
    pub fn save_preset(
        &self,
        store_dir: &Path,
        preset: &OptimizationPreset,
    ) -> FileFusionResult<PathBuf> {
        let _ = &self.defaults;
        validate_slug_id(&preset.preset_id, "preset_id")?;
        fs::create_dir_all(store_dir)?;
        let path = store_dir.join(format!("{}.json", preset.preset_id));
        write_json_atomic(&path, preset)?;
        Ok(path)
    }

    /// Loads a preset by `preset_id`, checking `store_dir` custom presets before falling back to
    /// factory [`builtin_presets`].
    ///
    /// # Errors
    /// Returns [`FileFusionError`] (`IoNotFound`) if missing or `FmtCorrupt` on malformed data.
    pub fn load_preset(
        &self,
        store_dir: Option<&Path>,
        preset_id: &str,
    ) -> FileFusionResult<OptimizationPreset> {
        let _ = &self.defaults;
        validate_slug_id(preset_id, "preset_id")?;
        if let Some(dir) = store_dir {
            let json_path = dir.join(format!("{preset_id}.json"));
            if json_path.exists() {
                let raw = fs::read_to_string(&json_path)?;
                return parse_preset_str(&raw);
            }
            let yaml_path = dir.join(format!("{preset_id}.yaml"));
            if yaml_path.exists() {
                let raw = fs::read_to_string(&yaml_path)?;
                return parse_preset_str(&raw);
            }
        }
        builtin_presets()
            .into_iter()
            .find(|p| p.preset_id == preset_id)
            .ok_or_else(|| {
                FileFusionError::new(
                    ErrorCode::IoNotFound,
                    format!("preset `{preset_id}` was not found"),
                )
            })
    }

    /// Lists all factory built-ins combined with custom presets persisted under `store_dir`.
    ///
    /// # Errors
    /// Returns [`FileFusionError`] on unreadable preset files or invalid JSON/YAML schemas.
    pub fn list_presets(
        &self,
        store_dir: Option<&Path>,
    ) -> FileFusionResult<Vec<OptimizationPreset>> {
        let _ = &self.defaults;
        let mut presets = builtin_presets();
        if let Some(dir) = store_dir
            && dir.exists()
        {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if !matches!(ext, "json" | "yaml" | "yml") {
                    continue;
                }
                let raw = fs::read_to_string(&path)?;
                let parsed = parse_preset_str(&raw)?;
                if let Some(slot) = presets.iter_mut().find(|p| p.preset_id == parsed.preset_id) {
                    *slot = parsed;
                } else {
                    presets.push(parsed);
                }
            }
        }
        presets.sort_by(|a, b| a.preset_id.cmp(&b.preset_id));
        Ok(presets)
    }

    /// Deletes a custom persisted preset from `store_dir` (`true` if removed, `false` if absent).
    ///
    /// # Errors
    /// Returns [`FileFusionError`] if `preset_id` fails slug validation or filesystem unlink fails.
    pub fn delete_preset(&self, store_dir: &Path, preset_id: &str) -> FileFusionResult<bool> {
        let _ = &self.defaults;
        validate_slug_id(preset_id, "preset_id")?;
        let mut removed = false;
        for ext in ["json", "yaml", "yml"] {
            let p = store_dir.join(format!("{preset_id}.{ext}"));
            if p.exists() {
                fs::remove_file(p)?;
                removed = true;
            }
        }
        Ok(removed)
    }

    /// Saves a [`ScheduledWorkflowTask`] atomically into `schedule_dir/<task_id>.json`.
    ///
    /// # Errors
    /// Returns [`FileFusionError`] on invalid `task_id`, invalid cron/interval syntax, or I/O errors.
    pub fn save_schedule(
        &self,
        schedule_dir: &Path,
        task: &ScheduledWorkflowTask,
    ) -> FileFusionResult<PathBuf> {
        let _ = &self.defaults;
        validate_slug_id(&task.task_id, "task_id")?;
        let _ = evaluate_due_tasks(std::slice::from_ref(task), current_unix_secs())?;
        fs::create_dir_all(schedule_dir)?;
        let path = schedule_dir.join(format!("{}.json", task.task_id));
        write_json_atomic(&path, task)?;
        Ok(path)
    }

    /// Loads all [`ScheduledWorkflowTask`] entries persisted under `schedule_dir`.
    ///
    /// # Errors
    /// Returns [`FileFusionError`] on directory read or malformed JSON errors.
    pub fn list_schedules(
        &self,
        schedule_dir: &Path,
    ) -> FileFusionResult<Vec<ScheduledWorkflowTask>> {
        let _ = &self.defaults;
        let mut tasks = Vec::new();
        if !schedule_dir.exists() {
            return Ok(tasks);
        }
        for entry in fs::read_dir(schedule_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("json") {
                let raw = fs::read_to_string(&path)?;
                let parsed: ScheduledWorkflowTask = serde_json::from_str(&raw).map_err(|err| {
                    FileFusionError::new(
                        ErrorCode::FmtCorrupt,
                        format!("invalid schedule file `{}`: {err}", path.display()),
                    )
                })?;
                tasks.push(parsed);
            }
        }
        tasks.sort_by(|a, b| a.task_id.cmp(&b.task_id));
        Ok(tasks)
    }

    /// Evaluates persisted schedules in `schedule_dir` against `now_unix_s` (defaulting to current time).
    ///
    /// # Errors
    /// Returns [`FileFusionError`] if persisted schedules contain corrupt JSON or invalid cron specs.
    pub fn evaluate_due_schedules(
        &self,
        schedule_dir: &Path,
        now_unix_s: Option<u64>,
    ) -> FileFusionResult<Vec<ScheduledTaskDue>> {
        let tasks = self.list_schedules(schedule_dir)?;
        evaluate_due_tasks(&tasks, now_unix_s.unwrap_or_else(current_unix_secs))
    }
}

pub(crate) fn validate_slug_id(id: &str, field_name: &str) -> FileFusionResult<()> {
    if id.is_empty()
        || id.len() > 96
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(FileFusionError::new(
            ErrorCode::SecTraversal,
            format!(
                "invalid `{field_name}` `{id}`: expected 1..=96 ASCII alphanumeric, `-`, or `_` characters"
            ),
        ));
    }
    Ok(())
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> FileFusionResult<()> {
    let json = serde_json::to_string_pretty(value).map_err(|err| {
        FileFusionError::new(
            ErrorCode::IntInvariant,
            format!("failed to serialize `{}`: {err}", path.display()),
        )
    })?;
    let staged = StagedPath::create_sibling(path)?;
    fs::write(staged.staged_path(), json)?;
    let _ = staged.promote(CollisionPolicy::Skip)?;
    Ok(())
}

fn current_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn utc_calendar_components(unix_secs: u64) -> (i64, u32, u32, u32) {
    let days = i64::try_from(unix_secs / 86_400).unwrap_or(0);
    // 1970-01-01 was Thursday (4 = Thursday in 0=Sun..6=Sat).
    let weekday = u32::try_from((days + 4).rem_euclid(7)).unwrap_or(0);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let m = u32::try_from(mp + if mp < 10 { 3 } else { -9 }).unwrap_or(1);
    let year = y + i64::from(m <= 2);
    (year, m, d, weekday)
}

fn cron_field_matches(
    spec: &str,
    value: u32,
    min_bound: u32,
    max_bound: u32,
    wrap_sunday_seven: bool,
) -> FileFusionResult<bool> {
    for part in spec.split(',') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            return Err(FileFusionError::new(
                ErrorCode::FmtCorrupt,
                format!("empty comma segment in cron field `{spec}`"),
            ));
        }
        if cron_part_matches(trimmed, value, min_bound, max_bound, wrap_sunday_seven)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn cron_part_matches(
    part: &str,
    value: u32,
    min_bound: u32,
    max_bound: u32,
    wrap_sunday_seven: bool,
) -> FileFusionResult<bool> {
    let (range_part, step) = if let Some((lhs, rhs)) = part.split_once('/') {
        let parsed_step = rhs.parse::<u32>().map_err(|_| {
            FileFusionError::new(
                ErrorCode::FmtCorrupt,
                format!("invalid cron step `{rhs}` in `{part}`"),
            )
        })?;
        if parsed_step == 0 {
            return Err(FileFusionError::new(
                ErrorCode::FmtCorrupt,
                format!("cron step cannot be 0 in `{part}`"),
            ));
        }
        (lhs, parsed_step)
    } else {
        (part, 1)
    };

    if range_part == "*" {
        return Ok((value.saturating_sub(min_bound)) % step == 0);
    }

    if let Some((start_s, end_s)) = range_part.split_once('-') {
        let start = parse_cron_num(start_s, min_bound, max_bound)?;
        let end = parse_cron_num(end_s, min_bound, max_bound)?;
        if end < start {
            return Err(FileFusionError::new(
                ErrorCode::FmtCorrupt,
                format!("cron range `{start}-{end}` is inverted"),
            ));
        }
        let check_val = if wrap_sunday_seven && value == 0 && end == 7 && start <= 7 {
            // Allow `5-7` or `0-7` on dow where both `0` and `7` denote Sunday.
            if value >= start { value } else { 7 }
        } else {
            value
        };
        return Ok(check_val >= start && check_val <= end && (check_val - start) % step == 0);
    }

    let exact = parse_cron_num(range_part, min_bound, max_bound)?;
    let normalized = if wrap_sunday_seven && exact == 7 {
        0
    } else {
        exact
    };
    Ok(value == normalized)
}

fn parse_cron_num(raw: &str, min_bound: u32, max_bound: u32) -> FileFusionResult<u32> {
    let n = raw.parse::<u32>().map_err(|_| {
        FileFusionError::new(
            ErrorCode::FmtCorrupt,
            format!("invalid cron number `{raw}`"),
        )
    })?;
    if n < min_bound || n > max_bound {
        return Err(FileFusionError::new(
            ErrorCode::FmtCorrupt,
            format!("cron value {n} out of range {min_bound}..={max_bound}"),
        ));
    }
    Ok(n)
}
