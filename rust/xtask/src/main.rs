mod build_real_roi;
mod licenses;
use clap::{Parser, Subcommand};
use prost::Message;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Parser, Debug)]
#[command(name = "xtask", about = "Offline model and release assurance tools")]
struct Cli {
    #[command(subcommand)]
    command: CommandKind,
}
#[derive(Subcommand, Debug)]
enum CommandKind {
    Verify {
        #[arg(long)]
        model_dir: PathBuf,
        #[arg(long, default_value = "rust/ocr_engine/models/pp_ocrv6_medium.json")]
        manifest: PathBuf,
        #[arg(long)]
        check_onnx: bool,
        /// Source-built OCR executable for full ONNX loading and real inference.
        #[arg(long, requires = "check_onnx")]
        engine: Option<PathBuf>,
        #[arg(long, default_value_t = 120)]
        timeout_secs: u64,
    },
    Download {
        #[arg(long)]
        output_dir: PathBuf,
        #[arg(long, default_value = "rust/ocr_engine/models/pp_ocrv6_medium.json")]
        manifest: PathBuf,
    },
    Dictionary {
        #[arg(long)]
        source: String,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        expected_sha256: Option<String>,
    },
    OnnxSmoke {
        #[arg(long)]
        model_dir: PathBuf,
        #[arg(long)]
        engine: Option<PathBuf>,
        #[arg(long, default_value_t = 120)]
        timeout_secs: u64,
    },
    /// Collect locked application transitive dependency licenses before manifest generation.
    Licenses {
        bundle: PathBuf,
        #[arg(long)]
        manifest_path: Option<PathBuf>,
    },
    Manifest {
        bundle: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Audit {
        bundle: PathBuf,
    },
    Size {
        #[arg(default_value = "dist/pubg-highlight-trim")]
        bundle: PathBuf,
        #[arg(long, default_value = "dist/pubg-highlight-trim-windows-x64.zip")]
        zip: PathBuf,
        #[arg(long, default_value_t = 20)]
        top: usize,
    },
    Compare {
        left: PathBuf,
        right: PathBuf,
        #[arg(long)]
        allow_missing_boxes: bool,
    },
    RealRoi {
        #[arg(long)]
        executable: PathBuf,
        #[arg(long)]
        model_dir: PathBuf,
        #[arg(long, default_value = "rust/ocr_engine/fixtures/real")]
        fixture: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Extract local CSV-selected video ROIs; never uploads media.
    BuildRealRoi {
        #[arg(long, required = true)]
        record: Vec<PathBuf>,
        #[arg(long)]
        media_root: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value_t = 4)]
        limit_per_language: usize,
        #[arg(long)]
        ffmpeg: PathBuf,
    },
    FrameBench {
        #[arg(long)]
        ffmpeg: PathBuf,
        #[arg(long)]
        ffprobe: PathBuf,
        #[arg(long)]
        av1: PathBuf,
        #[arg(long)]
        h264: PathBuf,
        #[arg(long, default_value_t = 2)]
        repeat: usize,
    },
}
#[derive(Debug, Deserialize)]
struct ModelManifest {
    model_version: String,
    license: Option<String>,
    detector: Asset,
    recognizer: Asset,
    dictionary: Asset,
    contract: Option<Contract>,
}
#[derive(Debug, Deserialize)]
struct Asset {
    file: String,
    source: String,
    sha256: String,
    extract: Option<String>,
}
#[derive(Debug, Deserialize)]
struct Contract {
    detector_input: Option<String>,
    detector_output: Option<String>,
    recognizer_input: Option<String>,
    recognizer_output: Option<String>,
}

fn digest(path: &Path) -> Result<String, String> {
    let mut f = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut h = Sha256::new();
    let mut b = vec![0u8; 1024 * 1024];
    loop {
        let n = f.read(&mut b).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        h.update(&b[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}
fn read_manifest(path: &Path) -> Result<ModelManifest, String> {
    serde_json::from_str(&fs::read_to_string(path).map_err(|e| e.to_string())?)
        .map_err(|e| format!("invalid manifest: {e}"))
}
fn assets(m: &ModelManifest) -> [(&str, &Asset); 3] {
    [
        ("detector", &m.detector),
        ("recognizer", &m.recognizer),
        ("dictionary", &m.dictionary),
    ]
}

#[derive(Clone, PartialEq, Message)]
struct OnnxModel {
    #[prost(int64, tag = "1")]
    ir_version: i64,
    #[prost(message, optional, tag = "7")]
    graph: Option<OnnxGraph>,
}
#[derive(Clone, PartialEq, Message)]
struct OnnxGraph {
    #[prost(message, repeated, tag = "11")]
    input: Vec<OnnxValueInfo>,
    #[prost(message, repeated, tag = "12")]
    output: Vec<OnnxValueInfo>,
}
#[derive(Clone, PartialEq, Message)]
struct OnnxValueInfo {
    #[prost(string, tag = "1")]
    name: String,
}
fn check_onnx(path: &Path, role: &str, contract: Option<&Contract>) -> Result<(), String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let graph = OnnxModel::decode(bytes.as_slice())
        .map_err(|e| format!("{role} ONNX protobuf decode failed: {e}"))?
        .graph
        .ok_or_else(|| format!("{role} ONNX graph missing"))?;
    if graph.input.is_empty() || graph.output.is_empty() {
        return Err(format!("{role} ONNX graph has no inputs or outputs"));
    }
    let (ei, eo) = if role == "detector" {
        (
            contract.and_then(|c| c.detector_input.as_deref()),
            contract.and_then(|c| c.detector_output.as_deref()),
        )
    } else {
        (
            contract.and_then(|c| c.recognizer_input.as_deref()),
            contract.and_then(|c| c.recognizer_output.as_deref()),
        )
    };
    if ei.is_some_and(|e| graph.input[0].name != e) {
        return Err(format!(
            "{role} input mismatch: expected {}, actual {}",
            ei.unwrap(),
            graph.input[0].name
        ));
    }
    if eo.is_some_and(|e| graph.output[0].name != e) {
        return Err(format!(
            "{role} output mismatch: expected {}, actual {}",
            eo.unwrap(),
            graph.output[0].name
        ));
    }
    Ok(())
}
fn verify(model_dir: &Path, manifest_path: &Path, do_onnx: bool) -> Result<(), String> {
    let m = read_manifest(manifest_path)?;
    let mut failures = Vec::new();
    for (role, a) in assets(&m) {
        let p = model_dir.join(&a.file);
        if !p.is_file() {
            failures.push(format!("missing {role}: {}", p.display()));
            continue;
        }
        match digest(&p) {
            Ok(actual) => {
                if a.sha256.len() != 64 || !actual.eq_ignore_ascii_case(&a.sha256) {
                    failures.push(format!(
                        "{role} sha256 mismatch: expected {}, actual {actual}",
                        a.sha256
                    ))
                }
            }
            Err(e) => failures.push(e),
        }
        if role == "dictionary" {
            match fs::read_to_string(&p) {
                Ok(s) => {
                    let n = s
                        .lines()
                        .filter(|x| !x.is_empty())
                        .collect::<BTreeSet<_>>()
                        .len();
                    if n < 100 {
                        failures.push(format!(
                            "dictionary content is unexpectedly small: {n} symbols"
                        ))
                    }
                }
                Err(e) => failures.push(format!("dictionary is not valid UTF-8: {e}")),
            }
        } else if do_onnx {
            if let Err(e) = check_onnx(&p, role, m.contract.as_ref()) {
                failures.push(e)
            }
        }
    }
    if failures.is_empty() {
        if !do_onnx {
            println!("OK model_version={} hashes-and-dictionary", m.model_version);
        }
        Ok(())
    } else {
        for f in failures {
            eprintln!("ERROR {f}")
        }
        Err("model verification failed".into())
    }
}

fn fetch(url: &str) -> Result<Vec<u8>, String> {
    if !url
        .strip_prefix("https://")
        .is_some_and(|rest| !rest.is_empty())
    {
        return Err(format!("refusing uncontrolled model source: {url}"));
    }
    let mut r = ureq::get(url)
        .timeout(std::time::Duration::from_secs(120))
        .call()
        .map_err(|e| e.to_string())?
        .into_reader();
    let mut b = Vec::new();
    r.read_to_end(&mut b).map_err(|e| e.to_string())?;
    Ok(b)
}
#[cfg(windows)]
fn replace_atomic(tmp: &Path, path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
    }
    let mut old: Vec<u16> = tmp
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut new: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let ok = unsafe { MoveFileExW(old.as_mut_ptr(), new.as_mut_ptr(), 0x1 | 0x8) };
    if ok == 0 {
        Err(std::io::Error::last_os_error().to_string())
    } else {
        Ok(())
    }
}
#[cfg(not(windows))]
fn replace_atomic(tmp: &Path, path: &Path) -> Result<(), String> {
    fs::rename(tmp, path).map_err(|e| e.to_string())
}
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p).map_err(|e| e.to_string())?
    }
    let tmp = path.with_extension(format!(
        "{}.part",
        path.extension().and_then(|x| x.to_str()).unwrap_or("tmp")
    ));
    let result = (|| {
        let mut f = fs::File::create(&tmp).map_err(|e| e.to_string())?;
        f.write_all(bytes).map_err(|e| e.to_string())?;
        f.sync_all().map_err(|e| e.to_string())?;
        replace_atomic(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}
fn yaml_dictionary(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let doc: serde_yaml::Value =
        serde_yaml::from_slice(bytes).map_err(|e| format!("invalid YAML: {e}"))?;
    let chars = doc
        .get("PostProcess")
        .and_then(|v| v.get("character_dict"))
        .and_then(|v| v.as_sequence())
        .ok_or("PostProcess.character_dict missing")?;
    let mut out = String::new();
    for c in chars {
        out.push_str(c.as_str().ok_or("dictionary entry is not a string")?);
        out.push('\n')
    }
    if out.is_empty() {
        return Err("dictionary is empty".into());
    }
    Ok(out.into_bytes())
}
fn safe_asset_path(out: &Path, file: &str) -> Result<PathBuf, String> {
    let rel = Path::new(file);
    if rel.is_absolute()
        || rel
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(format!("manifest asset path escapes output: {file}"));
    }
    Ok(out.join(rel))
}
fn download(out: &Path, manifest_path: &Path) -> Result<(), String> {
    let m = read_manifest(manifest_path)?;
    fs::create_dir_all(out).map_err(|e| e.to_string())?;
    for (role, a) in assets(&m) {
        let p = safe_asset_path(out, &a.file)?;
        if p.is_file() && digest(&p)?.eq_ignore_ascii_case(&a.sha256) {
            println!("{role} already verified sha256={}", a.sha256);
            continue;
        }
        println!("Downloading {role} from {}", a.source);
        let raw = fetch(&a.source)?;
        let content = if a.extract.as_deref() == Some("PostProcess.character_dict") {
            yaml_dictionary(&raw)?
        } else {
            raw
        };
        let actual = format!("{:x}", Sha256::digest(&content));
        if !actual.eq_ignore_ascii_case(&a.sha256) {
            return Err(format!(
                "sha256 mismatch for {}: expected {}, actual {actual}",
                a.file, a.sha256
            ));
        }
        atomic_write(&p, &content)?;
        println!("sha256={actual}")
    }
    Ok(())
}
fn dictionary(source: &str, out: &Path, expected: Option<&str>) -> Result<(), String> {
    let content = yaml_dictionary(&fetch(source)?)?;
    let actual = format!("{:x}", Sha256::digest(&content));
    if let Some(e) = expected.filter(|x| !x.is_empty()) {
        if !actual.eq_ignore_ascii_case(e) {
            return Err(format!("sha256 mismatch: expected {e}, actual {actual}"));
        }
    }
    let entries = content.iter().filter(|&&b| b == b'\n').count();
    atomic_write(out, &content)?;
    println!("{}", json!({"entries":entries,"sha256":actual}));
    Ok(())
}
/// Concurrent pipe draining and bounded wait prevent a broken engine from hanging tooling.
fn engine_request(
    exe: &Path,
    dir: &Path,
    request: Value,
    timeout_secs: u64,
) -> Result<std::process::Output, String> {
    if timeout_secs == 0 {
        return Err("timeout must be positive".into());
    }
    let mut child = Command::new(exe)
        .arg(dir)
        .arg("1")
        .arg("cpu")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut input = child.stdin.take().unwrap();
    let writer = std::thread::spawn(move || input.write_all(format!("{request}\n").as_bytes()));
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut b = Vec::new();
        stdout.read_to_end(&mut b).map(|_| b)
    });
    let errors = std::thread::spawn(move || {
        let mut b = Vec::new();
        stderr.read_to_end(&mut b).map(|_| b)
    });
    let start = std::time::Instant::now();
    let (status, timed_out) = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break (status, false);
        }
        if start.elapsed().as_secs() >= timeout_secs {
            let _ = child.kill();
            break (child.wait().map_err(|e| e.to_string())?, true);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    let written = writer.join().map_err(|_| "engine writer panicked")?;
    let stdout = reader
        .join()
        .map_err(|_| "engine reader panicked")?
        .map_err(|e| e.to_string())?;
    let stderr = errors
        .join()
        .map_err(|_| "engine stderr panicked")?
        .map_err(|e| e.to_string())?;
    if timed_out {
        return Err("OCR engine timed out (killed and reaped)".into());
    }
    written.map_err(|e| e.to_string())?;
    Ok(std::process::Output {
        status,
        stdout,
        stderr,
    })
}
fn onnx_smoke(dir: &Path, engine: Option<&Path>, timeout_secs: u64) -> Result<(), String> {
    let exe = engine.map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(if cfg!(windows) {
            "target/debug/ocr_engine.exe"
        } else {
            "target/debug/ocr_engine"
        })
    });
    if !exe.is_file() {
        return Err(format!("engine not found: {}", exe.display()));
    }
    // Existing repository ROI, not a generated blank image: detection must yield
    // boxes and recognition must yield the frozen text/event before passing.
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../ocr_engine/fixtures/real");
    let image = image::open(fixture.join("frame_001.jpg"))
        .map_err(|e| e.to_string())?
        .to_rgb8();
    let req =
        json!({"pixels_b64":b64(image.as_raw()),"width":image.width(),"height":image.height()});
    let out = engine_request(&exe, dir, req, timeout_secs)?;
    if !out.status.success() {
        return Err(format!(
            "engine failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let stdout = String::from_utf8(out.stdout).map_err(|e| e.to_string())?;
    let lines: Vec<_> = stdout.lines().collect();
    if lines.len() != 1 {
        return Err(format!(
            "engine protocol expected one response, got {}",
            lines.len()
        ));
    }
    let row: Value = serde_json::from_str(lines[0]).map_err(|e| e.to_string())?;
    let reference: Value = serde_json::from_slice(
        &fs::read(fixture.join("reference.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let expected = &reference["frames"][0];
    if expected["image_sha256"].as_str() != Some(digest(&fixture.join("frame_001.jpg"))?.as_str()) {
        return Err("smoke ROI hash differs from frozen reference".into());
    }
    use pubg_ocr_engine::events::{self, Target};
    let text = row["text"].as_str().unwrap_or("");
    let profile = events::language_profile("zh-Hans")?;
    let actual = events::extract_text_events(text, Target::Both, Some(&profile));
    let signatures = event_signatures(&actual);
    let frozen = expected["event_signatures"]
        .as_array()
        .ok_or("missing smoke event signatures")?;
    let initialized = String::from_utf8_lossy(&out.stderr)
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .any(|v| v["event"] == "engine-init" && v["provider"] == "cpu" && v["error"].is_null());
    if row["status"] != "ok"
        || row["provider"] != "cpu"
        || !initialized
        || !row["boxes"].as_array().is_some_and(|v| {
            !v.is_empty()
                && v.iter().all(|b| {
                    b.as_array().is_some_and(|a| {
                        if a.len() != 4 || !a.iter().all(|x| x.as_u64().is_some()) {
                            return false;
                        }
                        let c: Vec<_> = a.iter().map(|x| x.as_u64().unwrap()).collect();
                        c[0] < c[2]
                            && c[1] < c[3]
                            && c[2] <= image.width() as u64
                            && c[3] <= image.height() as u64
                    })
                })
        })
        || text_digest(&normalize(text))
            != expected["normalized_ocr_sha256"]
                .as_str()
                .ok_or("missing smoke OCR fingerprint")?
        || actual.is_empty()
        || signatures.as_array().unwrap() != frozen
    {
        return Err(format!(
            "real detector/recognizer smoke failed (sessions, text fingerprint, boxes or event fingerprints), status={}",row["status"]
        ));
    }
    println!(
        "{}",
        json!({"detector_session":"loaded-and-inferred","recognizer_session":"loaded-and-inferred","roi":"frame_001.jpg","status":row["status"],"provider":row["provider"],"normalized_ocr_sha256":text_digest(&normalize(text)),"event_signatures":signatures,"boxes":row["boxes"]})
    );
    Ok(())
}
fn all_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(d).map_err(|e| e.to_string())?.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p)
            } else {
                out.push(p)
            }
        }
    }
    out.sort();
    Ok(out)
}

#[derive(Serialize)]
struct Component {
    name: String,
    kind: String,
    version: Option<String>,
    source: String,
    license: Option<String>,
    files: Vec<String>,
    external: bool,
    notes: Option<String>,
}
fn component(
    name: &str,
    kind: &str,
    version: Option<String>,
    source: &str,
    license: Option<&str>,
    files: &[&str],
    external: bool,
    notes: Option<&str>,
) -> Component {
    Component {
        name: name.into(),
        kind: kind.into(),
        version,
        source: source.into(),
        license: license.map(|x| x.to_string()),
        files: files.iter().map(|x| x.to_string()).collect(),
        external,
        notes: notes.map(|x| x.to_string()),
    }
}
fn command_version(path: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new(path).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    text.split_whitespace()
        .map(|token| {
            token.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.' && c != '-')
        })
        .find(|token| {
            let dots = token.matches('.').count();
            dots >= 1 && token.chars().next().is_some_and(|c| c.is_ascii_digit())
        })
        .map(str::to_owned)
}
fn cargo_version(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    let in_package = text
        .lines()
        .scan(false, |seen, line| {
            if line.trim() == "[package]" {
                *seen = true
            } else if line.starts_with('[') {
                *seen = false
            }
            Some((*seen, line))
        })
        .find_map(|(seen, line)| {
            if seen && line.trim_start().starts_with("version") {
                line.split('=')
                    .nth(1)
                    .map(|x| x.trim().trim_matches('"').to_string())
            } else {
                None
            }
        });
    in_package
}
fn components(bundle: &Path) -> Vec<Component> {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| Path::new("."));
    let model_path = repo_root.join("rust/ocr_engine/models/pp_ocrv6_medium.json");
    let model = fs::read_to_string(model_path)
        .ok()
        .and_then(|x| serde_json::from_str::<ModelManifest>(&x).ok());
    let app_version = command_version(&bundle.join("pubg-highlight-trim.exe"), &["--version"]);
    let ocr_version = cargo_version(&repo_root.join("rust/ocr_engine/Cargo.toml"));
    let ffmpeg = bundle.join("_internal/vendor/ffmpeg/ffmpeg.exe");
    let ffmpeg_version = command_version(&ffmpeg, &["-version"]);
    let mut c = vec![
        component(
            "pubg-highlight-trim",
            "application",
            app_version,
            "this repository, Rust",
            Some("GPL-3.0-or-later"),
            &["pubg-highlight-trim.exe"],
            false,
            None,
        ),
        component(
            "pubg-ocr-engine",
            "application",
            ocr_version,
            "this repository, rust/ocr_engine",
            Some("GPL-3.0-or-later"),
            &["ocr_engine.exe"],
            false,
            None,
        ),
        component(
            "ONNX Runtime",
            "static library inside Rust application and OCR diagnostic",
            Some("1.20.0".to_string()),
            "https://parcel.pyke.io/v2/delivery/ortrs/packages/msort-binary/1.20.0/ortrs_static-v1.20.0-x86_64-pc-windows-msvc.tgz; SHA256 BB02D856F2747E4863C344BE38AF2A645E660E9055C871EE9984907497BE590A (ort-sys 2.0.0-rc.9 dist.txt)",
            Some("MIT"),
            &["pubg-highlight-trim.exe","ocr_engine.exe"],
            false,
            None,
        ),
        component(
            "ppocr-rs",
            "rust crate, vendored",
            Some("0.7.3".to_string()),
            "vendor/ppocr-rs (controlled by application source commit); upstream crate 0.7.3",
            Some("Apache-2.0"),
            &["pubg-highlight-trim.exe","ocr_engine.exe"],
            false,
            None,
        ),
        component(
            "ort",
            "rust crate",
            Some("2.0.0-rc.9".to_string()),
            "https://crates.io/crates/ort",
            Some("MIT OR Apache-2.0"),
            &["pubg-highlight-trim.exe","ocr_engine.exe"],
            false,
            None,
        ),
        component(
            "DirectML",
            "Windows inbox component, delay-loaded",
            None,
            "provided by Windows 10 1903+",
            Some("Microsoft Windows"),
            &[],
            true,
            Some("optional execution provider; CPU requires no DirectML"),
        ),
        component(
            "Visual C++ runtime",
            "redistributable DLLs",
            None,
            "Microsoft Visual C++ 2015-2022 x64 installed redistributable; individual DLL hashes in files inventory",
            Some("Microsoft software license terms (redistributable)"),
            &[
                "vcruntime140.dll",
                "vcruntime140_1.dll",
                "msvcp140.dll",
                "msvcp140_1.dll",
            ],
            false,
            None,
        ),
        component(
            "FFmpeg / FFprobe",
            "binary",
            ffmpeg_version,
            "BtbN FFmpeg builds; version read from bundled ffmpeg -version",
            Some("GPL-3.0-or-later"),
            &[
                "_internal/vendor/ffmpeg/ffmpeg.exe",
                "_internal/vendor/ffmpeg/ffprobe.exe",
            ],
            false,
            None,
        ),
    ];
    if let Some(m) = model {
        let source = format!(
            "detector {}#sha256={}; recognizer {}#sha256={}; dictionary {}#sha256={}",
            m.detector.source,
            m.detector.sha256,
            m.recognizer.source,
            m.recognizer.sha256,
            m.dictionary.source,
            m.dictionary.sha256
        );
        c.push(component(
            "PP-OCRv6 medium ONNX models",
            "model assets",
            Some(m.model_version),
            &source,
            m.license.as_deref(),
            &[
                "ocr_models/det.onnx",
                "ocr_models/rec.onnx",
                "ocr_models/dict.txt",
                "ocr_models/pp_ocrv6_medium.json",
            ],
            false,
            None,
        ));
    }
    c.into_iter()
        .filter(|entry| {
            entry.external
                || entry.files.is_empty()
                || entry.files.iter().any(|f| bundle.join(f).exists())
        })
        .collect()
}
fn manifest(bundle: &Path, output: Option<&Path>) -> Result<(), String> {
    if !bundle.is_dir() {
        return Err(format!("bundle not found: {}", bundle.display()));
    }
    let out = output
        .map(PathBuf::from)
        .unwrap_or_else(|| bundle.join("SUPPLY_CHAIN_MANIFEST.json"));
    let mut files = Vec::new();
    for p in all_files(bundle)? {
        if p == out {
            continue;
        }
        files.push(json!({"path":p.strip_prefix(bundle).unwrap().to_string_lossy().replace('\\',"/"),"sha256":digest(&p)?,"bytes":p.metadata().map_err(|e|e.to_string())?.len()}))
    }
    let data = json!({"schema":"pubg-highlight-trim-supply-chain/v1","bundle":bundle.file_name().unwrap_or_default().to_string_lossy(),"generated_by":"cargo xtask manifest","policy":"release runtime must not download models, binaries, or caches","components":components(bundle),"files":files});
    atomic_write(
        &out,
        serde_json::to_string_pretty(&data).unwrap().as_bytes(),
    )?;
    println!(
        "wrote {} ({} files hashed)",
        out.display(),
        data["files"].as_array().unwrap().len()
    );
    Ok(())
}
fn forbidden(name: &str) -> bool {
    let n = name.replace('\\', "/").to_ascii_lowercase();
    n.split('/').any(|p| {
        p == "paddle"
            || p == "paddleocr"
            || p == "paddlex"
            || p == "paddlex_cache"
            || p.starts_with("paddleocr")
            || p.starts_with("paddlepaddle")
            || p.starts_with("paddlex")
            || p.starts_with("libpaddle")
            || p == "numpy"
            || p.starts_with("numpy-")
            || p == "cv2"
            || p.starts_with("opencv")
            || p.starts_with("python3")
            || p.starts_with("python312")
            || p.starts_with("python311")
            || p.ends_with(".pyc")
            || p.ends_with(".pyd")
            || p == "base_library.zip"
    })
}
fn audit(bundle: &Path) -> Result<(), String> {
    let mut bad = Vec::new();
    for p in all_files(bundle)? {
        let rel = p
            .strip_prefix(bundle)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if forbidden(&rel) {
            bad.push(rel.clone())
        }
        if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")) {
            let f = fs::File::open(&p).map_err(|e| format!("cannot open archive {rel}: {e}"))?;
            let mut z = zip::ZipArchive::new(f).map_err(|e| format!("invalid zip {rel}: {e}"))?;
            for i in 0..z.len() {
                let n = z
                    .by_index(i)
                    .map_err(|e| format!("invalid zip entry {rel}: {e}"))?
                    .name()
                    .to_string();
                if forbidden(&n) {
                    bad.push(format!("{rel}!{n}"))
                }
            }
        }
    }
    if bad.is_empty() {
        println!("Default OCR bundle audit OK: no Python/Paddle/NumPy/OpenCV runtime residue");
        Ok(())
    } else {
        bad.sort();
        for x in bad {
            eprintln!("Unexpected Python/OCR runtime file: {x}")
        }
        Err("bundle audit failed".into())
    }
}
fn fmt_size(n: u64) -> String {
    format!("{:.1} MB", n as f64 / 1024.0 / 1024.0)
}
fn size(bundle: &Path, zip: &Path, top: usize) -> Result<(), String> {
    if !bundle.exists() {
        return Err(format!("Bundle does not exist: {}", bundle.display()));
    }
    let mut files = Vec::new();
    for p in all_files(bundle)? {
        files.push((p.metadata().map_err(|e| e.to_string())?.len(), p))
    }
    let total: u64 = files.iter().map(|x| x.0).sum();
    let mut groups: BTreeMap<String, u64> = BTreeMap::new();
    for (n, p) in &files {
        let r = p.strip_prefix(bundle).unwrap();
        let mut it = r.components();
        let first = it.next().unwrap().as_os_str().to_string_lossy();
        let g = if first == "_internal" {
            it.next()
                .map(|x| x.as_os_str().to_string_lossy().into_owned())
                .unwrap_or(first.into_owned())
        } else {
            first.into_owned()
        };
        *groups.entry(g).or_default() += n
    }
    println!("Bundle: {} ({})", bundle.display(), fmt_size(total));
    if zip.exists() {
        println!(
            "Zip: {} ({})",
            zip.display(),
            fmt_size(zip.metadata().unwrap().len())
        )
    }
    println!("\nTop groups:");
    let mut gs: Vec<_> = groups.into_iter().collect();
    gs.sort_by_key(|x| std::cmp::Reverse(x.1));
    for (g, n) in gs.into_iter().take(top) {
        println!("  {:>8}  {}", fmt_size(n), g)
    }
    println!("\nTop files:");
    files.sort_by_key(|x| std::cmp::Reverse(x.0));
    for (n, p) in files.into_iter().take(top) {
        println!("  {:>8}  {}", fmt_size(n), p.display())
    }
    Ok(())
}
fn normalize(s: &str) -> String {
    pubg_ocr_engine::events::normalize_text(s)
}
fn text_digest(s: &str) -> String {
    format!("{:x}", Sha256::digest(s.as_bytes()))
}
fn semantic_signatures(
    signatures: &Value,
    profile: &pubg_ocr_engine::events::LanguageProfile,
) -> Value {
    json!(signatures.as_array().unwrap().iter().map(|event|json!({"target":event["target"],"method":event["method"],"action":event["action"],"key_sha256":event["key_sha256"],"molotov_context":pubg_ocr_engine::events::is_molotov_weapon(event["weapon"].as_str().unwrap_or(""),Some(profile))})).collect::<Vec<_>>())
}
fn event_signatures(events: &[pubg_ocr_engine::events::TextEvent]) -> Value {
    json!(events.iter().map(|e|json!({"target":e.target,"method":e.method,"action":e.action,"weapon":e.weapon,"subject_sha256":text_digest(&e.subject),"key_sha256":text_digest(&e.key),"text_sha256":text_digest(&e.text)})).collect::<Vec<_>>())
}
fn load_jsonl(path: &Path) -> Result<BTreeMap<String, Value>, String> {
    let mut out = BTreeMap::new();
    for line in fs::read_to_string(path)
        .map_err(|e| e.to_string())?
        .lines()
        .filter(|x| !x.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
        let id = match &v["frame_id"] {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            x => x.to_string(),
        };
        out.insert(id, v);
    }
    Ok(out)
}
fn compare(left: &Path, right: &Path, allow: bool) -> Result<(), String> {
    let a = load_jsonl(left)?;
    let b = load_jsonl(right)?;
    let ids: BTreeSet<_> = a.keys().chain(b.keys()).cloned().collect();
    let mut tm = 0;
    let mut am = 0;
    let mut em = 0;
    let mut tg = 0;
    let mut bm = 0;
    let mut cb = 0;
    let mut timing_deltas = Vec::new();
    for id in &ids {
        let x = a.get(id).cloned().unwrap_or_default();
        let y = b.get(id).cloned().unwrap_or_default();
        tm += (normalize(x["text"].as_str().unwrap_or(""))
            == normalize(y["text"].as_str().unwrap_or(""))) as usize;
        am += (x["action"] == y["action"]) as usize;
        em += (x["event_kind"] == y["event_kind"]) as usize;
        tg += (x["target"] == y["target"]) as usize;
        if x.get("boxes").is_some()
            && y.get("boxes").is_some()
            && (!x["boxes"].as_array().is_none_or(|v| v.is_empty())
                || !y["boxes"].as_array().is_none_or(|v| v.is_empty())
                || !allow)
        {
            cb += 1;
            bm += (x["boxes"] == y["boxes"]) as usize
        }
        if let (Some(left_ms), Some(right_ms)) =
            (x["inference_ms"].as_f64(), y["inference_ms"].as_f64())
        {
            timing_deltas.push((left_ms - right_ms).abs());
        }
    }
    let n = ids.len().max(1);
    let report = json!({"frames":ids.len(),"text_match_rate":tm as f64/n as f64,"action_match_rate":am as f64/n as f64,"event_kind_match_rate":em as f64/n as f64,"target_match_rate":tg as f64/n as f64,"box_order_match_rate":if cb>0{Some(bm as f64/cb as f64)}else{None::<f64>},"box_comparable_frames":cb,"mean_inference_delta_ms":if timing_deltas.is_empty(){None}else{Some(timing_deltas.iter().sum::<f64>()/timing_deltas.len() as f64)},"missing_from_left":b.keys().filter(|k|!a.contains_key(*k)).collect::<Vec<_>>(),"missing_from_right":a.keys().filter(|k|!b.contains_key(*k)).collect::<Vec<_>>() });
    println!("{report}");
    if report["missing_from_left"].as_array().unwrap().is_empty()
        && report["missing_from_right"].as_array().unwrap().is_empty()
        && tm == ids.len()
        && am == ids.len()
        && em == ids.len()
        && tg == ids.len()
        && (allow || (cb == ids.len() && bm == ids.len()))
    {
        Ok(())
    } else {
        Err("baseline comparison failed".into())
    }
}
fn b64(bytes: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for c in bytes.chunks(3) {
        let a = c[0];
        let b = *c.get(1).unwrap_or(&0);
        let d = *c.get(2).unwrap_or(&0);
        s.push(T[(a >> 2) as usize] as char);
        s.push(T[((a & 3) << 4 | b >> 4) as usize] as char);
        s.push(if c.len() > 1 {
            T[((b & 15) << 2 | d >> 6) as usize] as char
        } else {
            '='
        });
        s.push(if c.len() > 2 {
            T[(d & 63) as usize] as char
        } else {
            '='
        })
    }
    s
}
fn real_roi(exe: &Path, model_dir: &Path, fixture: &Path, output: &Path) -> Result<(), String> {
    let baseline = fixture.join("baseline.jsonl");
    let mut rows: Vec<Value> = fs::read_to_string(&baseline)
        .map_err(|e| e.to_string())?
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).map_err(|e| e.to_string()))
        .collect::<Result<_, _>>()?;
    if rows.is_empty() {
        return Err("real ROI baseline must not be empty".into());
    }
    // Historical annotations combined events from different frames and mislabeled
    // two languages. Keep those assets unchanged; use the audited, hash-bound
    // a070076 reference for migration parity, including documented old defects.
    let reference = fixture.join("reference.json");
    if reference.is_file() {
        let document: Value =
            serde_json::from_slice(&fs::read(&reference).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        if document["schema"] != "pubg-highlight-trim/roi-reference/v2" {
            return Err("unsupported ROI reference schema".into());
        }
        if let Some(hashes) = document["model_sha256"].as_object() {
            for (name, expected) in hashes {
                if expected.as_str() != Some(digest(&model_dir.join(name))?.as_str()) {
                    return Err(format!("ROI reference model hash mismatch for {name}"));
                }
            }
        }
        let frames = document["frames"]
            .as_array()
            .ok_or("ROI reference frames missing")?;
        if frames.len() != rows.len() {
            return Err("ROI reference frame count mismatch".into());
        }
        let mut seen = BTreeSet::new();
        for row in &mut rows {
            let id = row["frame_id"].to_string();
            if !seen.insert(id.clone()) {
                return Err(format!("duplicate ROI frame {id}"));
            }
            let matches: Vec<_> = frames
                .iter()
                .filter(|r| r["frame_id"] == row["frame_id"])
                .collect();
            if matches.len() != 1 {
                return Err(format!("missing or duplicate ROI reference frame {id}"));
            }
            let r = matches[0];
            let image = fixture.join(row["image"].as_str().ok_or("missing image")?);
            if r["image_sha256"].as_str() != Some(digest(&image)?.as_str()) {
                return Err(format!("ROI reference image hash mismatch for {id}"));
            }
            row["annotation"] = row.clone();
            row["language"] = r["language"].clone();
            row["normalized_ocr_sha256"] = r["normalized_ocr_sha256"].clone();
            row["event_signatures"] = r["event_signatures"].clone();
            row["event_kind"] = r["event_kind"].clone();
            row["target"] = r["event_signatures"][0]["target"]
                .as_str()
                .unwrap_or("")
                .into();
            row["action"] = r["event_signatures"][0]["action"]
                .as_str()
                .unwrap_or("unknown")
                .into();
        }
    }
    let metadata = fixture.join("manifest.json");
    if metadata.is_file() {
        let metadata: Value =
            serde_json::from_slice(&fs::read(metadata).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        if metadata["count"].as_u64() != Some(rows.len() as u64) {
            return Err("ROI manifest sample count mismatch".into());
        }
        let mut languages: BTreeMap<String, usize> = BTreeMap::new();
        for row in &rows {
            *languages
                .entry(
                    row["language"]
                        .as_str()
                        .ok_or("missing ROI language")?
                        .to_owned(),
                )
                .or_default() += 1;
        }
        if metadata["languages"] != serde_json::to_value(languages).map_err(|e| e.to_string())? {
            return Err("ROI manifest language coverage mismatch".into());
        }
    }
    // Construct before spawning; malformed assets must not leave a live child.
    let mut requests = Vec::new();
    for row in &rows {
        let image = image::open(fixture.join(row["image"].as_str().ok_or("missing image")?))
            .map_err(|e| e.to_string())?
            .to_rgb8();
        requests.push(format!(
            "{}\n",
            json!({"pixels_b64":b64(image.as_raw()),"width":image.width(),"height":image.height()})
        ));
    }
    let mut child = Command::new(exe)
        .arg(model_dir)
        .arg("1")
        .arg("cpu")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut input = child.stdin.take().unwrap();
    // Feed and drain simultaneously: multi-frame JSON requests/responses can
    // exceed Windows pipe capacity in either direction.
    let writer = std::thread::spawn(move || -> std::io::Result<()> {
        for request in requests {
            input.write_all(request.as_bytes())?;
        }
        Ok(())
    });
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    writer
        .join()
        .map_err(|_| "OCR request writer panicked".to_string())?
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).to_string());
    }
    let results: Vec<Value> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).map_err(|e| e.to_string()))
        .collect::<Result<_, _>>()?;
    if results.len() != rows.len() {
        return Err(format!(
            "expected {} results, got {}",
            rows.len(),
            results.len()
        ));
    }
    let mut classifications = 0;
    let mut status_ok = true;
    let mut boxes_ok = true;
    let mut lines = Vec::new();
    for (row, res) in rows.iter().zip(&results) {
        let text = res["text"].as_str().unwrap_or("");
        let expected = normalize(row["text"].as_str().unwrap_or(""));
        let actual = normalize(text);
        use pubg_ocr_engine::events::{self, Target};
        let profile = events::language_profile(
            row["language"]
                .as_str()
                .ok_or("missing baseline language")?,
        )?;
        let expected_events = if row.get("event_signatures").is_some() {
            Vec::new()
        } else {
            events::extract_text_events(
                row["text"].as_str().ok_or("missing baseline text")?,
                Target::Both,
                Some(&profile),
            )
        };
        let actual_events = events::extract_text_events(text, Target::Both, Some(&profile));
        let event = if let Some(event) = actual_events.first() {
            event.action.as_str()
        } else if events::is_assist_own_kill_text(text, Some(&profile)) {
            "assist"
        } else if events::is_delayed_own_elim_text(text, Some(&profile)) {
            "delayed-elimination"
        } else {
            "unknown"
        };
        let action = actual_events
            .first()
            .map(|e| e.action.as_str())
            .unwrap_or("unknown");
        let target = actual_events
            .first()
            .map(|e| e.target.as_str())
            .unwrap_or("");
        let expected_event = row["event_kind"]
            .as_str()
            .ok_or("missing expected event kind")?;
        let negative = ["assist", "delayed-elimination", "unknown"].contains(&expected_event);
        let actual_signatures = event_signatures(&actual_events);
        let semantic_match = if let Some(frozen) = row.get("event_signatures") {
            semantic_signatures(frozen, &profile)
                == semantic_signatures(&actual_signatures, &profile)
                && expected_event == event
        } else if negative {
            event == expected_event && actual_events.is_empty()
        } else {
            !expected_events.is_empty()
                && expected_events.iter().any(|e| {
                    Some(e.target.as_str()) == row["target"].as_str()
                        && Some(e.action.as_str()) == row["action"].as_str()
                })
                && expected_events.len() == actual_events.len()
                && expected_events
                    .iter()
                    .zip(&actual_events)
                    .all(|(a, b)| events::same_text_event(a, b))
        };
        classifications += semantic_match as usize;
        status_ok &= res["status"] == "ok";
        boxes_ok &= res["boxes"].as_array().is_some_and(|v| !v.is_empty());
        let expected_signatures = row
            .get("event_signatures")
            .cloned()
            .unwrap_or_else(|| event_signatures(&expected_events));
        let expected_text_sha256 = row
            .get("normalized_ocr_sha256")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| text_digest(&expected));
        let actual_text_sha256 = text_digest(&actual);
        lines.push(json!({"frame_id":row["frame_id"],"expected_text_sha256":expected_text_sha256,"actual_text_sha256":actual_text_sha256,"text_match":expected_text_sha256==actual_text_sha256,"expected_action":row["action"],"actual_action":action,"expected_event_kind":expected_event,"actual_event_kind":event,"expected_target":row["target"],"actual_target":target,"expected_event_signatures":expected_signatures,"actual_event_signatures":actual_signatures,"semantic_match":semantic_match,"actual_status":res["status"],"actual_boxes":res["boxes"]}))
    }
    if let Some(p) = output.parent() {
        fs::create_dir_all(p).map_err(|e| e.to_string())?
    }
    fs::write(
        output,
        lines
            .iter()
            .map(|v| v.to_string() + "\n")
            .collect::<String>(),
    )
    .map_err(|e| e.to_string())?;
    let report = json!({"frames":rows.len(),"classification_match_rate":classifications as f64/rows.len().max(1)as f64,"all_status_ok":status_ok,"all_boxes_nonempty":boxes_ok});
    println!("{report}");
    if !status_ok || !boxes_ok || classifications != rows.len() {
        Err("real ROI semantic regression or invalid OCR status/boxes; inspect the per-frame report".into())
    } else {
        Ok(())
    }
}
fn probe_text(exe: &Path, args: &[&str], path: &Path) -> Result<String, String> {
    let o = Command::new(exe)
        .args(args)
        .arg(path)
        .output()
        .map_err(|e| e.to_string())?;
    if !o.status.success() {
        return Err(String::from_utf8_lossy(&o.stderr).to_string());
    }
    Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
}
fn frame_bench(
    ffmpeg: &Path,
    ffprobe: &Path,
    av1: &Path,
    h264: &Path,
    repeat: usize,
) -> Result<(), String> {
    let dims = probe_text(
        ffprobe,
        &[
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "csv=p=0",
        ],
        av1,
    )?;
    let mut di = dims.split(',');
    let w: usize = di
        .next()
        .ok_or("missing width")?
        .parse()
        .map_err(|_| "invalid width")?;
    let h: usize = di
        .next()
        .ok_or("missing height")?
        .parse()
        .map_err(|_| "invalid height")?;
    let dur: f64 = probe_text(
        ffprobe,
        &[
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=nw=1:nk=1",
        ],
        av1,
    )?
    .parse()
    .map_err(|_| "invalid duration")?;
    let mut times = Vec::new();
    let mut t = 4.0;
    while t < dur.trunc() - 2.0 {
        times.push(t);
        t += 4.0
    }
    if times.is_empty() {
        return Err("duration too short".into());
    }
    let anchor = times[times.len() / 2];
    times.extend(
        [-2.0, -4.0, -6.0, -4.5, -5.0, -3.5, 0.0, 1.0, 1.5, 2.0].map(|offset| anchor + offset),
    );
    if repeat == 0 {
        return Err("repeat must be positive".into());
    }
    let variants: Vec<(&str, Vec<&str>)> = vec![
        ("sw", vec![]),
        ("hwaccel-cuda", vec!["-hwaccel", "cuda"]),
        ("hwaccel-d3d11va", vec!["-hwaccel", "d3d11va"]),
        ("hwaccel-d3d12va", vec!["-hwaccel", "d3d12va"]),
        ("av1_cuvid", vec!["-c:v", "av1_cuvid"]),
        ("h264_cuvid", vec!["-c:v", "h264_cuvid"]),
    ];
    let mut report = Vec::new();
    for &(label, path) in [("AV1", av1), ("H264", h264)].iter() {
        for &(name, ref extra) in &variants {
            if (label == "AV1" && name == "h264_cuvid") || (label == "H264" && name == "av1_cuvid")
            {
                continue;
            }
            let grab = |sec: f64| -> Result<std::process::Output, String> {
                Command::new(ffmpeg)
                    .args(["-hide_banner", "-loglevel", "error"])
                    .args(extra)
                    .arg("-ss")
                    .arg(format!("{sec:.3}"))
                    .arg("-i")
                    .arg(path)
                    .args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "bgr24", "-"])
                    .output()
                    .map_err(|e| e.to_string())
            };
            let _ = grab(times[0])?; // Prior tool performs one unmeasured warmup per variant.
            let mut best: Option<Value> = None;
            for _ in 0..repeat {
                let mut samples = Vec::new();
                let mut bad = 0;
                for sec in &times {
                    let start = std::time::Instant::now();
                    let o = grab(*sec)?;
                    samples.push(start.elapsed().as_secs_f64() * 1000.0);
                    if !o.status.success() || o.stdout.len() != w * h * 3 {
                        bad += 1
                    }
                }
                samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
                let total = samples.iter().sum::<f64>();
                let mid = samples.len() / 2;
                let median = if samples.len() % 2 == 0 {
                    (samples[mid - 1] + samples[mid]) / 2.0
                } else {
                    samples[mid]
                };
                let row = json!({"label":label,"name":name,"n":samples.len(),"bad":bad,"total_ms":total,"mean_ms":total/samples.len() as f64,"median_ms":median,"p90_ms":samples[samples.len()*9/10],"repeat":repeat,"warmup_reads":1,"schedule":times});
                if best
                    .as_ref()
                    .is_none_or(|b| total < b["total_ms"].as_f64().unwrap())
                {
                    best = Some(row);
                }
            }
            report.push(best.unwrap());
        }
    }
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
    Ok(())
}
fn main() -> Result<(), String> {
    match Cli::parse().command {
        CommandKind::Verify {
            model_dir,
            manifest,
            check_onnx,
            engine,
            timeout_secs,
        } => {
            verify(&model_dir, &manifest, check_onnx)?;
            if check_onnx {
                onnx_smoke(&model_dir, Some(engine.as_deref().ok_or("--check-onnx requires --engine for full session loading and real det/rec inference")?), timeout_secs)?;
            }
            Ok(())
        }
        CommandKind::Download {
            output_dir,
            manifest,
        } => download(&output_dir, &manifest),
        CommandKind::Dictionary {
            source,
            output,
            expected_sha256,
        } => dictionary(&source, &output, expected_sha256.as_deref()),
        CommandKind::OnnxSmoke {
            model_dir,
            engine,
            timeout_secs,
        } => onnx_smoke(&model_dir, engine.as_deref(), timeout_secs),
        CommandKind::Licenses {
            bundle,
            manifest_path,
        } => licenses::run(
            &bundle,
            &manifest_path
                .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../Cargo.toml")),
        ),
        CommandKind::Manifest { bundle, output } => manifest(&bundle, output.as_deref()),
        CommandKind::Audit { bundle } => audit(&bundle),
        CommandKind::Size { bundle, zip, top } => size(&bundle, &zip, top),
        CommandKind::Compare {
            left,
            right,
            allow_missing_boxes,
        } => compare(&left, &right, allow_missing_boxes),
        CommandKind::RealRoi {
            executable,
            model_dir,
            fixture,
            output,
        } => real_roi(&executable, &model_dir, &fixture, &output),
        CommandKind::BuildRealRoi {
            record,
            media_root,
            output,
            limit_per_language,
            ffmpeg,
        } => build_real_roi::run(&record, &media_root, &output, limit_per_language, &ffmpeg),
        CommandKind::FrameBench {
            ffmpeg,
            ffprobe,
            av1,
            h264,
            repeat,
        } => frame_bench(&ffmpeg, &ffprobe, &av1, &h264, repeat),
    }
}
