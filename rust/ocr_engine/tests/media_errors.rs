//! Real child processes at the media boundary, without FFmpeg or OCR assets.
use pubg_ocr_engine::media::{FfmpegFrameSource, FfmpegTools, MediaError};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
    time::{Duration, Instant},
};
fn helper(name: &str) -> PathBuf {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    let root=ROOT.get_or_init(|| {
        let root=std::env::temp_dir().join(format!("pubg-media-errors-{}",std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let source=root.join("helper.rs");
        std::fs::write(&source,r#"
            fn main() {
                let name=std::env::current_exe().unwrap().file_stem().unwrap().to_string_lossy().into_owned();
                if name.contains("gpu") && std::env::args().any(|a|a=="h264_nvenc") { eprintln!("synthetic GPU runtime failure"); std::process::exit(-1073741515); }
                if name.contains("crash") { std::process::exit(-1073741515); }
                if name.contains("bad") { eprintln!("Invalid data found when processing input");std::process::exit(1); }
                if name.contains("other") { eprintln!("unexpected tool failure");std::process::exit(7); }
                if name.contains("hang") { std::thread::sleep(std::time::Duration::from_secs(120)); }
                println!("2,2");
            }
        "#).unwrap();
        assert!(Command::new("rustc").arg(&source).arg("-o").arg(root.join("good.exe")).status().unwrap().success());
        for name in ["bad","crash","hang","other","gpu"] { std::fs::copy(root.join("good.exe"),root.join(format!("{name}.exe"))).unwrap(); }
        root
    });
    root.join(format!("{name}.exe"))
}
fn tools(probe: &str, decoder: &str) -> FfmpegTools {
    FfmpegTools {
        verbose: false,
        ffprobe: helper(probe),
        ffmpeg: helper(decoder),
    }
}
#[test]
fn missing_or_nonexecutable_probe_and_decoder_are_fatal_io_errors() {
    let good = helper("good");
    let missing = good.with_file_name("missing.exe");
    let t = FfmpegTools {
        verbose: false,
        ffprobe: missing.clone(),
        ffmpeg: good.clone(),
    };
    assert!(matches!(
        t.probe(Path::new("video.mp4")),
        Err(MediaError::Io(_))
    ));
    let invalid = good.with_file_name("invalid.exe");
    std::fs::write(&invalid, b"not an executable").unwrap();
    let t = FfmpegTools {
        verbose: false,
        ffprobe: invalid,
        ffmpeg: good.clone(),
    };
    assert!(matches!(
        t.probe(Path::new("video.mp4")),
        Err(MediaError::Io(_))
    ));
    let t = FfmpegTools {
        verbose: false,
        ffprobe: good,
        ffmpeg: missing,
    };
    let mut source = FfmpegFrameSource::open(Path::new("video.mp4"), &t, None).unwrap();
    assert!(matches!(source.read_at(0.), Err(MediaError::Io(_))));
    let out = helper("good").with_file_name("trim-output.mp4");
    assert!(matches!(
        t.trim_clip(&helper("good"), &out, 0., 1.),
        Err(MediaError::Io(_))
    ));
}
#[test]
fn ordinary_probe_failure_is_unreadable_but_other_exit_is_fatal() {
    assert!(matches!(
        tools("bad", "good").probe(Path::new("video.mp4")),
        Err(MediaError::VideoUnreadable(_))
    ));
    assert!(matches!(
        tools("other", "good").probe(Path::new("video.mp4")),
        Err(MediaError::CommandFailed(_))
    ));
    assert!(matches!(
        tools("bad", "good").duration_sec(Path::new("video.mp4")),
        Err(MediaError::VideoUnreadable(_))
    ));
}
#[cfg(windows)]
#[test]
fn native_windows_missing_dll_exit_is_runtime_failure_for_probe_and_decode() {
    assert!(matches!(
        tools("crash", "good").probe(Path::new("video.mp4")),
        Err(MediaError::RuntimeUnavailable(_))
    ));
    let mut source = FfmpegFrameSource::open(
        Path::new("video.mp4"),
        &tools("good", "crash"),
        Some("cuda"),
    )
    .unwrap();
    assert!(matches!(
        source.read_at(0.),
        Err(MediaError::RuntimeUnavailable(_))
    ));
    assert_eq!(
        source.hwaccel(),
        Some("cuda"),
        "runtime crashes must not be consumed as hardware decode failures"
    );
    let out = helper("good").with_file_name("crash-output.mp4");
    assert!(matches!(
        tools("good", "crash").trim_clip(&helper("good"), &out, 0., 1.),
        Err(MediaError::RuntimeUnavailable(_))
    ));
}
#[test]
fn hung_probe_is_killed_and_propagates_timeout_not_unreadable() {
    let started = Instant::now();
    assert!(matches!(
        tools("hang", "good").probe_with_timeout(Path::new("video.mp4"), Duration::from_millis(30)),
        Err(MediaError::Timeout(_))
    ));
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[cfg(windows)]
#[test]
fn nvenc_only_native_runtime_failure_retries_software_but_common_runtime_failure_stays_fatal() {
    let src = helper("good");
    let out = src.with_file_name("gpu-fallback.mp4");
    assert_eq!(
        tools("good", "gpu").trim_clip(&src, &out, 0., 1.).unwrap(),
        "libx264"
    );
    assert!(matches!(
        tools("good", "crash").trim_clip(&src, &out, 0., 1.),
        Err(MediaError::RuntimeUnavailable(_))
    ));
}
