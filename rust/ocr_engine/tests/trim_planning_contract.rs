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
                if a.iter().any(|v|v=="format=duration") { println!("60"); return; }
                if a.iter().any(|v|v=="stream=width,height") { println!("16,16"); return; }
                if a.iter().any(|v|v=="rawvideo") {
                    if let Some(path) = std::env::var_os("BLOCK_FUTURE") {
                        let p = PathBuf::from(path);
                        fs::create_dir_all(p.parent().unwrap()).unwrap();
                        fs::write(&p, b"block").unwrap();
                        for n in 1..1000 { fs::write(p.with_file_name(format!("{}_{}.mp4",p.file_stem().unwrap().to_string_lossy(),n)), b"block").unwrap(); }
                    }
                    let i = a.iter().position(|v|v=="-ss").unwrap();
                    let t: f64 = a[i+1].parse().unwrap();
                    std::io::stdout().write_all(&vec![((t*100.).round() as u64 % 256) as u8;16*16*3]).unwrap(); return;
                }
                let out = a.last().unwrap();
                if a.iter().any(|v|v=="concat") {
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
                    writeln!(log, "{}", a.join("| ")).unwrap();
                    if let Some(path) = std::env::var_os("COLLIDE_ON_ENCODE") {
                        if !root.join("collided").exists() { fs::write(path, b"external-owner").unwrap(); fs::write(root.join("collided"), b"").unwrap(); }
                    }
                    if std::env::var_os("FAIL_ENCODE").is_some() { std::process::exit(7); }
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
        let b = root.join("b.mp4");
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
                &json!({"detections":detections[index],"provider":null}),
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
    json!({"duration_sec":60.,"event_sec":sec,"target":target,"event_key":key,"method":"ocr","detector":"ocr"})
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
fn late_source_planning_failure_prevents_every_encode() {
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
    // During global verification the external filesystem becomes full of names
    // for the later retained clip. Planning must fail before encoding source a.
    let out = f
        .command()
        .env("BLOCK_FUTURE", f.root.join("out/clips/002_own-kill_b.mp4"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("Could not create unique path"));
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
    let out = f.command().output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let summary = f.summary(&out);
    assert_eq!(summary["included_count"], 3);
    assert_eq!(summary["skipped_count"], 2);
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
