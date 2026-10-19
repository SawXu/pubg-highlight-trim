#![cfg(feature = "onnx")]
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
};
fn summary(o: &Output) -> Value {
    let stdout = String::from_utf8_lossy(&o.stdout);
    serde_json::from_str(
        stdout
            .lines()
            .find_map(|l| l.strip_prefix("SUMMARY "))
            .unwrap_or_else(|| panic!("missing summary: {}", String::from_utf8_lossy(&o.stderr))),
    )
    .unwrap()
}
#[test]
#[ignore = "requires pinned PUBG_OCR_MODEL_DIR and FFmpeg"]
fn provider_device_cache_isolation_verbose_cache_hits_and_strict_failure() {
    let temp = std::env::temp_dir().join(format!("ocr-provider-e2e-{}", std::process::id()));
    fs::create_dir_all(&temp).unwrap();
    let src = temp.join("blank.mp4");
    let ffmpeg = PathBuf::from(std::env::var_os("PUBG_HIGHLIGHT_TRIM_FFMPEG_DIR").unwrap())
        .join("ffmpeg.exe");
    assert!(Command::new(ffmpeg)
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=640x360:r=10:d=1",
            "-c:v",
            "libx264"
        ])
        .arg(&src)
        .status()
        .unwrap()
        .success());
    let invoke = |mode: Option<&str>, device: Option<&str>, verbose: bool| {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pubg_highlight_trim"));
        cmd.arg(&src)
            .args([
                "--dry-run",
                "--no-auto-candidate-csv",
                "--no-merge",
                "--overwrite",
                "--jobs",
                "1",
                "--cache-dir",
            ])
            .arg(temp.join("cache"))
            .arg("-o")
            .arg(temp.join("out"))
            // Synthetic legacy environment deliberately disagrees with CLI values.
            .env("PUBG_OCR_PROVIDER", "dml")
            .env("PUBG_OCR_DIRECTML_DEVICE_ID", "2147483647");
        if mode.is_none() && device.is_some() {
            cmd.env_remove("PUBG_OCR_PROVIDER");
        }
        if let Some(device) = device {
            cmd.args(["--ocr-device-id", device]);
        }
        if let Some(mode) = mode {
            cmd.args(["--ocr-provider", mode]);
        }
        if verbose {
            cmd.arg("--verbose");
        }
        cmd.output().unwrap()
    };
    let default = summary(&invoke(None, Some("0"), false));
    assert_eq!(default["ocr_providers"][0]["provider_actual"], "cpu");
    assert_eq!(default["cache_misses"], 1);
    let csv_header = || {
        fs::read_to_string(temp.join("out").join("检测与裁剪记录.csv"))
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .to_string()
    };
    let default_header = csv_header();
    let cpu = summary(&invoke(Some("CPU"), Some("0"), false));
    assert_eq!(cpu["cache_hits"], 1);
    let auto = summary(&invoke(Some("auto"), Some("0"), false));
    assert_eq!(auto["cache_hits"], 1); // auto resolves to CPU
    assert_eq!(
        summary(&invoke(Some("cpu"), Some("1"), false))["cache_misses"],
        1
    );
    // No CLI selection: both legacy environment values take effect.
    let fallback = summary(&invoke(None, None, false));
    assert_eq!(fallback["cache_misses"], 1);
    assert_eq!(fallback["ocr_providers"][0]["provider_actual"], "cpu");
    assert!(fallback["ocr_providers"][0]["fallback_reason"].is_string());
    let hit = invoke(Some("DiReCtMl"), Some("2147483647"), true);
    assert_eq!(summary(&hit)["cache_hits"], 1);
    let diagnostics = String::from_utf8_lossy(&hit.stderr);
    for expected in [
        "mode=directml",
        "requested=directml",
        "actual=cpu",
        "device_id=2147483647",
        "fallback=directml-init-failed",
        "cache_hit=true",
    ] {
        assert!(
            diagnostics.contains(expected),
            "missing {expected}: {diagnostics}"
        );
    }
    let strict = invoke(Some("directml-only"), Some("2147483647"), false);
    assert_eq!(strict.status.code(), Some(1));
    assert!(!String::from_utf8_lossy(&strict.stdout).contains("SUMMARY "));
    assert!(String::from_utf8_lossy(&strict.stderr).contains("directml-init-failed"));
    assert!(!String::from_utf8_lossy(&strict.stderr).contains("OCR mode="));
    let strict_verbose = invoke(Some("directml-only"), Some("2147483647"), true);
    assert_eq!(strict_verbose.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&strict_verbose.stdout);
    assert!(!stdout.contains("SUMMARY "));
    assert!(!stdout.contains("OCR mode="));
    let diagnostics = String::from_utf8_lossy(&strict_verbose.stderr);
    for expected in [
        "mode=directml-only",
        "requested=directml",
        "actual=unknown",
        "device_id=2147483647",
        "fallback=directml-init-failed",
        "cache_hit=false",
    ] {
        assert!(
            diagnostics.contains(expected),
            "missing {expected}: {diagnostics}"
        );
    }
    for (key, value) in default.as_object().unwrap() {
        if key != "ocr_providers" {
            assert_eq!(
                std::mem::discriminant(value),
                std::mem::discriminant(&cpu[key]),
                "summary type changed: {key}"
            );
        }
    }
    assert_eq!(csv_header(), default_header);
    for key in [
        "included_count",
        "skipped_count",
        "unreadable_count",
        "methods",
        "detectors",
    ] {
        assert_eq!(
            default[key], cpu[key],
            "default event results changed: {key}"
        );
    }
    fs::remove_dir_all(temp).unwrap();
}
