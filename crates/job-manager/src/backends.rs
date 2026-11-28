//! L5 backend registry and isolated Tier 2 subprocess execution (`FF-CTR-001` §3.3,
//! `MOD-RULE-01`, `MOD-RULE-04`).

use std::collections::HashMap;
#[cfg(feature = "native-logging")]
use std::path::PathBuf;
use std::sync::Arc;

use file_core::{
    CompressionEngine, ConversionEngine, ConversionRoute, ErrorCode, ExternalToolAdapter,
    FileFusionError, FileFusionResult, FormatId, IsolatedProcessRunner, JobContext,
    OutputValidator, ProcessInvocation, ProcessOutput, QualityEvaluator,
};

#[cfg(feature = "native-logging")]
const MAX_CAPTURE_BYTES: u64 = 1_048_576;

/// Native [`IsolatedProcessRunner`] enforcing cleared env, private temp dir,
/// argument vectors (zero shell invocation), wall-clock timeouts, and cancellation kill.
#[derive(Debug, Clone, Default)]
pub struct NativeIsolatedProcessRunner;

impl NativeIsolatedProcessRunner {
    /// Creates a new isolated subprocess runner instance.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[cfg(feature = "native-logging")]
struct TempWorkDirGuard(PathBuf);

#[cfg(feature = "native-logging")]
impl TempWorkDirGuard {
    fn create(job_id: &str) -> FileFusionResult<Self> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static RUNNER_SEQ: AtomicU64 = AtomicU64::new(1);

        let safe_id: String = job_id
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
            .take(24)
            .collect();
        let seq = RUNNER_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "filefusion_iso_{}_{}_{}",
            if safe_id.is_empty() { "job" } else { &safe_id },
            std::process::id(),
            seq
        ));
        std::fs::create_dir_all(&dir)?;
        Ok(Self(dir))
    }
}

#[cfg(feature = "native-logging")]
impl Drop for TempWorkDirGuard {
    fn drop(&mut self) {
        if self.0.exists() {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[cfg(feature = "native-logging")]
fn spawn_bounded_reader<R: std::io::Read + Send + 'static>(
    mut stream: R,
) -> std::thread::JoinHandle<Vec<u8>> {
    use std::io::Read as _;

    std::thread::spawn(move || {
        let mut out = Vec::new();
        let mut limited = (&mut stream).take(MAX_CAPTURE_BYTES);
        let _ = limited.read_to_end(&mut out);
        out
    })
}

#[cfg(feature = "native-logging")]
impl IsolatedProcessRunner for NativeIsolatedProcessRunner {
    fn run_isolated(
        &self,
        invocation: &ProcessInvocation,
        ctx: &JobContext,
    ) -> FileFusionResult<ProcessOutput> {
        use std::process::{Command, Stdio};
        use std::time::{Duration, Instant};

        ctx.check_cancelled()?;
        let guard = TempWorkDirGuard::create(&ctx.job_id)?;
        let work_dir = invocation
            .working_dir
            .as_deref()
            .unwrap_or(guard.0.as_path());

        let mut cmd = Command::new(&invocation.executable);
        cmd.args(&invocation.args)
            .current_dir(work_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env_clear();

        for base_var in ["PATH", "SYSTEMROOT", "WINDIR", "LD_LIBRARY_PATH", "LANG"] {
            if let Some(val) = std::env::var_os(base_var) {
                cmd.env(base_var, val);
            }
        }
        cmd.env("TEMP", &guard.0);
        cmd.env("TMP", &guard.0);
        cmd.env("TMPDIR", &guard.0);
        for key in &invocation.allow_env_keys {
            if let Some(val) = std::env::var_os(key) {
                cmd.env(key, val);
            }
        }

        let started = Instant::now();
        let mut child = cmd.spawn().map_err(|err| {
            FileFusionError::new(
                ErrorCode::EngMissing,
                format!(
                    "failed to launch isolated tool '{}': {err}",
                    invocation.executable.display()
                ),
            )
            .with_remediation("Verify the Tier 2 binary is installed via `filefusion doctor`.")
        })?;

        let stdout_thread = child.stdout.take().map(spawn_bounded_reader);
        let stderr_thread = child.stderr.take().map(spawn_bounded_reader);

        let timeout_limit = invocation
            .timeout_secs
            .max(1)
            .min(ctx.limits.subprocess_timeout_secs.max(1));
        let deadline = Duration::from_secs(timeout_limit);
        let poll_step = Duration::from_millis(25);

        let exit_status = loop {
            if ctx.cancellation.is_cancelled() {
                let _ = child.kill();
                let _ = child.wait();
                return ctx.check_cancelled().map(|()| ProcessOutput {
                    exit_code: -1,
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                    duration_ms: 0,
                    timed_out: false,
                });
            }
            if started.elapsed() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                let stdout = stdout_thread
                    .and_then(|h| h.join().ok())
                    .unwrap_or_default();
                let stderr = stderr_thread
                    .and_then(|h| h.join().ok())
                    .unwrap_or_default();
                return Ok(ProcessOutput {
                    exit_code: -1,
                    stdout,
                    stderr,
                    duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
                    timed_out: true,
                });
            }
            match child.try_wait()? {
                Some(status) => break status,
                None => std::thread::sleep(poll_step),
            }
        };

        let stdout = stdout_thread
            .and_then(|h| h.join().ok())
            .unwrap_or_default();
        let stderr = stderr_thread
            .and_then(|h| h.join().ok())
            .unwrap_or_default();

        Ok(ProcessOutput {
            exit_code: exit_status.code().unwrap_or(-1),
            stdout,
            stderr,
            duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            timed_out: false,
        })
    }
}

#[cfg(not(feature = "native-logging"))]
impl IsolatedProcessRunner for NativeIsolatedProcessRunner {
    fn run_isolated(
        &self,
        _invocation: &ProcessInvocation,
        _ctx: &JobContext,
    ) -> FileFusionResult<ProcessOutput> {
        Err(FileFusionError::new(
            ErrorCode::EngMissing,
            "subprocess execution is unavailable in wasm32 builds",
        )
        .with_remediation("Use Tier 1 in-memory codecs or configure optional remote API offload."))
    }
}

/// Pluggable L5 registry holding Tier 1 / Tier 2 engines, validators, and quality evaluators.
#[derive(Default, Clone)]
pub struct BackendCatalog {
    compressors: Vec<Arc<dyn CompressionEngine>>,
    converters: Vec<Arc<dyn ConversionEngine>>,
    external_adapters: HashMap<String, Arc<dyn ExternalToolAdapter>>,
    validators: Vec<Arc<dyn OutputValidator>>,
    evaluators: Vec<Arc<dyn QualityEvaluator>>,
}

impl BackendCatalog {
    /// Creates an empty [`BackendCatalog`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a [`BackendCatalog`] pre-registered with core L1 [`validation_engine::MagicByteOutputValidator`].
    #[must_use]
    pub fn with_defaults() -> Self {
        let mut catalog = Self::new();
        catalog.register_validator(Arc::new(validation_engine::MagicByteOutputValidator));
        catalog
    }

    /// Registers a compression engine backend.
    pub fn register_compressor(&mut self, engine: Arc<dyn CompressionEngine>) {
        self.compressors.push(engine);
    }

    /// Registers a universal conversion engine backend.
    pub fn register_converter(&mut self, engine: Arc<dyn ConversionEngine>) {
        self.converters.push(engine);
    }

    /// Registers an external Tier 2 binary adapter indexed by `adapter_id()`.
    pub fn register_external_adapter(&mut self, adapter: Arc<dyn ExternalToolAdapter>) {
        self.external_adapters
            .insert(adapter.adapter_id().to_owned(), adapter);
    }

    /// Registers a post-processing staged output validator.
    pub fn register_validator(&mut self, validator: Arc<dyn OutputValidator>) {
        self.validators.push(validator);
    }

    /// Registers an empirical quality evaluator (`SSIM`, `PSNR`, `VMAF`).
    pub fn register_evaluator(&mut self, evaluator: Arc<dyn QualityEvaluator>) {
        self.evaluators.push(evaluator);
    }

    /// Finds all registered compression engines supporting `format`.
    #[must_use]
    pub fn compressors_for(&self, format: FormatId) -> Vec<Arc<dyn CompressionEngine>> {
        self.compressors
            .iter()
            .filter(|e| e.supported_formats().contains(&format))
            .cloned()
            .collect()
    }

    /// Resolves the first conversion engine providing a `(from -> to)` route.
    #[must_use]
    pub fn find_conversion_route(
        &self,
        from: FormatId,
        to: FormatId,
    ) -> Option<(Arc<dyn ConversionEngine>, ConversionRoute)> {
        for engine in &self.converters {
            if let Some(route) = engine
                .routes()
                .iter()
                .find(|r| r.from == from && r.to == to)
            {
                return Some((engine.clone(), route.clone()));
            }
        }
        None
    }

    /// Looks up a registered external Tier 2 adapter by ID.
    #[must_use]
    pub fn external_adapter(&self, adapter_id: &str) -> Option<Arc<dyn ExternalToolAdapter>> {
        self.external_adapters.get(adapter_id).cloned()
    }

    /// Returns all registered staged output validators.
    #[must_use]
    pub fn validators(&self) -> &[Arc<dyn OutputValidator>] {
        &self.validators
    }

    /// Returns all registered quality evaluators.
    #[must_use]
    pub fn evaluators(&self) -> &[Arc<dyn QualityEvaluator>] {
        &self.evaluators
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};

    use file_core::{
        CompressionOutcome, CompressionRequest, CompressionStatus, FileInspectionReport,
        ProcessingRequest, StagedPath, ValidatedSource,
    };

    use super::*;

    struct DummyPngEngine;

    impl CompressionEngine for DummyPngEngine {
        fn engine_id(&self) -> &'static str {
            "dummy-png"
        }

        fn supported_formats(&self) -> &[FormatId] {
            &[FormatId::Png]
        }

        fn estimate_memory_mb(
            &self,
            _inspection: &FileInspectionReport,
            _opts: &CompressionRequest,
        ) -> u64 {
            32
        }

        fn compress(
            &self,
            input: &ValidatedSource,
            _staged_output: &StagedPath,
            _req: &CompressionRequest,
            ctx: &JobContext,
        ) -> FileFusionResult<CompressionOutcome> {
            ctx.check_cancelled()?;
            Ok(CompressionOutcome {
                status: CompressionStatus::NoOpRetainedOriginal,
                bytes_in: input.inspection.byte_size,
                bytes_out: input.inspection.byte_size,
                codec_used: String::from("png-noop"),
                quality: None,
            })
        }
    }

    struct StubToolAdapter;

    impl ExternalToolAdapter for StubToolAdapter {
        fn adapter_id(&self) -> &'static str {
            "adapter-rustc-probe"
        }

        fn capability_id(&self) -> &'static str {
            "tier2-rustc-probe"
        }

        fn spdx_license(&self) -> &'static str {
            "Apache-2.0 OR MIT"
        }

        fn build_args(
            &self,
            _input: &ValidatedSource,
            _staged_output: &StagedPath,
            _req: &ProcessingRequest,
        ) -> FileFusionResult<Vec<OsString>> {
            Ok(vec![OsString::from("--version")])
        }

        fn execute(
            &self,
            input: &ValidatedSource,
            staged_output: &StagedPath,
            req: &ProcessingRequest,
            runner: &dyn IsolatedProcessRunner,
            ctx: &JobContext,
        ) -> FileFusionResult<ProcessOutput> {
            let args = self.build_args(input, staged_output, req)?;
            let inv = ProcessInvocation {
                executable: PathBuf::from("rustc"),
                args,
                working_dir: None,
                timeout_secs: 10,
                allow_env_keys: Vec::new(),
            };
            runner.run_isolated(&inv, ctx)
        }
    }

    #[test]
    fn backend_catalog_wires_default_validator_and_custom_engines() {
        let mut catalog = BackendCatalog::with_defaults();
        assert_eq!(catalog.validators().len(), 1);
        assert_eq!(
            catalog.validators()[0].validator_id(),
            "validation-engine-magic-byte"
        );

        catalog.register_compressor(Arc::new(DummyPngEngine));
        assert_eq!(catalog.compressors_for(FormatId::Png).len(), 1);
        assert_eq!(catalog.compressors_for(FormatId::Pdf).len(), 0);

        catalog.register_external_adapter(Arc::new(StubToolAdapter));
        let adapter = catalog.external_adapter("adapter-rustc-probe");
        assert!(adapter.is_some());
        let _ = Path::new(".");
    }

    #[cfg(feature = "native-logging")]
    #[test]
    fn isolated_process_runner_executes_command_and_honors_cancellation() -> FileFusionResult<()> {
        let runner = NativeIsolatedProcessRunner::new();
        let ctx = JobContext::new("iso_ok", file_core::ResourceLimits::default());
        let inv = ProcessInvocation {
            executable: PathBuf::from("rustc"),
            args: vec![OsString::from("--version")],
            working_dir: None,
            timeout_secs: 15,
            allow_env_keys: Vec::new(),
        };
        let out = runner.run_isolated(&inv, &ctx)?;
        assert_eq!(out.exit_code, 0);
        assert!(!out.timed_out);
        let stdout_text = String::from_utf8_lossy(&out.stdout);
        assert!(stdout_text.contains("rustc"));

        let cancelled_ctx = JobContext::new("iso_cancel", file_core::ResourceLimits::default());
        cancelled_ctx.cancellation.cancel();
        let aborted = runner.run_isolated(&inv, &cancelled_ctx);
        assert!(aborted.is_err());
        if let Err(err) = aborted {
            assert_eq!(err.diagnostic.code, ErrorCode::JobCancelled);
        }
        Ok(())
    }
}
