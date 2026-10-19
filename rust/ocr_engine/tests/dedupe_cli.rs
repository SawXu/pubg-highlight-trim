//! Complete CLI batch with synthetic lossless recordings and the public persistent
//! scan-cache boundary. OCR sampling is not mocked or changed by verification.
#![cfg(feature = "onnx")]
use clap::Parser;
use pubg_ocr_engine::{
    config::Config,
    persistent_cache,
    scan_adapter::{source_config, EngineSettings},
    source_files,
};
use serde_json::{json, Value};
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
fn tools(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os("PUBG_HIGHLIGHT_TRIM_FFMPEG_DIR").expect("set FFmpeg directory"))
        .join(name)
}
fn video(path: &Path, event: usize, mode: &str, second: bool) {
    let mut child = Command::new(tools("ffmpeg.exe"))
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
    for frame in 0..event + 10 {
        let mut seed = (frame as i64 - event as i64 + 1000) as u64;
        if second && (mode == "different-game" || (mode == "early-mismatch" && frame < event)) {
            seed += 71;
        }
        if matches!(mode, "static" | "black" | "hud-only") {
            seed = 123;
        }
        let mut pixels = Vec::new();
        for _ in 0..64 * 48 * 3 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            pixels.push(seed as u8);
        }
        if mode == "black" {
            pixels.fill(0);
        }
        if mode == "hud-only" {
            pixels[..64 * 4 * 3].fill(frame as u8);
        }
        stdin.write_all(&pixels).unwrap();
    }
    drop(stdin);
    assert!(child.wait().unwrap().success());
}
fn event(sec: f64, key: &str) -> Value {
    json!({"duration_sec":sec+1.,"event_sec":sec,"method":"ocr","detector":"ocr","target":"self-death","event_key":key,"sampled_count":7,"ocr_requests":3,"termination_reason":"completed"})
}
#[test]
#[ignore = "requires pinned PUBG_OCR_MODEL_DIR and FFmpeg; synthetic media only"]
fn full_batch_preserves_clips_concat_skipped_records_and_later_more_complete_context() {
    for mode in [
        "duplicate",
        "later-complete",
        "early-mismatch",
        "different-game",
        "black",
        "static",
        "hud-only",
        "different-key",
        "missing-key",
        "extra-event",
        "same-source",
    ] {
        let root = Temp(
            std::env::temp_dir().join(format!("pubg-dedupe-cli-{}-{mode}", std::process::id())),
        );
        fs::create_dir_all(&root.0).unwrap();
        let first = root.0.join("a.mp4");
        let second = root.0.join("b.mp4");
        let (a, b) = if mode == "later-complete" {
            (20, 305)
        } else {
            (305, 20)
        };
        video(&first, a, mode, false);
        video(&second, b, mode, true);
        let originals = [fs::read(&first).unwrap(), fs::read(&second).unwrap()];
        let cache = root.0.join("cache");
        let out = root.0.join("output");
        let mut args = vec!["--files".to_string(), first.display().to_string()];
        if mode != "same-source" {
            args.push(second.display().to_string());
        }
        args.extend(
            [
                "--game-lang",
                "en",
                "--jobs",
                "1",
                "--no-auto-candidate-csv",
                "--merge",
                "--profile",
                "-y",
                "--cache-dir",
            ]
            .map(str::to_string),
        );
        args.push(cache.display().to_string());
        args.push("-o".into());
        args.push(out.display().to_string());
        let c = Config::parse_from(
            std::iter::once("pubg_highlight_trim".to_string()).chain(args.clone()),
        );
        let inputs = source_files::select(&c).unwrap();
        let mut settings = EngineSettings::discover().unwrap();
        settings.identity.push_str(":effective-threads=1");
        for (index, source) in inputs.sources.iter().enumerate() {
            let config = source_config(&c, source, None);
            let key = persistent_cache::cache_key(
                &source.path,
                &json!({"config":config,"candidate_times":[]}),
                &settings.identity,
            )
            .unwrap();
            let identity = if index == 1 && mode == "different-key" {
                "self-death:knock:another"
            } else if index == 1 && mode == "missing-key" {
                ""
            } else {
                "self-death:knock:enemy"
            };
            let mut detections = vec![event(if index == 0 { a } else { b } as f64 / 10., identity)];
            if mode == "same-source" {
                detections.push(event(15., "self-death:knock:enemy"));
            }
            if mode == "extra-event" && index == 1 {
                detections.push(event(2.5, "self-death:knock:unique"));
            }
            persistent_cache::write(
                &cache,
                &key,
                &json!({"detections":detections,"provider":null}),
            )
            .unwrap();
        }
        let result = Command::new(env!("CARGO_BIN_EXE_pubg_highlight_trim"))
            .args(&args)
            .output()
            .unwrap();
        assert_eq!(
            result.status.code(),
            Some(0),
            "{mode}: {} / {}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        let s: Value =
            serde_json::from_slice(&fs::read(out.join("summary.json")).unwrap()).unwrap();
        let duplicate = matches!(
            mode,
            "duplicate" | "later-complete" | "early-mismatch" | "extra-event"
        );
        let count = if duplicate && mode != "extra-event" {
            1
        } else {
            2
        };
        assert_eq!(s["included_count"], count, "{mode}");
        assert_eq!(s["skipped_count"], if duplicate { 1 } else { 0 }, "{mode}");
        assert_eq!(
            fs::read_dir(out.join("clips")).unwrap().count(),
            count as usize,
            "{mode}"
        );
        let concat = fs::read_to_string(s["concat_list"].as_str().unwrap()).unwrap();
        assert_eq!(concat.lines().count(), count as usize, "{mode}");
        let bytes = fs::read(out.join("检测与裁剪记录.csv")).unwrap();
        let mut csv = csv::Reader::from_reader(&bytes[3..]);
        let headers = csv.headers().unwrap().clone();
        let field = |name: &str| headers.iter().position(|s| s == name).unwrap();
        let rows = csv.records().map(Result::unwrap).collect::<Vec<_>>();
        assert!(rows
            .windows(2)
            .all(|r| r[0][field("Index")].parse::<u64>().unwrap()
                <= r[1][field("Index")].parse::<u64>().unwrap()));
        if duplicate {
            let skip = rows
                .iter()
                .find(|r| &r[field("Status")] == "skipped")
                .unwrap();
            assert_eq!(&skip[field("Method")], "skipped-cross-source-duplicate");
            assert_eq!(&skip[field("Output")], "");
            assert_eq!(&skip[field("OcrRequests")], "3");
            let include = rows
                .iter()
                .find(|r| &r[field("Status")] == "included")
                .unwrap();
            if mode != "extra-event" {
                assert_eq!(&include[field("KeepStartSec")], "25.500");
                assert_eq!(&include[field("KeepEndSec")], "31.500");
                assert!(
                    include[field("Output")].ends_with(if mode == "later-complete" {
                        "b.mp4"
                    } else {
                        "a.mp4"
                    })
                );
            }
            assert_eq!(s["methods"]["skipped-cross-source-duplicate"], 1);
        }
        if mode == "extra-event" {
            assert!(rows
                .iter()
                .any(|r| &r[field("Status")] == "included"
                    && r[field("EventKeys")].contains("unique")));
        }
        assert_eq!(fs::read(&first).unwrap(), originals[0]);
        assert_eq!(fs::read(&second).unwrap(), originals[1]);
    }
}
