//! Production CLI + real OCR + synthetic media. No private recordings are required.
#![cfg(feature = "onnx")]
use serde_json::Value;
mod common;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};
static ID: AtomicUsize = AtomicUsize::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "pubg-cli-e2e-{}-{}",
            std::process::id(),
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn tool(name: &str) -> PathBuf {
    let dir = std::env::var_os("PUBG_HIGHLIGHT_TRIM_FFMPEG_DIR")
        .expect("set PUBG_HIGHLIGHT_TRIM_FFMPEG_DIR");
    PathBuf::from(dir).join(name)
}
fn blank(path: &Path) {
    assert!(Command::new(tool("ffmpeg.exe"))
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=640x360:r=10:d=1",
            "-c:v",
            "libx264"
        ])
        .arg(path)
        .status()
        .unwrap()
        .success());
}
fn event(path: &Path) {
    let roi = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/missed_event_ocr/23.04.41.02_32.png");
    assert!(Command::new(tool("ffmpeg.exe"))
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-loop",
            "1",
            "-i"
        ])
        .arg(roi)
        .args([
            "-t",
            "4",
            "-vf",
            "pad=ceil(iw/2)*2:ceil(ih/2)*2",
            "-r",
            "10",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p"
        ])
        .arg(path)
        .status()
        .unwrap()
        .success());
}
fn invoke(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pubg_highlight_trim"))
        .args(args)
        .output()
        .unwrap()
}
fn summary(out: &Output) -> Value {
    let text = String::from_utf8_lossy(&out.stdout);
    let lines = text
        .lines()
        .filter_map(|l| l.strip_prefix("SUMMARY "))
        .collect::<Vec<_>>();
    assert_eq!(
        lines.len(),
        1,
        "stdout={text}\nstderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_str(lines[0]).unwrap()
}
fn progress(out: &Output, total: u64, workers: u64) {
    // Filter to scan-phase only: other phases (dedupe/trim/merge) also emit PROGRESS.
    let rows = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            l.strip_prefix("PROGRESS ")
                .map(|p| serde_json::from_str::<Value>(p).unwrap())
        })
        .filter(|v| v["phase"] == "scan")
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), total as usize + 1, "scan PROGRESS count");
    for (i, r) in rows.iter().enumerate() {
        assert_eq!(r["phase"], "scan");
        assert_eq!(r["current"], i, "scan current at index {i}");
        assert_eq!(r["total"], total);
        assert_eq!(r["workers"], workers)
    }
}
fn rows(path: &Path) -> Vec<BTreeMap<String, String>> {
    let b = fs::read(path).unwrap();
    assert!(b.starts_with(b"\xef\xbb\xbf"));
    let mut reader = csv::Reader::from_reader(&b[3..]);
    let headers = reader.headers().unwrap().clone();
    reader
        .records()
        .map(|r| {
            headers
                .iter()
                .zip(r.unwrap().iter())
                .map(|(k, v)| (k.into(), v.into()))
                .collect()
        })
        .collect()
}
fn hash(path: &Path) -> Vec<u8> {
    Sha256::digest(fs::read(path).unwrap()).to_vec()
}
#[test]
fn missing_models_is_a_failure_not_a_successful_probe_or_empty_detection() {
    let t = Temp::new();
    let src = t.0.join("blank.mp4");
    blank(&src);
    let outdir = t.0.join("out");
    let o = Command::new(env!("CARGO_BIN_EXE_pubg_highlight_trim"))
        .arg(&src)
        .args(["--scan-only", "-o"])
        .arg(&outdir)
        .env("PUBG_OCR_MODEL_DIR", t.0.join("missing-models"))
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(1));
    assert!(!String::from_utf8_lossy(&o.stdout).contains("SUMMARY "));
    assert!(!outdir.exists());
    assert!(src.exists());
}
#[test]
#[ignore = "requires pinned PUBG_OCR_MODEL_DIR and FFmpeg; run in local/release acceptance"]
fn blank_and_unreadable_sources_are_distinct_and_exit_two() {
    let t = Temp::new();
    let src = t.0.join("01 录像.mp4");
    blank(&src);
    let bad = t.0.join("02 bad.mp4");
    fs::write(&bad, b"not a video").unwrap();
    let before = hash(&src);
    let out = t.0.join("out");
    let o = invoke(&[
        "--files",
        src.to_str().unwrap(),
        bad.to_str().unwrap(),
        "--scan-only",
        "--jobs",
        "2",
        "--no-auto-candidate-csv",
        "-o",
        out.to_str().unwrap(),
    ]);
    assert_eq!(
        o.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(o.stderr.is_empty());
    progress(&o, 2, 2);
    let s = summary(&o);
    assert_eq!(s["source_count"], 2);
    assert_eq!(s["included_count"], 0);
    assert_eq!(s["skipped_count"], 2);
    assert_eq!(s["unreadable_count"], 1);
    let r = rows(&out.join("检测与裁剪记录.csv"));
    assert_eq!(r[0]["Status"], "skipped");
    assert_eq!(r[1]["Status"], "unreadable");
    assert_eq!(r[1]["Method"], "video-unreadable");
    assert_eq!(hash(&src), before);
}
#[test]
#[ignore = "requires pinned PUBG_OCR_MODEL_DIR and FFmpeg; run in local/release acceptance"]
fn candidate_discovery_enables_fast_scan_but_multikill_and_match_end_force_full_scan() {
    let t = Temp::new();
    let source = t.0.join("input");
    fs::create_dir(&source).unwrap();
    let first = source.join("01.单次淘汰.DVR.mp4");
    blank(&first);
    fs::copy(&first, source.join("02.双次淘汰.DVR.mp4")).unwrap();
    fs::copy(&first, source.join("03.比赛结束.DVR.mp4")).unwrap();
    let prior = t.0.join("fullscan_2s");
    fs::create_dir(&prior).unwrap();
    let candidate = prior.join("candidate_events.csv");
    fs::write(&candidate, "Name,EventSec\r\n").unwrap();
    let out = t.0.join("out");
    let o = invoke(&[
        source.to_str().unwrap(),
        "--scan-only",
        "--jobs",
        "1",
        "--scan-end",
        "0.5",
        "-o",
        out.to_str().unwrap(),
    ]);
    assert_eq!(
        o.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    let s = summary(&o);
    assert!(
        same_file::is_same_file(Path::new(s["candidate_csv"].as_str().unwrap()), &candidate)
            .unwrap()
    );
    assert_eq!(s["full_scan"], false);
    let r = rows(&out.join("检测与裁剪记录.csv"));
    assert_eq!(r.len(), 3);
    assert_eq!(r[0]["SampledFrames"], "0");
    assert_eq!(r[1]["SampledFrames"], "1");
    assert_eq!(r[1]["OcrGateSkippedFrames"], "1");
    assert_eq!(r[2]["OcrGateReason"], "disabled");
    assert_eq!(r[2]["OcrRequests"], "1");
}
#[test]
#[ignore = "requires pinned PUBG_OCR_MODEL_DIR and FFmpeg; run in local/release acceptance"]
fn provider_actual_and_fallback_are_recorded_without_polluting_lifecycle_output() {
    let t = Temp::new();
    let src = t.0.join("blank.mp4");
    blank(&src);
    let out = t.0.join("out");
    let o = Command::new(env!("CARGO_BIN_EXE_pubg_highlight_trim"))
        .arg(&src)
        .args([
            "--scan-only",
            "--jobs",
            "1",
            "--scan-end",
            "0.5",
            "--no-auto-candidate-csv",
            "-o",
        ])
        .arg(&out)
        .env("PUBG_OCR_PROVIDER", "directml")
        .env("PUBG_OCR_DIRECTML_DEVICE_ID", "2147483647")
        .output()
        .unwrap();
    assert_eq!(
        o.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    let s = summary(&o);
    let provider = &s["ocr_providers"][0];
    assert_eq!(provider["source_index"], 1);
    assert!(provider.get("source").is_none());
    assert!(!provider.to_string().contains(src.to_str().unwrap()));
    assert_eq!(provider["provider_requested"], "directml");
    if provider["provider_actual"] == "cpu" {
        let reason = provider["fallback_reason"].as_str().unwrap();
        assert!(!reason.is_empty());
        if !cfg!(feature = "directml") {
            assert!(reason.contains("not enabled"), "{reason}");
        }
        assert!(!reason.contains(src.to_str().unwrap()));
        assert!(!reason.contains(&std::env::var("PUBG_OCR_MODEL_DIR").unwrap()));
    } else {
        assert!(cfg!(feature = "directml"));
        assert_eq!(provider["provider_actual"], "directml");
    }
    for line in String::from_utf8_lossy(&o.stdout).lines() {
        assert!(
            line.starts_with("PROGRESS ")
                || line.starts_with("SUMMARY ")
                || line.contains("] SKIP "),
            "{line}"
        );
    }
    let p: Value = serde_json::from_slice(&fs::read(out.join("profile.json")).unwrap()).unwrap();
    assert_eq!(p["ocr_providers"], s["ocr_providers"]);
}
#[test]
#[ignore = "requires pinned PUBG_OCR_MODEL_DIR and FFmpeg; run in local/release acceptance"]
fn python_float_validation_stages_preserve_runtime_exit_codes() {
    let t = Temp::new();
    let src = t.0.join("blank.mp4");
    blank(&src);
    let out = t.0.join("out");
    for (option, value, expected, summary_expected) in [
        ("--before", "nan", 2, true),
        ("--after", "-inf", 2, true),
        ("--scan-start", "nan", 1, false),
        ("--scan-start", "inf", 1, false),
        ("--scan-end", "nan", 1, false),
        ("--candidate-lookback", "nan", 1, false),
        ("--adaptive-window", "nan", 1, false),
        ("--ocr-min-interval", "nan", 1, false),
        ("--refine-max-seconds", "nan", 1, false),
        ("--assist-max-seconds", "nan", 1, false),
        ("--priority-window", "nan:2", 1, false),
        ("--priority-window", "1:inf", 1, false),
        ("--roi", "nan,0,1,1", 2, false),
    ] {
        let value = format!("{option}={value}");
        let o = invoke(&[
            src.to_str().unwrap(),
            "--scan-only",
            "--jobs",
            "1",
            "--scan-end",
            "0.5",
            "--no-auto-candidate-csv",
            "-y",
            "-o",
            out.to_str().unwrap(),
            &value,
        ]);
        assert_eq!(
            o.status.code(),
            Some(expected),
            "{value}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&o.stdout).contains("SUMMARY "),
            summary_expected,
            "{value}"
        );
    }
}
#[test]
#[ignore = "requires pinned PUBG_OCR_MODEL_DIR and FFmpeg; run in local/release acceptance"]
fn cached_empty_detection_with_failed_probe_is_still_an_unreadable_source() {
    use clap::Parser;
    use pubg_ocr_engine::{
        config::Config,
        persistent_cache,
        scan_adapter::{source_config, EngineSettings},
        source_files,
    };
    let t = Temp::new();
    let src = t.0.join("bad.mp4");
    fs::write(&src, b"not a container").unwrap();
    let cache = t.0.join("cache");
    let out = t.0.join("out");
    let args = [
        src.to_str().unwrap(),
        "--scan-only",
        "--jobs",
        "1",
        "--no-auto-candidate-csv",
        "--cache-dir",
        cache.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
    ];
    let c = Config::try_parse_from(std::iter::once("app").chain(args)).unwrap();
    let inputs = source_files::select(&c).unwrap();
    let config = source_config(&c, &inputs.sources[0], None);
    let mut settings = EngineSettings::discover().unwrap();
    settings.identity.push_str(":effective-threads=1");
    let key = persistent_cache::cache_key(
        &src,
        &serde_json::json!({"config":config,"candidate_times":[]}),
        &settings.identity,
    )
    .unwrap();
    persistent_cache::write(
        &cache,
        &key,
        &serde_json::json!({"detections":[],"provider":null}),
    )
    .unwrap();
    let o = invoke(&args);
    assert_eq!(
        o.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    let s = summary(&o);
    assert_eq!(s["cache_hits"], 1);
    assert_eq!(s["unreadable_count"], 1);
    assert_eq!(s["included_count"], 0);
    let r = rows(&out.join("检测与裁剪记录.csv"));
    assert_eq!(r[0]["Status"], "unreadable");
    assert_eq!(r[0]["Method"], "video-unreadable");
}
#[test]
#[ignore = "requires pinned PUBG_OCR_MODEL_DIR and FFmpeg; run in local/release acceptance"]
fn own_kill_cli_trims_merges_reports_and_reuses_valid_cache_without_touching_original() {
    let t = Temp::new();
    let src = t.0.join("录像 with space.mp4");
    event(&src);
    let before = hash(&src);
    let cache = t.0.join("cache");
    let out = t.0.join("out");
    let args = vec![
        src.to_str().unwrap(),
        "--game-lang",
        "zh-Hant",
        "--target",
        "own-kill",
        "--roi",
        "0,0,1,1",
        "--no-brightness-gate",
        "--min-event-sec",
        "0",
        "--before",
        "1",
        "--after",
        "1",
        "--jobs",
        "1",
        "--no-auto-candidate-csv",
        "--scan-end",
        "3",
        "--coarse-step",
        "2",
        "-o",
        out.to_str().unwrap(),
        "-y",
        "--merge",
        "--cache-dir",
        cache.to_str().unwrap(),
    ];
    let o = invoke(&args);
    assert_eq!(
        o.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(o.stderr.is_empty());
    progress(&o, 1, 1);
    let s = summary(&o);
    assert_eq!(s["included_count"], 1);
    assert_eq!(s["cache_misses"], 1);
    assert_eq!(s["ocr_providers"][0]["source_index"], 1);
    assert!(s["ocr_providers"][0].get("source").is_none());
    for entry in fs::read_dir(&cache).unwrap() {
        let value: Value =
            serde_json::from_slice(&fs::read(entry.unwrap().path()).unwrap()).unwrap();
        assert!(value["payload"]["provider"].get("source").is_none());
        assert_eq!(value["payload"]["provider"]["source_index"], 1);
    }
    assert_eq!(s["merge_duration_sec"], 1.0);
    assert!(Path::new(s["merge_output"].as_str().unwrap()).is_file());
    let r = rows(&out.join("检测与裁剪记录.csv"));
    assert_eq!(r[0]["Status"], "included");
    assert_eq!(r[0]["Target"], "own-kill");
    assert_eq!(r[0]["KeepDurationSec"], "1.000");
    let profile: Value =
        serde_json::from_slice(&fs::read(out.join("profile.json")).unwrap()).unwrap();
    let snapshot_requests = r[0]["OcrRequests"].parse::<u64>().unwrap();
    assert!(
        profile["telemetry"]["current"]["ocr_requests"]
            .as_u64()
            .unwrap()
            > snapshot_requests,
        "scan continues after event snapshot: {profile}"
    );
    assert!(
        profile["telemetry"]["current"]["ocr_predict_seconds_sum"]
            .as_f64()
            .unwrap()
            > r[0]["OcrSeconds"].parse::<f64>().unwrap()
    );
    assert_eq!(
        profile["telemetry"]["current_basis"],
        "completed_scan_report"
    );
    assert!(
        r[0]["TotalSeconds"].parse::<f64>().unwrap()
            >= r[0]["DetectSeconds"].parse::<f64>().unwrap(),
        "single-worker total must include scanning"
    );
    let identities: Value =
        serde_json::from_str(include_str!("../fixtures/missed_events.json")).unwrap();
    let expected = identities["frames"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["target"] == "own-kill" && case["action"] == "knock")
        .unwrap();
    let mut video_identity = expected.clone();
    video_identity["identity_sha256"] = expected["video_identity_sha256"].clone();
    video_identity["identity_chars"] = expected["video_identity_chars"].clone();
    assert!(common::contains_identity(
        &r[0]["EventKeys"],
        &video_identity
    ));
    assert!(Path::new(&r[0]["Output"])
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("001_own-kill_"));
    let o = invoke(&args);
    assert_eq!(o.status.code(), Some(0));
    let s = summary(&o);
    assert_eq!(s["cache_hits"], 1);
    assert_eq!(s["cache_misses"], 0);
    let profile: Value =
        serde_json::from_slice(&fs::read(out.join("profile.json")).unwrap()).unwrap();
    assert_eq!(profile["telemetry"]["current"]["ocr_requests"], 0);
    assert_eq!(
        profile["telemetry"]["cached_historical"]["ocr_requests"],
        snapshot_requests
    );
    assert_eq!(
        profile["telemetry"]["cached_historical_basis"],
        "maximum_detection_snapshot_through_last_accepted_event_not_completed_scan"
    );
    assert!(profile["telemetry"]["cached_historical"]
        .get("refine_calls")
        .is_none());
    assert!(profile["telemetry"]["cached_historical"]
        .get("assist_calls")
        .is_none());
    assert_eq!(s["ocr_providers"][0]["provider_mode"], "auto");
    assert_eq!(s["ocr_providers"][0]["provider_requested"], "directml");
    let provider = &s["ocr_providers"][0];
    if provider["provider_actual"] == "cpu" {
        assert!(provider["fallback_reason"].is_string());
    } else {
        assert_eq!(provider["provider_actual"], "directml");
        assert!(provider["fallback_reason"].is_null());
    }
    assert_eq!(
        s["ocr_providers"][0]["model_version"],
        "PP-OCRv6_medium_onnx"
    );
    assert_eq!(hash(&src), before);
    let json: Value = serde_json::from_slice(&fs::read(out.join("summary.json")).unwrap()).unwrap();
    assert_eq!(json, s);
    assert!(fs::read(out.join("profile.json"))
        .unwrap()
        .starts_with(b"{"));
}
#[test]
#[ignore = "requires pinned PUBG_OCR_MODEL_DIR and FFmpeg; run in local/release acceptance"]
fn split_merge_archives_only_completed_sources_using_temporary_copies() {
    let t = Temp::new();
    let src = t.0.join("included.mp4");
    event(&src);
    let bad = t.0.join("bad.mp4");
    fs::write(&bad, b"bad container").unwrap();
    let before = hash(&src);
    let out = t.0.join("out");
    let merged = t.0.join("merged.mp4");
    let o = invoke(&[
        "--files",
        src.to_str().unwrap(),
        bad.to_str().unwrap(),
        "--game-lang",
        "zh-Hant",
        "--roi",
        "0,0,1,1",
        "--no-brightness-gate",
        "--min-event-sec",
        "0",
        "--before",
        "1",
        "--after",
        "1",
        "--scan-end",
        "0.5",
        "--jobs",
        "2",
        "--no-auto-candidate-csv",
        "-o",
        out.to_str().unwrap(),
        "--merge",
        merged.to_str().unwrap(),
        "--split-merge-by-target",
        "--move-originals",
    ]);
    assert_eq!(
        o.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    let s = summary(&o);
    assert_eq!(s["originals_moved"], 1);
    assert_eq!(s["originals_failed"], 0);
    assert_eq!(s["unreadable_count"], 1);
    assert!(!src.exists());
    assert!(bad.exists());
    assert_eq!(hash(&out.join("originals/included.mp4")), before);
    let outputs = s["merge_outputs"].as_array().unwrap();
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0]["target"], "own-kill");
    assert_eq!(
        Path::new(outputs[0]["path"].as_str().unwrap())
            .file_name()
            .unwrap(),
        "merged_own-kill.mp4"
    );
    assert!(!merged.exists());
    let archived = rows(&out.join("moved_sources.csv"));
    assert_eq!(archived.len(), 1);
    assert_eq!(archived[0]["Name"], "included.mp4");
}
#[cfg(windows)]
#[test]
#[ignore = "requires Windows console, pinned models and FFmpeg; run in local/release acceptance"]
fn console_cancel_exits_one_without_archiving_or_reporting_fake_success() {
    use std::{
        io::{BufRead, BufReader, Read},
        os::windows::process::CommandExt,
        process::Stdio,
        time::{Duration, Instant},
    };
    #[link(name = "kernel32")]
    extern "system" {
        fn GenerateConsoleCtrlEvent(event: u32, group: u32) -> i32;
        fn GetConsoleCP() -> u32;
        fn AllocConsole() -> i32;
        fn FreeConsole() -> i32;
        fn GetStdHandle(kind: u32) -> *mut std::ffi::c_void;
        fn SetStdHandle(kind: u32, handle: *mut std::ffi::c_void) -> i32;
    }
    // CI service processes may have no console at all. Own one only for this
    // single-threaded acceptance case; never attach to or signal a user's console.
    struct ConsoleLease(Option<[*mut std::ffi::c_void; 3]>);
    impl Drop for ConsoleLease {
        fn drop(&mut self) {
            if let Some(handles) = self.0 {
                unsafe {
                    FreeConsole();
                    for (kind, handle) in [(-10i32) as u32, (-11i32) as u32, (-12i32) as u32]
                        .into_iter()
                        .zip(handles)
                    {
                        SetStdHandle(kind, handle);
                    }
                }
            }
        }
    }
    let _console = unsafe {
        if GetConsoleCP() == 0 {
            let handles = [
                GetStdHandle((-10i32) as u32),
                GetStdHandle((-11i32) as u32),
                GetStdHandle((-12i32) as u32),
            ];
            assert_ne!(
                AllocConsole(),
                0,
                "allocate an owned console for cancellation acceptance"
            );
            ConsoleLease(Some(handles))
        } else {
            ConsoleLease(None)
        }
    };
    let t = Temp::new();
    let src = t.0.join("original.mp4");
    event(&src);
    let before = hash(&src);
    let out = t.0.join("out");
    let mut child = Command::new(env!("CARGO_BIN_EXE_pubg_highlight_trim"))
        .arg(&src)
        .args([
            "--game-lang",
            "zh-Hant",
            "--roi",
            "0,0,1,1",
            "--no-brightness-gate",
            "--move-originals",
            "--no-auto-candidate-csv",
            "-o",
        ])
        .arg(&out)
        .creation_flags(0x200)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let output = std::thread::spawn(move || {
        let mut collected = String::new();
        for line in BufReader::new(stdout).lines() {
            let line = line.unwrap();
            if line.starts_with("PROGRESS ") {
                let _ = tx.send(());
            }
            collected.push_str(&line);
            collected.push('\n');
        }
        collected
    });
    let stderr = child.stderr.take().unwrap();
    let errors = std::thread::spawn(move || {
        let mut text = String::new();
        BufReader::new(stderr).read_to_string(&mut text).unwrap();
        text
    });
    rx.recv_timeout(Duration::from_secs(30))
        .expect("scan started");
    assert_ne!(unsafe { GenerateConsoleCtrlEvent(1, child.id()) }, 0);
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if start.elapsed() > Duration::from_secs(30) {
            let _ = child.kill();
            panic!("cancellation did not stop child")
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let stdout = output.join().unwrap();
    let stderr = errors.join().unwrap();
    assert_eq!(status.code(), Some(1), "{stdout}\n{stderr}");
    assert!(stderr.to_lowercase().contains("cancel"));
    assert!(!stdout.contains("SUMMARY "));
    assert!(!out.exists());
    assert_eq!(hash(&src), before);
}
#[test]
#[ignore = "requires pinned PUBG_OCR_MODEL_DIR and FFmpeg; run in local/release acceptance"]
fn parallel_completion_keeps_source_order_and_detections_equal_to_single_worker() {
    let t = Temp::new();
    let a = t.0.join("01 event.mp4");
    let b = t.0.join("02 blank.mp4");
    event(&a);
    blank(&b);
    let mut outcomes = Vec::new();
    for jobs in ["1", "2", "auto"] {
        let out = t.0.join(format!("out-{jobs}"));
        let o = invoke(&[
            "--files",
            b.to_str().unwrap(),
            a.to_str().unwrap(),
            "--game-lang",
            "zh-Hant",
            "--roi",
            "0,0,1,1",
            "--no-brightness-gate",
            "--min-event-sec",
            "0",
            "--scan-only",
            "--scan-end",
            "0.5",
            "--jobs",
            jobs,
            "--no-auto-candidate-csv",
            "-o",
            out.to_str().unwrap(),
        ]);
        assert_eq!(
            o.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&o.stderr)
        );
        let rows = rows(&out.join("检测与裁剪记录.csv"));
        assert_eq!(rows[0]["Name"], "01 event.mp4");
        assert_eq!(rows[1]["Name"], "02 blank.mp4");
        outcomes.push(
            rows.iter()
                .map(|r| {
                    [
                        "Name",
                        "Status",
                        "Target",
                        "EventSec",
                        "EventKeys",
                        "KeepStartSec",
                        "KeepEndSec",
                    ]
                    .map(|key| r[key].clone())
                })
                .collect::<Vec<_>>(),
        );
    }
    assert_eq!(outcomes[0], outcomes[1]);
    assert_eq!(outcomes[1], outcomes[2]);
}

#[test]
#[ignore = "requires pinned PUBG_OCR_MODEL_DIR and FFmpeg; run in local/release acceptance"]
fn verbose_scan_failure_preserves_initialized_provider_diagnostics() {
    let t = Temp::new();
    let src = t.0.join("blank.mp4");
    blank(&src);
    let wrapper = media_wrapper(&t.0, "probe-other");
    let out = Command::new(env!("CARGO_BIN_EXE_pubg_highlight_trim"))
        .arg(&src)
        .args([
            "--scan-only",
            "--verbose",
            "--ocr-provider",
            "cpu",
            "--jobs",
            "1",
            "--no-auto-candidate-csv",
            "--ffprobe",
        ])
        .arg(wrapper)
        .arg("-o")
        .arg(t.0.join("out"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.contains("SUMMARY "));
    assert!(!stdout.contains("OCR mode="));
    let stderr = String::from_utf8_lossy(&out.stderr);
    for expected in [
        "mode=cpu",
        "requested=cpu",
        "actual=cpu",
        "device_id=0",
        "fallback=",
        "cache_hit=false",
    ] {
        assert!(stderr.contains(expected), "missing {expected}: {stderr}");
    }
}

// Native marker wrappers exercise the actual CLI subprocess diagnostic boundary.
fn media_wrapper(root: &Path, name: &str) -> PathBuf {
    let source = root.join("wrapper.rs");
    let binary = root.join(format!("{name}.exe"));
    fs::write(&source, r#"
        fn main() {
            use std::io::Write;
            let name=std::env::current_exe().unwrap().file_stem().unwrap().to_string_lossy().into_owned();
            if name.contains("dimension-bad") {
                if std::env::args().any(|a|a=="stream=width,height") {eprintln!("changed human diagnostic format");std::process::exit(1);}
                println!("1");return;
            }
            if name.contains("decode-bad") {eprintln!("changed human diagnostic format");std::process::exit(1);}
            if name.contains("other") { eprintln!("PROBE_FAILURE_MARKER"); std::process::exit(7); }
            if name.contains("hang") { std::thread::sleep(std::time::Duration::from_secs(90)); return; }
            let args:Vec<_>=std::env::args_os().skip(1).collect();
            if name.contains("probe") {
                eprintln!("PROBE_SUCCESS_MARKER");
            } else if args.iter().any(|a|a=="h264_nvenc") {
                eprintln!("NVENC_FALLBACK_MARKER"); std::process::exit(1);
            } else { eprintln!("FFMPEG_SUCCESS_MARKER"); }
            let tool=if name.contains("probe"){"ffprobe.exe"}else{"ffmpeg.exe"};
            let out=std::process::Command::new(std::path::PathBuf::from(std::env::var_os("WRAPPER_REAL_TOOLS").unwrap()).join(tool)).args(args).output().unwrap();
            std::io::stdout().write_all(&out.stdout).unwrap();
            std::io::stderr().write_all(&out.stderr).unwrap();
            std::process::exit(out.status.code().unwrap_or(1));
        }
    "#).unwrap();
    assert!(Command::new("rustc")
        .arg(source)
        .arg("-o")
        .arg(&binary)
        .status()
        .unwrap()
        .success());
    binary
}
#[test]
#[ignore = "requires pinned PUBG_OCR_MODEL_DIR and FFmpeg; run in local/release acceptance"]
fn native_media_diagnostics_are_stderr_only_and_require_verbose_not_profile() {
    let t = Temp::new();
    let src = t.0.join("event.mp4");
    event(&src);
    let ffmpeg = media_wrapper(&t.0, "ffmpeg-marker");
    let ffprobe = media_wrapper(&t.0, "probe-marker");
    for mode in ["default", "profile", "verbose"] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_pubg_highlight_trim"));
        command
            .arg(&src)
            .args([
                "--game-lang",
                "zh-Hant",
                "--target",
                "own-kill",
                "--roi",
                "0,0,1,1",
                "--no-brightness-gate",
                "--min-event-sec",
                "0",
                "--before",
                "1",
                "--after",
                "1",
                "--scan-end",
                "0.5",
                "--jobs",
                "1",
                "--no-auto-candidate-csv",
                "--ffmpeg",
            ])
            .arg(&ffmpeg)
            .arg("--ffprobe")
            .arg(&ffprobe)
            .arg("-o")
            .arg(t.0.join(mode))
            .env(
                "WRAPPER_REAL_TOOLS",
                std::env::var_os("PUBG_HIGHLIGHT_TRIM_FFMPEG_DIR").unwrap(),
            );
        if mode != "default" {
            command.arg(format!("--{mode}"));
        }
        let out = command.output().unwrap();
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(summary(&out)["included_count"], 1);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        for marker in [
            "PROBE_SUCCESS_MARKER",
            "FFMPEG_SUCCESS_MARKER",
            "NVENC_FALLBACK_MARKER",
        ] {
            assert!(!stdout.contains(marker));
            assert_eq!(
                stderr.contains(marker),
                mode == "verbose",
                "{mode}: missing or unexpected {marker}"
            );
        }
    }
}
#[test]
#[ignore = "requires pinned PUBG_OCR_MODEL_DIR and FFmpeg; includes a real 60-second timeout"]
fn cli_probe_system_failure_and_timeout_are_fatal_not_source_skips() {
    let t = Temp::new();
    let src = t.0.join("blank.mp4");
    blank(&src);
    for name in ["probe-other", "probe-hang"] {
        let probe = media_wrapper(&t.0, name);
        let outdir = t.0.join(name);
        let out = Command::new(env!("CARGO_BIN_EXE_pubg_highlight_trim"))
            .arg(&src)
            .args([
                "--scan-only",
                "--jobs",
                "1",
                "--no-auto-candidate-csv",
                "--ffprobe",
            ])
            .arg(probe)
            .arg("-o")
            .arg(&outdir)
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(1),
            "{}",
            String::from_utf8_lossy(&out.stdout)
        );
        assert!(!String::from_utf8_lossy(&out.stdout).contains("SUMMARY "));
        assert!(!outdir.exists());
        let error = String::from_utf8_lossy(&out.stderr);
        assert!(
            error.contains(if name.contains("hang") {
                "timed out"
            } else {
                "PROBE_FAILURE_MARKER"
            }),
            "{error}"
        );
        assert!(src.exists());
    }
}

#[test]
#[ignore = "requires pinned PUBG_OCR_MODEL_DIR and FFmpeg; run in local/release acceptance"]
fn reordered_candidates_and_parallel_jobs_reuse_cache_with_current_source_indexes() {
    let t = Temp::new();
    let a = t.0.join("a.mp4");
    blank(&a);
    let b = t.0.join("b.mp4");
    fs::copy(&a, &b).unwrap();
    let candidate = t.0.join("candidate.csv");
    let cache = t.0.join("cache");
    for (run, jobs) in [(0, "1"), (1, "2")] {
        fs::write(
            &candidate,
            if run == 0 {
                "Name,EventSec
a.mp4,0.8
a.mp4,0.2
b.mp4,0.8
b.mp4,0.2
"
            } else {
                "EventSec,Name
0.2,b.mp4
0.8,b.mp4
0.2,a.mp4
0.8,a.mp4
"
            },
        )
        .unwrap();
        let out = Command::new(env!("CARGO_BIN_EXE_pubg_highlight_trim"))
            .arg("--files")
            .arg(&a)
            .arg(&b)
            .args([
                "--scan-only",
                "--scan-end",
                "0.5",
                "--jobs",
                jobs,
                "--candidate-csv",
            ])
            .arg(&candidate)
            .arg("--cache-dir")
            .arg(&cache)
            .arg("-o")
            .arg(t.0.join(format!("out-{run}")))
            .env("PUBG_OCR_THREADS", "1")
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(2),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let s = summary(&out);
        assert_eq!(s["cache_hits"], if run == 0 { 0 } else { 2 });
        assert_eq!(s["cache_misses"], if run == 0 { 2 } else { 0 });
        assert_eq!(s["ocr_providers"][0]["source_index"], 1);
        assert_eq!(s["ocr_providers"][1]["source_index"], 2);
        assert!(s["ocr_providers"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p.get("source").is_none()));
    }
}

#[test]
#[ignore = "requires pinned PUBG_OCR_MODEL_DIR and FFmpeg; run in local/release acceptance"]
fn media_adapter_preserves_typed_factory_errors_and_frame_failure_tags() {
    use clap::Parser;
    use pubg_ocr_engine::{
        config::Config,
        media::{FfmpegTools, MediaError},
        scan_adapter::{self, EngineSettings},
        scanner::{ScanBackend, ScanFailureKind},
        source_files,
    };
    let t = Temp::new();
    let src = t.0.join("blank.mp4");
    blank(&src);
    let c = Config::try_parse_from(["app", src.to_str().unwrap()]).unwrap();
    let input = source_files::select(&c).unwrap();
    let engine = EngineSettings::discover().unwrap().engine().unwrap();
    let mut tools =
        FfmpegTools::discover(Some(&tool("ffmpeg.exe")), Some(&tool("ffprobe.exe"))).unwrap();
    let bad = t.0.join("bad.mp4");
    fs::write(&bad, b"bad container").unwrap();
    let mut source = input.sources[0].clone();
    source.path = bad;
    assert!(matches!(
        scan_adapter::backend(&source, &tools, &engine),
        Err(MediaError::VideoUnreadable(_))
    ));
    tools.ffprobe = media_wrapper(&t.0, "probe-other");
    assert!(matches!(
        scan_adapter::backend(&input.sources[0], &tools, &engine),
        Err(MediaError::CommandFailed(_))
    ));
    tools.ffprobe = tool("ffprobe.exe");
    let mut backend = scan_adapter::backend(&input.sources[0], &tools, &engine).unwrap();
    fs::write(&src, b"container became unreadable after open").unwrap();
    assert!(backend.read_frame(0.).is_err());
    assert_eq!(backend.failure_kind(), ScanFailureKind::SourceUnreadable);
    tools.ffmpeg = t.0.join("missing-decoder.exe");
    blank(&src);
    let mut backend = scan_adapter::backend(&input.sources[0], &tools, &engine).unwrap();
    assert!(backend.read_frame(0.).is_err());
    assert_eq!(backend.failure_kind(), ScanFailureKind::Runtime);
}

#[test]
#[ignore = "requires pinned PUBG_OCR_MODEL_DIR and FFmpeg; run in local/release acceptance"]
fn cli_dimensions_and_decode_unreadable_remain_skips_with_arbitrary_native_diagnostics() {
    let t = Temp::new();
    let src = t.0.join("blank.mp4");
    blank(&src);
    for (name, option) in [("dimension-bad", "--ffprobe"), ("decode-bad", "--ffmpeg")] {
        let wrapper = media_wrapper(&t.0, name);
        let out = Command::new(env!("CARGO_BIN_EXE_pubg_highlight_trim"))
            .arg(&src)
            .args([
                "--scan-only",
                "--scan-end",
                "0.5",
                "--jobs",
                "1",
                "--no-auto-candidate-csv",
                option,
            ])
            .arg(wrapper)
            .arg("-o")
            .arg(t.0.join(name))
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(2),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(summary(&out)["unreadable_count"], 1);
        assert!(out.stderr.is_empty());
    }
}
