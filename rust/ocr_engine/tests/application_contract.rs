use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};
static ID: AtomicUsize = AtomicUsize::new(0);
fn dir() -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "pubg-app-{}-{}",
        std::process::id(),
        ID.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&p).unwrap();
    p
}
fn invoke(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pubg_highlight_trim"))
        .args(args)
        .output()
        .unwrap()
}
#[test]
fn complete_cli_options_are_accepted_before_input_validation() {
    let out = invoke(&[
        "missing.mp4",
        "--recursive",
        "--min-event-sec",
        "0",
        "--molotov-elim-before",
        "10",
        "--split-merge-by-target",
        "-y",
        "--move-originals",
        "--fast-path",
        "--cache-dir",
        "cache",
        "--ffmpeg",
        "ffmpeg.exe",
        "--candidate-csv",
        "hints.csv",
        "--no-auto-candidate-csv",
        "--scan-mode",
        "full",
        "--priority-window",
        "1:2",
        "--scan-start",
        "0",
        "--scan-end",
        "9",
        "--coarse-step",
        "2",
        "--candidate-lookback",
        "8",
        "--candidate-lookahead",
        "0.5",
        "--candidate-step",
        "4",
        "--refine-before",
        "6",
        "--refine-after",
        "0.4",
        "--refine-step",
        "0.5",
        "--sampling-mode",
        "adaptive",
        "--adaptive-step",
        "0.5",
        "--adaptive-window",
        "2",
        "--ocr-max-calls",
        "10",
        "--ocr-min-interval",
        "0",
        "--refine-max-frames",
        "4",
        "--assist-max-frames",
        "5",
        "--refine-max-seconds",
        "1",
        "--assist-max-seconds",
        "2",
        "--roi",
        "0.3,0.66,0.7,0.75",
        "--ocr-width",
        "768",
        "--no-brightness-gate",
        "--gate-mode",
        "light",
    ]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("Input path does not exist"));
}
#[test]
fn invalid_roi_and_mutually_exclusive_fast_paths_are_usage_errors() {
    for args in [
        vec!["missing.mp4", "--roi", "0.7,0,0.3,1"],
        vec!["missing.mp4", "--fast-path", "--no-fast-path"],
        vec!["missing.mp4", "--adaptive-step=-1"],
    ] {
        assert_eq!(invoke(&args).status.code(), Some(2));
    }
}
#[test]
fn invalid_explicit_media_executable_is_a_system_failure_not_an_unreadable_skip() {
    let root = dir();
    let input = root.join("source.mp4");
    fs::write(&input, b"original").unwrap();
    let exe = root.join("invalid-ffprobe.exe");
    fs::write(&exe, b"not an executable").unwrap();
    let outdir = root.join("output");
    let out = invoke(&[
        input.to_str().unwrap(),
        "--ffprobe",
        exe.to_str().unwrap(),
        "--scan-only",
        "-o",
        outdir.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(1));
    assert!(!String::from_utf8_lossy(&out.stdout).contains("SUMMARY "));
    assert!(!outdir.exists());
    assert_eq!(fs::read(&input).unwrap(), b"original");
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn overwrite_must_not_delete_an_input_ancestor() {
    let root = dir();
    let input = root.join("source.mp4");
    fs::write(&input, b"precious-original").unwrap();
    let keep = root.join("unrelated.txt");
    fs::write(&keep, b"keep").unwrap();
    let out = invoke(&[
        input.to_str().unwrap(),
        "-o",
        root.to_str().unwrap(),
        "-y",
        "--scan-only",
    ]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(fs::read(&input).unwrap(), b"precious-original");
    assert_eq!(fs::read(&keep).unwrap(), b"keep");
    fs::remove_dir_all(root).unwrap();
}
