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

#[test]
fn cli_inspect_detects_real_signature_and_rejects_spoofed_extension() -> Result<(), Box<dyn Error>>
{
    let dir = temp_test_dir("inspect")?;
    let real_png = dir.join("valid.png");
    fs::write(&real_png, b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR")?;

    let exe = env!("CARGO_BIN_EXE_filefusion");
    let png_out = Command::new(exe)
        .args(["inspect", "--json", &real_png.to_string_lossy()])
        .output()?;
    assert!(png_out.status.success());
    let png_json: serde_json::Value = serde_json::from_slice(&png_out.stdout)?;
    assert_eq!(png_json["detected_format"], "png");
    assert_eq!(png_json["spoofing_mismatch"], false);

    let spoofed = dir.join("malware.pdf");
    fs::write(
        &spoofed,
        b"<!DOCTYPE html><html><script>evil()</script></html>",
    )?;
    let spoof_out = Command::new(exe)
        .args(["inspect", "--strict", "--json", &spoofed.to_string_lossy()])
        .output()?;
    assert_eq!(spoof_out.status.code(), Some(3));
    let stderr = String::from_utf8_lossy(&spoof_out.stderr);
    assert!(stderr.contains("FF-ERR-SEC-SPOOFED"));

    let _ = fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn cli_inspect_computes_digests_and_normalized_metadata() -> Result<(), Box<dyn Error>> {
    let dir = temp_test_dir("inspect_meta")?;
    let png_path = dir.join("sample.png");
    let mut png = Vec::new();
    png.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&320u32.to_be_bytes());
    ihdr.extend_from_slice(&240u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    png.extend_from_slice(&13u32.to_be_bytes());
    png.extend_from_slice(b"IHDR");
    png.extend_from_slice(&ihdr);
    png.extend_from_slice(&[0u8; 4]);
    let text = b"Author\0Tester";
    png.extend_from_slice(&(u32::try_from(text.len()).unwrap_or(0)).to_be_bytes());
    png.extend_from_slice(b"tEXt");
    png.extend_from_slice(text);
    png.extend_from_slice(&[0u8; 4]);
    png.extend_from_slice(&0u32.to_be_bytes());
    png.extend_from_slice(b"IEND");
    png.extend_from_slice(&[0u8; 4]);
    fs::write(&png_path, &png)?;

    let exe = env!("CARGO_BIN_EXE_filefusion");
    let out = Command::new(exe)
        .args(["inspect", "--json", &png_path.to_string_lossy()])
        .output()?;
    assert!(out.status.success());
    let parsed: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    assert_eq!(parsed["schema_version"], 1);
    assert_eq!(parsed["detected_format"], "png");
    assert_eq!(parsed["byte_size"], u64::try_from(png.len()).unwrap_or(0));
    assert_eq!(parsed["digests"]["blake3"].as_str().map(str::len), Some(64));
    assert_eq!(parsed["digests"]["sha256"].as_str().map(str::len), Some(64));
    assert_eq!(
        parsed["digests"]["xxhash64"].as_str().map(str::len),
        Some(16)
    );
    assert_eq!(parsed["metadata_summary"]["width"], 320);
    assert_eq!(parsed["metadata_summary"]["height"], 240);
    assert_eq!(parsed["metadata_summary"]["has_pii_author_risk"], true);
    assert_eq!(parsed["streams"][0]["kind"], "image");
    assert_eq!(parsed["classification"]["domain"], "raster_image");
    assert_eq!(parsed["classification"]["privacy_sensitive"], true);
    let actions = parsed["recommended_actions"].as_array();
    assert!(actions.is_some_and(|list| {
        list.iter()
            .any(|item| item["action_id"] == "privacy.scrub_metadata")
    }));

    let sig_out = Command::new(exe)
        .args([
            "inspect",
            "--signature-only",
            "--json",
            &png_path.to_string_lossy(),
        ])
        .output()?;
    assert!(sig_out.status.success());
    let sig_json: serde_json::Value = serde_json::from_slice(&sig_out.stdout)?;
    assert_eq!(sig_json["detected_format"], "png");
    assert!(sig_json.get("digests").is_none());

    let _ = fs::remove_dir_all(&dir);
    Ok(())
}
