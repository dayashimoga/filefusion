//! Shared L0 Processing API, cancellation/progress primitives, validated source
//! wrappers, atomic temp-file staging, and batch naming/collision contracts
//! (`FF-CTR-001` §3–§4, `MOD-RULE-01`).

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::config::{BatchDefaults, CollisionPolicy};
use crate::contracts::{IsolatedProcessRunner, ProcessInvocation, ProcessOutput};
use crate::error::{ErrorCode, ErrorDiagnostic, FileFusionError, FileFusionResult};
use crate::limits::{MemoryGovernor, MemoryReservation};
use crate::metadata::{FileInspectionReport, inspect_path};
use crate::{CORE_SCHEMA_VERSION, FormatId, JobStage, ProgressEvent, ResourceLimits};

static STAGING_COUNTER: AtomicU64 = AtomicU64::new(1);

fn next_staging_suffix() -> String {
    let seq = STAGING_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!(
        "{:08x}{:08x}{:04x}",
        std::process::id(),
        nanos & 0xFFFF_FFFF,
        seq & 0xFFFF
    )
}

type CancelHook = Arc<dyn Fn() + Send + Sync>;

struct CancelNode {
    cancelled: AtomicBool,
    parent: Option<Arc<CancelNode>>,
    next_hook_id: AtomicU64,
    hooks: Mutex<Vec<(u64, CancelHook)>>,
    children: Mutex<Vec<Weak<CancelNode>>>,
}

impl std::fmt::Debug for CancelNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CancelNode")
            .field("cancelled", &self.cancelled.load(Ordering::SeqCst))
            .field("has_parent", &self.parent.is_some())
            .finish_non_exhaustive()
    }
}

impl CancelNode {
    fn new(parent: Option<Arc<CancelNode>>, pre_cancelled: bool) -> Arc<Self> {
        Arc::new(Self {
            cancelled: AtomicBool::new(pre_cancelled),
            parent,
            next_hook_id: AtomicU64::new(1),
            hooks: Mutex::new(Vec::new()),
            children: Mutex::new(Vec::new()),
        })
    }

    fn is_cancelled(&self) -> bool {
        if self.cancelled.load(Ordering::SeqCst) {
            return true;
        }
        if let Some(parent) = &self.parent {
            return parent.is_cancelled();
        }
        false
    }

    fn cancel_cascade(&self) {
        let was_cancelled = self.cancelled.swap(true, Ordering::SeqCst);
        if !was_cancelled {
            tracing::info!(event = "job_cancelled", "cancellation signal raised");
        }

        let callbacks: Vec<CancelHook> = {
            let mut guard = match self.hooks.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            guard.drain(..).map(|(_, cb)| cb).collect()
        };
        for cb in callbacks {
            cb();
        }

        let live_children: Vec<Arc<CancelNode>> = {
            let mut guard = match self.children.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            let upgraded: Vec<Arc<CancelNode>> = guard.iter().filter_map(Weak::upgrade).collect();
            guard.retain(|w| w.strong_count() > 0);
            upgraded
        };
        for child in live_children {
            child.cancel_cascade();
        }
    }
}

/// RAII guard removing a cancellation hook when dropped (`NFR-REL-002`).
#[derive(Debug)]
pub struct CancelSubscription {
    node: Option<Arc<CancelNode>>,
    hook_id: u64,
}

impl Drop for CancelSubscription {
    fn drop(&mut self) {
        if let Some(node) = self.node.take() {
            let mut guard = match node.hooks.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            guard.retain(|(id, _)| *id != self.hook_id);
        }
    }
}

/// Cooperative hierarchical thread-safe cancellation token checked at chunk/step boundaries (`NFR-REL-002`).
#[derive(Debug, Clone)]
pub struct CancellationToken {
    node: Arc<CancelNode>,
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

impl CancellationToken {
    /// Creates a new active (non-cancelled) root token.
    #[must_use]
    pub fn new() -> Self {
        Self {
            node: CancelNode::new(None, false),
        }
    }

    /// Creates a linked child token.
    ///
    /// Cancelling the parent cascades immediately to the child, whereas cancelling
    /// only the child leaves the parent and sibling tokens active.
    #[must_use]
    pub fn child(&self) -> Self {
        let already = self.is_cancelled();
        let child_node = CancelNode::new(Some(Arc::clone(&self.node)), already);
        if !already {
            let mut guard = match self.node.children.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            guard.retain(|w| w.strong_count() > 0);
            guard.push(Arc::downgrade(&child_node));
        }
        Self { node: child_node }
    }

    /// Registers a callback executed immediately on cancellation (or right away if already cancelled).
    #[must_use]
    pub fn subscribe_on_cancel<F>(&self, callback: F) -> CancelSubscription
    where
        F: Fn() + Send + Sync + 'static,
    {
        if self.is_cancelled() {
            callback();
            return CancelSubscription {
                node: None,
                hook_id: 0,
            };
        }
        let hook_id = self.node.next_hook_id.fetch_add(1, Ordering::Relaxed);
        let hook: CancelHook = Arc::new(callback);
        {
            let mut guard = match self.node.hooks.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            guard.push((hook_id, hook));
        }
        if self.is_cancelled() {
            let maybe_cb = {
                let mut guard = match self.node.hooks.lock() {
                    Ok(g) => g,
                    Err(p) => p.into_inner(),
                };
                guard
                    .iter()
                    .position(|(id, _)| *id == hook_id)
                    .map(|pos| guard.remove(pos).1)
            };
            if let Some(cb) = maybe_cb {
                cb();
            }
            return CancelSubscription {
                node: None,
                hook_id: 0,
            };
        }
        CancelSubscription {
            node: Some(Arc::clone(&self.node)),
            hook_id,
        }
    }

    /// Signals cancellation across all cloned handles and linked child tokens immediately.
    pub fn cancel(&self) {
        self.node.cancel_cascade();
    }

    /// Returns `true` once [`CancellationToken::cancel`] has been called on this token or an ancestor.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.node.is_cancelled()
    }

    /// Returns `Err(FileFusionError)` (`FF-ERR-JOB-CANCELLED`) if cancelled.
    ///
    /// # Errors
    /// Returns [`FileFusionError`] with [`ErrorCode::JobCancelled`] when aborted.
    pub fn check(&self) -> FileFusionResult<()> {
        if self.is_cancelled() {
            return Err(
                FileFusionError::new(ErrorCode::JobCancelled, "job cancelled by user")
                    .with_remediation("Temporary staging files have been removed."),
            );
        }
        Ok(())
    }
}

/// Sink receiving monotonic [`ProgressEvent`] updates across CLI, FFI, WASM, and REST bridges.
pub trait ProgressSink: Send + Sync {
    /// Emits a structured progress update event.
    fn emit(&self, event: ProgressEvent);
}

/// Zero-overhead no-op progress sink.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoOpProgressSink;

impl ProgressSink for NoOpProgressSink {
    fn emit(&self, _event: ProgressEvent) {}
}

/// Thread-safe in-memory [`ProgressEvent`] recorder useful for tests and batch receipts.
#[derive(Debug, Default)]
pub struct VecProgressSink {
    events: Mutex<Vec<ProgressEvent>>,
}

impl VecProgressSink {
    /// Creates an empty recorder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a cloned snapshot of all recorded [`ProgressEvent`] items.
    #[must_use]
    pub fn snapshot(&self) -> Vec<ProgressEvent> {
        match self.events.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }
}

impl ProgressSink for VecProgressSink {
    fn emit(&self, event: ProgressEvent) {
        match self.events.lock() {
            Ok(mut guard) => guard.push(event),
            Err(poisoned) => poisoned.into_inner().push(event),
        }
    }
}

/// Closure-backed [`ProgressSink`] forwarding updates cleanly to Plane 2 adapters.
pub struct CallbackProgressSink<F> {
    callback: F,
}

impl<F> std::fmt::Debug for CallbackProgressSink<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CallbackProgressSink")
            .finish_non_exhaustive()
    }
}

impl<F> CallbackProgressSink<F>
where
    F: Fn(ProgressEvent) + Send + Sync,
{
    /// Wraps `callback` as a [`ProgressSink`].
    #[must_use]
    pub const fn new(callback: F) -> Self {
        Self { callback }
    }
}

impl<F> ProgressSink for CallbackProgressSink<F>
where
    F: Fn(ProgressEvent) + Send + Sync,
{
    fn emit(&self, event: ProgressEvent) {
        (self.callback)(event);
    }
}

/// [`ProgressSink`] decorator enforcing per-item monotonic percentages (`FF-CTR-001` §2.3).
#[derive(Debug)]
pub struct MonotonicProgressSink<S> {
    inner: S,
    high_water: Mutex<HashMap<u32, f32>>,
}

impl<S: ProgressSink> MonotonicProgressSink<S> {
    /// Creates a monotonic wrapper around `inner`.
    #[must_use]
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            high_water: Mutex::new(HashMap::new()),
        }
    }

    /// Consumes the wrapper and returns the inner [`ProgressSink`].
    #[must_use]
    pub fn into_inner(self) -> S {
        self.inner
    }
}

impl<S: ProgressSink> ProgressSink for MonotonicProgressSink<S> {
    fn emit(&self, mut event: ProgressEvent) {
        let clamped = if event.percent.is_finite() {
            event.percent.clamp(0.0, 100.0)
        } else {
            0.0
        };
        let normalized = {
            let mut guard = match self.high_water.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            let prev = guard.get(&event.item_index).copied().unwrap_or(0.0);
            let next = if matches!(event.stage, JobStage::Completed) {
                100.0
            } else {
                prev.max(clamped)
            };
            guard.insert(event.item_index, next);
            next
        };
        event.percent = normalized;
        self.inner.emit(event);
    }
}

/// [`ProgressSink`] decorator coalescing rapid non-terminal ticks within `min_interval`.
#[derive(Debug)]
pub struct ThrottledProgressSink<S> {
    inner: S,
    min_interval: Duration,
    #[cfg(not(target_arch = "wasm32"))]
    last_emitted: Mutex<HashMap<u32, (JobStage, std::time::Instant)>>,
    #[cfg(target_arch = "wasm32")]
    last_stage: Mutex<HashMap<u32, JobStage>>,
}

impl<S: ProgressSink> ThrottledProgressSink<S> {
    /// Creates a throttled decorator forwarding stage transitions immediately and rate-limiting same-stage ticks.
    #[must_use]
    pub fn new(inner: S, min_interval: Duration) -> Self {
        Self {
            inner,
            min_interval,
            #[cfg(not(target_arch = "wasm32"))]
            last_emitted: Mutex::new(HashMap::new()),
            #[cfg(target_arch = "wasm32")]
            last_stage: Mutex::new(HashMap::new()),
        }
    }

    /// Returns the configured throttling interval.
    #[must_use]
    pub const fn min_interval(&self) -> Duration {
        self.min_interval
    }

    /// Consumes the wrapper and returns the inner [`ProgressSink`].
    #[must_use]
    pub fn into_inner(self) -> S {
        self.inner
    }
}

const fn is_terminal_stage(stage: JobStage) -> bool {
    matches!(
        stage,
        JobStage::Completed | JobStage::Cancelled | JobStage::Failed
    )
}

impl<S: ProgressSink> ProgressSink for ThrottledProgressSink<S> {
    fn emit(&self, event: ProgressEvent) {
        #[cfg(not(target_arch = "wasm32"))]
        let should_emit = {
            let now = std::time::Instant::now();
            let mut guard = match self.last_emitted.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            let forward = match guard.get(&event.item_index) {
                None => true,
                Some((prev_stage, prev_at)) => {
                    is_terminal_stage(event.stage)
                        || *prev_stage != event.stage
                        || self.min_interval.is_zero()
                        || now.duration_since(*prev_at) >= self.min_interval
                }
            };
            if forward {
                guard.insert(event.item_index, (event.stage, now));
            }
            forward
        };

        #[cfg(target_arch = "wasm32")]
        let should_emit = {
            let mut guard = match self.last_stage.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            let forward = is_terminal_stage(event.stage)
                || guard.get(&event.item_index).copied() != Some(event.stage);
            if forward {
                guard.insert(event.item_index, event.stage);
            }
            forward
        };

        if should_emit {
            self.inner.emit(event);
        }
    }
}

/// Execution context passed down from `job-manager` into processing engines (`FF-CTR-001` §1).
#[derive(Clone)]
pub struct JobContext {
    /// Unique job identifier.
    pub job_id: String,
    /// Active resource and security ceilings.
    pub limits: ResourceLimits,
    /// Shared cancellation handle checked at step/chunk boundaries.
    pub cancellation: CancellationToken,
    /// Shared progress event sink.
    pub progress: Arc<dyn ProgressSink>,
    /// Optional injected isolated subprocess runner (`MOD-RULE-01`, `MOD-RULE-04`).
    pub process_runner: Option<Arc<dyn IsolatedProcessRunner>>,
    /// Optional shared working-set memory governor (`NFR-PERF-001`).
    pub memory_governor: Option<Arc<MemoryGovernor>>,
}

impl JobContext {
    /// Builds a [`JobContext`] wired to [`NoOpProgressSink`].
    #[must_use]
    pub fn new(job_id: impl Into<String>, limits: ResourceLimits) -> Self {
        Self {
            job_id: job_id.into(),
            limits,
            cancellation: CancellationToken::new(),
            progress: Arc::new(NoOpProgressSink),
            process_runner: None,
            memory_governor: None,
        }
    }

    /// Attaches a custom [`ProgressSink`].
    #[must_use]
    pub fn with_progress(mut self, progress: Arc<dyn ProgressSink>) -> Self {
        self.progress = progress;
        self
    }

    /// Attaches an existing [`CancellationToken`].
    #[must_use]
    pub fn with_cancellation(mut self, cancellation: CancellationToken) -> Self {
        self.cancellation = cancellation;
        self
    }

    /// Attaches an [`IsolatedProcessRunner`] implementation for Tier 2 tool execution.
    #[must_use]
    pub fn with_process_runner(mut self, runner: Arc<dyn IsolatedProcessRunner>) -> Self {
        self.process_runner = Some(runner);
        self
    }

    /// Attaches a shared [`MemoryGovernor`] enforcing working-set permits.
    #[must_use]
    pub fn with_memory_governor(mut self, governor: Arc<MemoryGovernor>) -> Self {
        self.memory_governor = Some(governor);
        self
    }

    /// Reserves `requested_mb` from the attached [`MemoryGovernor`], or validates against
    /// `limits.max_memory_mb` when running standalone without a shared governor.
    ///
    /// # Errors
    /// Returns [`FileFusionError`] (`ErrorCode::ResMemory`) when memory ceilings are exceeded.
    pub fn reserve_working_set_mb(
        &self,
        requested_mb: u64,
        purpose: &str,
    ) -> FileFusionResult<Option<MemoryReservation>> {
        if let Some(gov) = &self.memory_governor {
            return gov.try_reserve_mb(requested_mb, purpose).map(Some);
        }
        self.limits.validate_file_budget(0, requested_mb)?;
        Ok(None)
    }

    /// Executes a Tier 2 external binary using the injected [`IsolatedProcessRunner`].
    ///
    /// # Errors
    /// Returns [`FileFusionError`] (`ErrorCode::EngMissing`) if no runner is configured,
    /// or forwards runner execution errors.
    pub fn run_isolated_process(
        &self,
        invocation: &ProcessInvocation,
    ) -> FileFusionResult<ProcessOutput> {
        let Some(runner) = &self.process_runner else {
            return Err(FileFusionError::new(
                ErrorCode::EngMissing,
                "no IsolatedProcessRunner configured in JobContext",
            )
            .with_remediation(
                "Construct JobContext via JobManager::create_context or attach a runner.",
            ));
        };
        runner.run_isolated(invocation, self)
    }

    /// Checks whether cancellation was requested.
    ///
    /// # Errors
    /// Returns [`FileFusionError`] (`ErrorCode::JobCancelled`) if cancelled.
    pub fn check_cancelled(&self) -> FileFusionResult<()> {
        self.cancellation.check()
    }

    /// Emits a [`ProgressEvent`] bound to this job's `job_id`.
    pub fn emit_progress(
        &self,
        item_index: u32,
        item_total: u32,
        stage: JobStage,
        percent: f32,
        bytes_in: u64,
        detail: impl Into<String>,
    ) {
        self.emit_progress_estimate(
            item_index, item_total, stage, percent, bytes_in, None, None, detail,
        );
    }

    /// Emits a [`ProgressEvent`] including optional output-byte and ETA estimates (`FF-CTR-001` §2.3).
    #[allow(clippy::too_many_arguments)]
    pub fn emit_progress_estimate(
        &self,
        item_index: u32,
        item_total: u32,
        stage: JobStage,
        percent: f32,
        bytes_in: u64,
        bytes_out_estimate: Option<u64>,
        eta_ms: Option<u64>,
        detail: impl Into<String>,
    ) {
        let safe_pct = if percent.is_finite() {
            percent.clamp(0.0, 100.0)
        } else {
            0.0
        };
        self.progress.emit(ProgressEvent {
            job_id: self.job_id.clone(),
            item_index,
            item_total,
            stage,
            percent: safe_pct,
            bytes_in,
            bytes_out_estimate,
            eta_ms,
            detail: detail.into(),
        });
    }
}

/// Standard reproducible compression profiles (`FR-CMP-001`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompressionProfile {
    /// Bit-exact or visually lossless maximum compression.
    LosslessMaximum,
    /// Balanced speed, compatibility, and quality preset.
    BalancedOptimal,
    /// Modern web/mobile sharing preset.
    WebSharingReady,
    /// Aggressive size reduction targeting strict byte caps.
    ExtremeSizeTarget,
    /// Cold archival storage optimized for density and durability.
    ArchivalColdStorage,
}

impl CompressionProfile {
    /// Returns the template token label for `{profile}` substitution.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::LosslessMaximum => "lossless",
            Self::BalancedOptimal => "balanced",
            Self::WebSharingReady => "web",
            Self::ExtremeSizeTarget => "extreme",
            Self::ArchivalColdStorage => "archival",
        }
    }
}

/// Compression execution result status (`FF-CTR-001` §3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompressionStatus {
    /// Staged output is verified and smaller (or format changed as requested).
    Optimized,
    /// `FR-CMP-002`: same-format output was not smaller; original preserved untouched.
    NoOpRetainedOriginal,
}

/// Supported high-level pipeline operations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ProcessingOperation {
    /// Inspect signatures, digests, metadata, and classification without writing outputs.
    InspectOnly,
    /// Optimize or compress items using a [`CompressionProfile`].
    Compress {
        /// Selected quality/density preset.
        profile: CompressionProfile,
        /// Optional transcoding target format.
        target_format: Option<FormatId>,
        /// Strip non-essential EXIF/GPS/author metadata during processing.
        strip_metadata: bool,
    },
    /// Convert items into `target_format`.
    Convert {
        /// Destination format.
        target_format: FormatId,
        /// Strip sensitive metadata where supported.
        strip_metadata: bool,
    },
}

/// Unified batch processing request passed into `job-manager` (`FF-CTR-001`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessingRequest {
    /// Request schema version (`CORE_SCHEMA_VERSION`).
    pub schema_version: u32,
    /// Caller-supplied or generated unique job ID.
    pub job_id: String,
    /// Ordered input filesystem paths.
    pub inputs: Vec<PathBuf>,
    /// Optional destination directory (`None` writes alongside source files).
    pub output_dir: Option<PathBuf>,
    /// Requested operation and parameters.
    pub operation: ProcessingOperation,
    /// Batch naming, collision, and retention rules.
    pub batch: BatchDefaults,
    /// Reject mismatched file extensions with `FF-ERR-SEC-SPOOFED`.
    pub strict_spoofing: bool,
    /// Validate inputs and compute plan without modifying destination files.
    pub dry_run: bool,
}

/// Security-verified and inspected source handle passed to engines (`FF-CTR-001` §3.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedSource {
    /// Canonical or validated filesystem source path.
    pub path: PathBuf,
    /// Completed inspection snapshot covering digests, streams, and spoofing status.
    pub inspection: FileInspectionReport,
}

impl ValidatedSource {
    /// Validates `path` as a regular readable file and inspects it under security limits.
    ///
    /// # Errors
    /// Returns [`FileFusionError`] if missing, a directory, fails `strict_spoofing`, or exceeds ceilings.
    pub fn open(
        path: impl Into<PathBuf>,
        strict_spoofing: bool,
        limits: &ResourceLimits,
    ) -> FileFusionResult<Self> {
        let path = path.into();
        let meta = fs::metadata(&path)?;
        if !meta.is_file() {
            return Err(FileFusionError::new(
                ErrorCode::FmtUnsupported,
                "input path is not a regular file",
            )
            .with_remediation(
                "Provide a regular file path or expand directories before batch submission.",
            ));
        }
        if meta.len() > limits.max_uncompressed_bytes {
            return Err(FileFusionError::new(
                ErrorCode::SecBomb,
                format!(
                    "input size {} bytes exceeds configured limit {} bytes",
                    meta.len(),
                    limits.max_uncompressed_bytes
                ),
            )
            .with_remediation(
                "Increase max_uncompressed_bytes in config.toml if this file is trusted.",
            ));
        }
        let inspection = inspect_path(&path, strict_spoofing)?;
        Ok(Self { path, inspection })
    }
}

/// Atomic temp-file staging guard writing to `.filefusion.tmp.<id>` (`FF-CTR-001` §4.1).
///
/// Automatically deletes uncommitted temp files on `Drop` so aborted or failed
/// jobs never pollute destination directories.
#[derive(Debug)]
pub struct StagedPath {
    staged: PathBuf,
    destination: PathBuf,
    committed: bool,
}

impl StagedPath {
    /// Allocates a unique sibling staging file `.filefusion.tmp.<id>` next to `destination`.
    ///
    /// # Errors
    /// Returns [`FileFusionError`] if the destination directory cannot be created or the temp file cannot be opened.
    pub fn create_sibling(destination: impl Into<PathBuf>) -> FileFusionResult<Self> {
        let destination = destination.into();
        let parent = destination
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;

        let staged = parent.join(format!(".filefusion.tmp.{}", next_staging_suffix()));
        OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&staged)?;

        Ok(Self {
            staged,
            destination,
            committed: false,
        })
    }

    /// Returns the filesystem path of the hidden temporary staging file.
    #[must_use]
    pub fn staged_path(&self) -> &Path {
        &self.staged
    }

    /// Returns the eventual final destination path.
    #[must_use]
    pub fn destination_path(&self) -> &Path {
        &self.destination
    }

    /// Atomically promotes the verified staged file to `destination`, applying `collision_policy`.
    ///
    /// # Errors
    /// Returns [`FileFusionError`] if backup creation or atomic rename fails.
    pub fn promote(mut self, collision_policy: CollisionPolicy) -> FileFusionResult<PathBuf> {
        let final_dest = match collision_policy {
            CollisionPolicy::OverwriteWithBackup if self.destination.exists() => {
                let backup = PathBuf::from(format!("{}.bak", self.destination.to_string_lossy()));
                fs::copy(&self.destination, &backup)?;
                self.destination.clone()
            }
            _ => self.destination.clone(),
        };

        fs::rename(&self.staged, &final_dest)?;
        self.committed = true;
        Ok(final_dest)
    }
}

impl Drop for StagedPath {
    fn drop(&mut self) {
        if !self.committed && self.staged.exists() {
            let _ = fs::remove_file(&self.staged);
        }
    }
}

/// Resolves an output destination path from `BatchDefaults` naming template & collision rules (`FR-BAT-002`).
///
/// Returns `Ok(None)` when `CollisionPolicy::Skip` applies to an existing destination.
///
/// # Errors
/// Returns [`FileFusionError`] (`ErrorCode::SecTraversal`) if the rendered filename attempts traversal.
pub fn resolve_output_destination(
    source: &Path,
    output_dir: Option<&Path>,
    hash8: &str,
    profile_slug: &str,
    index: usize,
    target_ext: Option<&str>,
    batch: &BatchDefaults,
) -> FileFusionResult<Option<PathBuf>> {
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    let src_ext = source.extension().and_then(|e| e.to_str()).unwrap_or("");
    let ext = target_ext.unwrap_or(src_ext);

    let rendered = batch
        .naming_template
        .replace("{stem}", stem)
        .replace("{ext}", ext)
        .replace("{hash8}", hash8)
        .replace("{profile}", profile_slug)
        .replace("{index}", &index.to_string())
        .replace("{date}", "today");
    let clean_name = rendered.trim_end_matches('.');
    if clean_name.is_empty()
        || clean_name.contains('\0')
        || clean_name.contains("..")
        || clean_name.contains('/')
        || clean_name.contains('\\')
    {
        return Err(FileFusionError::new(
            ErrorCode::SecTraversal,
            "rendered output filename violated path safety rules",
        )
        .with_remediation("Adjust batch.naming_template to stay within destination directory."));
    }

    let base_dir = match output_dir {
        Some(dir) => dir,
        None => source
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new(".")),
    };
    let candidate = base_dir.join(clean_name);
    if !candidate.exists() {
        return Ok(Some(candidate));
    }

    match batch.collision_policy {
        CollisionPolicy::Skip => Ok(None),
        CollisionPolicy::OverwriteWithBackup => Ok(Some(candidate)),
        CollisionPolicy::RenameWithSuffix => {
            let candidate_stem = Path::new(clean_name)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(clean_name);
            let candidate_ext = Path::new(clean_name).extension().and_then(|e| e.to_str());
            for n in 1u32..10_000 {
                let alt_name = match candidate_ext {
                    Some(e) if !e.is_empty() => format!("{candidate_stem}_{n}.{e}"),
                    _ => format!("{candidate_stem}_{n}"),
                };
                let alt = base_dir.join(alt_name);
                if !alt.exists() {
                    return Ok(Some(alt));
                }
            }
            Err(FileFusionError::new(
                ErrorCode::IntInvariant,
                "exhausted collision rename suffixes for destination",
            ))
        }
    }
}

/// Terminal state of a single item in a [`JobReceipt`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemStatus {
    /// Item processed successfully.
    Succeeded,
    /// Item retained unchanged under `FR-CMP-002` no-op guard.
    NoOpRetained,
    /// Item skipped due to `CollisionPolicy::Skip`.
    SkippedCollision,
    /// Item aborted via `CancellationToken`.
    Cancelled,
    /// Item failed with diagnostic.
    Failed,
}

/// Per-item execution record returned inside [`JobReceipt`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemOutcome {
    /// 1-based batch index.
    pub item_index: u32,
    /// Input path processed.
    pub source_path: PathBuf,
    /// Resolved output path (if written or planned).
    pub output_path: Option<PathBuf>,
    /// Terminal item status.
    pub status: ItemStatus,
    /// Compression outcome flag when applicable.
    pub compression_status: Option<CompressionStatus>,
    /// Input file size in bytes.
    pub bytes_in: u64,
    /// Output file size in bytes (equals `bytes_in` for no-op / inspect / dry-run).
    pub bytes_out: u64,
    /// Input `BLAKE3` hex digest.
    pub blake3_in: String,
    /// Output `BLAKE3` hex digest if produced.
    pub blake3_out: Option<String>,
    /// Diagnostic payload when `status == ItemStatus::Failed`.
    pub error: Option<ErrorDiagnostic>,
}

/// Final batch execution receipt (`FF-CTR-001` §5.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobReceipt {
    /// Contract schema version (`CORE_SCHEMA_VERSION`).
    pub schema_version: u32,
    /// Job identifier.
    pub job_id: String,
    /// Final job stage (`Completed`, `Cancelled`, or `Failed`).
    pub status: JobStage,
    /// Total items submitted.
    pub total_items: u32,
    /// Items succeeded or retained safely as no-ops.
    pub succeeded: u32,
    /// Items skipped due to collision policy.
    pub skipped: u32,
    /// Items failed.
    pub failed: u32,
    /// Aggregate input byte total.
    pub bytes_in_total: u64,
    /// Aggregate output byte total.
    pub bytes_out_total: u64,
    /// Per-item outcome records.
    pub outcomes: Vec<ItemOutcome>,
}

impl JobReceipt {
    /// Builds a [`JobReceipt`] summary directly from item outcomes.
    #[must_use]
    pub fn from_outcomes(job_id: String, status: JobStage, outcomes: Vec<ItemOutcome>) -> Self {
        let mut succeeded = 0u32;
        let mut skipped = 0u32;
        let mut failed = 0u32;
        let mut bytes_in_total = 0u64;
        let mut bytes_out_total = 0u64;

        for item in &outcomes {
            bytes_in_total = bytes_in_total.saturating_add(item.bytes_in);
            bytes_out_total = bytes_out_total.saturating_add(item.bytes_out);
            match item.status {
                ItemStatus::Succeeded | ItemStatus::NoOpRetained => succeeded += 1,
                ItemStatus::SkippedCollision => skipped += 1,
                ItemStatus::Cancelled | ItemStatus::Failed => failed += 1,
            }
        }

        Self {
            schema_version: CORE_SCHEMA_VERSION,
            job_id,
            status,
            total_items: u32::try_from(outcomes.len()).unwrap_or(u32::MAX),
            succeeded,
            skipped,
            failed,
            bytes_in_total,
            bytes_out_total,
            outcomes,
        }
    }
}

/// Unified high-level item execution hook implemented by Stage 1/2 pipeline runners.
pub trait ProcessingHandler: Send + Sync {
    /// Executes or stages a single [`ValidatedSource`] into `destination`.
    ///
    /// # Errors
    /// Returns [`FileFusionError`] on validation, processing, cancellation, or I/O failure.
    fn process_item(
        &self,
        source: &ValidatedSource,
        destination: Option<&Path>,
        req: &ProcessingRequest,
        item_index: u32,
        ctx: &JobContext,
    ) -> FileFusionResult<ItemOutcome>;
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::io::Write;

    use super::*;

    fn temp_work_dir(label: &str) -> Result<PathBuf, Box<dyn Error>> {
        let dir = std::env::temp_dir().join(format!(
            "filefusion_pipeline_{label}_{}",
            std::process::id()
        ));
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    #[test]
    fn staged_path_cleans_up_on_drop_and_promotes_atomically() -> Result<(), Box<dyn Error>> {
        let dir = temp_work_dir("staging")?;
        let dest = dir.join("out.txt");

        let dropped_tmp = {
            let staged = StagedPath::create_sibling(&dest)?;
            assert!(staged.staged_path().exists());
            staged.staged_path().to_path_buf()
        };
        assert!(!dropped_tmp.exists());
        assert!(!dest.exists());

        let staged = StagedPath::create_sibling(&dest)?;
        OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(staged.staged_path())?
            .write_all(b"verified payload")?;
        let promoted = staged.promote(CollisionPolicy::RenameWithSuffix)?;
        assert_eq!(promoted, dest);
        assert_eq!(fs::read(&dest)?, b"verified payload");

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn collision_resolver_handles_skip_suffix_and_traversal() -> Result<(), Box<dyn Error>> {
        let dir = temp_work_dir("collision")?;
        let src = dir.join("clip.png");
        fs::write(&src, b"png")?;
        let existing = dir.join("clip_balanced_1.webp");
        fs::write(&existing, b"already-here")?;

        let mut batch = BatchDefaults {
            naming_template: String::from("{stem}_{profile}_{index}.{ext}"),
            collision_policy: CollisionPolicy::Skip,
            ..BatchDefaults::default()
        };
        let skipped = resolve_output_destination(
            &src,
            Some(&dir),
            "12345678",
            "balanced",
            1,
            Some("webp"),
            &batch,
        )?;
        assert!(skipped.is_none());

        batch.collision_policy = CollisionPolicy::RenameWithSuffix;
        let renamed = resolve_output_destination(
            &src,
            Some(&dir),
            "12345678",
            "balanced",
            1,
            Some("webp"),
            &batch,
        )?;
        assert_eq!(renamed, Some(dir.join("clip_balanced_1_1.webp")));

        batch.naming_template = String::from("../{stem}.{ext}");
        let traversal = resolve_output_destination(
            &src,
            Some(&dir),
            "12345678",
            "balanced",
            1,
            Some("webp"),
            &batch,
        );
        assert!(traversal.is_err());

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn hierarchical_cancellation_cascades_and_subscriptions_fire_once() {
        let root = CancellationToken::new();
        let child_a = root.child();
        let child_b = root.child();

        let hook_hits = Arc::new(AtomicU64::new(0));
        let hits_clone = Arc::clone(&hook_hits);
        let _sub = child_a.subscribe_on_cancel(move || {
            hits_clone.fetch_add(1, Ordering::SeqCst);
        });

        let dropped_hits = Arc::new(AtomicU64::new(0));
        let dropped_clone = Arc::clone(&dropped_hits);
        {
            let temp_sub = root.subscribe_on_cancel(move || {
                dropped_clone.fetch_add(1, Ordering::SeqCst);
            });
            drop(temp_sub);
        }

        // Cancelling child_b does not cancel root or child_a.
        child_b.cancel();
        assert!(child_b.is_cancelled());
        assert!(!root.is_cancelled());
        assert!(!child_a.is_cancelled());
        assert_eq!(hook_hits.load(Ordering::SeqCst), 0);

        // Cancelling root cascades immediately to child_a and triggers its live hook.
        root.cancel();
        assert!(root.is_cancelled());
        assert!(child_a.is_cancelled());
        assert_eq!(hook_hits.load(Ordering::SeqCst), 1);
        assert_eq!(dropped_hits.load(Ordering::SeqCst), 0);

        // Subscribing after cancellation fires immediately.
        let late_hits = Arc::clone(&hook_hits);
        let late_sub = child_a.subscribe_on_cancel(move || {
            late_hits.fetch_add(1, Ordering::SeqCst);
        });
        drop(late_sub);
        assert_eq!(hook_hits.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn monotonic_and_throttled_sinks_preserve_order_and_stages() {
        let recorded = Arc::new(VecProgressSink::new());
        let throttled = ThrottledProgressSink::new(
            MonotonicProgressSink::new(CallbackProgressSink::new({
                let out = Arc::clone(&recorded);
                move |ev| out.emit(ev)
            })),
            Duration::from_secs(60),
        );

        throttled.emit(ProgressEvent {
            job_id: String::from("j1"),
            item_index: 1,
            item_total: 1,
            stage: JobStage::Processing,
            percent: 40.0,
            bytes_in: 100,
            bytes_out_estimate: None,
            eta_ms: None,
            detail: String::from("first"),
        });
        // Same non-terminal stage within 60s min_interval should be coalesced.
        throttled.emit(ProgressEvent {
            job_id: String::from("j1"),
            item_index: 1,
            item_total: 1,
            stage: JobStage::Processing,
            percent: 50.0,
            bytes_in: 100,
            bytes_out_estimate: None,
            eta_ms: None,
            detail: String::from("throttled"),
        });
        // Regressing stage change emits immediately and clamps to monotonic 40.0.
        throttled.emit(ProgressEvent {
            job_id: String::from("j1"),
            item_index: 1,
            item_total: 1,
            stage: JobStage::Validating,
            percent: 15.0,
            bytes_in: 100,
            bytes_out_estimate: None,
            eta_ms: None,
            detail: String::from("validating"),
        });
        // Terminal completion always passes immediately and snaps to 100.0.
        throttled.emit(ProgressEvent {
            job_id: String::from("j1"),
            item_index: 1,
            item_total: 1,
            stage: JobStage::Completed,
            percent: 90.0,
            bytes_in: 100,
            bytes_out_estimate: Some(64),
            eta_ms: Some(0),
            detail: String::from("done"),
        });

        let events = recorded.snapshot();
        assert_eq!(events.len(), 3);
        assert!((events[0].percent - 40.0).abs() < f32::EPSILON);
        assert_eq!(events[1].stage, JobStage::Validating);
        assert!((events[1].percent - 40.0).abs() < f32::EPSILON);
        assert_eq!(events[2].stage, JobStage::Completed);
        assert!((events[2].percent - 100.0).abs() < f32::EPSILON);
    }
}
