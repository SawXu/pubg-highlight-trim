use pubg_ocr_engine::media::{FfmpegFrameSource, FfmpegTools};
use std::{path::PathBuf, process::Command};

fn tools() -> FfmpegTools {
    match FfmpegTools::discover(None, None) {
        Ok(tools) => tools,
        Err(error) => {
            // Linked worktrees intentionally do not duplicate untracked vendor
            // binaries. Reuse the primary checkout's bundle for tests only;
            // application discovery must never depend on Git being installed.
            if std::env::var_os("PUBG_HIGHLIGHT_TRIM_FFMPEG_DIR").is_none() {
                if let Ok(output) = Command::new("git")
                    .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
                    .current_dir(env!("CARGO_MANIFEST_DIR"))
                    .output()
                {
                    if output.status.success() {
                        let common = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
                        if common.file_name().is_some_and(|name| name == ".git") {
                            if let Some(root) = common.parent() {
                                let dir = root.join("vendor/ffmpeg");
                                if let Ok(tools) = FfmpegTools::discover(
                                    Some(&dir.join("ffmpeg.exe")),
                                    Some(&dir.join("ffprobe.exe")),
                                ) {
                                    return tools;
                                }
                            }
                        }
                    }
                }
            }
            panic!("Media integration tests require real FFmpeg/FFprobe (not OCR models). Run scripts/download_ffmpeg.ps1 or set PUBG_HIGHLIGHT_TRIM_FFMPEG_DIR. No media checks were skipped: {error}");
        }
    }
}
#[test]
fn seek_outside_video_is_not_an_unreadable_video() {
    let tools = tools();
    let dir = std::env::temp_dir().join(format!("pubg-media-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("合成 clip.mp4");
    let status = Command::new(&tools.ffmpeg)
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=red:size=32x24:rate=10",
            "-t",
            "1",
            "-c:v",
            "libx264",
        ])
        .arg(&path)
        .status()
        .unwrap();
    assert!(status.success());
    assert_eq!(tools.source_codec(&path).as_deref(), Some("h264"));
    let version = tools.version_token().expect("real ffmpeg version token");
    assert!(!version.contains(['/', '\\', ' ']));
    assert_eq!(tools.source_codec(&dir.join("missing.mp4")), None);
    let mut source = FfmpegFrameSource::open(&path, &tools, None).unwrap();
    assert_eq!((source.width(), source.height()), (32, 24));
    assert!(source.read_at(99.0).unwrap().is_none());
    let frame = source.read_at(0.2).unwrap().unwrap();
    assert!(frame.pixels()[2] > 240 && frame.pixels()[0] < 10);
    assert_eq!(source.hwaccel(), None); // software is the default.
    let mut hardware =
        FfmpegFrameSource::open(&path, &tools, Some("not_a_real_accelerator")).unwrap();
    assert_eq!(hardware.read_at(0.2).unwrap().unwrap(), frame);
    assert_eq!(hardware.hwaccel(), None); // first failure permanently disables hardware.
    assert_eq!(hardware.read_at(0.2).unwrap().unwrap(), frame);
    let mut timed_out = FfmpegFrameSource::open(&path, &tools, None).unwrap();
    timed_out.set_decode_timeout(std::time::Duration::ZERO);
    assert!(timed_out.read_at(0.0).is_err());
    assert!(timed_out.last_diagnostic().contains("timed out"));
    timed_out.set_decode_timeout(std::time::Duration::from_secs(60));
    assert_eq!(timed_out.read_at(0.2).unwrap().unwrap(), frame);
    // A later bad read must not retroactively mark the whole source unreadable.
    source.set_decode_timeout(std::time::Duration::ZERO);
    assert!(source.read_at(0.3).unwrap().is_none());
    source.close();
    assert!(!source.is_open());
    assert!(source.read_at(0.0).unwrap().is_none());
    let mut never_decoded = FfmpegFrameSource::open(&path, &tools, None).unwrap();
    std::fs::write(&path, b"not decodable anymore").unwrap();
    assert!(matches!(
        never_decoded.read_at(0.0),
        Err(pubg_ocr_engine::media::MediaError::VideoUnreadable(_))
    ));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn bad_container_is_an_error_not_a_clean_empty_result() {
    let tools = tools();
    assert!(FfmpegFrameSource::open(&PathBuf::from("missing.mp4"), &tools, None).is_err());
    assert!(FfmpegTools::discover(Some(std::path::Path::new("missing-ffmpeg.exe")), None).is_err());
}

#[test]
fn av1_software_seek_and_explicit_hardware_failure_have_identical_pixels() {
    let tools = tools();
    let dir = std::env::temp_dir().join(format!("pubg-av1-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("av1.mp4");
    assert!(Command::new(&tools.ffmpeg)
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=64x48:rate=10",
            "-t",
            "1",
            "-c:v",
            "libaom-av1",
            "-cpu-used",
            "8",
            "-crf",
            "40"
        ])
        .arg(&path)
        .status()
        .unwrap()
        .success());
    let mut software = FfmpegFrameSource::open(&path, &tools, None).unwrap();
    let mut fallback = FfmpegFrameSource::open(&path, &tools, Some("invalid_hwaccel")).unwrap();
    for sec in [0.6, 0.1, 0.8] {
        assert_eq!(
            software.read_at(sec).unwrap().unwrap(),
            fallback.read_at(sec).unwrap().unwrap()
        );
    }
    assert_eq!(fallback.hwaccel(), None);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn synthesized_av1_seek_latency_stays_within_historical_h264_smoke_bound() {
    let tools = tools();
    let dir = std::env::temp_dir().join(format!("pubg-seek-latency-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let av1 = dir.join("av1.mp4");
    let h264 = dir.join("h264.mp4");
    for (path, codec) in [
        (
            &av1,
            vec!["-c:v", "libaom-av1", "-cpu-used", "8", "-crf", "40"],
        ),
        (&h264, vec!["-c:v", "libx264"]),
    ] {
        assert!(Command::new(&tools.ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=2:size=320x240:rate=30",
                "-pix_fmt",
                "yuv420p"
            ])
            .args(codec)
            .arg(path)
            .status()
            .unwrap()
            .success());
    }
    let median = |path: &std::path::Path| {
        let mut source = FfmpegFrameSource::open(path, &tools, None).unwrap();
        let mut latencies = Vec::new();
        for sec in [0.2, 0.8, 1.4, 0.5, 1.0] {
            let start = std::time::Instant::now();
            assert!(source.read_at(sec).unwrap().is_some());
            latencies.push(start.elapsed().as_secs_f64());
        }
        latencies.sort_by(f64::total_cmp);
        latencies[latencies.len() / 2]
    };
    let av1_median = median(&av1);
    let h264_median = median(&h264);
    // Preserves the old synthetic smoke limit, not a new full-corpus performance approval.
    assert!(
        av1_median <= h264_median * 4.0,
        "AV1 median {av1_median}s > 4 * H264 median {h264_median}s"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn trim_and_merge_encode_only_the_supplied_event_windows() {
    let tools = tools();
    let dir = std::env::temp_dir().join(format!("pubg-trim-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("source.mp4");
    assert!(Command::new(&tools.ffmpeg)
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=red:size=64x48:rate=20:duration=2[r];color=blue:size=64x48:rate=20:duration=2[b];[r][b]concat=n=2:v=1:a=0",
            "-t",
            "4",
            "-c:v",
            "libx264"
        ])
        .arg(&src)
        .status()
        .unwrap()
        .success());
    let original = std::fs::read(&src).unwrap();
    let one = dir.join("一' clip.mp4");
    let two = dir.join("two.mp4");
    let merged = dir.join("merged.mp4");
    let mut encoder = pubg_ocr_engine::media::TrimEncoder::new(&tools);
    let codec = encoder.trim_clip(&src, &one, 0.5, 0.75).unwrap();
    assert!(["available", "unavailable"].contains(&encoder.diagnostics().capability));
    assert!(encoder.diagnostics().probe_seconds > 0.0);
    assert!(["libx264", "h264_nvenc"].contains(&codec));
    encoder.trim_clip(&src, &two, 2.0, 0.5).unwrap();
    assert!((tools.duration_sec(&one).unwrap() - 0.75).abs() < 0.06);
    assert!((tools.duration_sec(&two).unwrap() - 0.5).abs() < 0.06);
    let list = tools
        .concat_clips(&[one.clone(), two.clone()], &merged)
        .unwrap();
    assert!(list.exists());
    assert!((tools.probe(&merged).unwrap().duration - 1.25).abs() < 0.1);
    let mut source = FfmpegFrameSource::open(&merged, &tools, None).unwrap();
    let first = source.read_at(0.0).unwrap().unwrap();
    assert!(
        first.pixels()[2] > 240 && first.pixels()[0] < 10,
        "first window must come from red half"
    );
    let second = source.read_at(1.0).unwrap().unwrap();
    assert!(
        second.pixels()[0] > 240 && second.pixels()[2] < 10,
        "second window must come from blue half, not beginning of source"
    );
    assert!(tools.trim_clip(&src, &src, 0., 1.).is_err());
    let alias = dir.join("hardlink.mp4");
    std::fs::hard_link(&src, &alias).unwrap();
    assert!(tools.trim_clip(&src, &alias, 0., 1.).is_err());
    assert!(tools.concat_clips(&[src.clone()], &src).is_err());
    assert!(tools.trim_clip(&src, &one, 0., f64::NAN).is_err());
    assert_eq!(std::fs::read(&src).unwrap(), original);
    std::fs::remove_dir_all(dir).unwrap();
}
