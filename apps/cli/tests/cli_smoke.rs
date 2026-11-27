//! End-to-end integration tests for the `filefusion` CLI binary.

#![forbid(unsafe_code)]

use std::error::Error;
use std::process::Command;

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
fn cli_doctor_json_returns_valid_schema() -> Result<(), Box<dyn Error>> {
    let exe = env!("CARGO_BIN_EXE_filefusion");
    let output = Command::new(exe).args(["doctor", "--json"]).output()?;
    assert!(output.status.success());
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(parsed["schema_version"], 1);
    assert!(parsed["logical_cpus"].as_u64().unwrap_or(0) >= 1);
    assert_eq!(parsed["capabilities"][0]["id"], "file-core-workspace");
    assert_eq!(parsed["capabilities"][0]["available"], true);
    Ok(())
}
