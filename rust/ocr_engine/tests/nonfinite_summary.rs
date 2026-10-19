#![cfg(feature = "onnx")]
use serde_json::Value;
use std::{fs, process::Command};
#[test]
#[ignore = "requires pinned models and FFmpeg; mandatory provisioned CLI acceptance"]
fn legacy_positive_infinity_is_not_silently_changed_to_json_null() {
    let tools = pubg_ocr_engine::media::FfmpegTools::discover(None, None).unwrap();
    let dir = std::env::temp_dir().join(format!(
        "pubg-infinity-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&dir).unwrap();
    let source = dir.join("source.mp4");
    assert!(Command::new(&tools.ffmpeg)
        .args([
            "-nostdin",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=320x240:d=1",
            "-c:v",
            "mpeg4",
            "-n"
        ])
        .arg(&source)
        .status()
        .unwrap()
        .success());
    for (index, field) in ["--min-event-sec", "--molotov-elim-before"]
        .iter()
        .enumerate()
    {
        let output = dir.join(format!("out{index}"));
        let run = Command::new(env!("CARGO_BIN_EXE_pubg_highlight_trim"))
            .arg(&source)
            .args([
                field,
                "inf",
                "--scan-only",
                "--no-auto-candidate-csv",
                "--no-merge",
                "--jobs",
                "1",
                "--ffmpeg",
            ])
            .arg(&tools.ffmpeg)
            .arg("--ffprobe")
            .arg(&tools.ffprobe)
            .arg("-o")
            .arg(&output)
            .output()
            .unwrap();
        assert_eq!(
            run.status.code(),
            Some(2),
            "{}",
            String::from_utf8_lossy(&run.stderr)
        );
        let stdout = String::from_utf8(run.stdout).unwrap();
        let summary = stdout
            .lines()
            .find_map(|line| line.strip_prefix("SUMMARY "))
            .unwrap();
        let key = field.trim_start_matches("--").replace('-', "_");
        assert!(
            summary.contains(&format!("\"{key}\":Infinity")),
            "legacy Python JSON extension must not become null: {summary}"
        );
        let disk = fs::read_to_string(output.join("summary.json")).unwrap();
        assert!(disk.contains(&format!("\"{key}\":Infinity")));
        // Other fields remain ordinary interoperable JSON.
        let normalized = summary.replace(":Infinity", ":1e300");
        assert_eq!(
            serde_json::from_str::<Value>(&normalized).unwrap()["source_count"],
            1
        );
    }
    fs::remove_dir_all(dir).unwrap();
}
