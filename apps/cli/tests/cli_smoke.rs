//! End-to-end integration tests for the `filefusion` CLI binary.

#![forbid(unsafe_code)]

use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn temp_test_dir(label: &str) -> Result<PathBuf, Box<dyn Error>> {
    let dir = std::env::temp_dir().join(format!("filefusion_cli_{label}_{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

#[test]
fn cli_version_succeeds() -> Result<(), Box<dyn Error>> {
    let exe = env!("CARGO_BIN_EXE_filefusion");
    let output = Command::new(exe).arg("--version").output()?;
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("filefusion"));
    Ok(())
}

#[test]
fn cli_doctor_json_returns_valid_schema_and_routes_logs_to_stderr() -> Result<(), Box<dyn Error>> {
    let exe = env!("CARGO_BIN_EXE_filefusion");
    let output = Command::new(exe)
        .args(["--log-level", "info", "--log-json", "doctor", "--json"])
        .env_remove("RUST_LOG")
        .output()?;
    assert!(output.status.success());

    // Stdout must remain pure JSON even when structured JSON logs are active on stderr.
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(parsed["schema_version"], 1);
    assert!(parsed["logical_cpus"].as_u64().unwrap_or(0) >= 1);
    assert_eq!(parsed["capabilities"][0]["id"], "file-core-workspace");
    assert_eq!(parsed["capabilities"][0]["available"], true);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("doctor diagnostics collected"));
    Ok(())
}

#[test]
fn cli_config_init_show_and_doctor_overrides_work_end_to_end() -> Result<(), Box<dyn Error>> {
    let dir = temp_test_dir("e2e")?;
    let cfg_path = dir.join("custom.toml");
    let exe = env!("CARGO_BIN_EXE_filefusion");

    let init_out = Command::new(exe)
        .args([
            "--config",
            &cfg_path.to_string_lossy(),
            "--workers",
            "3",
            "config",
            "init",
            "--json",
        ])
        .output()?;
    assert!(init_out.status.success());
    let init_json: serde_json::Value = serde_json::from_slice(&init_out.stdout)?;
    assert_eq!(init_json["ok"], true);
    assert_eq!(init_json["action"], "init");

    let doc_out = Command::new(exe)
        .args([
            "--config",
            &cfg_path.to_string_lossy(),
            "--max-memory-mb",
            "640",
            "doctor",
            "--json",
        ])
        .env_remove("FILEFUSION_WORKERS")
        .env_remove("FILEFUSION_MAX_MEMORY_MB")
        .output()?;
    assert!(doc_out.status.success());
    let doc_json: serde_json::Value = serde_json::from_slice(&doc_out.stdout)?;
    assert_eq!(doc_json["effective_limits"]["max_workers"], 3);
    assert_eq!(doc_json["effective_limits"]["max_memory_mb"], 640);

    let _ = fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn cli_rejects_invalid_config_with_exit_code_two() -> Result<(), Box<dyn Error>> {
    let dir = temp_test_dir("invalid")?;
    let bad_path = dir.join("bad.toml");
    fs::write(&bad_path, "[limits]\nmax_workers = 0\n")?;

    let exe = env!("CARGO_BIN_EXE_filefusion");
    let output = Command::new(exe)
        .args([
            "--config",
            &bad_path.to_string_lossy(),
            "config",
            "validate",
        ])
        .output()?;
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("max_workers"));

    let _ = fs::remove_dir_all(&dir);
    Ok(())
}
