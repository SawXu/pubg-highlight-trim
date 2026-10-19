#![cfg(feature = "onnx")]
//! Real public OCR crops embedded in synthetic video, exercised only through
//! the supported application CLI. CI must run this after provisioning models/FFmpeg.
mod common;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "pubg-e2e-录像 {}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn checked(command: &mut Command) -> Output {
    let output = command.output().expect("launch media/application command");
    assert!(
        output.status.success(),
        "{command:?}\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}
fn hash(path: &Path) -> Vec<u8> {
    Sha256::digest(fs::read(path).unwrap()).to_vec()
}
fn summary(output: &Output) -> Value {
    let stdout = String::from_utf8(output.stdout.clone()).unwrap();
    let records: Vec<_> = stdout
        .lines()
        .filter_map(|s| s.strip_prefix("SUMMARY "))
        .collect();
    assert_eq!(records.len(), 1, "one complete summary per run: {stdout}");
    serde_json::from_str(records[0]).unwrap()
}
fn create_video(ffmpeg: &Path, png: &Path, out: &Path, pad: bool) {
    let filter = if pad {
        "[1:v]pad=768:98:0:0:black[roi];[0:v][roi]overlay=x=576:y=712:enable='between(t,2,4)'"
    } else {
        "[0:v][1:v]overlay=x=576:y=712:enable='between(t,2,4)'"
    };
    checked(
        Command::new(ffmpeg)
            .args([
                "-nostdin",
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=black:s=1920x1080:r=2:d=6",
                "-loop",
                "1",
                "-framerate",
                "2",
                "-i",
            ])
            .arg(png)
            .args([
                "-filter_complex",
                filter,
                "-t",
                "6",
                "-c:v",
                "libx264",
                "-crf",
                "0",
                "-preset",
                "ultrafast",
                "-pix_fmt",
                "yuv444p",
                "-n",
            ])
            .arg(out),
    );
}
#[test]
#[ignore = "requires provisioned pinned ONNX models and FFmpeg; mandatory in release CI"]
fn public_roi_video_pipeline_preserves_events_reports_media_and_sources() {
    let tools = pubg_ocr_engine::media::FfmpegTools::discover(None, None)
        .expect("provision FFmpeg tools before e2e");
    let models =
        std::env::var_os("PUBG_OCR_MODEL_DIR").expect("provision PUBG_OCR_MODEL_DIR before e2e");
    let temp = Temp::new();
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/missed_event_ocr");
    let self_source = temp.0.join("合成 self.mp4");
    let own_source = temp.0.join("合成 own.mp4");
    create_video(
        &tools.ffmpeg,
        &fixture.join("23.20.15.09_2.png"),
        &self_source,
        false,
    );
    create_video(
        &tools.ffmpeg,
        &fixture.join("23.04.41.02_32.png"),
        &own_source,
        true,
    );
    let original_hashes = [hash(&self_source), hash(&own_source)];
    let out = temp.0.join("输出 clips");
    let execute = |output: &Path, jobs: &str, dry_run: bool| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_pubg_highlight_trim"));
        command
            .arg("--files")
            .arg(&self_source)
            .arg(&own_source)
            .args([
                "--game-lang",
                "zh-Hant",
                "--coarse-step",
                "1",
                "--before",
                "1",
                "--after",
                "1",
                "--no-auto-candidate-csv",
                "--no-brightness-gate",
                "--jobs",
                jobs,
                "--min-event-sec",
                "0",
                "--merge",
                "--split-merge-by-target",
                "--ffmpeg",
            ])
            .arg(&tools.ffmpeg)
            .arg("--ffprobe")
            .arg(&tools.ffprobe)
            .arg("-o")
            .arg(output)
            .env("PUBG_OCR_MODEL_DIR", &models)
            .env("PUBG_OCR_PROVIDER", "cpu");
        if dry_run {
            command.arg("--scan-only");
        }
        checked(&mut command)
    };
    let output = execute(&out, "2", false);
    let report = summary(&output);
    let contract: Value = serde_json::from_str(include_str!(
        "../../../docs/baselines/issue66_cli_contract.json"
    ))
    .unwrap();
    for (field, kind) in contract["report_schema"]["summary_types"]
        .as_object()
        .unwrap()
    {
        let value = report
            .get(field)
            .unwrap_or_else(|| panic!("missing historical summary field {field}"));
        let valid = match kind.as_str().unwrap() {
            "integer" => value.is_i64() || value.is_u64(),
            "number" => value.is_f64(),
            "string" => value.is_string(),
            "boolean" => value.is_boolean(),
            "array" => value.is_array(),
            "object" => value.is_object(),
            "null" => value.is_null(),
            _ => false,
        };
        assert!(
            valid,
            "historical summary type changed for {field}: expected {kind}, got {value}"
        );
    }
    assert_eq!(report["source_count"], 2);
    assert_eq!(report["included_count"], 2);
    assert_eq!(report["skipped_count"], 0);
    assert_eq!(report["unreadable_count"], 0);
    assert_eq!(report["dry_run"], false);
    assert_eq!(report["split_merge_by_target"], true);
    assert_eq!(report["merge_outputs"].as_array().unwrap().len(), 2);
    assert!(
        output.stderr.is_empty(),
        "default third-party diagnostics must be suppressed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let disk: Value = serde_json::from_slice(&fs::read(out.join("summary.json")).unwrap()).unwrap();
    assert_eq!(report, disk);
    let csv_bytes = fs::read(Path::new(report["csv"].as_str().unwrap())).unwrap();
    assert!(
        csv_bytes.starts_with(b"\xef\xbb\xbf"),
        "CSV UTF-8 BOM contract"
    );
    let mut reader = csv::Reader::from_reader(&csv_bytes[3..]);
    let headers = reader.headers().unwrap().clone();
    let rows: Vec<_> = reader.records().map(Result::unwrap).collect();
    assert_eq!(rows.len(), 2);
    let index = |field: &str| headers.iter().position(|s| s == field).expect(field);
    let identities: Value =
        serde_json::from_str(include_str!("../fixtures/missed_events.json")).unwrap();
    for target in ["own-kill", "self-death"] {
        let row = rows
            .iter()
            .find(|r| r.get(index("Target")) == Some(target))
            .expect("each target produces a clip");
        assert_eq!(row.get(index("Status")), Some("included"));
        assert_eq!(row.get(index("EventSec")), Some("2.000"));
        assert_eq!(row.get(index("KeepStartSec")), Some("1.000"));
        assert_eq!(row.get(index("KeepEndSec")), Some("3.000"));
        let expected = identities["frames"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["target"] == target && case["action"] == "knock")
            .unwrap();
        let mut video_identity = expected.clone();
        video_identity["identity_sha256"] = expected["video_identity_sha256"].clone();
        video_identity["identity_chars"] = expected["video_identity_chars"].clone();
        assert!(
            common::contains_identity(row.get(index("EventKeys")).unwrap(), &video_identity),
            "recognized subject identity changed"
        );
        let path = Path::new(row.get(index("Output")).unwrap());
        assert!((tools.duration_sec(path).unwrap() - 2.0).abs() < 0.15);
        checked(
            Command::new(&tools.ffmpeg)
                .args(["-nostdin", "-v", "error", "-i"])
                .arg(path)
                .args(["-f", "null", "-"]),
        );
    }
    for merged in report["merge_outputs"].as_array().unwrap() {
        let path = Path::new(merged["path"].as_str().unwrap());
        assert!((tools.duration_sec(path).unwrap() - 2.0).abs() < 0.15);
        checked(
            Command::new(&tools.ffmpeg)
                .args(["-nostdin", "-v", "error", "-i"])
                .arg(path)
                .args(["-f", "null", "-"]),
        );
    }
    let single = summary(&execute(&temp.0.join("单任务"), "1", true));
    assert_eq!(single["included_count"], report["included_count"]);
    assert_eq!(single["methods"], report["methods"]);
    assert_eq!(single["detectors"], report["detectors"]);
    let bytes = fs::read(Path::new(single["csv"].as_str().unwrap())).unwrap();
    let mut single_reader = csv::Reader::from_reader(bytes.strip_prefix(b"\xef\xbb\xbf").unwrap());
    assert_eq!(single_reader.headers().unwrap(), &headers);
    let single_rows: Vec<_> = single_reader.records().map(Result::unwrap).collect();
    assert_eq!(single_rows.len(), rows.len());
    for (parallel, sequential) in rows.iter().zip(&single_rows) {
        for field in [
            "Name",
            "Target",
            "Status",
            "DurationSec",
            "EventSec",
            "EventSecs",
            "EventCount",
            "EventKeys",
            "EventWeapon",
            "ContextRule",
            "BeforeSec",
            "AfterSec",
            "KeepStartSec",
            "KeepEndSec",
            "KeepDurationSec",
            "Method",
            "Detector",
            "PaddleText",
            "PaddleScores",
        ] {
            assert_eq!(
                parallel.get(index(field)),
                sequential.get(index(field)),
                "worker scheduling must not change {field}"
            );
        }
    }
    assert_eq!(original_hashes, [hash(&self_source), hash(&own_source)]);
}
