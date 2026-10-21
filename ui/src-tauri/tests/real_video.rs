//! Opt-in media acceptance through the same public Shell used by the UI.
//! Inputs are immutable copies of a fixed corpus; this test never archives them.
use highlight_shell::{
    storage::{RuntimeLayout, Storage},
    RunOptions, Shell,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::Read;

fn source_hash(path: &str) -> Vec<u8> {
    let mut file = fs::File::open(path).unwrap();
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 65536];
    loop {
        let n = file.read(&mut buffer).unwrap();
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    hash.finalize().to_vec()
}
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

fn summary(path: &Path) -> Value {
    let mut value: Value =
        serde_json::from_slice(&fs::read(path.join("summary.json")).unwrap()).unwrap();
    // Only runtime measurements and output locations are volatile. All decision,
    // provider, source, event and merge counters remain in the comparison.
    let object = value.as_object_mut().unwrap();
    for key in [
        "run_id",
        "profile_total_sec",
        "merge_size_mb",
        "output_dir",
        "csv",
        "profile",
        "candidate_csv",
        "candidate_events_csv",
        "concat_list",
        "merge_output",
        "merge_outputs",
    ] {
        object.remove(key);
    }
    value
}
fn records(path: &Path) -> Vec<BTreeMap<String, String>> {
    let mut reader = csv::Reader::from_path(path.join("检测与裁剪记录.csv")).unwrap();
    let headers = reader.headers().unwrap().clone();
    reader
        .records()
        .map(|record| {
            headers
                .iter()
                .zip(record.unwrap().iter())
                .filter(|(name, _)| {
                    !matches!(
                        *name,
                        "OcrSeconds"
                            | "DetectSeconds"
                            | "OcrFrameSeconds"
                            | "RefineOcrSeconds"
                            | "AssistOcrSeconds"
                            | "ProbeSeconds"
                            | "TrimSeconds"
                            | "TotalSeconds"
                    )
                })
                .map(|(name, value)| {
                    (
                        name.to_owned(),
                        if name == "Output" {
                            Path::new(value)
                                .file_name()
                                .map(|v| v.to_string_lossy().into_owned())
                                .unwrap_or_default()
                        } else {
                            value.to_owned()
                        },
                    )
                })
                .collect()
        })
        .collect()
}
fn media(root: &Path, ffmpeg: &Path, ffprobe: &Path) -> BTreeMap<PathBuf, (Value, String)> {
    let mut result = BTreeMap::new();
    fn visit(
        root: &Path,
        path: &Path,
        ffmpeg: &Path,
        ffprobe: &Path,
        result: &mut BTreeMap<PathBuf, (Value, String)>,
    ) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, ffmpeg, ffprobe, result);
            } else if path
                .extension()
                .is_some_and(|v| v.eq_ignore_ascii_case("mp4"))
            {
                let probe = Command::new(ffprobe)
                    .args([
                        "-v",
                        "error",
                        "-show_entries",
                        "stream=codec_type,width,height,sample_rate,channels,duration,nb_frames",
                        "-of",
                        "json",
                    ])
                    .arg(&path)
                    .output()
                    .unwrap();
                assert!(probe.status.success());
                let decoded = Command::new(ffmpeg)
                    .args(["-v", "error", "-i"])
                    .arg(&path)
                    .args(["-map", "0", "-f", "framemd5", "-"])
                    .output()
                    .unwrap();
                assert!(decoded.status.success());
                let hashes = String::from_utf8(decoded.stdout)
                    .unwrap()
                    .lines()
                    .filter(|l| !l.starts_with('#'))
                    .collect::<Vec<_>>()
                    .join("\n");
                result.insert(
                    path.strip_prefix(root).unwrap().to_path_buf(),
                    (serde_json::from_slice(&probe.stdout).unwrap(), hashes),
                );
            }
        }
    }
    visit(root, root, ffmpeg, ffprobe, &mut result);
    result
}

#[test]
#[ignore = "requires fixed real videos and a complete current-source Windows CLI bundle; see docs/tauri-ui-verification.md"]
fn real_video_shell_matches_direct_cli_decisions_records_and_decoded_media() {
    let inputs: Vec<String> = serde_json::from_str(
        &std::env::var("PUBG_UI_REAL_INPUTS").expect("JSON array of absolute MP4 paths"),
    )
    .unwrap();
    assert!(!inputs.is_empty());
    let bundle = PathBuf::from(
        std::env::var_os("PUBG_UI_REAL_BUNDLE").expect("complete CLI bundle directory"),
    )
    .canonicalize()
    .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let app = temp.path().join("app");
    fs::create_dir_all(app.join("cli")).unwrap();
    // Hard links avoid copying hundreds of MB and do not modify the source bundle.
    fn link_tree(source: &Path, dest: &Path) {
        for entry in fs::read_dir(source).unwrap() {
            let entry = entry.unwrap();
            let to = dest.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                fs::create_dir_all(&to).unwrap();
                link_tree(&entry.path(), &to);
            } else {
                fs::hard_link(entry.path(), &to)
                    .or_else(|_| fs::copy(entry.path(), &to).map(|_| ()))
                    .unwrap();
            }
        }
    }
    link_tree(&bundle, &app.join("cli"));
    let layout = RuntimeLayout::from_root(&app).unwrap();
    let before: Vec<_> = inputs.iter().map(|p| source_hash(p)).collect();
    let direct = temp.path().join("direct");
    let via_shell = temp.path().join("shell");
    let mut options = RunOptions {
        input_paths: inputs.clone(),
        output_directory: Some(direct.to_string_lossy().into_owned()),
        jobs: "1".into(),
        trim_jobs: "1".into(),
        ..Default::default()
    };
    let result = Command::new(&layout.cli)
        .args(options.arguments().unwrap())
        .current_dir(layout.cli.parent().unwrap())
        .env("PUBG_OCR_MODEL_DIR", &layout.models)
        .output()
        .unwrap();
    assert!(
        matches!(result.status.code(), Some(0 | 2)),
        "direct CLI failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let storage = Storage::open(temp.path().join("data")).unwrap();
    let shell = Shell::new(storage, Ok(layout));
    options.output_directory = Some(via_shell.to_string_lossy().into_owned());
    shell.start(options, false).unwrap();
    let deadline = Instant::now() + Duration::from_secs(300);
    while shell.state().running {
        if Instant::now() > deadline {
            let _ = shell.cancel();
            panic!("real-video run timed out");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let state = shell.state();
    assert_eq!(state.exit_code, result.status.code());
    assert!(state.error.is_none(), "{:?}", state.error);
    assert_eq!(summary(&direct), summary(&via_shell));
    assert_eq!(records(&direct), records(&via_shell));
    let ffmpeg = bundle.join("_internal/vendor/ffmpeg/ffmpeg.exe");
    let ffprobe = bundle.join("_internal/vendor/ffmpeg/ffprobe.exe");
    let direct_media = media(&direct, &ffmpeg, &ffprobe);
    if std::env::var_os("PUBG_UI_REQUIRE_MEDIA").is_some() {
        assert!(!direct_media.is_empty(), "event corpus must produce media");
    }
    assert_eq!(direct_media, media(&via_shell, &ffmpeg, &ffprobe));
    assert_eq!(
        before,
        inputs.iter().map(|p| source_hash(p)).collect::<Vec<_>>()
    );
    println!(
        "Real-video acceptance passed: {} sources, {} decoded outputs",
        inputs.len(),
        direct_media.len()
    );
}
