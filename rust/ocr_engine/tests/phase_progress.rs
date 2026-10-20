//! Phase PROGRESS protocol: dedupe callback, trim and merge emission.
//! Production-feature tests use cached scans and synthetic FFmpeg video to
//! avoid real OCR execution. Dedupe callback is testable without onnx.

use pubg_ocr_engine::{dedupe, planning::Detection};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

fn detection_with_key(event_sec: f64, key: &str) -> Detection {
    serde_json::from_value(serde_json::json!({
        "duration_sec": event_sec + 10.0,
        "event_sec": event_sec,
        "method": "ocr",
        "detector": "ocr",
        "target": "self-death",
        "event_key": key,
        "sampled_count": 1,
        "ocr_requests": 1,
        "termination_reason": "completed",
    }))
    .unwrap()
}

/// Dedupe progress callback is called once per candidate with monotonically
/// increasing `current`, and `total` matches the candidate count upfront.
#[test]
fn dedupe_progress_callback_called_once_per_candidate() {
    let sources: Vec<(PathBuf, Vec<Detection>)> = vec![
        (
            PathBuf::from("a.mp4"),
            vec![
                detection_with_key(5.0, "self-death:knock:enemy"),
                detection_with_key(15.0, "self-death:knock:other"),
            ],
        ),
        (
            PathBuf::from("b.mp4"),
            vec![detection_with_key(5.1, "self-death:knock:enemy")],
        ),
    ];

    let calls: Arc<Mutex<Vec<(usize, usize)>>> = Arc::new(Mutex::new(Vec::new()));
    let calls_clone = calls.clone();
    let progress = move |current: usize, total: usize| {
        calls_clone.lock().unwrap().push((current, total));
    };

    let (_skipped, work) = dedupe::cross_source_duplicates_profiled(&sources, Some(&progress));

    let calls = calls.lock().unwrap();
    // 3 total candidates (2 from a.mp4, 1 from b.mp4)
    assert_eq!(work.candidates, 3, "work.candidates");
    assert_eq!(calls.len(), 3, "progress called once per candidate");
    // All calls report the same total = 3
    assert!(
        calls.iter().all(|&(_, total)| total == 3),
        "total must be 3"
    );
    // current values are 1, 2, 3 (one-based increments)
    let currents: Vec<usize> = calls.iter().map(|&(c, _)| c).collect();
    assert_eq!(currents, vec![1, 2, 3], "currents must be 1..=3");
}

/// Dedupe with no candidates still calls progress zero times without panicking.
#[test]
fn dedupe_progress_with_no_candidates_is_stable() {
    let sources: Vec<(PathBuf, Vec<Detection>)> = vec![
        (PathBuf::from("a.mp4"), vec![]),
        (PathBuf::from("b.mp4"), vec![]),
    ];

    let call_count = Arc::new(Mutex::new(0usize));
    let count_clone = call_count.clone();
    let progress = move |_: usize, _: usize| {
        *count_clone.lock().unwrap() += 1;
    };

    let (skipped, work) = dedupe::cross_source_duplicates_profiled(&sources, Some(&progress));

    assert_eq!(work.candidates, 0);
    assert!(skipped.is_empty());
    assert_eq!(
        *call_count.lock().unwrap(),
        0,
        "no calls with zero candidates"
    );
}

/// Passing None for the progress callback is accepted and behaves identically
/// to passing Some empty closure (backward compatibility).
#[test]
fn dedupe_progress_callback_none_is_accepted() {
    let sources: Vec<(PathBuf, Vec<Detection>)> = vec![(
        PathBuf::from("a.mp4"),
        vec![detection_with_key(5.0, "self-death:knock:enemy")],
    )];

    let (skipped, work) = dedupe::cross_source_duplicates_profiled(&sources, None);

    assert_eq!(work.candidates, 1);
    assert!(skipped.is_empty());
}

#[cfg(feature = "onnx")]
mod cli_progress {
    use clap::Parser;
    use pubg_ocr_engine::{
        config::Config,
        persistent_cache,
        scan_adapter::{source_config, EngineSettings},
        source_files,
    };
    use std::{
        fs,
        io::Write,
        path::{Path, PathBuf},
        process::{Command, Stdio},
    };

    struct Temp(PathBuf);
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn ffmpeg() -> PathBuf {
        PathBuf::from(
            std::env::var_os("PUBG_HIGHLIGHT_TRIM_FFMPEG_DIR")
                .expect("set PUBG_HIGHLIGHT_TRIM_FFMPEG_DIR"),
        )
        .join("ffmpeg.exe")
    }

    /// Create a short synthetic lossless RGB video (unique per seed).
    fn make_video(path: &Path, frames: usize, seed_offset: u64) {
        let mut child = Command::new(ffmpeg())
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "bgr24",
                "-s",
                "64x48",
                "-r",
                "10",
                "-i",
                "pipe:0",
                "-c:v",
                "libx264rgb",
                "-crf",
                "0",
                "-preset",
                "ultrafast",
            ])
            .arg(path)
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        for i in 0..frames {
            let mut seed = (i as u64 ^ 0xdeadbeef).wrapping_add(seed_offset);
            let mut pixels = Vec::with_capacity(64 * 48 * 3);
            for _ in 0..64 * 48 * 3 {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                pixels.push(seed as u8);
            }
            stdin.write_all(&pixels).unwrap();
        }
        drop(stdin);
        assert!(child.wait().unwrap().success());
    }

    /// Populate persistent cache for each source using the exact same Config as the CLI call.
    fn populate_cache(args: &[&str], detections_per_source: &[serde_json::Value]) {
        let c =
            Config::parse_from(std::iter::once("pubg_highlight_trim").chain(args.iter().copied()));
        let inputs = source_files::select(&c).unwrap();
        let cache_dir = c.cache_dir.as_ref().expect("--cache-dir required");
        let mut settings = EngineSettings::discover().unwrap();
        settings.identity.push_str(":effective-threads=1");
        for (index, source) in inputs.sources.iter().enumerate() {
            let config = source_config(&c, source, None);
            let key = persistent_cache::cache_key(
                &source.path,
                &serde_json::json!({"config": config, "candidate_times": []}),
                &settings.identity,
            )
            .unwrap();
            persistent_cache::write(
                cache_dir,
                &key,
                &serde_json::json!({"detections": detections_per_source[index], "provider": null}),
            )
            .unwrap();
        }
    }

    fn event_self_death(sec: f64, duration: f64) -> serde_json::Value {
        serde_json::json!({
            "duration_sec": duration,
            "event_sec": sec,
            "method": "ocr",
            "detector": "ocr",
            "target": "self-death",
            "event_key": "self-death:knock:enemy",
            "sampled_count": 7,
            "ocr_requests": 3,
            "termination_reason": "completed",
        })
    }

    fn event_own_kill(sec: f64, duration: f64) -> serde_json::Value {
        serde_json::json!({
            "duration_sec": duration,
            "event_sec": sec,
            "method": "ocr",
            "detector": "ocr",
            "target": "own-kill",
            "event_key": "own-kill:eliminate:enemy",
            "sampled_count": 7,
            "ocr_requests": 3,
            "termination_reason": "completed",
        })
    }

    fn progress_phases(stdout: &str) -> Vec<String> {
        stdout
            .lines()
            .filter(|l| l.starts_with("PROGRESS "))
            .filter_map(|l| {
                let v: serde_json::Value = serde_json::from_str(&l[9..]).ok()?;
                v["phase"].as_str().map(str::to_owned)
            })
            .collect()
    }

    fn has_phase_event(stdout: &str, phase: &str, event: &str) -> bool {
        stdout
            .lines()
            .filter(|l| l.starts_with("PROGRESS "))
            .any(|l| {
                let Ok(v): Result<serde_json::Value, _> = serde_json::from_str(&l[9..]) else {
                    return false;
                };
                v["phase"].as_str() == Some(phase) && v["event"].as_str() == Some(event)
            })
    }

    fn has_phase_current(stdout: &str, phase: &str) -> bool {
        stdout
            .lines()
            .filter(|l| l.starts_with("PROGRESS "))
            .any(|l| {
                let Ok(v): Result<serde_json::Value, _> = serde_json::from_str(&l[9..]) else {
                    return false;
                };
                v["phase"].as_str() == Some(phase) && v.get("current").is_some_and(|c| !c.is_null())
            })
    }

    /// Cached sources exercise the real application and native media failure
    /// boundary, without interpreting a failed encode as published progress.
    #[test]
    #[ignore = "requires PUBG_OCR_MODEL_DIR and PUBG_HIGHLIGHT_TRIM_FFMPEG_DIR; synthetic media only"]
    fn failed_trim_or_merge_never_emits_done_or_summary() {
        let root =
            Temp(std::env::temp_dir().join(format!("pubg-phase-fail-{}", std::process::id())));
        fs::create_dir_all(&root.0).unwrap();
        let a = root.0.join("a.mp4");
        make_video(&a, 100, 0);
        let wrapper_source = root.0.join("wrapper.rs");
        let wrapper = root.0.join("ffmpeg-wrapper.exe");
        fs::write(
            &wrapper_source,
            r#"
            fn main() {
                use std::io::Write;
                let args: Vec<_> = std::env::args_os().skip(1).collect();
                let concat = args.iter().any(|a| a == "concat");
                let trim = args.iter().any(|a| a == "-ss");
                let mode = std::env::var("PHASE_FAIL").unwrap();
                if (mode == "trim" && trim) || (mode == "merge" && concat) {
                    eprintln!("NATIVE_PHASE_FAILURE"); std::process::exit(7);
                }
                let out = std::process::Command::new(std::env::var_os("REAL_FFMPEG").unwrap())
                    .args(args).output().unwrap();
                std::io::stdout().write_all(&out.stdout).unwrap();
                std::io::stderr().write_all(&out.stderr).unwrap();
                std::process::exit(out.status.code().unwrap_or(1));
            }
        "#,
        )
        .unwrap();
        assert!(Command::new("rustc")
            .arg(&wrapper_source)
            .arg("-o")
            .arg(&wrapper)
            .status()
            .unwrap()
            .success());
        for phase in ["trim", "merge"] {
            let cache = root.0.join(format!("cache-{phase}"));
            fs::create_dir_all(&cache).unwrap();
            let out = root.0.join(format!("out-{phase}"));
            let (a_str, cache_str, out_str, wrapper_str) = (
                a.display().to_string(),
                cache.display().to_string(),
                out.display().to_string(),
                wrapper.display().to_string(),
            );
            let args = [
                &a_str as &str,
                "--game-lang",
                "en",
                "--jobs",
                "1",
                "--no-auto-candidate-csv",
                "--merge",
                "--cache-dir",
                &cache_str,
                "-o",
                &out_str,
                "--ffmpeg",
                &wrapper_str,
            ];
            populate_cache(&args, &[serde_json::json!([event_self_death(5.0, 10.0)])]);
            let result = Command::new(env!("CARGO_BIN_EXE_pubg_highlight_trim"))
                .args(args)
                .env("PHASE_FAIL", phase)
                .env("REAL_FFMPEG", ffmpeg())
                .output()
                .unwrap();
            let stdout = String::from_utf8_lossy(&result.stdout);
            assert_eq!(
                result.status.code(),
                Some(1),
                "{phase}: {stdout}\n{}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert!(has_phase_event(&stdout, phase, "start"), "{stdout}");
            assert!(!has_phase_event(&stdout, phase, "done"), "{stdout}");
            assert!(!stdout.contains("SUMMARY "), "{stdout}");
            let rows: Vec<serde_json::Value> = stdout
                .lines()
                .filter_map(|l| l.strip_prefix("PROGRESS "))
                .map(|l| serde_json::from_str(l).unwrap())
                .filter(|v: &serde_json::Value| v["phase"] == phase)
                .collect();
            assert!(
                rows.iter().all(|v| v["current"] == 0),
                "failed output is not completed: {rows:?}"
            );
            if phase == "trim" {
                assert!(
                    !progress_phases(&stdout).contains(&"merge".to_owned()),
                    "{stdout}"
                );
            } else {
                assert!(has_phase_event(&stdout, "trim", "done"), "{stdout}");
            }
        }
    }

    #[test]
    #[ignore = "requires PUBG_OCR_MODEL_DIR and PUBG_HIGHLIGHT_TRIM_FFMPEG_DIR; synthetic media only"]
    fn no_work_phases_finish_and_merge_skip_reason_is_explicit() {
        let root =
            Temp(std::env::temp_dir().join(format!("pubg-phase-zero-{}", std::process::id())));
        fs::create_dir_all(&root.0).unwrap();
        let a = root.0.join("a.mp4");
        make_video(&a, 40, 0);
        for merge in ["--merge", "--no-merge"] {
            let cache = root.0.join(format!("cache{merge}"));
            fs::create_dir_all(&cache).unwrap();
            let out = root.0.join(format!("out{merge}"));
            let (a_str, cache_str, out_str) = (
                a.display().to_string(),
                cache.display().to_string(),
                out.display().to_string(),
            );
            let args = [
                &a_str as &str,
                "--game-lang",
                "en",
                "--jobs",
                "1",
                "--no-auto-candidate-csv",
                merge,
                "--cache-dir",
                &cache_str,
                "-o",
                &out_str,
            ];
            populate_cache(&args, &[serde_json::json!([])]);
            let result = Command::new(env!("CARGO_BIN_EXE_pubg_highlight_trim"))
                .args(args)
                .output()
                .unwrap();
            let stdout = String::from_utf8_lossy(&result.stdout);
            assert_eq!(
                result.status.code(),
                Some(2),
                "{stdout}\n{}",
                String::from_utf8_lossy(&result.stderr)
            );
            for phase in ["dedupe", "trim"] {
                assert!(has_phase_event(&stdout, phase, "done"), "{stdout}");
            }
            let rows: Vec<serde_json::Value> = stdout
                .lines()
                .filter_map(|l| l.strip_prefix("PROGRESS "))
                .map(|l| serde_json::from_str(l).unwrap())
                .filter(|v: &serde_json::Value| v["phase"] != "scan")
                .collect();
            assert!(
                rows.iter()
                    .all(|v| v["current"] == 0 && v["total"] == 0 && v["workers"] == 0),
                "{rows:?}"
            );
            assert!(
                rows.iter().any(|v| v["phase"] == "merge"
                    && v["event"] == "skipped"
                    && v["reason"]
                        == if merge == "--merge" {
                            "no-clips"
                        } else {
                            "disabled"
                        }),
                "{rows:?}"
            );
        }
    }

    /// Each child is a fresh process because cancellation intentionally cannot reset.
    #[test]
    #[ignore = "requires PUBG_OCR_MODEL_DIR and PUBG_HIGHLIGHT_TRIM_FFMPEG_DIR; synthetic media only"]
    fn cancellation_during_trim_or_merge_never_emits_done() {
        let Ok(phase) = std::env::var("PHASE_CANCEL_CHILD") else {
            for phase in ["trim", "merge"] {
                let result = Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "cli_progress::cancellation_during_trim_or_merge_never_emits_done",
                        "--ignored",
                        "--nocapture",
                    ])
                    .env("PHASE_CANCEL_CHILD", phase)
                    .output()
                    .unwrap();
                let stdout = String::from_utf8_lossy(&result.stdout);
                assert!(
                    result.status.success(),
                    "{stdout}\n{}",
                    String::from_utf8_lossy(&result.stderr)
                );
                assert!(has_phase_event(&stdout, phase, "start"), "{stdout}");
                assert!(!has_phase_event(&stdout, phase, "done"), "{stdout}");
                assert!(!stdout.contains("SUMMARY "), "{stdout}");
                if phase == "trim" {
                    assert!(
                        !progress_phases(&stdout).contains(&"merge".to_owned()),
                        "{stdout}"
                    );
                } else {
                    assert!(has_phase_event(&stdout, "trim", "done"), "{stdout}");
                }
            }
            return;
        };
        let root =
            Temp(std::env::temp_dir().join(format!("pubg-phase-cancel-{}", std::process::id())));
        fs::create_dir_all(&root.0).unwrap();
        let a = root.0.join("a.mp4");
        make_video(&a, 100, 0);
        let wrapper_source = root.0.join("wrapper.rs");
        let wrapper = root.0.join("cancel-wrapper.exe");
        fs::write(&wrapper_source, r#"
            fn main() {
                use std::io::Write;
                let args: Vec<_> = std::env::args_os().skip(1).collect();
                let phase = std::env::var("PHASE_CANCEL_CHILD").unwrap();
                if (phase == "trim" && args.iter().any(|a| a == "-ss")) ||
                   (phase == "merge" && args.iter().any(|a| a == "concat")) {
                    std::fs::write(std::env::var_os("PHASE_CANCEL_MARKER").unwrap(), b"ready").unwrap();
                    std::thread::sleep(std::time::Duration::from_secs(30));
                    std::process::exit(7);
                }
                let out = std::process::Command::new(std::env::var_os("REAL_FFMPEG").unwrap()).args(args).output().unwrap();
                std::io::stdout().write_all(&out.stdout).unwrap();
                std::io::stderr().write_all(&out.stderr).unwrap();
                std::process::exit(out.status.code().unwrap_or(1));
            }
        "#).unwrap();
        assert!(Command::new("rustc")
            .arg(&wrapper_source)
            .arg("-o")
            .arg(&wrapper)
            .status()
            .unwrap()
            .success());
        let marker = root.0.join("ready");
        std::env::set_var("PHASE_CANCEL_MARKER", &marker);
        std::env::set_var("REAL_FFMPEG", ffmpeg());
        let cache = root.0.join("cache");
        fs::create_dir_all(&cache).unwrap();
        let out = root.0.join("out");
        let (a_str, cache_str, out_str, wrapper_str) = (
            a.display().to_string(),
            cache.display().to_string(),
            out.display().to_string(),
            wrapper.display().to_string(),
        );
        let args = [
            &a_str as &str,
            "--game-lang",
            "en",
            "--jobs",
            "1",
            "--no-auto-candidate-csv",
            "--merge",
            "--cache-dir",
            &cache_str,
            "-o",
            &out_str,
            "--ffmpeg",
            &wrapper_str,
        ];
        populate_cache(&args, &[serde_json::json!([event_self_death(5.0, 10.0)])]);
        let config = Config::parse_from(std::iter::once("trim").chain(args));
        let requester = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
            while !marker.exists() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "native {phase} stage was not reached"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            pubg_ocr_engine::cancellation::request();
        });
        let result = pubg_ocr_engine::application::run(config);
        requester.join().unwrap();
        assert!(pubg_ocr_engine::cancellation::is_cancelled());
        assert!(result.is_err(), "cancelled application must fail");
    }

    /// Run the CLI with two cached sources that have distinct events (no dedup),
    /// merge enabled. Verify PROGRESS lines cover scan/dedupe/trim/merge phases.
    #[test]
    #[ignore = "requires PUBG_OCR_MODEL_DIR and PUBG_HIGHLIGHT_TRIM_FFMPEG_DIR; synthetic media only"]
    fn all_phases_emit_progress_lines() {
        let root =
            Temp(std::env::temp_dir().join(format!("pubg-phase-progress-{}", std::process::id())));
        fs::create_dir_all(&root.0).unwrap();
        let a = root.0.join("a.mp4");
        let b = root.0.join("b.mp4");
        // 40 frames @ 10fps = 4s; event at 1.5s gives enough before/after context
        make_video(&a, 40, 0);
        make_video(&b, 40, 0xdeadbeef);

        let cache = root.0.join("cache");
        let out = root.0.join("output");
        fs::create_dir_all(&cache).unwrap();
        fs::create_dir_all(&out).unwrap();

        // Build the exact CLI args used for both cache population and invocation.
        let cache_str = cache.display().to_string();
        let out_str = out.display().to_string();
        let a_str = a.display().to_string();
        let b_str = b.display().to_string();
        let args = [
            "--files",
            &a_str,
            &b_str,
            "--game-lang",
            "en",
            "--jobs",
            "1",
            "--no-auto-candidate-csv",
            "--merge",
            "--cache-dir",
            &cache_str,
            "-o",
            &out_str,
        ];
        populate_cache(
            &args,
            &[
                serde_json::json!([event_self_death(1.5, 4.0)]),
                serde_json::json!([event_self_death(2.0, 4.0)]),
            ],
        );

        let result = Command::new(env!("CARGO_BIN_EXE_pubg_highlight_trim"))
            .args(args)
            .output()
            .unwrap();

        let stdout = String::from_utf8_lossy(&result.stdout);
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert_eq!(
            result.status.code(),
            Some(0),
            "stdout:\n{stdout}\nstderr:\n{stderr}"
        );

        let phases = progress_phases(&stdout);
        assert!(
            phases.contains(&"scan".to_owned()),
            "scan phase: {phases:?}"
        );
        assert!(
            phases.contains(&"dedupe".to_owned()),
            "dedupe phase: {phases:?}\nstdout:\n{stdout}"
        );
        assert!(
            phases.contains(&"trim".to_owned()),
            "trim phase: {phases:?}\nstdout:\n{stdout}"
        );
        assert!(
            phases.contains(&"merge".to_owned()),
            "merge phase: {phases:?}\nstdout:\n{stdout}"
        );

        assert!(
            has_phase_event(&stdout, "dedupe", "start"),
            "dedupe start\n{stdout}"
        );
        assert!(
            has_phase_event(&stdout, "trim", "start"),
            "trim start\n{stdout}"
        );
        assert!(
            has_phase_event(&stdout, "merge", "start"),
            "merge start\n{stdout}"
        );
        for phase in ["dedupe", "trim", "merge"] {
            assert!(
                has_phase_event(&stdout, phase, "done"),
                "{phase} done\n{stdout}"
            );
        }

        // Real progress within dedupe: 2 candidates → should emit current=1 and current=2
        assert!(
            has_phase_current(&stdout, "dedupe"),
            "dedupe progress current\n{stdout}"
        );
        // Trim: at least one clip processed
        assert!(
            has_phase_current(&stdout, "trim"),
            "trim progress current\n{stdout}"
        );
    }

    /// Dry-run reports zero actual work while retaining the planned clip count.
    #[test]
    #[ignore = "requires PUBG_OCR_MODEL_DIR and PUBG_HIGHLIGHT_TRIM_FFMPEG_DIR; synthetic media only"]
    fn dry_run_trim_progress_is_accurate() {
        let root =
            Temp(std::env::temp_dir().join(format!("pubg-phase-dryrun-{}", std::process::id())));
        fs::create_dir_all(&root.0).unwrap();
        let a = root.0.join("a.mp4");
        // 100 frames @ 10fps = 10s; event at 5.0s is safely above default min_event_sec=2.0
        make_video(&a, 100, 0);
        let cache = root.0.join("cache");
        let out = root.0.join("output");
        fs::create_dir_all(&cache).unwrap();
        fs::create_dir_all(&out).unwrap();

        let cache_str = cache.display().to_string();
        let out_str = out.display().to_string();
        let a_str = a.display().to_string();
        let args = [
            &a_str as &str,
            "--game-lang",
            "en",
            "--jobs",
            "1",
            "--no-auto-candidate-csv",
            "--dry-run",
            "--cache-dir",
            &cache_str,
            "-o",
            &out_str,
        ];
        populate_cache(&args, &[serde_json::json!([event_self_death(5.0, 10.0)])]);

        let result = Command::new(env!("CARGO_BIN_EXE_pubg_highlight_trim"))
            .args(args)
            .output()
            .unwrap();

        let stdout = String::from_utf8_lossy(&result.stdout);
        let trim_total: Option<u64> = stdout
            .lines()
            .filter(|l| l.starts_with("PROGRESS "))
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(&l[9..]).ok())
            .filter(|v| {
                v["phase"].as_str() == Some("trim") && v["event"].as_str() == Some("skipped")
            })
            .filter_map(|v| v["total"].as_u64())
            .next();

        assert_eq!(
            trim_total,
            Some(0),
            "dry-run trim skipped total\nstdout:\n{stdout}"
        );
        assert!(
            stdout
                .lines()
                .filter_map(|l| l.strip_prefix("PROGRESS "))
                .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
                .any(|v| v["phase"] == "trim"
                    && v["event"] == "skipped"
                    && v["workers"] == 0
                    && v["planned"] == 1
                    && v["reason"] == "dry-run"),
            "dry-run must report no active encoder and retain planned count: {stdout}"
        );

        let trim_currents: Vec<u64> = stdout
            .lines()
            .filter(|l| l.starts_with("PROGRESS "))
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(&l[9..]).ok())
            .filter(|v| v["phase"].as_str() == Some("trim"))
            .filter_map(|v| v["current"].as_u64())
            .collect();

        for &c in &trim_currents {
            assert!(c <= 1, "trim current {c} exceeds total 1");
        }
    }

    /// Split-merge: merge PROGRESS total matches the number of split targets that have clips.
    #[test]
    #[ignore = "requires PUBG_OCR_MODEL_DIR and PUBG_HIGHLIGHT_TRIM_FFMPEG_DIR; synthetic media only"]
    fn split_merge_progress_total_matches_target_count() {
        let root =
            Temp(std::env::temp_dir().join(format!("pubg-phase-split-{}", std::process::id())));
        fs::create_dir_all(&root.0).unwrap();
        let a = root.0.join("a.mp4");
        // 20s video: enough room for events at 3.0s and 12.0s with default 5s before, 1s after
        make_video(&a, 200, 0);
        let cache = root.0.join("cache");
        let out = root.0.join("output");
        fs::create_dir_all(&cache).unwrap();
        fs::create_dir_all(&out).unwrap();

        let cache_str = cache.display().to_string();
        let out_str = out.display().to_string();
        let a_str = a.display().to_string();
        let args = [
            &a_str as &str,
            "--game-lang",
            "en",
            "--jobs",
            "1",
            "--no-auto-candidate-csv",
            "--merge",
            "--split-merge-by-target",
            "--target",
            "both",
            "--cache-dir",
            &cache_str,
            "-o",
            &out_str,
        ];
        // Two events: self-death at 3.0s and own-kill at 12.0s
        populate_cache(
            &args,
            &[serde_json::json!([
                event_self_death(3.0, 20.0),
                event_own_kill(12.0, 20.0),
            ])],
        );

        let result = Command::new(env!("CARGO_BIN_EXE_pubg_highlight_trim"))
            .args(args)
            .output()
            .unwrap();

        let stdout = String::from_utf8_lossy(&result.stdout);
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(
            result.status.code() == Some(0),
            "stdout:\n{stdout}\nstderr:\n{stderr}"
        );

        let merge_start_total: Option<u64> = stdout
            .lines()
            .filter(|l| l.starts_with("PROGRESS "))
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(&l[9..]).ok())
            .filter(|v| {
                v["phase"].as_str() == Some("merge") && v["event"].as_str() == Some("start")
            })
            .filter_map(|v| v["total"].as_u64())
            .next();

        // Both self-death and own-kill have clips → 2 merge targets
        assert_eq!(
            merge_start_total,
            Some(2),
            "split merge total must be 2\nstdout:\n{stdout}"
        );
    }

    /// start events carry workers field; dry-run trim produces no current > 0.
    #[test]
    #[ignore = "requires PUBG_OCR_MODEL_DIR and PUBG_HIGHLIGHT_TRIM_FFMPEG_DIR; synthetic media only"]
    fn dry_run_trim_current_never_exceeds_zero() {
        let root = Temp(
            std::env::temp_dir().join(format!("pubg-phase-dryrun-nocur-{}", std::process::id())),
        );
        fs::create_dir_all(&root.0).unwrap();
        let a = root.0.join("a.mp4");
        make_video(&a, 100, 0);
        let cache = root.0.join("cache");
        let out = root.0.join("output");
        fs::create_dir_all(&cache).unwrap();
        fs::create_dir_all(&out).unwrap();

        let cache_str = cache.display().to_string();
        let out_str = out.display().to_string();
        let a_str = a.display().to_string();
        let args = [
            &a_str as &str,
            "--game-lang",
            "en",
            "--jobs",
            "1",
            "--no-auto-candidate-csv",
            "--dry-run",
            "--cache-dir",
            &cache_str,
            "-o",
            &out_str,
        ];
        populate_cache(&args, &[serde_json::json!([event_self_death(5.0, 10.0)])]);

        let result = Command::new(env!("CARGO_BIN_EXE_pubg_highlight_trim"))
            .args(args)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&result.stdout);

        // Dry-run: no publish occurs, so no PROGRESS with trim current > 0.
        let max_trim_current: Option<u64> = stdout
            .lines()
            .filter(|l| l.starts_with("PROGRESS "))
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(&l[9..]).ok())
            .filter(|v| v["phase"].as_str() == Some("trim"))
            .filter_map(|v| v["current"].as_u64())
            .filter(|&c| c > 0)
            .next();
        assert_eq!(
            max_trim_current, None,
            "dry-run must not emit trim current > 0\nstdout:\n{stdout}"
        );

        // start event must carry workers field.
        let has_workers: bool = stdout
            .lines()
            .filter(|l| l.starts_with("PROGRESS "))
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(&l[9..]).ok())
            .filter(|v| v["event"].as_str() == Some("start"))
            .any(|v| v.get("workers").is_some());
        assert!(
            has_workers,
            "start events must carry workers field\n{stdout}"
        );
    }

    /// Merge explicitly skips dry-run work with zero active workers.
    #[test]
    #[ignore = "requires PUBG_OCR_MODEL_DIR and PUBG_HIGHLIGHT_TRIM_FFMPEG_DIR; synthetic media only"]
    fn merge_emits_skipped_with_zero_total_in_dry_run() {
        let root = Temp(
            std::env::temp_dir().join(format!("pubg-phase-merge-dryrun-{}", std::process::id())),
        );
        fs::create_dir_all(&root.0).unwrap();
        let a = root.0.join("a.mp4");
        make_video(&a, 100, 0);
        let cache = root.0.join("cache");
        let out = root.0.join("output");
        fs::create_dir_all(&cache).unwrap();
        fs::create_dir_all(&out).unwrap();

        let cache_str = cache.display().to_string();
        let out_str = out.display().to_string();
        let a_str = a.display().to_string();
        let args = [
            &a_str as &str,
            "--game-lang",
            "en",
            "--jobs",
            "1",
            "--no-auto-candidate-csv",
            "--dry-run",
            "--merge",
            "--cache-dir",
            &cache_str,
            "-o",
            &out_str,
        ];
        populate_cache(&args, &[serde_json::json!([event_self_death(5.0, 10.0)])]);

        let result = Command::new(env!("CARGO_BIN_EXE_pubg_highlight_trim"))
            .args(args)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&result.stdout);
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert_eq!(
            result.status.code(),
            Some(0),
            "stdout:\n{stdout}\nstderr:\n{stderr}"
        );

        // No clips are published in dry-run; skipped is terminal, not a running stage.
        let merge_start_total: Option<u64> = stdout
            .lines()
            .filter(|l| l.starts_with("PROGRESS "))
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(&l[9..]).ok())
            .filter(|v| {
                v["phase"].as_str() == Some("merge")
                    && v["event"].as_str() == Some("skipped")
                    && v["reason"] == "dry-run"
                    && v["workers"] == 0
            })
            .filter_map(|v| v["total"].as_u64())
            .next();
        assert_eq!(
            merge_start_total,
            Some(0),
            "dry-run: merge skipped must have total=0\nstdout:\n{stdout}"
        );
    }
}
