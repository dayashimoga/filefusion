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
    assert_eq!(parsed["capabilities"].as_array().map(Vec::len), Some(10));
    assert_eq!(
        parsed["tool_availability"].as_array().map(Vec::len),
        Some(7)
    );

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

fn png_crc32_test(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

fn build_stored_png_bytes() -> Result<Vec<u8>, Box<dyn Error>> {
    use std::io::Write as _;

    let mut scanlines = Vec::with_capacity(48 * (1 + 48 * 4));
    for y in 0..48u8 {
        scanlines.push(0);
        for x in 0..48u8 {
            scanlines.extend_from_slice(&[x.wrapping_mul(5), y.wrapping_mul(5), 140, 255]);
        }
    }
    let mut png_bytes = Vec::from(b"\x89PNG\r\n\x1a\n".as_slice());
    let mut push_chunk = |tag: [u8; 4], body: &[u8]| {
        png_bytes.extend_from_slice(&(u32::try_from(body.len()).unwrap_or(0)).to_be_bytes());
        png_bytes.extend_from_slice(&tag);
        png_bytes.extend_from_slice(body);
        let mut crc_in = Vec::with_capacity(4 + body.len());
        crc_in.extend_from_slice(&tag);
        crc_in.extend_from_slice(body);
        png_bytes.extend_from_slice(&png_crc32_test(&crc_in).to_be_bytes());
    };
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&48u32.to_be_bytes());
    ihdr.extend_from_slice(&48u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    push_chunk(*b"IHDR", &ihdr);

    let mut zlib_stored = Vec::new();
    let mut block_stream = std::io::Cursor::new(&mut zlib_stored);
    let chunk_len = u16::try_from(scanlines.len()).unwrap_or(0);
    block_stream.write_all(&[0x78, 0x01, 0x01])?;
    block_stream.write_all(&chunk_len.to_le_bytes())?;
    block_stream.write_all(&(!chunk_len).to_le_bytes())?;
    block_stream.write_all(&scanlines)?;
    let (mut s1, mut s2) = (1u32, 0u32);
    for &b in &scanlines {
        s1 = (s1 + u32::from(b)) % 65521;
        s2 = (s2 + s1) % 65521;
    }
    let adler = (s2 << 16) | s1;
    block_stream.write_all(&adler.to_be_bytes())?;
    push_chunk(*b"IDAT", &zlib_stored);
    push_chunk(*b"IEND", &[]);
    Ok(png_bytes)
}

#[test]
fn cli_compress_archive_and_bench_work_end_to_end() -> Result<(), Box<dyn Error>> {
    let dir = temp_test_dir("stage2_cli")?;
    let exe = env!("CARGO_BIN_EXE_filefusion");

    // 1. Compress an uncompressed PNG via `filefusion compress --json`.
    let raw_png = dir.join("bloated.png");
    let opt_png = dir.join("shrunk.png");
    fs::write(&raw_png, build_stored_png_bytes()?)?;

    let cmp_out = Command::new(exe)
        .args([
            "compress",
            &raw_png.to_string_lossy(),
            "-o",
            &opt_png.to_string_lossy(),
            "--profile",
            "lossless",
            "--json",
        ])
        .output()?;
    assert!(
        cmp_out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&cmp_out.stderr)
    );
    let cmp_json: serde_json::Value = serde_json::from_slice(&cmp_out.stdout)?;
    assert_eq!(cmp_json["status"], "optimized");
    assert!(
        cmp_json["bytes_out"].as_u64().unwrap_or(u64::MAX)
            < cmp_json["bytes_in"].as_u64().unwrap_or(0)
    );
    assert!(opt_png.exists());

    // 2. Archive create -> list -> extract roundtrip (`tar.zst`).
    let doc_a = dir.join("note.txt");
    fs::write(&doc_a, "FileFusion archive CLI test payload\n")?;
    let arc_path = dir.join("bundle.tar.zst");
    let extract_dir = dir.join("unpacked");

    let arc_create = Command::new(exe)
        .args([
            "archive",
            "create",
            &arc_path.to_string_lossy(),
            &doc_a.to_string_lossy(),
            &opt_png.to_string_lossy(),
            "--json",
        ])
        .output()?;
    assert!(
        arc_create.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&arc_create.stderr)
    );
    let arc_create_json: serde_json::Value = serde_json::from_slice(&arc_create.stdout)?;
    assert_eq!(arc_create_json["entry_count"], 2);

    let arc_list = Command::new(exe)
        .args(["archive", "list", &arc_path.to_string_lossy(), "--json"])
        .output()?;
    assert!(arc_list.status.success());
    let listed: serde_json::Value = serde_json::from_slice(&arc_list.stdout)?;
    assert_eq!(listed.as_array().map(Vec::len), Some(2));

    let arc_extract = Command::new(exe)
        .args([
            "archive",
            "extract",
            &arc_path.to_string_lossy(),
            "-o",
            &extract_dir.to_string_lossy(),
            "--json",
        ])
        .output()?;
    assert!(arc_extract.status.success());
    assert_eq!(
        fs::read_to_string(extract_dir.join("note.txt"))?,
        "FileFusion archive CLI test payload\n"
    );

    // 3. `filefusion bench --iterations 2 --json`.
    let bench_out = Command::new(exe)
        .args(["bench", "--iterations", "2", "--json"])
        .output()?;
    assert!(bench_out.status.success());
    let bench_json: serde_json::Value = serde_json::from_slice(&bench_out.stdout)?;
    assert_eq!(bench_json["results"].as_array().map(Vec::len), Some(2));

    let _ = fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn cli_convert_stage3_markup_and_epub_fidelity_corpus() -> Result<(), Box<dyn Error>> {
    let dir = temp_test_dir("stage3_markup_epub")?;
    let exe = env!("CARGO_BIN_EXE_filefusion");
    let md_in = dir.join("guide.md");
    let html_out = dir.join("guide.html");
    let epub_out = dir.join("guide.epub");
    fs::write(
        &md_in,
        "# Architecture\n\n| Layer | Crate |\n| --- | --- |\n| L4 | conversion-core |\n\n[Spec](https://example.org) ![Diagram](d.png)\n",
    )?;

    let html_res = Command::new(exe)
        .args([
            "convert",
            &md_in.to_string_lossy(),
            "--to",
            "html",
            "-o",
            &html_out.to_string_lossy(),
            "--strip-metadata",
            "--json",
        ])
        .output()?;
    assert!(
        html_res.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&html_res.stderr)
    );
    let html_json: serde_json::Value = serde_json::from_slice(&html_res.stdout)?;
    assert_eq!(html_json["fidelity"]["headings_preserved"], 1);
    assert_eq!(html_json["fidelity"]["tables_preserved"], 1);
    assert_eq!(html_json["fidelity"]["links_preserved"], 1);
    assert_eq!(html_json["fidelity"]["images_preserved"], 1);

    let epub_res = Command::new(exe)
        .args([
            "convert",
            &md_in.to_string_lossy(),
            "--to",
            "epub",
            "-o",
            &epub_out.to_string_lossy(),
            "--json",
        ])
        .output()?;
    assert!(epub_res.status.success());
    let epub_insp = Command::new(exe)
        .args(["inspect", "--json", &epub_out.to_string_lossy()])
        .output()?;
    assert!(epub_insp.status.success());
    let epub_json: serde_json::Value = serde_json::from_slice(&epub_insp.stdout)?;
    assert_eq!(epub_json["detected_format"], "epub");

    let _ = fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn cli_convert_stage3_spreadsheet_preflight_and_force_loss_gate() -> Result<(), Box<dyn Error>> {
    let dir = temp_test_dir("stage3_sheet")?;
    let exe = env!("CARGO_BIN_EXE_filefusion");
    let csv_in = dir.join("budget.csv");
    let xlsx_out = dir.join("budget.xlsx");
    let csv_rt = dir.join("budget_rt.csv");
    fs::write(&csv_in, "dept,amount\nCore,250\nWeb,120\n")?;

    let xlsx_res = Command::new(exe)
        .args([
            "convert",
            &csv_in.to_string_lossy(),
            "--to",
            "xlsx",
            "-o",
            &xlsx_out.to_string_lossy(),
            "--json",
        ])
        .output()?;
    assert!(xlsx_res.status.success());

    let preflight = Command::new(exe)
        .args([
            "convert",
            &xlsx_out.to_string_lossy(),
            "--to",
            "csv",
            "--preflight-only",
            "--json",
        ])
        .output()?;
    assert!(preflight.status.success());
    let pre_json: serde_json::Value = serde_json::from_slice(&preflight.stdout)?;
    assert_eq!(pre_json["warnings"][0]["code"], "FID-XLSX-FORMULAS");

    let blocked = Command::new(exe)
        .args([
            "convert",
            &xlsx_out.to_string_lossy(),
            "--to",
            "csv",
            "-o",
            &csv_rt.to_string_lossy(),
        ])
        .output()?;
    assert!(!blocked.status.success());

    let forced = Command::new(exe)
        .args([
            "convert",
            &xlsx_out.to_string_lossy(),
            "--to",
            "csv",
            "-o",
            &csv_rt.to_string_lossy(),
            "--force-loss",
            "--json",
        ])
        .output()?;
    assert!(forced.status.success());
    let rt_text = fs::read_to_string(&csv_rt)?;
    assert!(rt_text.contains("amount,dept"));
    assert!(rt_text.contains("250,Core"));
    assert!(rt_text.contains("120,Web"));

    let _ = fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn cli_pdf_studio_merge_split_rotate_crop_and_watermark_e2e() -> Result<(), Box<dyn Error>> {
    let dir = temp_test_dir("pdf_studio")?;
    let exe = env!("CARGO_BIN_EXE_filefusion");
    let pdf_a = dir.join("a.pdf");
    let pdf_b = dir.join("b.pdf");
    let merged = dir.join("merged.pdf");
    let rotated = dir.join("rotated.pdf");
    let cropped = dir.join("cropped.pdf");
    let watermarked = dir.join("watermarked.pdf");
    let split_dir = dir.join("split");

    fs::write(
        &pdf_a,
        job_manager::build_simple_text_pdf_bytes(&["Alpha Page"]),
    )?;
    fs::write(
        &pdf_b,
        job_manager::build_simple_text_pdf_bytes(&["Beta Page", "Gamma Page"]),
    )?;

    let merge_out = Command::new(exe)
        .args([
            "pdf",
            "merge",
            &merged.to_string_lossy(),
            &pdf_a.to_string_lossy(),
            &pdf_b.to_string_lossy(),
            "--json",
        ])
        .output()?;
    assert!(merge_out.status.success());
    let merge_json: serde_json::Value = serde_json::from_slice(&merge_out.stdout)?;
    assert_eq!(merge_json["operation"], "merge");
    assert_eq!(merge_json["page_count"], 3);

    let split_out = Command::new(exe)
        .args([
            "pdf",
            "split",
            &merged.to_string_lossy(),
            "-o",
            &split_dir.to_string_lossy(),
            "--ranges",
            "1-2;3",
            "--json",
        ])
        .output()?;
    assert!(split_out.status.success());
    let split_json: serde_json::Value = serde_json::from_slice(&split_out.stdout)?;
    assert_eq!(split_json["output_paths"].as_array().map(Vec::len), Some(2));

    for args in [
        vec![
            "pdf".to_owned(),
            "rotate".to_owned(),
            merged.to_string_lossy().into_owned(),
            "--degrees".to_owned(),
            "90".to_owned(),
            "-p".to_owned(),
            "2".to_owned(),
            "-o".to_owned(),
            rotated.to_string_lossy().into_owned(),
        ],
        vec![
            "pdf".to_owned(),
            "crop".to_owned(),
            rotated.to_string_lossy().into_owned(),
            "--margin".to_owned(),
            "24".to_owned(),
            "-o".to_owned(),
            cropped.to_string_lossy().into_owned(),
        ],
        vec![
            "pdf".to_owned(),
            "watermark".to_owned(),
            cropped.to_string_lossy().into_owned(),
            "--text".to_owned(),
            "STAGE4-OK".to_owned(),
            "-p".to_owned(),
            "all".to_owned(),
            "-o".to_owned(),
            watermarked.to_string_lossy().into_owned(),
        ],
    ] {
        assert!(Command::new(exe).args(&args).output()?.status.success());
    }

    let _ = fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn cli_pdf_security_scan_verify_protect_and_unlock_e2e() -> Result<(), Box<dyn Error>> {
    let dir = temp_test_dir("pdf_security")?;
    let exe = env!("CARGO_BIN_EXE_filefusion");
    let src = dir.join("doc.pdf");
    let protected = dir.join("protected.pdf");
    let unlocked = dir.join("unlocked.pdf");

    fs::write(
        &src,
        job_manager::build_simple_text_pdf_bytes(&["P1", "P2", "P3"]),
    )?;

    let scan_out = Command::new(exe)
        .args(["pdf", "scan-check", &src.to_string_lossy(), "--json"])
        .output()?;
    assert!(scan_out.status.success());
    let scan_json: serde_json::Value = serde_json::from_slice(&scan_out.stdout)?;
    assert_eq!(scan_json["total_pages"], 3);
    assert_eq!(scan_json["is_scanned_image_only"], false);

    let sig_out = Command::new(exe)
        .args(["pdf", "verify-sig", &src.to_string_lossy(), "--json"])
        .output()?;
    assert!(sig_out.status.success());
    let sig_json: serde_json::Value = serde_json::from_slice(&sig_out.stdout)?;
    assert_eq!(sig_json["has_signatures"], false);

    assert!(
        Command::new(exe)
            .args([
                "pdf",
                "protect",
                &src.to_string_lossy(),
                "-o",
                &protected.to_string_lossy(),
                "--owner-password",
                "owner-422",
                "--user-password",
                "user-422",
                "--cipher",
                "aes256",
                "--print-only",
            ])
            .output()?
            .status
            .success()
    );

    let bad_unlock = Command::new(exe)
        .args([
            "pdf",
            "unlock",
            &protected.to_string_lossy(),
            "-o",
            &unlocked.to_string_lossy(),
            "--password",
            "wrong-pw",
            "--json",
        ])
        .output()?;
    assert!(!bad_unlock.status.success());
    assert!(String::from_utf8_lossy(&bad_unlock.stderr).contains("FF-ERR-FMT-DRM"));

    let good_unlock = Command::new(exe)
        .args([
            "pdf",
            "unlock",
            &protected.to_string_lossy(),
            "-o",
            &unlocked.to_string_lossy(),
            "--password",
            "user-422",
            "--json",
        ])
        .output()?;
    assert!(good_unlock.status.success());
    assert!(unlocked.is_file());

    let _ = fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn cli_repair_corpus_recovers_corrupt_pdf_and_zip_and_rejects_noise() -> Result<(), Box<dyn Error>>
{
    let dir = temp_test_dir("repair_corpus")?;
    let exe = env!("CARGO_BIN_EXE_filefusion");

    // 1. Corrupt PDF missing xref/startxref/%%EOF -> diagnose + repair cleanly.
    let valid_pdf = job_manager::build_simple_text_pdf_bytes(&["Corpus Recoverable PDF"]);
    let xref_idx = valid_pdf
        .windows(4)
        .position(|w| w == b"xref")
        .ok_or("missing xref in generated PDF")?;
    let broken_pdf = dir.join("broken.pdf");
    let fixed_pdf = dir.join("fixed.pdf");
    fs::write(&broken_pdf, &valid_pdf[..xref_idx])?;

    let diag_out = Command::new(exe)
        .args([
            "repair",
            &broken_pdf.to_string_lossy(),
            "--diagnose-only",
            "--json",
        ])
        .output()?;
    assert!(diag_out.status.success());
    let diag_json: serde_json::Value = serde_json::from_slice(&diag_out.stdout)?;
    assert_eq!(diag_json["detected_format"], "pdf");
    assert_eq!(diag_json["recoverability"], "fully_repairable");

    let fix_out = Command::new(exe)
        .args([
            "repair",
            &broken_pdf.to_string_lossy(),
            "-o",
            &fixed_pdf.to_string_lossy(),
            "--json",
        ])
        .output()?;
    assert!(fix_out.status.success());
    let post_diag = Command::new(exe)
        .args([
            "repair",
            &fixed_pdf.to_string_lossy(),
            "--diagnose-only",
            "--json",
        ])
        .output()?;
    assert!(post_diag.status.success());
    let post_json: serde_json::Value = serde_json::from_slice(&post_diag.stdout)?;
    assert_eq!(post_json["recoverability"], "healthy");

    // 2. Truncated ZIP missing EOCD -> repair rebuilds Central Directory -> listable via `archive list`.
    let src_txt = dir.join("payload.txt");
    let good_zip = dir.join("good.zip");
    let broken_zip = dir.join("broken.zip");
    let fixed_zip = dir.join("fixed.zip");
    fs::write(&src_txt, b"Stage 4 ZIP central directory recovery fixture")?;
    assert!(
        Command::new(exe)
            .args([
                "archive",
                "create",
                &good_zip.to_string_lossy(),
                &src_txt.to_string_lossy(),
            ])
            .output()?
            .status
            .success()
    );
    let zip_bytes = fs::read(&good_zip)?;
    fs::write(&broken_zip, &zip_bytes[..zip_bytes.len() - 22])?;
    assert!(
        Command::new(exe)
            .args([
                "repair",
                &broken_zip.to_string_lossy(),
                "-o",
                &fixed_zip.to_string_lossy(),
            ])
            .output()?
            .status
            .success()
    );
    let listed = Command::new(exe)
        .args(["archive", "list", &fixed_zip.to_string_lossy(), "--json"])
        .output()?;
    assert!(listed.status.success());
    let list_json: serde_json::Value = serde_json::from_slice(&listed.stdout)?;
    assert_eq!(list_json[0]["path"], "payload.txt");

    // 3. Unrecoverable random payload -> honest classification & non-zero repair rejection.
    let noise = dir.join("noise.bin");
    fs::write(&noise, b"NOT_A_SUPPORTED_CONTAINER_RANDOM_BYTES")?;
    let noise_diag = Command::new(exe)
        .args([
            "repair",
            &noise.to_string_lossy(),
            "--diagnose-only",
            "--json",
        ])
        .output()?;
    assert!(noise_diag.status.success());
    let noise_json: serde_json::Value = serde_json::from_slice(&noise_diag.stdout)?;
    assert_eq!(noise_json["recoverability"], "unrecoverable");

    let noise_fix = Command::new(exe)
        .args(["repair", &noise.to_string_lossy(), "--json"])
        .output()?;
    assert!(!noise_fix.status.success());
    assert!(String::from_utf8_lossy(&noise_fix.stderr).contains("FF-ERR-FMT-CORRUPT"));

    let _ = fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn cli_metadata_encrypt_decrypt_and_ffenc_tamper_security_corpus() -> Result<(), Box<dyn Error>> {
    let dir = temp_test_dir("sec_corpus")?;
    let exe = env!("CARGO_BIN_EXE_filefusion");

    let pdf_in = dir.join("report.pdf");
    let pdf_clean = dir.join("report_clean.pdf");
    let vault = dir.join("report_clean.ffenc");
    let tampered = dir.join("report_tampered.ffenc");
    let restored = dir.join("report_restored.pdf");

    fs::write(
        &pdf_in,
        job_manager::build_simple_text_pdf_bytes(&["Audit Record"]),
    )?;

    let meta_out = Command::new(exe)
        .args([
            "metadata",
            "scrub",
            &pdf_in.to_string_lossy(),
            "-o",
            &pdf_clean.to_string_lossy(),
            "--title",
            "Clean Report",
            "--author",
            "Release Bot",
            "--json",
        ])
        .output()?;
    assert!(meta_out.status.success());
    let meta_json: serde_json::Value = serde_json::from_slice(&meta_out.stdout)?;
    assert_eq!(meta_json["report"]["after"]["title"], "Clean Report");
    assert_eq!(meta_json["report"]["after"]["author"], "Release Bot");

    let inspect_meta = Command::new(exe)
        .args([
            "metadata",
            "inspect",
            &pdf_clean.to_string_lossy(),
            "--json",
        ])
        .output()?;
    assert!(inspect_meta.status.success());
    let inspect_json: serde_json::Value = serde_json::from_slice(&inspect_meta.stdout)?;
    assert_eq!(inspect_json["title"], "Clean Report");

    // Encrypt using passphrase-env + ChaCha20-Poly1305.
    let enc_out = Command::new(exe)
        .args([
            "encrypt",
            &pdf_clean.to_string_lossy(),
            "-o",
            &vault.to_string_lossy(),
            "--cipher",
            "chacha20-poly1305",
            "--passphrase-env",
            "FF_STAGE4_TEST_SECRET",
            "--fast-kdf",
            "--json",
        ])
        .env("FF_STAGE4_TEST_SECRET", "top-secret-422")
        .output()?;
    assert!(enc_out.status.success());
    let vault_bytes = fs::read(&vault)?;
    assert!(vault_bytes.starts_with(b"FFENC\x01\x00"));

    // Flip 1 ciphertext bit -> AEAD decryption MUST fail with FF-ERR-FMT-DRM.
    let mut bad_bytes = vault_bytes;
    if let Some(last) = bad_bytes.last_mut() {
        *last ^= 0x55;
    }
    fs::write(&tampered, &bad_bytes)?;
    let tamper_dec = Command::new(exe)
        .args([
            "decrypt",
            &tampered.to_string_lossy(),
            "-o",
            &restored.to_string_lossy(),
            "--passphrase-env",
            "FF_STAGE4_TEST_SECRET",
            "--json",
        ])
        .env("FF_STAGE4_TEST_SECRET", "top-secret-422")
        .output()?;
    assert!(!tamper_dec.status.success());
    assert!(String::from_utf8_lossy(&tamper_dec.stderr).contains("FF-ERR-FMT-DRM"));

    // Decrypt valid .ffenc -> byte-for-byte identity with pdf_clean.
    let good_dec = Command::new(exe)
        .args([
            "decrypt",
            &vault.to_string_lossy(),
            "-o",
            &restored.to_string_lossy(),
            "--passphrase-env",
            "FF_STAGE4_TEST_SECRET",
            "--json",
        ])
        .env("FF_STAGE4_TEST_SECRET", "top-secret-422")
        .output()?;
    assert!(good_dec.status.success());
    assert_eq!(fs::read(&restored)?, fs::read(&pdf_clean)?);

    let _ = fs::remove_dir_all(&dir);
    Ok(())
}
