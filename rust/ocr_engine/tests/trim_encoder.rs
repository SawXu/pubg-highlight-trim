//! Native executable doubles exercise the public per-run encoder, not internal mocks.
use pubg_ocr_engine::media::{FfmpegTools, TrimEncoder};
use std::{fs, path::PathBuf, process::Command};

fn fixture(mode: &str) -> (PathBuf, FfmpegTools) {
    let root = std::env::temp_dir().join(format!("pubg-encoder-{}-{mode}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let source = root.join("fake.rs");
    fs::write(&source, r#"
use std::{fs,io::Write};
fn main() {
 let root=std::env::current_exe().unwrap().parent().unwrap().to_path_buf();
 let args:Vec<String>=std::env::args().skip(1).collect();
 let probe=args.iter().any(|x|x=="lavfi");
 let nvenc=args.iter().any(|x|x=="h264_nvenc");
 let label=if probe {"capability"} else if nvenc {"nvenc"} else {"software"};
 let mut log=fs::OpenOptions::new().append(true).create(true).open(root.join("calls")).unwrap();
 writeln!(log,"{}",label).unwrap();
 if probe {
  fs::write(root.join("probe-path"), args.last().unwrap()).unwrap();
  assert!(args.iter().any(|x|x=="p4")); assert!(args.iter().any(|x|x=="vbr"));
  assert!(args.iter().any(|x|x=="22")); assert!(args.last().unwrap().ends_with(".mp4"));
 }
 std::thread::sleep(std::time::Duration::from_millis(35));
 let mode=fs::read_to_string(root.join("mode")).unwrap();
 if mode==format!("hang-{label}") { std::thread::sleep(std::time::Duration::from_secs(120)); }
 if ((mode=="unavailable" || mode=="hang-software") && nvenc) || (mode=="production-failure" && nvenc && !probe) {
  eprintln!("private C:/secret/source.mp4 driver error"); std::process::exit(1);
 }
 fs::write(args.last().unwrap(),b"encoded").unwrap();
}
"#).unwrap();
    fs::write(root.join("mode"), mode).unwrap();
    let exe = root.join("fake.exe");
    assert!(Command::new("rustc")
        .arg(source)
        .arg("-o")
        .arg(&exe)
        .status()
        .unwrap()
        .success());
    (
        root,
        FfmpegTools {
            ffmpeg: exe.clone(),
            ffprobe: exe,
            verbose: false,
        },
    )
}

#[test]
fn unavailable_nvenc_is_probed_once_per_run_and_never_retried_for_each_clip() {
    let (root, tools) = fixture("unavailable");
    let mut run = TrimEncoder::new(&tools);
    for i in 0..3 {
        assert_eq!(
            run.trim_clip(
                &root.join("source.mp4"),
                &root.join(format!("{i}.mp4")),
                0.,
                1.
            )
            .unwrap(),
            "libx264"
        );
    }
    assert_eq!(
        fs::read_to_string(root.join("calls")).unwrap(),
        "capability\nsoftware\nsoftware\nsoftware\n"
    );
    let metadata = serde_json::to_value(run.diagnostics()).unwrap();
    assert_eq!(metadata["capability"], "unavailable");
    assert!(metadata["probe_seconds"].as_f64().unwrap() >= 0.03);
    assert!(!metadata.to_string().contains("secret"));
    let mut next_run = TrimEncoder::new(&tools);
    next_run
        .trim_clip(&root.join("source.mp4"), &root.join("next.mp4"), 0., 1.)
        .unwrap();
    assert_eq!(
        fs::read_to_string(root.join("calls"))
            .unwrap()
            .matches("capability")
            .count(),
        2
    );
    let probe_path = PathBuf::from(fs::read_to_string(root.join("probe-path")).unwrap());
    assert!(!probe_path.parent().unwrap().exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn hung_capability_is_bounded_cleaned_and_cached_as_software() {
    let (root, tools) = fixture("hang-capability");
    let mut run = TrimEncoder::new(&tools);
    let started = std::time::Instant::now();
    for i in 0..2 {
        assert_eq!(
            run.trim_clip(
                &root.join("source.mp4"),
                &root.join(format!("{i}.mp4")),
                0.,
                1.
            )
            .unwrap(),
            "libx264"
        );
    }
    assert!(started.elapsed().as_secs_f64() < 20.0);
    assert!(run.diagnostics().probe_seconds >= 15.0);
    assert_eq!(
        fs::read_to_string(root.join("calls")).unwrap(),
        "capability\nsoftware\nsoftware\n"
    );
    let probe_path = PathBuf::from(fs::read_to_string(root.join("probe-path")).unwrap());
    assert!(!probe_path.parent().unwrap().exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn successful_probe_retains_production_fallback_and_reuses_capability() {
    for mode in ["available", "production-failure"] {
        let (root, tools) = fixture(mode);
        let mut run = TrimEncoder::new(&tools);
        for i in 0..2 {
            let codec = run
                .trim_clip(
                    &root.join("source.mp4"),
                    &root.join(format!("{i}.mp4")),
                    0.,
                    1.,
                )
                .unwrap();
            assert_eq!(
                codec,
                if mode == "available" {
                    "h264_nvenc"
                } else {
                    "libx264"
                }
            );
        }
        let calls = fs::read_to_string(root.join("calls")).unwrap();
        assert_eq!(
            calls,
            if mode == "available" {
                "capability\nnvenc\nnvenc\n"
            } else {
                "capability\nnvenc\nsoftware\nnvenc\nsoftware\n"
            }
        );
        assert_eq!(
            run.diagnostics().production_fallbacks,
            if mode == "available" { 0 } else { 2 }
        );
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn cancellation_kills_the_active_capability_or_production_child_without_fallback_or_publication() {
    if let Ok(mode) = std::env::var("ENCODER_CANCEL_CHILD") {
        use pubg_ocr_engine::{cancellation, media::MediaError, media_output::MediaOutput};
        let (root, tools) = fixture(&mode);
        let log = root.join("calls");
        let expected = mode.strip_prefix("hang-").unwrap().to_owned();
        let cancel = std::thread::spawn(move || {
            let started = std::time::Instant::now();
            loop {
                if fs::read_to_string(&log)
                    .unwrap_or_default()
                    .lines()
                    .any(|l| l == expected)
                {
                    break;
                }
                assert!(
                    started.elapsed().as_secs() < 10,
                    "active child never started"
                );
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            std::thread::sleep(std::time::Duration::from_millis(80));
            cancellation::request();
        });
        let destination = root.join("result.mp4");
        fs::write(&destination, b"previous complete").unwrap();
        let staged = MediaOutput::reserve(&destination, true, &[]).unwrap();
        let temporary = staged.temporary().to_path_buf();
        let started = std::time::Instant::now();
        let result =
            TrimEncoder::new(&tools).trim_clip(&root.join("source.mp4"), &temporary, 0., 1.);
        cancel.join().unwrap();
        assert!(matches!(result, Err(MediaError::Cancelled)), "{result:?}");
        assert!(started.elapsed().as_secs() < 10);
        drop(staged);
        assert!(!temporary.parent().unwrap().exists());
        let probe_path = PathBuf::from(fs::read_to_string(root.join("probe-path")).unwrap());
        assert!(!probe_path.parent().unwrap().exists());
        assert_eq!(fs::read(&destination).unwrap(), b"previous complete");
        assert_eq!(
            fs::read_to_string(root.join("calls")).unwrap(),
            if mode == "hang-capability" {
                "capability\n"
            } else if mode == "hang-software" {
                "capability\nsoftware\n"
            } else {
                "capability\nnvenc\n"
            }
        );
        fs::remove_dir_all(root).unwrap();
        return;
    }
    for mode in ["hang-capability", "hang-nvenc", "hang-software"] {
        let out = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "cancellation_kills_the_active_capability_or_production_child_without_fallback_or_publication", "--nocapture"])
            .env("ENCODER_CANCEL_CHILD", mode).output().unwrap();
        assert!(
            out.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
