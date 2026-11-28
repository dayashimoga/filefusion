//! Runtime resource governance, device profiles, thread-safe working-set
//! memory permits, and bounded streaming I/O (`NFR-PERF-001`, `NFR-PERF-003`,
//! `FF-ARCH-001` §4).

use std::io::{Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use serde::{Deserialize, Serialize};

use crate::ResourceLimits;
use crate::error::{ErrorCode, FileFusionError, FileFusionResult};
use crate::pipeline::CancellationToken;

/// Target hardware execution class determining default memory and worker bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceProfile {
    /// Desktop / server workstation (`<= min(4096 MB, RAM/4)`, reserves 1 OS core).
    Workstation,
    /// Battery- and RAM-constrained mobile devices (`PLAT-AND`, `PLAT-IOS`).
    Mobile,
    /// Single-worker browser WebAssembly heap (`PLAT-WEB`, `<= 2 GB` tab budget).
    WebWasm,
}

/// Live atomic snapshot of a [`MemoryGovernor`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryBudgetSnapshot {
    /// Configured working-set budget ceiling in megabytes.
    pub limit_mb: u64,
    /// Currently reserved megabytes across active tasks/buffers.
    pub used_mb: u64,
    /// Remaining available megabytes under ceiling.
    pub available_mb: u64,
    /// High-water mark of simultaneous reserved megabytes.
    pub peak_used_mb: u64,
    /// Count of currently live RAII [`MemoryReservation`] handles.
    pub active_reservations: usize,
    /// Cumulative number of reservation attempts rejected with `FF-ERR-RES-MEMORY`.
    pub rejected_requests: u64,
}

impl MemoryBudgetSnapshot {
    /// Integer utilization percentage (`0..=100`).
    #[must_use]
    pub fn utilization_percent(&self) -> u64 {
        if self.limit_mb == 0 {
            return 100;
        }
        ((self.used_mb.saturating_mul(100)) / self.limit_mb).min(100)
    }
}

#[derive(Debug)]
struct MemoryGovernorInner {
    limit_mb: u64,
    max_uncompressed_bytes: u64,
    used_mb: AtomicU64,
    peak_used_mb: AtomicU64,
    active_reservations: AtomicUsize,
    rejected_requests: AtomicU64,
}

/// RAII guard representing an active working-set reservation inside [`MemoryGovernor`].
///
/// Automatically releases reserved megabytes when dropped, preventing budget leaks
/// on early returns, cancellation, or error unwinding.
#[derive(Debug)]
pub struct MemoryReservation {
    inner: Arc<MemoryGovernorInner>,
    reserved_mb: u64,
}

impl MemoryReservation {
    /// Returns the number of megabytes held by this reservation.
    #[must_use]
    pub const fn reserved_mb(&self) -> u64 {
        self.reserved_mb
    }
}

impl Drop for MemoryReservation {
    fn drop(&mut self) {
        if self.reserved_mb > 0 {
            self.inner
                .used_mb
                .fetch_sub(self.reserved_mb, Ordering::SeqCst);
        }
        self.inner
            .active_reservations
            .fetch_sub(1, Ordering::SeqCst);
    }
}

/// Thread-safe global working-set budget tracker enforcing `ResourceLimits::max_memory_mb`
/// (`NFR-PERF-001`, `ARCHITECTURE.md` §4.2).
#[derive(Debug, Clone)]
pub struct MemoryGovernor {
    inner: Arc<MemoryGovernorInner>,
}

impl Default for MemoryGovernor {
    fn default() -> Self {
        Self::new(ResourceLimits::default())
    }
}

impl MemoryGovernor {
    /// Creates a [`MemoryGovernor`] enforcing `limits.max_memory_mb` and `limits.max_uncompressed_bytes`.
    #[must_use]
    pub fn new(limits: ResourceLimits) -> Self {
        Self {
            inner: Arc::new(MemoryGovernorInner {
                limit_mb: limits.max_memory_mb.max(1),
                max_uncompressed_bytes: limits.max_uncompressed_bytes,
                used_mb: AtomicU64::new(0),
                peak_used_mb: AtomicU64::new(0),
                active_reservations: AtomicUsize::new(0),
                rejected_requests: AtomicU64::new(0),
            }),
        }
    }

    /// Attempts to reserve `requested_mb` atomically from the global budget for `purpose`.
    ///
    /// # Errors
    /// Returns [`FileFusionError`] (`ErrorCode::ResMemory`) if allocating `requested_mb`
    /// would exceed `limit_mb`.
    pub fn try_reserve_mb(
        &self,
        requested_mb: u64,
        purpose: &str,
    ) -> FileFusionResult<MemoryReservation> {
        let mut current = self.inner.used_mb.load(Ordering::SeqCst);
        loop {
            let next = match current.checked_add(requested_mb) {
                Some(total) if total <= self.inner.limit_mb => total,
                _ => {
                    self.inner.rejected_requests.fetch_add(1, Ordering::SeqCst);
                    let avail = self.inner.limit_mb.saturating_sub(current);
                    return Err(FileFusionError::new(
                        ErrorCode::ResMemory,
                        format!(
                            "insufficient working-set budget for {purpose}: requested {requested_mb} MB, available {avail} MB of {} MB ceiling",
                            self.inner.limit_mb
                        ),
                    )
                    .with_remediation(
                        "Reduce worker concurrency or raise limits.max_memory_mb in config.toml.",
                    ));
                }
            };
            match self.inner.used_mb.compare_exchange_weak(
                current,
                next,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => {
                    self.record_peak(next);
                    self.inner
                        .active_reservations
                        .fetch_add(1, Ordering::SeqCst);
                    return Ok(MemoryReservation {
                        inner: Arc::clone(&self.inner),
                        reserved_mb: requested_mb,
                    });
                }
                Err(actual) => current = actual,
            }
        }
    }

    /// Validates `bytes <= max_uncompressed_bytes` and reserves ceiling megabytes for `purpose`.
    ///
    /// # Errors
    /// Returns [`FileFusionError`] (`ErrorCode::SecBomb`) if `bytes` exceeds the size cap,
    /// or `ErrorCode::ResMemory` if working-set memory is exhausted.
    pub fn try_reserve_bytes(
        &self,
        bytes: u64,
        purpose: &str,
    ) -> FileFusionResult<MemoryReservation> {
        if bytes > self.inner.max_uncompressed_bytes {
            return Err(FileFusionError::new(
                ErrorCode::SecBomb,
                format!(
                    "payload for {purpose} ({bytes} bytes) exceeds max_uncompressed_bytes ({} bytes)",
                    self.inner.max_uncompressed_bytes
                ),
            )
            .with_remediation("Increase limits.max_uncompressed_bytes if this input is trusted."));
        }
        let mb = if bytes == 0 {
            0
        } else {
            bytes.div_ceil(1_048_576)
        };
        self.try_reserve_mb(mb, purpose)
    }

    /// Returns a point-in-time [`MemoryBudgetSnapshot`].
    #[must_use]
    pub fn snapshot(&self) -> MemoryBudgetSnapshot {
        let used_mb = self.inner.used_mb.load(Ordering::SeqCst);
        MemoryBudgetSnapshot {
            limit_mb: self.inner.limit_mb,
            used_mb,
            available_mb: self.inner.limit_mb.saturating_sub(used_mb),
            peak_used_mb: self.inner.peak_used_mb.load(Ordering::SeqCst),
            active_reservations: self.inner.active_reservations.load(Ordering::SeqCst),
            rejected_requests: self.inner.rejected_requests.load(Ordering::SeqCst),
        }
    }

    fn record_peak(&self, used: u64) {
        let mut prev = self.inner.peak_used_mb.load(Ordering::Relaxed);
        while used > prev {
            match self.inner.peak_used_mb.compare_exchange_weak(
                prev,
                used,
                Ordering::SeqCst,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => prev = actual,
            }
        }
    }
}

impl ResourceLimits {
    /// Minimum streaming ring-buffer chunk size (`64 KiB`).
    pub const MIN_STREAM_CHUNK_BYTES: usize = 64 * 1024;
    /// Maximum streaming ring-buffer chunk size (`1 MiB`).
    pub const MAX_STREAM_CHUNK_BYTES: usize = 1024 * 1024;

    /// Computes platform-tailored [`ResourceLimits`] for a [`DeviceProfile`] (`PLAT-AND`,
    /// `PLAT-IOS`, `PLAT-WEB`, Desktop).
    #[must_use]
    pub fn for_profile(profile: DeviceProfile, logical_cpus: usize, total_ram_mb: u64) -> Self {
        match profile {
            DeviceProfile::Workstation => Self::for_host(logical_cpus, total_ram_mb),
            DeviceProfile::Mobile => {
                let reserved = if logical_cpus > 1 {
                    logical_cpus - 1
                } else {
                    1
                };
                let max_memory_mb = (total_ram_mb / 4).clamp(128, 1024);
                let mem_workers = usize::try_from((max_memory_mb / 512).max(1)).unwrap_or(1);
                Self {
                    max_workers: reserved.min(mem_workers).clamp(1, 2),
                    max_memory_mb,
                    max_expansion_ratio: Self::DEFAULT_MAX_EXPANSION_RATIO,
                    max_uncompressed_bytes: 512 * 1024 * 1024,
                    max_archive_depth: Self::DEFAULT_MAX_ARCHIVE_DEPTH,
                    subprocess_timeout_secs: 600,
                }
            }
            DeviceProfile::WebWasm => Self {
                max_workers: 1,
                max_memory_mb: 512,
                max_expansion_ratio: Self::DEFAULT_MAX_EXPANSION_RATIO,
                max_uncompressed_bytes: 1024 * 1024 * 1024,
                max_archive_depth: Self::DEFAULT_MAX_ARCHIVE_DEPTH,
                subprocess_timeout_secs: 60,
            },
        }
    }

    /// Computes bounded streaming chunk size in `[64 KiB, 1 MiB]` based on per-worker budget
    /// (`ARCHITECTURE.md` §4.2).
    #[must_use]
    pub fn recommended_chunk_bytes(&self) -> usize {
        let per_worker_mb =
            self.max_memory_mb / u64::try_from(self.max_workers.max(1)).unwrap_or(1);
        if per_worker_mb <= 192 {
            64 * 1024
        } else if per_worker_mb <= 384 {
            256 * 1024
        } else if per_worker_mb <= 768 {
            512 * 1024
        } else {
            1024 * 1024
        }
    }

    /// Computes effective worker count bounded by both CPU cores and memory cost per job
    /// (`NFR-PERF-003`: `max(1, min(max_workers, max_memory_mb / per_job_estimate_mb))`).
    #[must_use]
    pub fn clamp_workers_for_job(&self, per_job_estimate_mb: u64) -> usize {
        let est = per_job_estimate_mb.max(1);
        let mem_bound = usize::try_from((self.max_memory_mb / est).max(1)).unwrap_or(1);
        self.max_workers.max(1).min(mem_bound).max(1)
    }

    /// Validates an input payload byte size and estimated peak working-set cost against limits.
    ///
    /// # Errors
    /// Returns [`FileFusionError`] (`ErrorCode::SecBomb`) if `file_bytes` exceeds `max_uncompressed_bytes`,
    /// or `ErrorCode::ResMemory` if `estimated_working_set_mb` exceeds `max_memory_mb`.
    pub fn validate_file_budget(
        &self,
        file_bytes: u64,
        estimated_working_set_mb: u64,
    ) -> FileFusionResult<()> {
        if file_bytes > self.max_uncompressed_bytes {
            return Err(FileFusionError::new(
                ErrorCode::SecBomb,
                format!(
                    "input size {file_bytes} bytes exceeds max_uncompressed_bytes {} bytes",
                    self.max_uncompressed_bytes
                ),
            )
            .with_remediation("Increase limits.max_uncompressed_bytes if this input is trusted."));
        }
        if estimated_working_set_mb > self.max_memory_mb.max(1) {
            return Err(FileFusionError::new(
                ErrorCode::ResMemory,
                format!(
                    "estimated working set ({estimated_working_set_mb} MB) exceeds configured max_memory_mb ({} MB)",
                    self.max_memory_mb
                ),
            )
            .with_remediation("Increase limits.max_memory_mb or process a smaller input."));
        }
        Ok(())
    }
}

/// Parses Linux/Android `/proc/meminfo` `MemTotal` in megabytes without allocations or `unsafe`.
#[must_use]
pub fn parse_proc_meminfo_mb(meminfo: &str) -> Option<u64> {
    for line in meminfo.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            let kb: u64 = rest.split_whitespace().next()?.parse().ok()?;
            return Some((kb / 1024).max(1));
        }
    }
    None
}

pub(crate) fn detect_default_limits() -> ResourceLimits {
    if cfg!(target_arch = "wasm32") {
        return ResourceLimits::for_profile(DeviceProfile::WebWasm, 1, 2048);
    }
    let cpus = std::thread::available_parallelism().map_or(2, std::num::NonZeroUsize::get);
    let ram_mb = probe_host_ram_mb().unwrap_or(4096);
    if cfg!(any(target_os = "android", target_os = "ios")) {
        ResourceLimits::for_profile(DeviceProfile::Mobile, cpus, ram_mb)
    } else {
        ResourceLimits::for_profile(DeviceProfile::Workstation, cpus, ram_mb)
    }
}

fn probe_host_ram_mb() -> Option<u64> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        if let Ok(raw) = std::fs::read_to_string("/proc/meminfo") {
            return parse_proc_meminfo_mb(&raw);
        }
    }
    None
}

/// Streams `reader` to `writer` through a fixed bounded ring buffer (`64 KiB`–`1 MiB`)
/// without loading full files into RAM (`NFR-PERF-001`, `ARCHITECTURE.md` §4.2).
///
/// Checks `cancellation` between chunks (`NFR-REL-002`) and aborts with [`ErrorCode::SecBomb`]
/// if streamed bytes exceed `limits.max_uncompressed_bytes`.
///
/// # Errors
/// Returns [`FileFusionError`] on I/O failure, cancellation, or byte ceiling exceedance.
pub fn copy_bounded_streaming<R: Read, W: Write>(
    mut reader: R,
    mut writer: W,
    limits: &ResourceLimits,
    cancellation: Option<&CancellationToken>,
) -> FileFusionResult<u64> {
    let chunk_len = limits.recommended_chunk_bytes().clamp(
        ResourceLimits::MIN_STREAM_CHUNK_BYTES,
        ResourceLimits::MAX_STREAM_CHUNK_BYTES,
    );
    let mut buf = vec![0u8; chunk_len];
    let mut total: u64 = 0;

    loop {
        if let Some(token) = cancellation {
            token.check()?;
        }
        let read_bytes = reader.read(&mut buf)?;
        if read_bytes == 0 {
            break;
        }
        total = total.saturating_add(u64::try_from(read_bytes).unwrap_or(u64::MAX));
        if total > limits.max_uncompressed_bytes {
            return Err(FileFusionError::new(
                ErrorCode::SecBomb,
                format!(
                    "streamed payload exceeded max_uncompressed_bytes ({} bytes)",
                    limits.max_uncompressed_bytes
                ),
            )
            .with_remediation(
                "Increase limits.max_uncompressed_bytes if this stream is trusted.",
            ));
        }
        writer.write_all(&buf[..read_bytes])?;
    }
    writer.flush()?;
    Ok(total)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn memory_governor_enforces_ceilings_and_releases_on_drop() -> FileFusionResult<()> {
        let limits = ResourceLimits {
            max_memory_mb: 200,
            ..ResourceLimits::default()
        };
        let governor = MemoryGovernor::new(limits);

        let r1 = governor.try_reserve_mb(120, "decode_frame")?;
        assert_eq!(governor.snapshot().used_mb, 120);
        assert_eq!(governor.snapshot().available_mb, 80);
        assert_eq!(governor.snapshot().utilization_percent(), 60);

        let r2_err = governor.try_reserve_mb(100, "second_frame");
        assert!(r2_err.is_err());
        if let Err(err) = r2_err {
            assert_eq!(err.diagnostic.code, ErrorCode::ResMemory);
        }
        assert_eq!(governor.snapshot().rejected_requests, 1);

        drop(r1);
        assert_eq!(governor.snapshot().used_mb, 0);
        assert_eq!(governor.snapshot().peak_used_mb, 120);
        assert_eq!(governor.snapshot().active_reservations, 0);

        let r3 = governor.try_reserve_mb(100, "second_frame_after_drop")?;
        assert_eq!(r3.reserved_mb(), 100);
        drop(r3);

        // Overflow saturation at u64::MAX must reject cleanly without underflow on drop.
        let max_gov = MemoryGovernor::new(ResourceLimits {
            max_memory_mb: u64::MAX,
            ..ResourceLimits::default()
        });
        let held = max_gov.try_reserve_mb(1, "base_hold")?;
        let overflow = max_gov.try_reserve_mb(u64::MAX, "u64_max_overflow");
        assert!(overflow.is_err());
        if let Err(err) = overflow {
            assert_eq!(err.diagnostic.code, ErrorCode::ResMemory);
        }
        assert_eq!(max_gov.snapshot().rejected_requests, 1);
        drop(held);
        assert_eq!(max_gov.snapshot().used_mb, 0);
        Ok(())
    }

    #[test]
    fn device_profiles_chunk_bounds_and_worker_clamping_work() {
        let mobile = ResourceLimits::for_profile(DeviceProfile::Mobile, 8, 3072);
        assert_eq!(mobile.max_memory_mb, 768);
        assert_eq!(mobile.max_workers, 1);
        assert_eq!(mobile.max_uncompressed_bytes, 512 * 1024 * 1024);

        let web = ResourceLimits::for_profile(DeviceProfile::WebWasm, 8, 16_384);
        assert_eq!(web.max_workers, 1);
        assert_eq!(web.max_memory_mb, 512);
        assert_eq!(web.recommended_chunk_bytes(), 512 * 1024);

        let desktop = ResourceLimits::for_profile(DeviceProfile::Workstation, 12, 16_384);
        assert_eq!(desktop.max_memory_mb, 4096);
        assert_eq!(desktop.max_workers, 8);
        assert_eq!(desktop.recommended_chunk_bytes(), 512 * 1024);
        assert_eq!(desktop.clamp_workers_for_job(1500), 2);
        assert_eq!(desktop.clamp_workers_for_job(4096), 1);

        let tiny = ResourceLimits {
            max_workers: 4,
            max_memory_mb: 256,
            ..desktop
        };
        assert_eq!(tiny.recommended_chunk_bytes(), 64 * 1024);
    }

    #[test]
    fn parse_proc_meminfo_extracts_megabytes() {
        let sample = "MemTotal:       16384000 kB\nMemFree:         4000000 kB\n";
        assert_eq!(parse_proc_meminfo_mb(sample), Some(16_000));
        assert_eq!(parse_proc_meminfo_mb("Invalid: 123\n"), None);
    }

    #[test]
    fn bounded_streaming_copy_enforces_size_limit_and_cancellation() -> FileFusionResult<()> {
        let payload = vec![b'A'; 150_000];
        let mut limits = ResourceLimits {
            max_uncompressed_bytes: 200_000,
            ..ResourceLimits::default()
        };

        let mut sink = Vec::new();
        let copied = copy_bounded_streaming(Cursor::new(&payload), &mut sink, &limits, None)?;
        assert_eq!(copied, 150_000);
        assert_eq!(sink.len(), 150_000);

        limits.max_uncompressed_bytes = 100_000;
        let mut overflow_sink = Vec::new();
        let overflow =
            copy_bounded_streaming(Cursor::new(&payload), &mut overflow_sink, &limits, None);
        assert!(overflow.is_err());
        if let Err(err) = overflow {
            assert_eq!(err.diagnostic.code, ErrorCode::SecBomb);
        }

        let token = CancellationToken::new();
        token.cancel();
        let mut cancel_sink = Vec::new();
        let cancelled = copy_bounded_streaming(
            Cursor::new(&payload),
            &mut cancel_sink,
            &limits,
            Some(&token),
        );
        assert!(cancelled.is_err());
        if let Err(err) = cancelled {
            assert_eq!(err.diagnostic.code, ErrorCode::JobCancelled);
        }
        Ok(())
    }
}
