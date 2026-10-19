#[cfg(windows)]
#[test]
fn windows_loader_failure_is_system_error_not_corrupt_source() {
    use pubg_ocr_engine::media::{FfmpegTools, MediaError};
    use std::{fs, process::Command};
    let root = std::env::temp_dir().join(format!("pubg-loader-error-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let code = root.join("fail.rs");
    let exe = root.join("fail.exe");
    fs::write(
        &code,
        "fn main(){std::process::exit(0xC0000135u32 as i32);}",
    )
    .unwrap();
    assert!(Command::new("rustc")
        .arg(&code)
        .args(["--crate-name", "loader_error", "-o"])
        .arg(&exe)
        .status()
        .unwrap()
        .success());
    let tools = FfmpegTools {
        verbose: false,
        ffmpeg: exe.clone(),
        ffprobe: exe,
    };
    let result = tools.duration_sec(&root.join("source.mp4"));
    assert!(
        matches!(&result,Err(MediaError::RuntimeUnavailable(message))if message.contains("required DLL is missing")),
        "{result:?}"
    );
    fs::remove_dir_all(root).unwrap();
}
