//! L5 structured progress bus, rolling ETA estimation, and NDJSON formatting
//! (`FF-ARCH-001`, `FF-CTR-001` §2.3, §5.3).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use file_core::{
    ErrorCode, FileFusionError, FileFusionResult, JobStage, ProgressEvent, ProgressSink,
};
use serde::{Deserialize, Serialize};

#[derive(Debug)]
struct ProgressBusInner {
    next_sub_id: AtomicU64,
    total_emitted: AtomicU64,
    subscribers: Mutex<Vec<(u64, Sender<ProgressEvent>)>>,
    items: Mutex<HashMap<(String, u32), ProgressEvent>>,
    #[cfg(not(target_arch = "wasm32"))]
    job_starts: Mutex<HashMap<String, std::time::Instant>>,
    last_eta_ms: Mutex<Option<u64>>,
}

impl Default for ProgressBusInner {
    fn default() -> Self {
        Self {
            next_sub_id: AtomicU64::new(1),
            total_emitted: AtomicU64::new(0),
            subscribers: Mutex::new(Vec::new()),
            items: Mutex::new(HashMap::new()),
            #[cfg(not(target_arch = "wasm32"))]
            job_starts: Mutex::new(HashMap::new()),
            last_eta_ms: Mutex::new(None),
        }
    }
}

/// Summary metrics reported by [`ProgressBus::stats`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgressBusStats {
    /// Total normalized events broadcast across all subscribers.
    pub total_emitted: u64,
    /// Number of currently attached stream subscribers.
    pub active_subscribers: usize,
    /// Distinct `(job_id, item_index)` items observed so far.
    pub tracked_items: usize,
    /// Items that have reached a terminal stage (`Completed`, `Cancelled`, or `Failed`).
    pub completed_items: usize,
    /// Most recent empirically estimated remaining milliseconds, if available.
    pub last_eta_ms: Option<u64>,
}

/// Live stream subscription returned by [`ProgressBus::subscribe`].
///
/// Automatically unsubscribes from the bus when dropped.
#[derive(Debug)]
pub struct ProgressSubscription {
    id: u64,
    rx: Receiver<ProgressEvent>,
    bus: Arc<ProgressBusInner>,
}

impl ProgressSubscription {
    /// Attempts to read one pending [`ProgressEvent`] without blocking.
    #[must_use]
    pub fn try_recv(&self) -> Option<ProgressEvent> {
        self.rx.try_recv().ok()
    }

    /// Blocks up to `timeout` waiting for the next [`ProgressEvent`].
    #[must_use]
    pub fn recv_timeout(&self, timeout: Duration) -> Option<ProgressEvent> {
        self.rx.recv_timeout(timeout).ok()
    }

    /// Drains all currently queued [`ProgressEvent`] updates without blocking.
    #[must_use]
    pub fn drain(&self) -> Vec<ProgressEvent> {
        let mut out = Vec::new();
        while let Some(ev) = self.try_recv() {
            out.push(ev);
        }
        out
    }
}

impl Drop for ProgressSubscription {
    fn drop(&mut self) {
        let mut guard = match self.bus.subscribers.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard.retain(|(id, _)| *id != self.id);
    }
}

/// Multi-subscriber structured progress bus computing monotonic per-item progress
/// and rolling empirical ETA estimates (`FF-CTR-001` §2.3).
#[derive(Debug, Clone, Default)]
pub struct ProgressBus {
    inner: Arc<ProgressBusInner>,
}

impl ProgressBus {
    /// Creates a new [`ProgressBus`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Subscribes to live normalized [`ProgressEvent`] updates.
    #[must_use]
    pub fn subscribe(&self) -> ProgressSubscription {
        let id = self.inner.next_sub_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel();
        let mut guard = match self.inner.subscribers.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard.push((id, tx));
        ProgressSubscription {
            id,
            rx,
            bus: Arc::clone(&self.inner),
        }
    }

    /// Returns a snapshot of the latest normalized [`ProgressEvent`] per tracked item.
    #[must_use]
    pub fn latest_events(&self) -> Vec<ProgressEvent> {
        let guard = match self.inner.items.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let mut out: Vec<ProgressEvent> = guard.values().cloned().collect();
        out.sort_by(|a, b| {
            a.job_id
                .cmp(&b.job_id)
                .then_with(|| a.item_index.cmp(&b.item_index))
        });
        out
    }

    /// Returns current throughput/delivery counters from the bus.
    #[must_use]
    pub fn stats(&self) -> ProgressBusStats {
        let active_subscribers = match self.inner.subscribers.lock() {
            Ok(g) => g.len(),
            Err(p) => p.into_inner().len(),
        };
        let (tracked_items, completed_items) = {
            let guard = match self.inner.items.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            let done = guard
                .values()
                .filter(|e| {
                    matches!(
                        e.stage,
                        JobStage::Completed | JobStage::Cancelled | JobStage::Failed
                    )
                })
                .count();
            (guard.len(), done)
        };
        let last_eta_ms = match self.inner.last_eta_ms.lock() {
            Ok(g) => *g,
            Err(p) => *p.into_inner(),
        };
        ProgressBusStats {
            total_emitted: self.inner.total_emitted.load(Ordering::SeqCst),
            active_subscribers,
            tracked_items,
            completed_items,
            last_eta_ms,
        }
    }

    /// Formats a single [`ProgressEvent`] as compact newline-ready JSON (`FF-CTR-001` §5.3).
    ///
    /// # Errors
    /// Returns [`FileFusionError`] (`ErrorCode::IntInvariant`) if JSON serialization fails.
    pub fn ndjson_line(event: &ProgressEvent) -> FileFusionResult<String> {
        serde_json::to_string(event).map_err(|err| {
            FileFusionError::new(
                ErrorCode::IntInvariant,
                format!("failed to serialize ProgressEvent to JSON line: {err}"),
            )
        })
    }
}

fn percent_basis_points(percent: f32) -> u64 {
    let clamped = if percent.is_finite() {
        percent.clamp(0.0, 100.0)
    } else {
        0.0
    };
    let int_pct = (0..=100u64)
        .rfind(|&n| f32::from(u16::try_from(n).unwrap_or(100)) <= clamped)
        .unwrap_or(0);
    int_pct.saturating_mul(100)
}

#[cfg(not(target_arch = "wasm32"))]
fn estimate_remaining_ms(
    inner: &ProgressBusInner,
    job_id: &str,
    item_total: u32,
    items: &HashMap<(String, u32), ProgressEvent>,
) -> Option<u64> {
    let elapsed_ms = {
        let mut starts = match inner.job_starts.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let start = *starts
            .entry(job_id.to_owned())
            .or_insert_with(std::time::Instant::now);
        u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
    };

    let done_bps: u64 = items
        .iter()
        .filter(|((jid, _), _)| jid == job_id)
        .map(|(_, ev)| percent_basis_points(ev.percent))
        .sum();
    let denom_items = u64::from(item_total.max(1));
    let total_bps = denom_items.saturating_mul(10_000);

    if done_bps == 0 || elapsed_ms == 0 {
        return None;
    }
    if done_bps >= total_bps {
        return Some(0);
    }
    let remaining_bps = total_bps.saturating_sub(done_bps);
    Some(elapsed_ms.saturating_mul(remaining_bps) / done_bps)
}

#[cfg(target_arch = "wasm32")]
const fn estimate_remaining_ms(
    _inner: &ProgressBusInner,
    _job_id: &str,
    _item_total: u32,
    _items: &HashMap<(String, u32), ProgressEvent>,
) -> Option<u64> {
    None
}

impl ProgressSink for ProgressBus {
    fn emit(&self, mut event: ProgressEvent) {
        let clamped = if event.percent.is_finite() {
            event.percent.clamp(0.0, 100.0)
        } else {
            0.0
        };

        let computed_eta = {
            let mut items = match self.inner.items.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            let key = (event.job_id.clone(), event.item_index);
            let prev = items.get(&key).map_or(0.0, |e| e.percent);
            event.percent = if matches!(event.stage, JobStage::Completed) {
                100.0
            } else {
                prev.max(clamped)
            };
            items.insert(key, event.clone());

            if event.eta_ms.is_some() {
                event.eta_ms
            } else {
                let eta =
                    estimate_remaining_ms(&self.inner, &event.job_id, event.item_total, &items);
                if let Some(val) = eta {
                    event.eta_ms = Some(val);
                    if let Some(stored) = items.get_mut(&(event.job_id.clone(), event.item_index)) {
                        stored.eta_ms = Some(val);
                    }
                }
                eta
            }
        };

        if let Some(eta) = computed_eta {
            let mut last_eta = match self.inner.last_eta_ms.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            *last_eta = Some(eta);
        }

        self.inner.total_emitted.fetch_add(1, Ordering::SeqCst);

        let mut subs = match self.inner.subscribers.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        subs.retain(|(_, tx)| tx.send(event.clone()).is_ok());
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::path::Path;
    use std::sync::atomic::AtomicBool;
    use std::time::Instant;

    use file_core::{
        BatchDefaults, CORE_SCHEMA_VERSION, ItemOutcome, ItemStatus, JobContext, ProcessingHandler,
        ProcessingOperation, ProcessingRequest, ResourceLimits, ValidatedSource,
    };

    use super::*;
    use crate::JobManager;

    #[test]
    fn progress_bus_computes_monotonic_progress_eta_ndjson_and_broadcasts()
    -> Result<(), Box<dyn Error>> {
        let bus = ProgressBus::new();
        let sub = bus.subscribe();
        assert_eq!(bus.stats().active_subscribers, 1);

        bus.emit(ProgressEvent {
            job_id: String::from("job_eta"),
            item_index: 1,
            item_total: 2,
            stage: JobStage::Probing,
            percent: 25.0,
            bytes_in: 1024,
            bytes_out_estimate: None,
            eta_ms: None,
            detail: String::from("Probing 1"),
        });

        std::thread::sleep(Duration::from_millis(12));

        // Regressing percent (10.0 < 25.0) must clamp cleanly to >= 25.0
        bus.emit(ProgressEvent {
            job_id: String::from("job_eta"),
            item_index: 1,
            item_total: 2,
            stage: JobStage::Processing,
            percent: 10.0,
            bytes_in: 1024,
            bytes_out_estimate: Some(512),
            eta_ms: None,
            detail: String::from("Processing 1"),
        });

        bus.emit(ProgressEvent {
            job_id: String::from("job_eta"),
            item_index: 1,
            item_total: 2,
            stage: JobStage::Completed,
            percent: 98.0,
            bytes_in: 1024,
            bytes_out_estimate: Some(480),
            eta_ms: None,
            detail: String::from("Done 1"),
        });

        let events = sub.drain();
        assert_eq!(events.len(), 3);
        assert!((events[0].percent - 25.0).abs() < f32::EPSILON);
        assert!((events[1].percent - 25.0).abs() < f32::EPSILON);
        assert!((events[2].percent - 100.0).abs() < f32::EPSILON);
        assert!(events[1].eta_ms.is_some());

        let ndjson = ProgressBus::ndjson_line(&events[2])?;
        let reparsed: ProgressEvent = serde_json::from_str(&ndjson)?;
        assert_eq!(reparsed.job_id, "job_eta");
        assert_eq!(reparsed.stage, JobStage::Completed);

        let stats = bus.stats();
        assert_eq!(stats.total_emitted, 3);
        assert_eq!(stats.tracked_items, 1);
        assert_eq!(stats.completed_items, 1);

        drop(sub);
        assert_eq!(bus.stats().active_subscribers, 0);
        Ok(())
    }

    struct SlowCancelCheckHandler {
        started: Arc<AtomicBool>,
    }

    impl ProcessingHandler for SlowCancelCheckHandler {
        fn process_item(
            &self,
            source: &ValidatedSource,
            destination: Option<&Path>,
            _req: &ProcessingRequest,
            item_index: u32,
            ctx: &JobContext,
        ) -> FileFusionResult<ItemOutcome> {
            self.started.store(true, Ordering::SeqCst);
            for _ in 0..40 {
                ctx.check_cancelled()?;
                std::thread::sleep(Duration::from_millis(15));
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

    #[test]
    fn job_manager_cancel_job_aborts_active_batch_within_latency_budget()
    -> Result<(), Box<dyn Error>> {
        let dir =
            std::env::temp_dir().join(format!("filefusion_cancel_job_{}", std::process::id()));
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        std::fs::create_dir_all(&dir)?;
        let f1 = dir.join("slow.txt");
        std::fs::write(&f1, b"active-cancellation-latency")?;

        let manager = JobManager::new(ResourceLimits::default(), BatchDefaults::default());
        let bus = Arc::new(ProgressBus::new());
        let ctx = manager
            .create_context("job_live_cancel")
            .with_progress(bus.clone());

        let mut req = ProcessingRequest {
            schema_version: CORE_SCHEMA_VERSION,
            job_id: String::from("job_live_cancel"),
            inputs: vec![f1],
            output_dir: Some(dir.clone()),
            operation: ProcessingOperation::Compress {
                profile: file_core::CompressionProfile::BalancedOptimal,
                target_format: None,
                strip_metadata: false,
            },
            batch: manager.defaults().clone(),
            strict_spoofing: false,
            dry_run: false,
        };
        req.batch.naming_template = String::from("{stem}.{ext}");

        let started_flag = Arc::new(AtomicBool::new(false));
        let handler = SlowCancelCheckHandler {
            started: Arc::clone(&started_flag),
        };

        let mgr_thread = manager.clone();
        let cancel_handle = std::thread::spawn(move || {
            while !started_flag.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(5));
            }
            let t0 = Instant::now();
            assert!(mgr_thread.cancel_job("job_live_cancel"));
            t0
        });

        let res = manager.execute_batch(&req, Some(&handler), &ctx);
        let cancel_at = cancel_handle.join().map_err(|_| "cancel thread panicked")?;
        let abort_elapsed = cancel_at.elapsed();

        assert!(res.is_err());
        if let Err(err) = res {
            assert_eq!(err.diagnostic.code, ErrorCode::JobCancelled);
        }
        assert!(abort_elapsed < Duration::from_millis(350));
        assert_eq!(manager.active_job_ids(), Vec::<String>::new());

        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }
}
