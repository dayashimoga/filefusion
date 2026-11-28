//! Bounded dual-pool Tokio/Rayon batch job scheduler (`FR-BAT-001`, `NFR-PERF-003`, `ADR-003`).
//!
//! Coordinates priority-ordered admission (`High` > `Normal` > `Low` with FIFO ties),
//! memory-weighted concurrency permits (`max_workers` and `max_memory_mb`), pause/resume
//! gating, per-item cancellation, and global cancellation within `<= 500 ms` (`NFR-REL-002`).
//! Under `--features wasm` (without `native-scheduler` / `rayon-parallel`), falls back to
//! deterministic single-worker sequential execution (`FF-MOD-001` §6).

use std::cmp::{Ordering as CmpOrdering, Reverse};
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use file_core::{
    CancellationToken, ErrorCode, FileFusionConfig, FileFusionError, FileFusionResult, ItemOutcome,
    ItemStatus, JobContext, JobReceipt, JobStage, MemoryBudgetSnapshot, MemoryGovernor,
    ProcessingHandler, ProcessingRequest, ResourceLimits,
};
use serde::{Deserialize, Serialize};

use crate::run_validated_item;

/// Scheduling priority assigned to queued batch items (`FR-BAT-001`).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum JobPriority {
    /// Background / low-urgency batch work.
    Low = 1,
    /// Standard default batch priority.
    #[default]
    Normal = 2,
    /// Latency-sensitive interactive or foreground work.
    High = 3,
}

/// Per-item submission descriptor consumed by [`JobScheduler`].
#[derive(Debug, Clone)]
pub struct ScheduledItemSpec {
    /// 1-based batch item index matching [`ItemOutcome::item_index`].
    pub item_index: u32,
    /// 1-based template sequence number passed to naming templates.
    pub seq_1based: usize,
    /// Input filesystem path to validate and process.
    pub input_path: PathBuf,
    /// Dispatch priority level (`High` > `Normal` > `Low`).
    pub priority: JobPriority,
    /// Estimated peak working-set cost in megabytes (`1` MB minimum unit permit).
    pub estimated_memory_mb: u64,
    /// Cooperative per-item cancellation handle.
    pub cancellation: CancellationToken,
}

impl ScheduledItemSpec {
    /// Builds a [`ScheduledItemSpec`] with [`JobPriority::Normal`] and `1` MB baseline permit.
    #[must_use]
    pub fn new(item_index: u32, seq_1based: usize, input_path: impl Into<PathBuf>) -> Self {
        Self {
            item_index,
            seq_1based,
            input_path: input_path.into(),
            priority: JobPriority::Normal,
            estimated_memory_mb: 1,
            cancellation: CancellationToken::new(),
        }
    }

    /// Assigns an explicit [`JobPriority`].
    #[must_use]
    pub const fn with_priority(mut self, priority: JobPriority) -> Self {
        self.priority = priority;
        self
    }

    /// Assigns an explicit peak memory estimate in megabytes.
    #[must_use]
    pub const fn with_estimated_memory_mb(mut self, memory_mb: u64) -> Self {
        self.estimated_memory_mb = memory_mb;
        self
    }

    /// Attaches a caller-owned [`CancellationToken`] for per-item abort control.
    #[must_use]
    pub fn with_cancellation(mut self, cancellation: CancellationToken) -> Self {
        self.cancellation = cancellation;
        self
    }
}

/// Live observability snapshot of a [`SchedulerController`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulerStats {
    /// Total items submitted in the current or most recent batch.
    pub total_submitted: u32,
    /// Items currently waiting in the priority queue.
    pub queued: u32,
    /// Items currently running inside worker slots.
    pub running: u32,
    /// Items completed (succeeded, no-op, or skipped by collision policy).
    pub completed: u32,
    /// Items aborted cleanly via per-item or job cancellation.
    pub cancelled: u32,
    /// Items terminated with an error diagnostic (including `FF-ERR-RES-MEMORY`).
    pub failed: u32,
    /// Number of currently held worker slots.
    pub active_workers: usize,
    /// High-water mark of simultaneously active worker slots.
    pub peak_active_workers: usize,
    /// Currently reserved working-set memory permits in megabytes.
    pub allocated_memory_mb: u64,
    /// High-water mark of concurrently allocated memory permits in megabytes.
    pub peak_allocated_memory_mb: u64,
    /// Configured maximum worker pool bound.
    pub max_workers: usize,
    /// Configured maximum memory permit budget in megabytes.
    pub max_memory_mb: u64,
    /// Whether admission of queued items is currently paused.
    pub paused: bool,
}

#[derive(Debug)]
struct SchedulerState {
    max_workers: usize,
    max_memory_mb: u64,
    paused: AtomicBool,
    all_cancelled: AtomicBool,
    total_submitted: AtomicU32,
    queued: AtomicU32,
    running: AtomicU32,
    completed: AtomicU32,
    cancelled: AtomicU32,
    failed: AtomicU32,
    active_workers: AtomicUsize,
    peak_active_workers: AtomicUsize,
    allocated_memory_mb: AtomicU64,
    peak_allocated_memory_mb: AtomicU64,
    cancelled_items: Mutex<HashSet<u32>>,
    item_tokens: Mutex<HashMap<u32, CancellationToken>>,
    governor: MemoryGovernor,
    #[cfg(all(feature = "native-scheduler", feature = "rayon-parallel"))]
    notify: tokio::sync::Notify,
}

impl SchedulerState {
    fn new(limits: ResourceLimits) -> Self {
        Self {
            max_workers: limits.max_workers.max(1),
            max_memory_mb: limits.max_memory_mb.max(1),
            paused: AtomicBool::new(false),
            all_cancelled: AtomicBool::new(false),
            total_submitted: AtomicU32::new(0),
            queued: AtomicU32::new(0),
            running: AtomicU32::new(0),
            completed: AtomicU32::new(0),
            cancelled: AtomicU32::new(0),
            failed: AtomicU32::new(0),
            active_workers: AtomicUsize::new(0),
            peak_active_workers: AtomicUsize::new(0),
            allocated_memory_mb: AtomicU64::new(0),
            peak_allocated_memory_mb: AtomicU64::new(0),
            cancelled_items: Mutex::new(HashSet::new()),
            item_tokens: Mutex::new(HashMap::new()),
            governor: MemoryGovernor::new(limits),
            #[cfg(all(feature = "native-scheduler", feature = "rayon-parallel"))]
            notify: tokio::sync::Notify::new(),
        }
    }

    fn wake(&self) {
        #[cfg(all(feature = "native-scheduler", feature = "rayon-parallel"))]
        self.notify.notify_waiters();
        #[cfg(not(all(feature = "native-scheduler", feature = "rayon-parallel")))]
        let _ = &self.paused;
    }

    fn reset_counters_for_batch(&self, items: &[ScheduledItemSpec]) {
        self.all_cancelled.store(false, Ordering::SeqCst);
        let total = u32::try_from(items.len()).unwrap_or(u32::MAX);
        self.total_submitted.store(total, Ordering::SeqCst);
        self.queued.store(total, Ordering::SeqCst);
        self.running.store(0, Ordering::SeqCst);
        self.completed.store(0, Ordering::SeqCst);
        self.cancelled.store(0, Ordering::SeqCst);
        self.failed.store(0, Ordering::SeqCst);
        self.active_workers.store(0, Ordering::SeqCst);
        self.peak_active_workers.store(0, Ordering::SeqCst);
        self.allocated_memory_mb.store(0, Ordering::SeqCst);
        self.peak_allocated_memory_mb.store(0, Ordering::SeqCst);

        let mut tokens = match self.item_tokens.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        tokens.clear();
        let pre_cancelled = match self.cancelled_items.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        };
        for item in items {
            if pre_cancelled.contains(&item.item_index) {
                item.cancellation.cancel();
            }
            tokens.insert(item.item_index, item.cancellation.clone());
        }
    }

    fn cancel_all_tokens(&self) {
        self.all_cancelled.store(true, Ordering::SeqCst);
        let tokens = match self.item_tokens.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        for token in tokens.values() {
            token.cancel();
        }
        self.wake();
    }

    fn record_peak_workers(&self, active: usize) {
        let mut prev = self.peak_active_workers.load(Ordering::Relaxed);
        while active > prev {
            match self.peak_active_workers.compare_exchange_weak(
                prev,
                active,
                Ordering::SeqCst,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => prev = actual,
            }
        }
    }

    fn record_peak_memory(&self, allocated_mb: u64) {
        let mut prev = self.peak_allocated_memory_mb.load(Ordering::Relaxed);
        while allocated_mb > prev {
            match self.peak_allocated_memory_mb.compare_exchange_weak(
                prev,
                allocated_mb,
                Ordering::SeqCst,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => prev = actual,
            }
        }
    }

    fn mark_dispatched(&self, est_mb: u64) {
        self.queued.fetch_sub(1, Ordering::SeqCst);
        self.running.fetch_add(1, Ordering::SeqCst);
        let active = self.active_workers.fetch_add(1, Ordering::SeqCst) + 1;
        let allocated = self.allocated_memory_mb.fetch_add(est_mb, Ordering::SeqCst) + est_mb;
        self.record_peak_workers(active);
        self.record_peak_memory(allocated);
    }

    fn mark_worker_released(&self, est_mb: u64, status: ItemStatus) {
        self.running.fetch_sub(1, Ordering::SeqCst);
        self.active_workers.fetch_sub(1, Ordering::SeqCst);
        self.allocated_memory_mb.fetch_sub(est_mb, Ordering::SeqCst);
        match status {
            ItemStatus::Succeeded | ItemStatus::NoOpRetained | ItemStatus::SkippedCollision => {
                self.completed.fetch_add(1, Ordering::SeqCst);
            }
            ItemStatus::Cancelled => {
                self.cancelled.fetch_add(1, Ordering::SeqCst);
            }
            ItemStatus::Failed => {
                self.failed.fetch_add(1, Ordering::SeqCst);
            }
        }
    }

    fn mark_queued_terminal(&self, status: ItemStatus) {
        self.queued.fetch_sub(1, Ordering::SeqCst);
        match status {
            ItemStatus::Cancelled => {
                self.cancelled.fetch_add(1, Ordering::SeqCst);
            }
            ItemStatus::Failed => {
                self.failed.fetch_add(1, Ordering::SeqCst);
            }
            ItemStatus::Succeeded | ItemStatus::NoOpRetained | ItemStatus::SkippedCollision => {
                self.completed.fetch_add(1, Ordering::SeqCst);
            }
        }
    }

    fn is_item_cancelled(&self, item_index: u32) -> bool {
        let set = match self.cancelled_items.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        set.contains(&item_index)
    }
}

/// Thread-safe handle controlling pause/resume, per-item cancellation, and live metrics.
#[derive(Debug, Clone)]
pub struct SchedulerController {
    state: Arc<SchedulerState>,
}

impl SchedulerController {
    /// Creates a new [`SchedulerController`] configured for `limits`.
    #[must_use]
    pub fn new(limits: ResourceLimits) -> Self {
        Self {
            state: Arc::new(SchedulerState::new(limits)),
        }
    }

    /// Pauses admission of newly queued items while letting active workers finish cleanly.
    pub fn pause(&self) {
        self.state.paused.store(true, Ordering::SeqCst);
        self.state.wake();
    }

    /// Resumes dispatching queued items according to priority order and permit availability.
    pub fn resume(&self) {
        self.state.paused.store(false, Ordering::SeqCst);
        self.state.wake();
    }

    /// Returns `true` when queued admission is paused.
    #[must_use]
    pub fn is_paused(&self) -> bool {
        self.state.paused.load(Ordering::SeqCst)
    }

    /// Cancels a single queued or active item by its 1-based `item_index`.
    pub fn cancel_item(&self, item_index: u32) {
        {
            let mut set = match self.state.cancelled_items.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            set.insert(item_index);
        }
        {
            let tokens = match self.state.item_tokens.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            if let Some(token) = tokens.get(&item_index) {
                token.cancel();
            }
        }
        self.state.wake();
    }

    /// Returns `true` if `item_index` has been marked for cancellation.
    #[must_use]
    pub fn is_item_cancelled(&self, item_index: u32) -> bool {
        self.state.is_item_cancelled(item_index)
    }

    /// Triggers cancellation for all queued and active items immediately.
    pub fn cancel_all(&self) {
        self.state.cancel_all_tokens();
    }

    /// Returns `true` once whole-batch cancellation has been triggered.
    #[must_use]
    pub fn is_all_cancelled(&self) -> bool {
        self.state.all_cancelled.load(Ordering::SeqCst)
    }

    /// Returns the registered [`CancellationToken`] for `item_index`, if present.
    #[must_use]
    pub fn item_token(&self, item_index: u32) -> Option<CancellationToken> {
        let tokens = match self.state.item_tokens.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        tokens.get(&item_index).cloned()
    }

    /// Returns an atomic point-in-time [`SchedulerStats`] snapshot.
    #[must_use]
    pub fn snapshot(&self) -> SchedulerStats {
        SchedulerStats {
            total_submitted: self.state.total_submitted.load(Ordering::SeqCst),
            queued: self.state.queued.load(Ordering::SeqCst),
            running: self.state.running.load(Ordering::SeqCst),
            completed: self.state.completed.load(Ordering::SeqCst),
            cancelled: self.state.cancelled.load(Ordering::SeqCst),
            failed: self.state.failed.load(Ordering::SeqCst),
            active_workers: self.state.active_workers.load(Ordering::SeqCst),
            peak_active_workers: self.state.peak_active_workers.load(Ordering::SeqCst),
            allocated_memory_mb: self.state.allocated_memory_mb.load(Ordering::SeqCst),
            peak_allocated_memory_mb: self.state.peak_allocated_memory_mb.load(Ordering::SeqCst),
            max_workers: self.state.max_workers,
            max_memory_mb: self.state.max_memory_mb,
            paused: self.is_paused(),
        }
    }

    /// Returns a live [`MemoryBudgetSnapshot`] from the scheduler's [`MemoryGovernor`].
    #[must_use]
    pub fn memory_snapshot(&self) -> MemoryBudgetSnapshot {
        self.state.governor.snapshot()
    }
}

#[derive(Debug, Clone)]
struct QueuedEntry {
    priority: JobPriority,
    seq: Reverse<u32>,
    spec: ScheduledItemSpec,
}

impl PartialEq for QueuedEntry {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.seq == other.seq
    }
}

impl Eq for QueuedEntry {}

impl PartialOrd for QueuedEntry {
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
        Some(self.cmp(other))
    }
}

impl Ord for QueuedEntry {
    fn cmp(&self, other: &Self) -> CmpOrdering {
        self.priority
            .cmp(&other.priority)
            .then_with(|| self.seq.cmp(&other.seq))
    }
}

/// Resource-aware Tokio + Rayon job scheduler enforcing `ResourceLimits` (`FR-BAT-001`, `ADR-003`).
#[derive(Debug, Clone)]
pub struct JobScheduler {
    limits: ResourceLimits,
}

impl Default for JobScheduler {
    fn default() -> Self {
        Self::new(ResourceLimits::default())
    }
}

impl JobScheduler {
    /// Builds a [`JobScheduler`] governed by `limits`.
    #[must_use]
    pub const fn new(limits: ResourceLimits) -> Self {
        Self { limits }
    }

    /// Builds a [`JobScheduler`] from a validated [`FileFusionConfig`].
    #[must_use]
    pub const fn from_config(config: &FileFusionConfig) -> Self {
        Self::new(config.limits)
    }

    /// Returns the [`ResourceLimits`] enforced by this scheduler.
    #[must_use]
    pub const fn limits(&self) -> ResourceLimits {
        self.limits
    }

    /// Creates a compatible [`SchedulerController`] bound to this scheduler's ceilings.
    #[must_use]
    pub fn create_controller(&self) -> SchedulerController {
        SchedulerController::new(self.limits)
    }

    /// Schedules all `req.inputs` with [`JobPriority::Normal`] and `1` MB unit estimates.
    ///
    /// # Errors
    /// Returns [`FileFusionError`] when `req.inputs` is empty or the batch is globally cancelled.
    pub fn execute_batch(
        &self,
        req: &ProcessingRequest,
        handler: Option<&dyn ProcessingHandler>,
        ctx: &JobContext,
    ) -> FileFusionResult<JobReceipt> {
        let specs: Vec<ScheduledItemSpec> = req
            .inputs
            .iter()
            .enumerate()
            .map(|(idx, path)| {
                let item_index = u32::try_from(idx + 1).unwrap_or(u32::MAX);
                ScheduledItemSpec::new(item_index, idx + 1, path.clone())
                    .with_cancellation(ctx.cancellation.child())
            })
            .collect();
        self.execute_scheduled_items(req, &specs, handler, None, ctx)
    }

    /// Executes custom [`ScheduledItemSpec`] items respecting priority, memory permits,
    /// pause/resume, per-item cancellation, and bounded dual-pool concurrency.
    ///
    /// # Errors
    /// Returns [`FileFusionError`] when `items` is empty, thread pools fail to initialize,
    /// or job-wide [`CancellationToken`] aborts execution (`ErrorCode::JobCancelled`).
    pub fn execute_scheduled_items(
        &self,
        req: &ProcessingRequest,
        items: &[ScheduledItemSpec],
        handler: Option<&dyn ProcessingHandler>,
        controller: Option<&SchedulerController>,
        ctx: &JobContext,
    ) -> FileFusionResult<JobReceipt> {
        if items.is_empty() {
            return Err(file_core::FileFusionError::new(
                ErrorCode::FmtUnsupported,
                "no input files provided",
            )
            .with_exit_code(file_core::CliExitCode::InvalidUsage)
            .with_remediation("Supply at least one input file path."));
        }

        ctx.check_cancelled()?;
        let owned_ctrl;
        let active_ctrl = if let Some(c) = controller {
            if c.is_all_cancelled() {
                return Err(
                    FileFusionError::new(ErrorCode::JobCancelled, "job cancelled by user")
                        .with_remediation("Temporary staging files have been removed."),
                );
            }
            c
        } else {
            owned_ctrl = self.create_controller();
            &owned_ctrl
        };

        active_ctrl.state.reset_counters_for_batch(items);
        let span = tracing::info_span!(
            "schedule_batch",
            job_id = %req.job_id,
            items = items.len(),
            max_workers = self.limits.max_workers.max(1),
            max_memory_mb = self.limits.max_memory_mb.max(1)
        );
        let _guard = span.enter();

        let mut outcomes = self.run_scheduled_items_impl(req, items, handler, active_ctrl, ctx)?;
        outcomes.sort_by_key(|o| o.item_index);

        let final_stage = if outcomes.iter().any(|o| o.status == ItemStatus::Failed) {
            JobStage::Failed
        } else if outcomes.iter().all(|o| o.status == ItemStatus::Cancelled) {
            JobStage::Cancelled
        } else {
            JobStage::Completed
        };
        Ok(JobReceipt::from_outcomes(
            req.job_id.clone(),
            final_stage,
            outcomes,
        ))
    }
}

fn cancelled_item_outcome(spec: &ScheduledItemSpec, total: u32, ctx: &JobContext) -> ItemOutcome {
    let pct = (f32::from(u16::try_from(spec.item_index).unwrap_or(u16::MAX))
        / f32::from(u16::try_from(total.max(1)).unwrap_or(1)))
        * 100.0;
    let err = FileFusionError::new(ErrorCode::JobCancelled, "item cancelled by user");
    ctx.emit_progress(
        spec.item_index,
        total,
        JobStage::Cancelled,
        pct,
        0,
        err.diagnostic.message.clone(),
    );
    ItemOutcome {
        item_index: spec.item_index,
        source_path: spec.input_path.clone(),
        output_path: None,
        status: ItemStatus::Cancelled,
        compression_status: None,
        bytes_in: 0,
        bytes_out: 0,
        blake3_in: String::new(),
        blake3_out: None,
        error: Some(err.diagnostic),
    }
}

fn memory_rejected_outcome(
    spec: &ScheduledItemSpec,
    est_mb: u64,
    max_mb: u64,
    total: u32,
    ctx: &JobContext,
) -> ItemOutcome {
    let pct = (f32::from(u16::try_from(spec.item_index).unwrap_or(u16::MAX))
        / f32::from(u16::try_from(total.max(1)).unwrap_or(1)))
        * 100.0;
    let err = FileFusionError::new(
        ErrorCode::ResMemory,
        format!(
            "estimated working set ({est_mb} MB) exceeds configured max_memory_mb ({max_mb} MB)"
        ),
    )
    .with_remediation("Increase max_memory_mb or split workload into smaller inputs.");
    ctx.emit_progress(
        spec.item_index,
        total,
        JobStage::Failed,
        pct,
        0,
        err.diagnostic.message.clone(),
    );
    ItemOutcome {
        item_index: spec.item_index,
        source_path: spec.input_path.clone(),
        output_path: None,
        status: ItemStatus::Failed,
        compression_status: None,
        bytes_in: 0,
        bytes_out: 0,
        blake3_in: String::new(),
        blake3_out: None,
        error: Some(err.diagnostic),
    }
}

fn drain_cancelled_entries(
    heap: &mut BinaryHeap<QueuedEntry>,
    state: &SchedulerState,
    total: u32,
    ctx: &JobContext,
    outcomes: &mut Vec<ItemOutcome>,
) {
    let drained: Vec<QueuedEntry> = heap.drain().collect();
    for entry in drained {
        if state.is_item_cancelled(entry.spec.item_index) || entry.spec.cancellation.is_cancelled()
        {
            state.mark_queued_terminal(ItemStatus::Cancelled);
            outcomes.push(cancelled_item_outcome(&entry.spec, total, ctx));
        } else {
            heap.push(entry);
        }
    }
}

fn build_priority_heap(
    items: &[ScheduledItemSpec],
    max_memory_mb: u64,
    total: u32,
    state: &SchedulerState,
    ctx: &JobContext,
) -> (BinaryHeap<QueuedEntry>, Vec<ItemOutcome>) {
    let mut heap = BinaryHeap::with_capacity(items.len());
    let mut outcomes = Vec::with_capacity(items.len());
    for spec in items {
        let est_mb = spec.estimated_memory_mb.max(1);
        if est_mb > max_memory_mb {
            state.mark_queued_terminal(ItemStatus::Failed);
            outcomes.push(memory_rejected_outcome(
                spec,
                est_mb,
                max_memory_mb,
                total,
                ctx,
            ));
            continue;
        }
        heap.push(QueuedEntry {
            priority: spec.priority,
            seq: Reverse(spec.item_index),
            spec: spec.clone(),
        });
    }
    (heap, outcomes)
}

#[cfg(all(feature = "native-scheduler", feature = "rayon-parallel"))]
struct WorkerCompletion {
    est_mb: u64,
    reservation: file_core::MemoryReservation,
    spec: ScheduledItemSpec,
    result: FileFusionResult<ItemOutcome>,
}

#[cfg(all(feature = "native-scheduler", feature = "rayon-parallel"))]
fn build_native_pools(
    max_workers: usize,
) -> FileFusionResult<(tokio::runtime::Runtime, rayon::ThreadPool)> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(max_workers.clamp(1, 4))
        .enable_time()
        .thread_name("ff-sched-io")
        .build()
        .map_err(|err| {
            FileFusionError::new(
                ErrorCode::IntInvariant,
                format!("failed to initialize async job scheduler runtime: {err}"),
            )
        })?;

    let rayon_pool = rayon::ThreadPoolBuilder::new()
        .num_threads(max_workers)
        .thread_name(|idx| format!("ff-sched-cpu-{idx}"))
        .build()
        .map_err(|err| {
            FileFusionError::new(
                ErrorCode::IntInvariant,
                format!("failed to initialize CPU worker pool: {err}"),
            )
        })?;

    Ok((rt, rayon_pool))
}

#[cfg(all(feature = "native-scheduler", feature = "rayon-parallel"))]
struct NativeLoopState<'a> {
    limits: ResourceLimits,
    total: u32,
    max_workers: usize,
    active_workers: usize,
    cancelled_aborted: bool,
    heap: BinaryHeap<QueuedEntry>,
    outcomes: Vec<ItemOutcome>,
    controller: &'a SchedulerController,
    req: &'a ProcessingRequest,
    handler: Option<&'a dyn ProcessingHandler>,
    ctx: &'a JobContext,
}

#[cfg(all(feature = "native-scheduler", feature = "rayon-parallel"))]
impl NativeLoopState<'_> {
    fn handle_completion(&mut self, done: WorkerCompletion) -> FileFusionResult<()> {
        drop(done.reservation);
        self.active_workers = self.active_workers.saturating_sub(1);
        match done.result {
            Ok(outcome) => {
                self.controller
                    .state
                    .mark_worker_released(done.est_mb, outcome.status);
                self.outcomes.push(outcome);
            }
            Err(err) => {
                if err.diagnostic.code == ErrorCode::JobCancelled
                    && !self.ctx.cancellation.is_cancelled()
                    && !self.controller.is_all_cancelled()
                {
                    self.controller
                        .state
                        .mark_worker_released(done.est_mb, ItemStatus::Cancelled);
                    self.outcomes
                        .push(cancelled_item_outcome(&done.spec, self.total, self.ctx));
                } else {
                    self.controller
                        .state
                        .mark_worker_released(done.est_mb, ItemStatus::Failed);
                    return Err(err);
                }
            }
        }
        Ok(())
    }
}

#[cfg(all(feature = "native-scheduler", feature = "rayon-parallel"))]
fn dispatch_ready_entries<'a, 'scope>(
    loop_state: &mut NativeLoopState<'a>,
    cpu_scope: &rayon::Scope<'scope>,
    done_tx: &tokio::sync::mpsc::UnboundedSender<WorkerCompletion>,
) where
    'a: 'scope,
{
    while !loop_state.controller.is_paused() && loop_state.active_workers < loop_state.max_workers {
        let Some(next_entry) = loop_state.heap.peek() else {
            break;
        };
        let est_mb = next_entry.spec.estimated_memory_mb.max(1);
        let purpose = format!("scheduled_item_{}", next_entry.spec.item_index);
        let Ok(reservation) = loop_state
            .controller
            .state
            .governor
            .try_reserve_mb(est_mb, &purpose)
        else {
            break;
        };
        let Some(entry) = loop_state.heap.pop() else {
            drop(reservation);
            break;
        };
        let spec = entry.spec;
        loop_state.controller.state.mark_dispatched(est_mb);
        loop_state.active_workers += 1;

        let limits = loop_state.limits;
        let total = loop_state.total;
        let req = loop_state.req;
        let handler = loop_state.handler;
        let ctx = loop_state.ctx;
        let tx = done_tx.clone();

        cpu_scope.spawn(move |_| {
            let mut item_ctx = ctx.clone();
            item_ctx.cancellation = spec.cancellation.clone();
            let res = run_validated_item(
                &limits,
                &spec.input_path,
                spec.item_index,
                spec.seq_1based,
                total,
                req,
                handler,
                &item_ctx,
            );
            let _ = tx.send(WorkerCompletion {
                est_mb,
                reservation,
                spec,
                result: res,
            });
        });
    }
}

impl JobScheduler {
    #[cfg(all(feature = "native-scheduler", feature = "rayon-parallel"))]
    fn run_scheduled_items_impl(
        &self,
        req: &ProcessingRequest,
        items: &[ScheduledItemSpec],
        handler: Option<&dyn ProcessingHandler>,
        controller: &SchedulerController,
        ctx: &JobContext,
    ) -> FileFusionResult<Vec<ItemOutcome>> {
        use std::time::Duration;

        let max_workers = self.limits.max_workers.max(1);
        let max_memory_mb = self.limits.max_memory_mb.max(1);
        let total = u32::try_from(items.len()).unwrap_or(u32::MAX);
        let (rt, rayon_pool) = build_native_pools(max_workers)?;
        let (heap, outcomes) =
            build_priority_heap(items, max_memory_mb, total, &controller.state, ctx);

        let mut loop_state = NativeLoopState {
            limits: self.limits,
            total,
            max_workers,
            active_workers: 0,
            cancelled_aborted: false,
            heap,
            outcomes,
            controller,
            req,
            handler,
            ctx,
        };

        let _cancel_sub = ctx.cancellation.subscribe_on_cancel({
            let ctrl = controller.clone();
            move || ctrl.cancel_all()
        });

        let loop_res: FileFusionResult<()> = rayon_pool.in_place_scope(|cpu_scope| {
            rt.block_on(async {
                let (done_tx, mut done_rx) =
                    tokio::sync::mpsc::unbounded_channel::<WorkerCompletion>();

                loop {
                    if loop_state.ctx.cancellation.is_cancelled()
                        || loop_state.controller.is_all_cancelled()
                    {
                        loop_state.controller.cancel_all();
                        loop_state.cancelled_aborted = true;
                        let pending_rem = loop_state.heap.drain().count();
                        if pending_rem > 0 {
                            let rem_u32 = u32::try_from(pending_rem).unwrap_or(u32::MAX);
                            loop_state
                                .controller
                                .state
                                .queued
                                .fetch_sub(rem_u32, Ordering::SeqCst);
                            loop_state
                                .controller
                                .state
                                .cancelled
                                .fetch_add(rem_u32, Ordering::SeqCst);
                        }
                    } else {
                        drain_cancelled_entries(
                            &mut loop_state.heap,
                            &loop_state.controller.state,
                            loop_state.total,
                            loop_state.ctx,
                            &mut loop_state.outcomes,
                        );
                        dispatch_ready_entries(&mut loop_state, cpu_scope, &done_tx);
                    }

                    if loop_state.active_workers == 0
                        && (loop_state.heap.is_empty() || loop_state.cancelled_aborted)
                    {
                        break;
                    }

                    tokio::select! {
                        maybe_done = done_rx.recv(), if loop_state.active_workers > 0 => {
                            if let Some(done) = maybe_done {
                                loop_state.handle_completion(done)?;
                            }
                        }
                        () = loop_state.controller.state.notify.notified(), if !loop_state.cancelled_aborted => {}
                        () = tokio::time::sleep(Duration::from_millis(20)), if !loop_state.cancelled_aborted => {}
                    }
                }

                if loop_state.cancelled_aborted {
                    return Err(FileFusionError::new(
                        ErrorCode::JobCancelled,
                        "job cancelled by user",
                    )
                    .with_remediation("Temporary staging files have been removed."));
                }
                Ok(())
            })
        });

        loop_res?;
        Ok(loop_state.outcomes)
    }

    #[cfg(not(all(feature = "native-scheduler", feature = "rayon-parallel")))]
    fn run_scheduled_items_impl(
        &self,
        req: &ProcessingRequest,
        items: &[ScheduledItemSpec],
        handler: Option<&dyn ProcessingHandler>,
        controller: &SchedulerController,
        ctx: &JobContext,
    ) -> FileFusionResult<Vec<ItemOutcome>> {
        let max_memory_mb = self.limits.max_memory_mb.max(1);
        let total = u32::try_from(items.len()).unwrap_or(u32::MAX);
        let state = &controller.state;
        let (mut heap, mut outcomes) = build_priority_heap(items, max_memory_mb, total, state, ctx);

        drain_cancelled_entries(&mut heap, state, total, ctx, &mut outcomes);
        while let Some(entry) = heap.pop() {
            if ctx.cancellation.is_cancelled() || controller.is_all_cancelled() {
                controller.cancel_all();
                return Err(
                    FileFusionError::new(ErrorCode::JobCancelled, "job cancelled by user")
                        .with_remediation("Temporary staging files have been removed."),
                );
            }
            if state.is_item_cancelled(entry.spec.item_index)
                || entry.spec.cancellation.is_cancelled()
            {
                state.mark_queued_terminal(ItemStatus::Cancelled);
                outcomes.push(cancelled_item_outcome(&entry.spec, total, ctx));
                continue;
            }

            let est_mb = entry.spec.estimated_memory_mb.max(1);
            let purpose = format!("scheduled_item_{}", entry.spec.item_index);
            let mem_guard = state.governor.try_reserve_mb(est_mb, &purpose)?;
            state.mark_dispatched(est_mb);
            let mut item_ctx = ctx.clone();
            item_ctx.cancellation = entry.spec.cancellation.clone();

            let item_res = run_validated_item(
                &self.limits,
                &entry.spec.input_path,
                entry.spec.item_index,
                entry.spec.seq_1based,
                total,
                req,
                handler,
                &item_ctx,
            );
            drop(mem_guard);

            match item_res {
                Ok(outcome) => {
                    state.mark_worker_released(est_mb, outcome.status);
                    outcomes.push(outcome);
                }
                Err(err) => {
                    if err.diagnostic.code == ErrorCode::JobCancelled
                        && !ctx.cancellation.is_cancelled()
                        && !controller.is_all_cancelled()
                    {
                        state.mark_worker_released(est_mb, ItemStatus::Cancelled);
                        outcomes.push(cancelled_item_outcome(&entry.spec, total, ctx));
                    } else {
                        state.mark_worker_released(est_mb, ItemStatus::Failed);
                        return Err(err);
                    }
                }
            }
        }

        Ok(outcomes)
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::fs;
    #[cfg(all(feature = "native-scheduler", feature = "rayon-parallel"))]
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::AtomicUsize;
    use std::time::Duration;
    #[cfg(all(feature = "native-scheduler", feature = "rayon-parallel"))]
    use std::time::Instant;

    use file_core::{
        BatchDefaults, CORE_SCHEMA_VERSION, CompressionStatus, ProcessingOperation, ValidatedSource,
    };

    use super::*;

    fn temp_sched_dir(label: &str) -> Result<PathBuf, Box<dyn Error>> {
        let dir =
            std::env::temp_dir().join(format!("filefusion_sched_{label}_{}", std::process::id()));
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    struct ConcurrencyTrackingHandler {
        current: AtomicUsize,
        peak: AtomicUsize,
        hold: Duration,
        order: Mutex<Vec<u32>>,
    }

    impl ConcurrencyTrackingHandler {
        fn new(hold: Duration) -> Self {
            Self {
                current: AtomicUsize::new(0),
                peak: AtomicUsize::new(0),
                hold,
                order: Mutex::new(Vec::new()),
            }
        }
    }

    impl ProcessingHandler for ConcurrencyTrackingHandler {
        fn process_item(
            &self,
            source: &ValidatedSource,
            destination: Option<&Path>,
            _req: &ProcessingRequest,
            item_index: u32,
            ctx: &JobContext,
        ) -> FileFusionResult<ItemOutcome> {
            let active = self.current.fetch_add(1, Ordering::SeqCst) + 1;
            let mut prev = self.peak.load(Ordering::Relaxed);
            while active > prev {
                match self.peak.compare_exchange_weak(
                    prev,
                    active,
                    Ordering::SeqCst,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => break,
                    Err(v) => prev = v,
                }
            }

            {
                let mut log = match self.order.lock() {
                    Ok(g) => g,
                    Err(p) => p.into_inner(),
                };
                log.push(item_index);
            }

            if !self.hold.is_zero() {
                std::thread::sleep(self.hold);
            }
            let check = ctx.check_cancelled();
            self.current.fetch_sub(1, Ordering::SeqCst);
            check?;

            Ok(ItemOutcome {
                item_index,
                source_path: source.path.clone(),
                output_path: destination.map(Path::to_path_buf),
                status: ItemStatus::Succeeded,
                compression_status: Some(CompressionStatus::NoOpRetainedOriginal),
                bytes_in: source.inspection.byte_size,
                bytes_out: source.inspection.byte_size,
                blake3_in: source.inspection.digests.blake3.clone(),
                blake3_out: None,
                error: None,
            })
        }
    }

    use std::path::Path;

    fn sample_request(job_id: &str, inputs: Vec<PathBuf>, out_dir: PathBuf) -> ProcessingRequest {
        ProcessingRequest {
            schema_version: CORE_SCHEMA_VERSION,
            job_id: String::from(job_id),
            inputs,
            output_dir: Some(out_dir),
            operation: ProcessingOperation::InspectOnly,
            batch: BatchDefaults::default(),
            strict_spoofing: false,
            dry_run: true,
        }
    }

    #[test]
    fn scheduler_executes_1200_items_within_worker_and_memory_bounds() -> Result<(), Box<dyn Error>>
    {
        let dir = temp_sched_dir("stress_1200")?;
        let sample = dir.join("payload.txt");
        fs::write(&sample, b"bounded-concurrency-payload")?;

        let count = 1200usize;
        let inputs = vec![sample; count];
        let mut req = sample_request("sched_1200", inputs.clone(), dir.clone());
        req.operation = ProcessingOperation::Compress {
            profile: file_core::CompressionProfile::BalancedOptimal,
            target_format: None,
            strip_metadata: false,
        };
        req.dry_run = false;

        let limits = ResourceLimits {
            max_workers: 3,
            max_memory_mb: 128,
            ..ResourceLimits::default()
        };
        let scheduler = JobScheduler::new(limits);
        let controller = scheduler.create_controller();
        let specs: Vec<ScheduledItemSpec> = inputs
            .iter()
            .enumerate()
            .map(|(i, p)| {
                ScheduledItemSpec::new(u32::try_from(i + 1).unwrap_or(u32::MAX), i + 1, p)
                    .with_estimated_memory_mb(4)
            })
            .collect();

        let handler = ConcurrencyTrackingHandler::new(Duration::ZERO);
        let ctx = JobContext::new("sched_1200", limits);
        let receipt = scheduler.execute_scheduled_items(
            &req,
            &specs,
            Some(&handler),
            Some(&controller),
            &ctx,
        )?;

        assert_eq!(receipt.status, JobStage::Completed);
        assert_eq!(receipt.total_items, 1200);
        assert_eq!(receipt.succeeded, 1200);
        assert_eq!(receipt.failed, 0);
        for (idx, item) in receipt.outcomes.iter().enumerate() {
            assert_eq!(item.item_index, u32::try_from(idx + 1).unwrap_or(u32::MAX));
        }

        let stats = controller.snapshot();
        assert_eq!(stats.completed, 1200);
        assert_eq!(stats.active_workers, 0);
        assert_eq!(stats.allocated_memory_mb, 0);
        assert!(stats.peak_active_workers <= 3);
        assert!(stats.peak_allocated_memory_mb <= 128);
        assert!(handler.peak.load(Ordering::SeqCst) <= 3);

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn scheduler_enforces_memory_permits_and_rejects_oversized_tasks() -> Result<(), Box<dyn Error>>
    {
        let dir = temp_sched_dir("mem_permits")?;
        let sample = dir.join("frame.txt");
        fs::write(&sample, b"heavy-frame")?;

        let mut req = sample_request("sched_mem", vec![sample.clone(); 3], dir.clone());
        req.operation = ProcessingOperation::Compress {
            profile: file_core::CompressionProfile::BalancedOptimal,
            target_format: None,
            strip_metadata: false,
        };
        req.dry_run = false;

        let limits = ResourceLimits {
            max_workers: 4,
            max_memory_mb: 100,
            ..ResourceLimits::default()
        };
        let scheduler = JobScheduler::new(limits);
        let controller = scheduler.create_controller();
        let specs = vec![
            ScheduledItemSpec::new(1, 1, &sample).with_estimated_memory_mb(60),
            ScheduledItemSpec::new(2, 2, &sample).with_estimated_memory_mb(60),
            ScheduledItemSpec::new(3, 3, &sample).with_estimated_memory_mb(150),
        ];

        let handler = ConcurrencyTrackingHandler::new(Duration::from_millis(15));
        let ctx = JobContext::new("sched_mem", limits);
        let receipt = scheduler.execute_scheduled_items(
            &req,
            &specs,
            Some(&handler),
            Some(&controller),
            &ctx,
        )?;

        assert_eq!(receipt.status, JobStage::Failed);
        assert_eq!(receipt.succeeded, 2);
        assert_eq!(receipt.failed, 1);
        assert_eq!(receipt.outcomes[2].status, ItemStatus::Failed);
        let err = receipt.outcomes[2].error.as_ref();
        assert_eq!(err.map(|e| e.code), Some(ErrorCode::ResMemory));

        let stats = controller.snapshot();
        assert_eq!(stats.peak_allocated_memory_mb, 60);
        assert_eq!(handler.peak.load(Ordering::SeqCst), 1);

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn scheduler_respects_priority_ordering_and_fifo_ties() -> Result<(), Box<dyn Error>> {
        let dir = temp_sched_dir("priority")?;
        let sample = dir.join("doc.txt");
        fs::write(&sample, b"priority-order-test")?;

        let mut req = sample_request("sched_prio", vec![sample.clone(); 5], dir.clone());
        req.operation = ProcessingOperation::Compress {
            profile: file_core::CompressionProfile::BalancedOptimal,
            target_format: None,
            strip_metadata: false,
        };
        req.dry_run = false;

        let limits = ResourceLimits {
            max_workers: 1,
            max_memory_mb: 256,
            ..ResourceLimits::default()
        };
        let scheduler = JobScheduler::new(limits);
        let specs = vec![
            ScheduledItemSpec::new(1, 1, &sample).with_priority(JobPriority::Low),
            ScheduledItemSpec::new(2, 2, &sample).with_priority(JobPriority::Normal),
            ScheduledItemSpec::new(3, 3, &sample).with_priority(JobPriority::High),
            ScheduledItemSpec::new(4, 4, &sample).with_priority(JobPriority::Normal),
            ScheduledItemSpec::new(5, 5, &sample).with_priority(JobPriority::High),
        ];

        let handler = ConcurrencyTrackingHandler::new(Duration::ZERO);
        let ctx = JobContext::new("sched_prio", limits);
        let receipt =
            scheduler.execute_scheduled_items(&req, &specs, Some(&handler), None, &ctx)?;
        assert_eq!(receipt.succeeded, 5);

        let observed = match handler.order.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        };
        assert_eq!(observed, vec![3, 5, 2, 4, 1]);

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[cfg(all(feature = "native-scheduler", feature = "rayon-parallel"))]
    struct PauseTriggerHandler {
        controller: SchedulerController,
        first_started: Arc<AtomicBool>,
    }

    #[cfg(all(feature = "native-scheduler", feature = "rayon-parallel"))]
    impl ProcessingHandler for PauseTriggerHandler {
        fn process_item(
            &self,
            source: &ValidatedSource,
            destination: Option<&Path>,
            _req: &ProcessingRequest,
            item_index: u32,
            _ctx: &JobContext,
        ) -> FileFusionResult<ItemOutcome> {
            if item_index == 1 {
                self.controller.pause();
                self.first_started.store(true, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(25));
            }
            Ok(ItemOutcome {
                item_index,
                source_path: source.path.clone(),
                output_path: destination.map(Path::to_path_buf),
                status: ItemStatus::Succeeded,
                compression_status: None,
                bytes_in: source.inspection.byte_size,
                bytes_out: source.inspection.byte_size,
                blake3_in: source.inspection.digests.blake3.clone(),
                blake3_out: None,
                error: None,
            })
        }
    }

    #[cfg(all(feature = "native-scheduler", feature = "rayon-parallel"))]
    #[test]
    fn scheduler_supports_pause_resume_and_per_item_cancellation() -> Result<(), Box<dyn Error>> {
        let dir = temp_sched_dir("pause_cancel")?;
        let sample = dir.join("item.txt");
        fs::write(&sample, b"pause-resume-item")?;

        let mut req = sample_request("sched_pause", vec![sample.clone(); 3], dir.clone());
        req.operation = ProcessingOperation::Compress {
            profile: file_core::CompressionProfile::BalancedOptimal,
            target_format: None,
            strip_metadata: false,
        };
        req.dry_run = false;

        let limits = ResourceLimits {
            max_workers: 1,
            max_memory_mb: 256,
            ..ResourceLimits::default()
        };
        let scheduler = JobScheduler::new(limits);
        let controller = scheduler.create_controller();
        let ctrl_thread = controller.clone();
        let first_started = Arc::new(AtomicBool::new(false));
        let flag_thread = Arc::clone(&first_started);

        let control_handle = std::thread::spawn(move || {
            while !flag_thread.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(5));
            }
            std::thread::sleep(Duration::from_millis(35));
            ctrl_thread.cancel_item(2);
            ctrl_thread.resume();
        });

        let specs = vec![
            ScheduledItemSpec::new(1, 1, &sample).with_priority(JobPriority::High),
            ScheduledItemSpec::new(2, 2, &sample).with_priority(JobPriority::Normal),
            ScheduledItemSpec::new(3, 3, &sample).with_priority(JobPriority::Low),
        ];
        let handler = PauseTriggerHandler {
            controller: controller.clone(),
            first_started,
        };
        let ctx = JobContext::new("sched_pause", limits);
        let receipt = scheduler.execute_scheduled_items(
            &req,
            &specs,
            Some(&handler),
            Some(&controller),
            &ctx,
        )?;
        let _ = control_handle.join();

        assert_eq!(receipt.status, JobStage::Completed);
        assert_eq!(receipt.succeeded, 2);
        assert_eq!(receipt.outcomes[0].status, ItemStatus::Succeeded);
        assert_eq!(receipt.outcomes[1].status, ItemStatus::Cancelled);
        assert_eq!(receipt.outcomes[2].status, ItemStatus::Succeeded);

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[cfg(all(feature = "native-scheduler", feature = "rayon-parallel"))]
    #[test]
    fn scheduler_aborts_promptly_on_global_cancellation() -> Result<(), Box<dyn Error>> {
        let dir = temp_sched_dir("global_cancel")?;
        let sample = dir.join("wait.txt");
        fs::write(&sample, b"cancel-latency-check")?;

        let req = sample_request("sched_global_cancel", vec![sample.clone(); 4], dir.clone());
        let limits = ResourceLimits {
            max_workers: 2,
            max_memory_mb: 256,
            ..ResourceLimits::default()
        };
        let scheduler = JobScheduler::new(limits);
        let controller = scheduler.create_controller();
        controller.pause();

        let ctx = JobContext::new("sched_global_cancel", limits);
        let token = ctx.cancellation.clone();
        let cancel_handle = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            token.cancel();
        });

        let started = Instant::now();
        let specs: Vec<ScheduledItemSpec> = (1..=4u32)
            .map(|i| ScheduledItemSpec::new(i, usize::try_from(i).unwrap_or(1), &sample))
            .collect();
        let res = scheduler.execute_scheduled_items(&req, &specs, None, Some(&controller), &ctx);
        let elapsed = started.elapsed();
        let _ = cancel_handle.join();

        assert!(res.is_err());
        if let Err(err) = res {
            assert_eq!(err.diagnostic.code, ErrorCode::JobCancelled);
        }
        assert!(elapsed < Duration::from_millis(450));

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn scheduler_cancels_marked_item_without_aborting_remaining_batch() -> Result<(), Box<dyn Error>>
    {
        let dir = temp_sched_dir("pre_cancel_item")?;
        let sample = dir.join("batch.txt");
        fs::write(&sample, b"per-item-cancel-data")?;

        let req = sample_request("sched_item_cancel", vec![sample.clone(); 3], dir.clone());
        let limits = ResourceLimits::default();
        let scheduler = JobScheduler::new(limits);
        let controller = scheduler.create_controller();
        controller.cancel_item(2);

        let specs = vec![
            ScheduledItemSpec::new(1, 1, &sample),
            ScheduledItemSpec::new(2, 2, &sample),
            ScheduledItemSpec::new(3, 3, &sample),
        ];
        let ctx = JobContext::new("sched_item_cancel", limits);
        let receipt =
            scheduler.execute_scheduled_items(&req, &specs, None, Some(&controller), &ctx)?;

        assert_eq!(receipt.status, JobStage::Completed);
        assert_eq!(receipt.succeeded, 2);
        assert_eq!(receipt.outcomes[0].status, ItemStatus::Succeeded);
        assert_eq!(receipt.outcomes[1].status, ItemStatus::Cancelled);
        assert_eq!(receipt.outcomes[2].status, ItemStatus::Succeeded);

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }
}
