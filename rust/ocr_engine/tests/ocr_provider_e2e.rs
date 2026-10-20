#![cfg(feature = "onnx")]
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
};
fn summary(o: &Output) -> Value {
    assert!(
        o.status.success() || o.status.code() == Some(2),
        "CLI failed: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    let stdout = String::from_utf8_lossy(&o.stdout);
    serde_json::from_str(
        stdout
            .lines()
            .find_map(|l| l.strip_prefix("SUMMARY "))
            .unwrap_or_else(|| panic!("missing summary: {}", String::from_utf8_lossy(&o.stderr))),
    )
    .unwrap()
}
fn assert_provider(output: &Output, mode: &str, requested: &str, cache_hit: bool) -> Value {
    let result = summary(output);
    assert!(
        result["ocr_unavailable_reason"].is_null() || result["ocr_unavailable_reason"] == "",
        "{result}"
    );
    assert_eq!(result["unreadable_count"], 0, "{result}");
    assert!(
        !result["detectors"].as_object().unwrap().is_empty(),
        "real OCR inference must run: {result}"
    );
    assert_eq!(result["ocr_providers"].as_array().unwrap().len(), 1);
    let provider = &result["ocr_providers"][0];
    assert_eq!(provider["provider_mode"], mode);
    assert_eq!(provider["provider_requested"], requested);
    assert_eq!(provider["cache_hit"], cache_hit);
    match provider["provider_actual"].as_str().unwrap() {
        "directml" => assert!(provider["fallback_reason"].is_null()),
        "cpu" if requested == "directml" => assert!(
            provider["fallback_reason"]
                .as_str()
                .is_some_and(|reason| reason.starts_with("directml-")),
            "CPU fallback must explain why: {provider}"
        ),
        "cpu" => assert!(provider["fallback_reason"].is_null()),
        other => panic!("unexpected actual provider: {other}"),
    }
    assert!(!String::from_utf8_lossy(&output.stdout).contains("OCR mode="));
    let p: Value = serde_json::from_slice(
        &fs::read(PathBuf::from(result["output_dir"].as_str().unwrap()).join("profile.json"))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(p["schema_version"], "2");
    assert_eq!(p["stages"]["total"], result["profile_total_sec"]);
    assert_eq!(
        p["runtime"]["ocr_workers_active"],
        if cache_hit { 0 } else { 1 }
    );
    let measured = if cache_hit {
        "cached_historical"
    } else {
        "current"
    };
    let unmeasured = if cache_hit {
        "current"
    } else {
        "cached_historical"
    };
    assert!(
        p["telemetry"][measured]["ocr_requests"].as_u64().unwrap() > 0,
        "real inference telemetry: {p}"
    );
    assert_eq!(p["telemetry"][unmeasured]["ocr_requests"], 0);
    assert_eq!(
        p["ocr_providers_current"].as_array().unwrap().len(),
        if cache_hit { 0 } else { 1 }
    );
    assert_eq!(
        p["ocr_providers_cached_historical"]
            .as_array()
            .unwrap()
            .len(),
        if cache_hit { 1 } else { 0 }
    );
    result
}
#[test]
#[ignore = "requires pinned PUBG_OCR_MODEL_DIR and FFmpeg; mandatory in release CI"]
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
                "--no-brightness-gate",
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
        // The first run must exercise the real default, with neither CLI nor
        // legacy environment selecting a provider or device.
        if mode.is_none() && device.is_none() {
            cmd.env_remove("PUBG_OCR_PROVIDER")
                .env_remove("PUBG_OCR_DIRECTML_DEVICE_ID");
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
    let default_output = invoke(None, None, false);
    let default = assert_provider(&default_output, "auto", "directml", false);
    assert_eq!(default["ocr_providers"][0]["device_id"], 0);
    assert_eq!(default["cache_misses"], 1);
    assert!(!String::from_utf8_lossy(&default_output.stderr).contains("OCR mode="));
    // CI declares its no-GPU contract explicitly; GPU developer machines may
    // successfully use DirectML instead of taking the CPU fallback.
    if std::env::var("PUBG_OCR_REQUIRE_CPU_FALLBACK").as_deref() == Ok("1") {
        assert_eq!(default["ocr_providers"][0]["provider_actual"], "cpu");
    }
    let csv_header = || {
        fs::read_to_string(temp.join("out").join("检测与裁剪记录.csv"))
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .to_string()
    };
    let default_header = csv_header();
    let cpu_output = invoke(Some("CPU"), Some("0"), false);
    let cpu = assert_provider(&cpu_output, "cpu", "cpu", false);
    assert_eq!(cpu["ocr_providers"][0]["provider_actual"], "cpu");
    assert_eq!(cpu["cache_misses"], 1); // CPU never reuses auto's DirectML request.
    let auto = assert_provider(
        &invoke(Some("auto"), Some("0"), false),
        "auto",
        "directml",
        true,
    );
    assert_eq!(auto["cache_hits"], 1);
    let directml_hit = invoke(Some("directml"), Some("0"), true);
    let directml = assert_provider(&directml_hit, "directml", "directml", true);
    assert_eq!(directml["cache_hits"], 1); // Same request/device, different mode.
    assert_eq!(
        directml["ocr_providers"][0]["provider_actual"],
        default["ocr_providers"][0]["provider_actual"]
    );
    let diagnostics = String::from_utf8_lossy(&directml_hit.stderr);
    for expected in [
        "mode=directml",
        "requested=directml",
        "device_id=0",
        "cache_hit=true",
    ] {
        assert!(
            diagnostics.contains(expected),
            "missing {expected}: {diagnostics}"
        );
    }
    let auto_hit = invoke(Some("auto"), Some("0"), true);
    assert_provider(&auto_hit, "auto", "directml", true);
    assert!(String::from_utf8_lossy(&auto_hit.stderr).contains("OCR mode=auto requested=directml"));
    assert_eq!(
        assert_provider(&invoke(Some("cpu"), Some("0"), false), "cpu", "cpu", true)["cache_hits"],
        1
    );
    assert_eq!(
        summary(&invoke(Some("cpu"), Some("1"), false))["cache_misses"],
        1
    );
    // An invalid device guarantees fallback on GPU and non-GPU machines alike.
    let fallback_output = invoke(Some("auto"), Some("2147483647"), false);
    let fallback = assert_provider(&fallback_output, "auto", "directml", false);
    assert_eq!(fallback["cache_misses"], 1);
    assert_eq!(fallback["ocr_providers"][0]["provider_actual"], "cpu");
    assert!(fallback["ocr_providers"][0]["fallback_reason"].is_string());
    let hit = invoke(Some("DiReCtMl"), Some("2147483647"), true);
    assert_eq!(
        assert_provider(&hit, "directml", "directml", true)["cache_hits"],
        1
    );
    let diagnostics = String::from_utf8_lossy(&hit.stderr);
    let fallback_reason = fallback["ocr_providers"][0]["fallback_reason"]
        .as_str()
        .unwrap();
    assert!(
        diagnostics.contains(&format!("fallback={fallback_reason}")),
        "cache-hit log must preserve the actual fallback reason: {diagnostics}"
    );
    for expected in [
        "mode=directml",
        "requested=directml",
        "actual=cpu",
        "device_id=2147483647",
        "fallback=directml-",
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
    assert!(String::from_utf8_lossy(&strict.stderr).contains("directml-"));
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
        "fallback=directml-",
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
