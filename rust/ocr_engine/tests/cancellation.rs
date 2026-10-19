//! Cancellation at the public media/application boundary, isolated in its own test process.
use pubg_ocr_engine::{
    cancellation,
    media::{FfmpegTools, MediaError},
};
use std::{
    fs,
    process::Command,
    time::{Duration, Instant},
};
#[test]
fn cancellation_stops_running_media_and_does_not_turn_it_into_an_unreadable_source() {
    let tools = FfmpegTools::discover(None, None).expect("FFmpeg for media acceptance");
    let root = std::env::temp_dir().join(format!("pubg-cancel-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let src = root.join("original.mp4");
    let output = root.join("partial.mp4");
    assert!(Command::new(&tools.ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=s=1920x1080:r=60:d=5",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast"
        ])
        .arg(&src)
        .status()
        .unwrap()
        .success());
    let bytes = fs::read(&src).unwrap();
    let started = Instant::now();
    let cancel = std::thread::spawn(|| {
        std::thread::sleep(Duration::from_millis(100));
        cancellation::request();
    });
    let result = tools.trim_clip(&src, &output, 0.0, 5.0);
    cancel.join().unwrap();
    assert!(matches!(result, Err(MediaError::Cancelled)), "{result:?}");
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(fs::read(&src).unwrap(), bytes);
    // The child was waited/reaped; cleanup of its partial output is not locked.
    if output.exists() {
        fs::remove_file(output).unwrap()
    };
    fs::remove_dir_all(root).unwrap();
}
