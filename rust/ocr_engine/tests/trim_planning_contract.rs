//! Production CLI orchestration with cached detections and controlled native media.
//! No ONNX session is created: tiny hash-validated model files are cache identities only.
#![cfg(feature = "onnx")]
use clap::Parser;
use pubg_ocr_engine::{
    config::Config,
    persistent_cache,
    scan_adapter::{source_config, EngineSettings},
    source_files,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::{
        atomic::{AtomicUsize, Ordering},
        OnceLock,
    },
};
static ID: AtomicUsize = AtomicUsize::new(0);
struct Fixture {
    root: PathBuf,
    models: PathBuf,
    args: Vec<String>,
}
fn media() -> &'static PathBuf {
    static BIN: OnceLock<PathBuf> = OnceLock::new();
    BIN.get_or_init(|| {
        let root = std::env::temp_dir().join(format!("trim-media-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let src = root.join("media.rs");
        let bin = root.join("media.exe");
        fs::write(&src, r#"
            fn main() {
                use std::{fs, io::Write, path::PathBuf};
                let a: Vec<String> = std::env::args().skip(1).collect();
                let root = PathBuf::from(std::env::var_os("TRIM_FIXTURE").unwrap());
                if a.iter().any(|v|v=="-version") {
                    if std::env::var_os("BAD_DIAGNOSTICS").is_some() { println!("ffmpeg version C:/private/ffmpeg"); return; }
                    println!("ffmpeg version fixture-1 private/build/path"); return;
                }
                if a.iter().any(|v|v=="stream=codec_name") {
                    if let Some(path) = std::env::var_os("BLOCK_FUTURE") {
                        let p = PathBuf::from(path);
                        fs::create_dir_all(p.parent().unwrap()).unwrap();
                        fs::write(&p, b"block").unwrap();
                        for n in 1..1000 { fs::write(p.with_file_name(format!("{}_{}.mp4",p.file_stem().unwrap().to_string_lossy(),n)), b"block").unwrap(); }
                    }
                    if std::env::var_os("BAD_DIAGNOSTICS").is_some() { std::process::exit(1); }
                    println!("h264"); return;
                }
                if a.iter().any(|v|v=="format=duration") {
                    if std::env::var_os("FAIL_VALIDATION").is_some() && a.last().unwrap().contains(".partial") { std::process::exit(1); }
                    if std::env::var_os("SLOW_VALIDATION").is_some() { std::thread::sleep(std::time::Duration::from_millis(80)); }
                    println!("60"); return;
                }
                if a.iter().any(|v|v=="stream=width,height") { println!("16,16"); return; }
                if a.iter().any(|v|v=="rawvideo") {
                    assert!(std::env::var_os("FORBID_FRAME_DECODE").is_none(), "dedupe must not decode frames");
                    let i = a.iter().position(|v|v=="-ss").unwrap();
                    let t: f64 = a[i+1].parse().unwrap();
                    std::io::stdout().write_all(&vec![((t*100.).round() as u64 % 256) as u8;16*16*3]).unwrap(); return;
                }
                if a.iter().any(|v|v=="lavfi") {
                    let mut log = fs::OpenOptions::new().append(true).create(true).open(root.join("capability.log")).unwrap();
                    writeln!(log, "probe").unwrap();
                    fs::write(a.last().unwrap(), b"probe").unwrap(); return;
                }
                let out = a.last().unwrap();
                if a.iter().any(|v|v=="concat") {
                    let mut log = fs::OpenOptions::new().append(true).create(true).open(root.join("merge.log")).unwrap();
                    writeln!(log, "{}", a.join("| ")).unwrap();
                    let i = a.iter().position(|v|v=="-i").unwrap();
                    assert!(fs::read_to_string(&a[i+1]).unwrap().lines().all(|line| {
                        let path = line.strip_prefix("file '").unwrap().strip_suffix("'").unwrap();
                        PathBuf::from(path).is_file()
                    }));
                } else {
                    if std::env::var_os("CHECK_SERIAL").is_some() {
                        let completed = fs::read_to_string(root.join("encode.log")).unwrap_or_default().lines().count();
                        // Each preceding clip must already be validated and published;
                        // concurrent encodes or deferring publication violate this.
                        let published = fs::read_dir(root.join("out/clips")).unwrap().filter(|e| {
                            e.as_ref().unwrap().file_name().to_string_lossy().ends_with(".mp4")
                                && !e.as_ref().unwrap().file_name().to_string_lossy().starts_with('.')
                        }).count();
                        assert_eq!(published, completed);
                    }
                    let mut log = fs::OpenOptions::new().append(true).create(true).open(root.join("encode.log")).unwrap();
                    log.write_all(format!("{}\n", a.join("| ")).as_bytes()).unwrap();
                    if let Some(path) = std::env::var_os("COLLIDE_ON_ENCODE") {
                        if !root.join("collided").exists() { fs::write(path, b"external-owner").unwrap(); fs::write(root.join("collided"), b"").unwrap(); }
                    }
                    if std::env::var_os("FAIL_ENCODE").is_some() { std::process::exit(7); }
                }
                if !a.iter().any(|v|v=="concat") && std::env::var_os("CONTROLLED_TRIM").is_some() {
                    use std::time::{Duration, Instant};
                    let i = a.iter().position(|v|v=="-ss").unwrap();
                    let start = &a[i+1];
                    let first = std::env::var("FIRST_START").unwrap_or("9.000".into());
                    let second = std::env::var("SECOND_START").unwrap_or("29.000".into());
                    let log = |event: &str| {
                        let mut file = fs::OpenOptions::new().append(true).create(true).open(root.join("workers.log")).unwrap();
                        // One append call per record: formatting directly into the
                        // shared file can split a line across concurrent processes.
                        file.write_all(format!("{event} {start}\n").as_bytes()).unwrap();
                    };
                    log("start");
                    fs::write(root.join(format!("started-{start}")), b"").unwrap();
                    if std::env::var_os("FAIL_WITH_BLOCKED_PEER").is_some() {
                        let peer = if start == &first { &second } else { &first };
                        let deadline = Instant::now() + Duration::from_secs(15);
                        while !root.join(format!("started-{peer}")).exists() {
                            assert!(Instant::now() < deadline, "peer worker never started");
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        fs::write(out, b"incomplete").unwrap();
                        let failing = std::env::var("FAILING_START").unwrap_or(first.clone());
                        if start == &failing { std::process::exit(7); }
                        let deadline = Instant::now() + Duration::from_secs(120);
                        while Instant::now() < deadline && !root.join("watchdog-release").exists() {
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        return;
                    }
                    if start == &second {
                        let deadline = Instant::now() + Duration::from_secs(15);
                        while !root.join(format!("started-{first}")).exists() {
                            assert!(Instant::now() < deadline, "first worker never started");
                            std::thread::sleep(Duration::from_millis(10));
                        }
                    }
                    if start == &first {
                        let deadline = Instant::now() + Duration::from_secs(15);
                        while !root.join(format!("finished-{second}")).exists() {
                            assert!(Instant::now() < deadline, "second worker did not complete while first was held");
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        // The bounded window must not launch a third task while a
                        // completed second result waits for its predecessor.
                        std::thread::sleep(Duration::from_millis(150));
                        assert!(!root.join("started-19.000").exists(), "unbounded pending results");
                        // A later completed task must remain private until its predecessor publishes.
                        assert!(fs::read_dir(root.join("out/clips")).unwrap().all(|e| {
                            e.unwrap().file_name().to_string_lossy().starts_with('.')
                        }), "out-of-order publication");
                        if std::env::var_os("CANCEL_TRIM").is_some() {
                            loop { std::thread::sleep(Duration::from_millis(10)); }
                        }
                        if std::env::var_os("FAIL_FIRST").is_some() {
                            fs::write(out, b"incomplete").unwrap();
                            log("finish");
                            std::process::exit(7);
                        }
                    }
                    fs::write(out, b"encoded").unwrap();
                    log("finish");
                    fs::write(root.join(format!("finished-{start}")), b"").unwrap();
                    return;
                }
                fs::write(out, b"encoded").unwrap();
            }
        "#).unwrap();
        assert!(Command::new("rustc").arg(src).arg("-o").arg(&bin).status().unwrap().success());
        bin
    })
}
impl Fixture {
    fn new(extra: &[&str]) -> Self {
        Self::with_detections(
            extra,
            [
                vec![
                    detection(10., "own-kill", "one"),
                    detection(30., "self-death", "two"),
                ],
                vec![detection(20., "own-kill", "three")],
            ],
        )
    }
    fn with_detections(extra: &[&str], detections: [Vec<Value>; 2]) -> Self {
        Self::with_paths(extra, detections, false)
    }
    fn with_paths(extra: &[&str], detections: [Vec<Value>; 2], same_basename: bool) -> Self {
        let root = std::env::temp_dir().join(format!(
            "trim-plan-{}-{}",
            std::process::id(),
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let models = root.join("models");
        fs::create_dir(&models).unwrap();
        let digest = format!("{:x}", Sha256::digest(b"cache-only"));
        let mut manifest = json!({"model_version":"cache-only"});
        for (key, file) in [
            ("detector", "det.onnx"),
            ("recognizer", "rec.onnx"),
            ("dictionary", "dict.txt"),
        ] {
            fs::write(models.join(file), b"cache-only").unwrap();
            manifest[key] = json!({"file":file,"sha256":digest});
        }
        fs::write(models.join("pp_ocrv6_medium.json"), manifest.to_string()).unwrap();
        let a = root.join("a.mp4");
        let b = if same_basename {
            fs::create_dir(root.join("other")).unwrap();
            root.join("other/a.mp4")
        } else {
            root.join("b.mp4")
        };
        fs::write(&a, b"original-a").unwrap();
        fs::write(&b, b"original-b").unwrap();
        let mut args = vec![
            "--files".into(),
            a.display().to_string(),
            b.display().to_string(),
            "--jobs".into(),
            "1".into(),
            "--ocr-provider".into(),
            "cpu".into(),
            "--ocr-device-id".into(),
            "0".into(),
            "--no-auto-candidate-csv".into(),
            "--cache-dir".into(),
            root.join("cache").display().to_string(),
            "-o".into(),
            root.join("out").display().to_string(),
            "--ffmpeg".into(),
            media().display().to_string(),
            "--ffprobe".into(),
            media().display().to_string(),
            "--before".into(),
            "1".into(),
            "--after".into(),
            "1".into(),
        ];
        args.extend(extra.iter().map(|s| s.to_string()));
        // EngineSettings uses environment for model discovery. Only fixture setup
        // touches it; serialize setup while subprocesses get explicit environments.
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _lock = LOCK.lock().unwrap();
        let old = std::env::var_os("PUBG_OCR_MODEL_DIR");
        let old_threads = std::env::var_os("PUBG_OCR_THREADS");
        std::env::set_var("PUBG_OCR_MODEL_DIR", &models);
        std::env::set_var("PUBG_OCR_THREADS", "1");
        let c =
            Config::try_parse_from(std::iter::once("app").chain(args.iter().map(String::as_str)))
                .unwrap();
        let mut settings = EngineSettings::from_config(&c).unwrap();
        match old {
            Some(v) => std::env::set_var("PUBG_OCR_MODEL_DIR", v),
            None => std::env::remove_var("PUBG_OCR_MODEL_DIR"),
        }
        match old_threads {
            Some(v) => std::env::set_var("PUBG_OCR_THREADS", v),
            None => std::env::remove_var("PUBG_OCR_THREADS"),
        }
        settings.identity.push_str(":effective-threads=1");
        let inputs = source_files::select(&c).unwrap();
        for (index, source) in inputs.sources.iter().enumerate() {
            let key = persistent_cache::cache_key(
                &source.path,
                &json!({"config":source_config(&c, source, None),"candidate_times":[]}),
                &settings.identity,
            )
            .unwrap();
            persistent_cache::write(
                c.cache_dir.as_ref().unwrap(),
                &key,
                &json!({"detections":detections[index],"provider":{"provider_actual":"cpu","provider_requested":"directml","fallback_reason":"directml-unavailable","cache_hit":false}}),
            )
            .unwrap();
        }
        Self { root, models, args }
    }
    fn command(&self) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_pubg_highlight_trim"));
        c.args(&self.args)
            .env("PUBG_OCR_MODEL_DIR", &self.models)
            .env("PUBG_OCR_THREADS", "1")
            .env("TRIM_FIXTURE", &self.root);
        c
    }
    fn summary(&self, out: &Output) -> Value {
        let text = String::from_utf8_lossy(&out.stdout);
        serde_json::from_str(
            text.lines()
                .find_map(|l| l.strip_prefix("SUMMARY "))
                .unwrap_or_else(|| panic!("{text}\n{}", String::from_utf8_lossy(&out.stderr))),
        )
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn detection(sec: f64, target: &str, key: &str) -> Value {
    json!({"duration_sec":60.,"event_sec":sec,"target":target,"event_key":key,"method":"ocr","detector":"ocr", "ocr_seconds":2.0,"ocr_requests":7,"detect_seconds":3.0})
}

#[test]
fn parallel_trim_accepts_independent_worker_limit() {
    let f = Fixture::new(&["--merge"]);
    let out = f
        .command()
        .args(["--trim-jobs", "2"])
        .env("CONTROLLED_TRIM", "1")
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let profile: Value =
        serde_json::from_slice(&fs::read(f.root.join("out/profile.json")).unwrap()).unwrap();
    assert_eq!(profile["runtime"]["trim_workers"], 2);
    assert_eq!(profile["runtime"]["ocr_worker_capacity"], 1);
    assert_controlled_workers(&f);
    assert_eq!(
        fs::read_to_string(f.root.join("capability.log")).unwrap(),
        "probe\n"
    );
    let expected = [
        "001_own-kill_a.mp4",
        "002_self-death_a.mp4",
        "003_own-kill_b.mp4",
    ];
    let records = rows(&f);
    assert_eq!(records.len(), expected.len());
    for (row, name) in records.iter().zip(expected) {
        assert_eq!(PathBuf::from(&row["Output"]).file_name().unwrap(), name);
    }
    let summary = f.summary(&out);
    let concat = fs::read_to_string(summary["concat_list"].as_str().unwrap()).unwrap();
    assert_eq!(concat.lines().count(), expected.len());
    for (line, name) in concat.lines().zip(expected) {
        assert!(line.ends_with(&format!("{name}'")));
    }
}

fn assert_controlled_workers(f: &Fixture) {
    let log = fs::read_to_string(f.root.join("workers.log")).unwrap();
    let mut active = 0;
    let mut maximum = 0;
    for line in log.lines() {
        if line.starts_with("start ") {
            active += 1;
        } else {
            active -= 1;
        }
        maximum = maximum.max(active);
        assert!((0..=2).contains(&active), "unbounded trim workers: {log}");
    }
    assert_eq!(maximum, 2, "test must observe overlapping workers: {log}");
    assert_eq!(active, 0);
    let finishes = log
        .lines()
        .filter(|line| line.starts_with("finish "))
        .collect::<Vec<_>>();
    assert_eq!(finishes.len(), 3);
    assert!(
        finishes[0].ends_with("29.000"),
        "completion must be out of order: {log}"
    );
}

#[test]
fn no_planned_clips_never_probe_nvenc() {
    let f = Fixture::with_detections(&[], [vec![], vec![]]);
    let out = f.command().output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(!f.root.join("capability.log").exists());
    assert!(!f.root.join("encode.log").exists());
}

#[test]
fn strict_output_validation_failure_never_publishes_merges_or_archives() {
    let f = Fixture::new(&["--merge", "--move-originals"]);
    let out = f.command().env("FAIL_VALIDATION", "1").output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        if let Some(payload) = line.strip_prefix("PROGRESS ") {
            let progress: Value = serde_json::from_str(payload).unwrap();
            assert!(!(progress["phase"] == "trim" && progress["event"] == "done"));
            if progress["phase"] == "trim" {
                assert_eq!(progress["current"], 0);
            }
        }
    }
    assert_eq!(fs::read_dir(f.root.join("out/clips")).unwrap().count(), 0);
    assert!(!f.root.join("out/profile.json").exists());
    assert!(f.root.join("a.mp4").exists());
    assert!(f.root.join("b.mp4").exists());
}

#[test]
fn production_profile_separates_wall_clock_media_and_historical_cache_telemetry() {
    let f = Fixture::new(&["--merge", "--profile"]);
    let out = f.command().env("SLOW_VALIDATION", "1").output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let summary = f.summary(&out);
    let p: Value =
        serde_json::from_slice(&fs::read(f.root.join("out/profile.json")).unwrap()).unwrap();
    assert_eq!(p["schema_version"], "2");
    let stages = p["stages"].as_object().unwrap();
    assert_eq!(stages.len(), 9);
    for name in [
        "setup",
        "scan",
        "dedupe",
        "planning",
        "trim",
        "merge",
        "archive",
        "reporting",
        "total",
    ] {
        assert!(stages[name].as_f64().unwrap() >= 0.0);
    }
    assert!(p["stages"]["merge"].as_f64().unwrap() < 60.0);
    assert!(p["stages"]["merge"].as_f64().unwrap() >= 0.08);
    assert!(p["stages"]["trim"].as_f64().unwrap() >= 0.24);
    assert!(
        p["stages"]["trim"].as_f64().unwrap()
            - p["aggregates"]["trim_encode_seconds_sum"].as_f64().unwrap()
            >= 0.23
    );
    let sum: f64 = stages
        .iter()
        .filter(|(k, _)| k.as_str() != "total")
        .map(|(_, v)| v.as_f64().unwrap())
        .sum();
    assert!((sum - stages["total"].as_f64().unwrap()).abs() <= 0.005);
    assert_eq!(summary["merge_duration_sec"], 60.0);
    assert_eq!(summary["profile_total_sec"], p["stages"]["total"]);
    assert_eq!(p["runtime"]["ocr_worker_capacity"], 1);
    assert_eq!(p["runtime"]["ocr_workers_active"], 0);
    assert_eq!(p["runtime"]["trim_workers"], 1);
    assert_eq!(p["runtime"]["validation"], "strict");
    assert_eq!(p["runtime"]["trim_validation"], "ffprobe_duration_strict");
    let tasks = p["trim_tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 3);
    for task in tasks {
        assert!(task["output_probe_seconds"].as_f64().unwrap() >= 0.08);
        let parts = [
            "encode_seconds",
            "output_probe_seconds",
            "publication_seconds",
        ]
        .iter()
        .map(|k| task[k].as_f64().unwrap())
        .sum::<f64>();
        assert!(task["total_seconds"].as_f64().unwrap() + 0.002 >= parts);
    }
    assert!(
        p["aggregates"]["trim_output_probe_seconds_sum"]
            .as_f64()
            .unwrap()
            >= 0.24
    );
    assert_eq!(p["runtime"]["trim_encoder"]["capability"], "available");
    assert_eq!(
        fs::read_to_string(f.root.join("capability.log")).unwrap(),
        "probe\n"
    );
    let csv_encode: f64 = rows(&f)
        .iter()
        .map(|r| r["TrimSeconds"].parse::<f64>().unwrap())
        .sum();
    assert!(
        (csv_encode - p["aggregates"]["trim_encode_seconds_sum"].as_f64().unwrap()).abs() <= 0.002
    );
    assert_eq!(
        p["runtime"]["source_scan_configs"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        p["runtime"]["source_scan_configs"][0]["config"]["coarse_step"],
        4.0
    );
    assert_eq!(p["runtime"]["source_scan_configs"][0]["source_index"], 1);
    assert_eq!(p["runtime"]["source_codec_counts"]["h264"], 2);
    assert_eq!(p["runtime"]["ffmpeg_version"], "fixture-1");
    assert_eq!(p["ocr_providers_current"], json!([]));
    assert_eq!(
        p["ocr_providers_cached_historical"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        p["ocr_providers_cached_historical"][0]["provider_actual"],
        "cpu"
    );
    assert_eq!(
        p["ocr_providers_cached_historical"][0]["fallback_reason"],
        "directml-unavailable"
    );
    assert_eq!(p["telemetry"]["current"]["ocr_requests"], 0);
    assert_eq!(p["telemetry"]["cached_historical"]["ocr_requests"], 14);
    assert_eq!(
        p["telemetry"]["cached_historical"]["ocr_predict_seconds_sum"],
        4.0
    );
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text
        .lines()
        .find_map(|l| l.strip_prefix("PROFILE {"))
        .unwrap();
    let stdout: Value = serde_json::from_str(&format!("{{{line}")).unwrap();
    assert_eq!(stdout["stages"], p["stages"]);
    assert_eq!(stdout["run_id"], p["run_id"]);
    assert!(!p["runtime"]
        .to_string()
        .contains(&f.root.display().to_string()));
}

#[test]
fn failed_codec_diagnostics_do_not_change_source_classification_or_leak_version_paths() {
    let f = Fixture::new(&["--scan-only"]);
    let out = f.command().env("BAD_DIAGNOSTICS", "1").output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(f.summary(&out)["unreadable_count"], 0);
    let p: Value =
        serde_json::from_slice(&fs::read(f.root.join("out/profile.json")).unwrap()).unwrap();
    assert_eq!(p["runtime"]["source_codec_counts"]["unknown"], 2);
    assert!(p["runtime"]["ffmpeg_version"].is_null());
    assert!(!p.to_string().contains("C:/private"));
}

#[test]
fn cache_hit_empty_detection_duration_probe_is_current_measurement_not_historical() {
    let f = Fixture::with_detections(
        &["--scan-only"],
        [vec![], vec![detection(10., "own-kill", "b")]],
    );
    let out = f.command().env("SLOW_VALIDATION", "1").output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let p: Value =
        serde_json::from_slice(&fs::read(f.root.join("out/profile.json")).unwrap()).unwrap();
    assert!(
        p["telemetry"]["current"]["probe_seconds_sum"]
            .as_f64()
            .unwrap()
            >= 0.08
    );
    assert_eq!(
        p["telemetry"]["cached_historical"]["probe_seconds_sum"],
        0.0
    );
    assert_eq!(
        p["telemetry"]["cached_historical"]["sources_without_detection_telemetry"],
        1
    );
}

#[test]
fn telemetry_counts_distinct_same_named_sources_before_filtering_and_grouping() {
    let mut early = detection(0.5, "own-kill", "early");
    early["ocr_requests"] = json!(19);
    let f = Fixture::with_paths(
        &["--scan-only"],
        [
            vec![early, detection(10., "own-kill", "a")],
            vec![detection(10., "own-kill", "b")],
        ],
        true,
    );
    let out = f.command().output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let p: Value =
        serde_json::from_slice(&fs::read(f.root.join("out/profile.json")).unwrap()).unwrap();
    assert_eq!(p["telemetry"]["cached_historical"]["sources"], 2);
    assert_eq!(p["telemetry"]["cached_historical"]["ocr_requests"], 26);
    assert_eq!(
        p["telemetry"]["cached_historical"]["ocr_predict_seconds_sum"],
        4.0
    );
    assert!(rows(&f)
        .iter()
        .any(|r| r["Method"] == "skipped-before-min-event-sec"));
}

#[test]
fn all_output_paths_are_fixed_before_first_encode_and_collision_never_archives_sources() {
    let f = Fixture::new(&["--move-originals", "--merge"]);
    let future = f.root.join("out/clips/003_own-kill_b.mp4");
    let out = f
        .command()
        .env("COLLIDE_ON_ENCODE", &future)
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(1),
        "a late collision must fail rather than renumber: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(!String::from_utf8_lossy(&out.stdout).contains("SUMMARY "));
    assert_eq!(fs::read(&future).unwrap(), b"external-owner");
    assert_eq!(fs::read(f.root.join("a.mp4")).unwrap(), b"original-a");
    assert_eq!(fs::read(f.root.join("b.mp4")).unwrap(), b"original-b");
    assert!(!f.root.join("out/originals").exists());
    assert!(!f.root.join("out/clips/003_own-kill_b_1.mp4").exists());
}

fn rows(f: &Fixture) -> Vec<std::collections::BTreeMap<String, String>> {
    let bytes = fs::read(f.root.join("out/检测与裁剪记录.csv")).unwrap();
    let mut reader = csv::Reader::from_reader(&bytes[3..]);
    let headers = reader.headers().unwrap().clone();
    reader
        .records()
        .map(|r| {
            headers
                .iter()
                .zip(r.unwrap().iter())
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                .collect()
        })
        .collect()
}

#[test]
fn output_mutation_during_source_diagnostics_prevents_every_encode() {
    let f = Fixture::with_detections(
        &["--move-originals"],
        [
            vec![detection(10., "own-kill", "shared")],
            vec![
                detection(10., "own-kill", "shared"),
                detection(30., "own-kill", "unique"),
            ],
        ],
    );
    // Exhaust later clip names during source codec diagnostics, after output
    // directory selection but before planning. Dedupe no longer decodes frames.
    let out = f
        .command()
        .env("BLOCK_FUTURE", f.root.join("out/clips/002_own-kill_b.mp4"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("Output paths changed after preflight"));
    assert!(!f.root.join("encode.log").exists());
    assert!(f.root.join("a.mp4").exists());
    assert!(f.root.join("b.mp4").exists());
}

#[test]
fn serial_multievent_outputs_reports_and_merge_keep_planned_order() {
    let f = Fixture::new(&["--merge"]);
    let out = f.command().env("CHECK_SERIAL", "1").output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let summary = f.summary(&out);
    assert_eq!(summary["included_count"], 3);
    assert_eq!(summary["cache_hits"], 2);
    let rows = rows(&f);
    let expected = [
        "001_own-kill_a.mp4",
        "002_self-death_a.mp4",
        "003_own-kill_b.mp4",
    ];
    for (row, name) in rows.iter().zip(expected) {
        assert_eq!(PathBuf::from(&row["Output"]).file_name().unwrap(), name);
        assert_eq!(row["Encoder"], "h264_nvenc");
    }
    assert_eq!(
        rows.iter()
            .map(|r| r["KeepStartSec"].as_str())
            .collect::<Vec<_>>(),
        ["9.000", "29.000", "19.000"]
    );
    let concat = fs::read_to_string(summary["concat_list"].as_str().unwrap()).unwrap();
    assert_eq!(concat.lines().count(), 3);
    for (line, name) in concat.lines().zip(expected) {
        assert!(line.ends_with(&format!("{name}'")));
    }
    let log = fs::read_to_string(f.root.join("encode.log")).unwrap();
    assert_eq!(log.lines().count(), 3);
    for (line, start) in log.lines().zip(["9.000", "29.000", "19.000"]) {
        assert!(line.contains(&format!("-ss| {start}|")));
    }
}

#[test]
fn split_targets_cross_source_duplicates_and_early_skips_preserve_records_and_merge_order() {
    split_targets_contract(false);
}

#[test]
fn parallel_split_targets_duplicates_and_early_skips_keep_planned_order() {
    split_targets_contract(true);
}

fn split_targets_contract(parallel: bool) {
    let f = Fixture::with_detections(
        &["--merge", "--split-merge-by-target"],
        [
            vec![
                detection(10., "own-kill", "shared"),
                detection(30., "self-death", "death"),
                detection(0.5, "own-kill", "early"),
            ],
            vec![
                detection(10., "own-kill", "shared"),
                detection(20., "own-kill", "unique"),
            ],
        ],
    );
    let mut command = f.command();
    command.env("FORBID_FRAME_DECODE", "1");
    if parallel {
        command
            .args(["--trim-jobs", "2"])
            .env("CONTROLLED_TRIM", "1")
            .env("FIRST_START", "29.000")
            .env("SECOND_START", "9.000");
    }
    let out = command.output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let summary = f.summary(&out);
    assert_eq!(summary["included_count"], 3);
    assert_eq!(summary["skipped_count"], 2);
    let profile: Value =
        serde_json::from_slice(&fs::read(f.root.join("out/profile.json")).unwrap()).unwrap();
    assert_eq!(profile["dedupe_work"]["candidates"], 4);
    assert_eq!(profile["dedupe_work"]["pair_comparisons"], 1);
    assert_eq!(profile["dedupe_work"]["verification_windows"], 0);
    assert_eq!(profile["dedupe_work"]["verification_failures"], 0);
    assert_eq!(profile["dedupe_work"]["duplicates"], 1);
    let r = rows(&f);
    assert_eq!(
        r.iter().map(|r| r["Method"].as_str()).collect::<Vec<_>>(),
        [
            "skipped-before-min-event-sec",
            "ocr",
            "ocr",
            "skipped-cross-source-duplicate",
            "ocr"
        ]
    );
    let included = r
        .iter()
        .filter(|r| r["Status"] == "included")
        .collect::<Vec<_>>();
    for (row, name) in included.iter().zip([
        "001_self-death_a.mp4",
        "002_own-kill_a.mp4",
        "003_own-kill_b.mp4",
    ]) {
        assert_eq!(PathBuf::from(&row["Output"]).file_name().unwrap(), name);
    }
    let merges = summary["merge_outputs"].as_array().unwrap();
    assert_eq!(
        merges
            .iter()
            .map(|m| m["target"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["self-death", "own-kill"]
    );
    let own = fs::read_to_string(merges[1]["concat_list"].as_str().unwrap()).unwrap();
    for (line, name) in own
        .lines()
        .zip(["002_own-kill_a.mp4", "003_own-kill_b.mp4"])
    {
        assert!(line.ends_with(&format!("{name}'")));
    }
}

#[test]
fn dry_run_keeps_planned_windows_but_never_encodes_merges_or_archives() {
    let f = Fixture::new(&["--scan-only", "--merge", "--move-originals"]);
    let out = f.command().output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let summary = f.summary(&out);
    assert_eq!(summary["included_count"], 3);
    assert_eq!(summary["merge_outputs"], json!([]));
    assert_eq!(summary["originals_moved"], 0);
    assert!(!f.root.join("encode.log").exists());
    assert!(!f.root.join("out/clips").exists());
    assert!(!f.root.join("capability.log").exists());
    let profile: Value =
        serde_json::from_slice(&fs::read(f.root.join("out/profile.json")).unwrap()).unwrap();
    assert_eq!(
        profile["runtime"]["trim_encoder"]["capability"],
        "not_probed"
    );
    assert_eq!(profile["trim_tasks"], json!([]));
    let r = rows(&f);
    assert_eq!(
        r.iter()
            .map(|r| r["KeepEndSec"].as_str())
            .collect::<Vec<_>>(),
        ["11.000", "31.000", "21.000"]
    );
    assert!(r
        .iter()
        .all(|r| r["Output"].is_empty() && r["Encoder"].is_empty()));
    assert!(f.root.join("a.mp4").exists());
    assert!(f.root.join("b.mp4").exists());
}

#[test]
fn trim_worker_modes_are_independent_of_ocr_jobs_and_report_effective_capacity() {
    for (trim, jobs) in [("auto", "2"), ("1", "2"), ("2", "1")] {
        let mut f = Fixture::new(&[]);
        let jobs_index = f.args.iter().position(|arg| arg == "--jobs").unwrap();
        f.args[jobs_index + 1] = jobs.into();
        let mut command = f.command();
        command.args(["--trim-jobs", trim]);
        if trim != "2" {
            command.env("CHECK_SERIAL", "1");
        }
        let out = command.output().unwrap();
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let p: Value =
            serde_json::from_slice(&fs::read(f.root.join("out/profile.json")).unwrap()).unwrap();
        assert_eq!(
            p["runtime"]["trim_workers"],
            if trim == "2" { 2 } else { 1 }
        );
        assert_eq!(
            p["runtime"]["ocr_worker_capacity"],
            jobs.parse::<usize>().unwrap()
        );
    }
    for invalid in ["0", "3", "-1", "many"] {
        let f = Fixture::new(&[]);
        let out = f.command().args(["--trim-jobs", invalid]).output().unwrap();
        assert_eq!(out.status.code(), Some(2));
        assert!(!f.root.join("encode.log").exists());
    }
}

#[test]
fn trim_worker_profile_reports_effective_count_for_zero_or_one_planned_task() {
    for count in [0, 1] {
        let detections = if count == 0 {
            vec![]
        } else {
            vec![detection(10., "own-kill", "only")]
        };
        let f = Fixture::with_detections(&[], [detections, vec![]]);
        let out = f.command().args(["--trim-jobs", "2"]).output().unwrap();
        assert_eq!(out.status.code(), Some(if count == 0 { 2 } else { 0 }));
        let p: Value =
            serde_json::from_slice(&fs::read(f.root.join("out/profile.json")).unwrap()).unwrap();
        assert_eq!(p["runtime"]["trim_workers"], count);
        assert_eq!(p["trim_tasks"].as_array().unwrap().len(), count as usize);
    }
}

fn assert_failed_parallel_inputs_are_safe(f: &Fixture) {
    assert!(
        !f.root.join("merge.log").exists(),
        "Merge must never start after failed Trim"
    );
    assert_eq!(
        fs::read_dir(f.root.join("out/clips")).unwrap().count(),
        0,
        "all unpublished staging and later completed outputs must be removed"
    );
    assert!(!f.root.join("out/originals").exists());
    assert!(!f.root.join("out/profile.json").exists());
    assert_eq!(fs::read(f.root.join("a.mp4")).unwrap(), b"original-a");
    assert_eq!(fs::read(f.root.join("b.mp4")).unwrap(), b"original-b");
}

#[test]
fn parallel_worker_failure_discards_completed_unpublished_outputs_and_never_archives() {
    let f = Fixture::new(&["--merge", "--move-originals"]);
    let out = f
        .command()
        .args(["--trim-jobs", "2"])
        .env("CONTROLLED_TRIM", "1")
        .env("FAIL_FIRST", "1")
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        f.root.join("finished-29.000").exists(),
        "later task must finish before failure"
    );
    assert!(!String::from_utf8_lossy(&out.stdout).contains("SUMMARY "));
    assert_failed_parallel_inputs_are_safe(&f);
}

#[test]
fn parallel_worker_failure_promptly_stops_a_blocked_peer_and_cleans_staging() {
    use std::{
        process::Stdio,
        time::{Duration, Instant},
    };
    for failing in ["9.000", "29.000"] {
        let f = Fixture::new(&["--merge", "--move-originals"]);
        // File-backed output lets the watchdog wait without a full pipe blocking the CLI.
        let stdout = fs::File::create(f.root.join("stdout.log")).unwrap();
        let stderr = fs::File::create(f.root.join("stderr.log")).unwrap();
        let started = Instant::now();
        let mut child = f
            .command()
            .args(["--trim-jobs", "2"])
            .env("CONTROLLED_TRIM", "1")
            .env("FAIL_WITH_BLOCKED_PEER", "1")
            .env("FAILING_START", failing)
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr))
            .spawn()
            .unwrap();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if started.elapsed() >= Duration::from_secs(10) {
                // A failing implementation must not leave the 120-second native
                // simulator behind: release only after the acceptance deadline.
                fs::write(f.root.join("watchdog-release"), b"").unwrap();
                break child.wait().unwrap();
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let elapsed = started.elapsed();
        assert!(f.root.join("started-9.000").exists());
        assert!(f.root.join("started-29.000").exists());
        let encodes = fs::read_to_string(f.root.join("encode.log")).unwrap();
        let first_attempts = encodes
            .lines()
            .filter(|line| line.contains(&format!("-ss| {failing}|")))
            .collect::<Vec<_>>();
        assert_eq!(
            first_attempts.len(),
            2,
            "both NVENC and software attempts must fail: {encodes}"
        );
        assert!(first_attempts
            .iter()
            .any(|line| line.contains("h264_nvenc")));
        assert!(first_attempts.iter().any(|line| line.contains("libx264")));
        assert_eq!(status.code(), Some(1));
        assert!(!fs::read_to_string(f.root.join("stdout.log"))
            .unwrap()
            .contains("SUMMARY "));
        assert_failed_parallel_inputs_are_safe(&f);
        assert!(
            elapsed < Duration::from_secs(10),
            "worker {failing} failure waited for blocked peer: {elapsed:?}"
        );
    }
}

#[test]
fn parallel_cancellation_discards_completed_unpublished_outputs_and_never_archives() {
    // Cancellation is process-global and intentionally cannot be reset. Use a fresh
    // test process, but enter through the same production application boundary.
    const CHILD: &str = "TRIM_CANCEL_CONTRACT_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let out = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "parallel_cancellation_discards_completed_unpublished_outputs_and_never_archives",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(!String::from_utf8_lossy(&out.stdout).contains("SUMMARY "));
        return;
    }
    let f = Fixture::new(&["--merge", "--move-originals"]);
    std::env::set_var("PUBG_OCR_MODEL_DIR", &f.models);
    std::env::set_var("PUBG_OCR_THREADS", "1");
    std::env::set_var("TRIM_FIXTURE", &f.root);
    std::env::set_var("CONTROLLED_TRIM", "1");
    std::env::set_var("CANCEL_TRIM", "1");
    let config = Config::try_parse_from(
        std::iter::once("app")
            .chain(f.args.iter().map(String::as_str))
            .chain(["--trim-jobs", "2"]),
    )
    .unwrap();
    let root = f.root.clone();
    let cancel = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while !root.join("finished-29.000").exists() {
            if std::time::Instant::now() >= deadline {
                pubg_ocr_engine::cancellation::request();
                panic!("later worker never completed before cancellation");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        // Let the executor collect the later result while its predecessor is held.
        std::thread::sleep(std::time::Duration::from_millis(250));
        pubg_ocr_engine::cancellation::request();
    });
    let result = pubg_ocr_engine::application::run(config);
    cancel.join().unwrap();
    assert!(result.unwrap_err().to_lowercase().contains("cancel"));
    assert_failed_parallel_inputs_are_safe(&f);
}

#[test]
fn encode_failure_cleans_staging_and_never_merges_or_archives() {
    let f = Fixture::new(&["--merge", "--move-originals"]);
    let out = f.command().env("FAIL_ENCODE", "1").output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(!String::from_utf8_lossy(&out.stdout).contains("SUMMARY "));
    assert_eq!(fs::read_dir(f.root.join("out/clips")).unwrap().count(), 0);
    assert!(!f.root.join("out/originals").exists());
    assert_eq!(fs::read(f.root.join("a.mp4")).unwrap(), b"original-a");
    assert_eq!(fs::read(f.root.join("b.mp4")).unwrap(), b"original-b");
}
