use serde_json::json;
use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn temp(name: &str) -> std::path::PathBuf {
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!("xtask-{name}-{id}"));
    fs::create_dir_all(&p).unwrap();
    p
}
fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(args)
        .output()
        .unwrap()
}
fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

#[test]
fn verify_checks_hashes_and_dictionary_contract() {
    let dir = temp("verify");
    let dictionary = (0..120).map(|i| format!("symbol{i}\n")).collect::<String>();
    let files = [
        ("det.onnx", b"det".as_slice()),
        ("rec.onnx", b"rec".as_slice()),
        ("dict.txt", dictionary.as_bytes()),
    ];
    for (name, bytes) in files {
        fs::write(dir.join(name), bytes).unwrap();
    }
    let manifest = dir.join("manifest.json");
    let doc = json!({"model_version":"fixture","detector":{"file":"det.onnx","source":"file:///det","sha256":sha256(b"det")},"recognizer":{"file":"rec.onnx","source":"file:///rec","sha256":sha256(b"rec")},"dictionary":{"file":"dict.txt","source":"file:///dict","sha256":sha256(dictionary.as_bytes())}});
    fs::write(&manifest, serde_json::to_vec(&doc).unwrap()).unwrap();
    let out = run(&[
        "verify",
        "--model-dir",
        dir.to_str().unwrap(),
        "--manifest",
        manifest.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let repeated = "a\n".repeat(120);
    fs::write(dir.join("dict.txt"), &repeated).unwrap();
    let mut doc = doc;
    doc["dictionary"]["sha256"] = sha256(repeated.as_bytes()).into();
    fs::write(&manifest, serde_json::to_vec(&doc).unwrap()).unwrap();
    let out = run(&[
        "verify",
        "--model-dir",
        dir.to_str().unwrap(),
        "--manifest",
        manifest.to_str().unwrap(),
    ]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("1 symbols"));

    fs::write(dir.join("det.onnx"), b"corrupt").unwrap();
    fs::remove_file(dir.join("rec.onnx")).unwrap();
    let out = run(&[
        "verify",
        "--model-dir",
        dir.to_str().unwrap(),
        "--manifest",
        manifest.to_str().unwrap(),
    ]);
    assert!(!out.status.success());
    let errors = String::from_utf8_lossy(&out.stderr);
    assert!(errors.contains("sha256 mismatch"));
    assert!(errors.contains("missing recognizer"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn audit_rejects_paddle_residue_even_inside_zip() {
    let dir = temp("audit");
    fs::write(dir.join("paddleocr.dll"), b"bad").unwrap();
    let out = run(&["audit", dir.to_str().unwrap()]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("paddleocr.dll"));
}

#[test]
fn manifest_is_stable_and_audit_rejects_bad_zip() {
    let dir = temp("manifest");
    fs::write(dir.join("app.exe"), b"app").unwrap();
    let out1 = run(&["manifest", dir.to_str().unwrap()]);
    assert!(out1.status.success());
    let first = fs::read_to_string(dir.join("SUPPLY_CHAIN_MANIFEST.json")).unwrap();
    let out2 = run(&["manifest", dir.to_str().unwrap()]);
    assert!(out2.status.success());
    assert_eq!(
        first,
        fs::read_to_string(dir.join("SUPPLY_CHAIN_MANIFEST.json")).unwrap()
    );
    fs::write(dir.join("broken.zip"), b"not a zip").unwrap();
    let audit = run(&["audit", dir.to_str().unwrap()]);
    assert!(!audit.status.success());
    assert!(String::from_utf8_lossy(&audit.stderr).contains("invalid zip"));
}

#[test]
fn download_rejects_non_https_sources_without_touching_output() {
    let dir = temp("source");
    let manifest = dir.join("manifest.json");
    let doc = json!({"model_version":"fixture","detector":{"file":"det.onnx","source":"file:///det","sha256":sha256(b"det")},"recognizer":{"file":"rec.onnx","source":"file:///rec","sha256":sha256(b"rec")},"dictionary":{"file":"dict.txt","source":"http://example.invalid/dict","sha256":sha256(b"dict")}});
    fs::write(&manifest, serde_json::to_vec(&doc).unwrap()).unwrap();
    let out = run(&[
        "download",
        "--output-dir",
        dir.to_str().unwrap(),
        "--manifest",
        manifest.to_str().unwrap(),
    ]);
    assert!(!out.status.success());
    assert!(!dir.join("det.onnx").exists());
}

#[test]
fn download_failure_preserves_existing_file_and_rejects_path_escape() {
    let dir = temp("download");
    let old = dir.join("det.onnx");
    fs::write(&old, b"old-content").unwrap();
    let manifest = dir.join("manifest.json");
    let doc = json!({"model_version":"fixture","detector":{"file":"det.onnx","source":"http://127.0.0.1:1/unreachable","sha256":sha256(b"new-content")},"recognizer":{"file":"rec.onnx","source":"http://127.0.0.1:1/unreachable","sha256":sha256(b"rec")},"dictionary":{"file":"dict.txt","source":"http://127.0.0.1:1/unreachable","sha256":sha256(b"dict")}});
    fs::write(&manifest, serde_json::to_vec(&doc).unwrap()).unwrap();
    let out = run(&[
        "download",
        "--output-dir",
        dir.to_str().unwrap(),
        "--manifest",
        manifest.to_str().unwrap(),
    ]);
    assert!(!out.status.success());
    assert_eq!(fs::read(&old).unwrap(), b"old-content");
    let escape = dir.join("escape.json");
    let escaped = json!({"model_version":"fixture","detector":{"file":"../outside.onnx","source":"http://127.0.0.1:1/unreachable","sha256":sha256(b"x")},"recognizer":{"file":"rec.onnx","source":"http://127.0.0.1:1/unreachable","sha256":sha256(b"x")},"dictionary":{"file":"dict.txt","source":"http://127.0.0.1:1/unreachable","sha256":sha256(b"x")}});
    fs::write(&escape, serde_json::to_vec(&escaped).unwrap()).unwrap();
    let escaped_out = run(&[
        "download",
        "--output-dir",
        dir.to_str().unwrap(),
        "--manifest",
        escape.to_str().unwrap(),
    ]);
    assert!(!escaped_out.status.success());
    assert!(!dir.parent().unwrap().join("outside.onnx").exists());
}

#[test]
fn rust_bundle_audit_keeps_all_native_ffmpeg_assets_unchanged() {
    let dir = temp("prune");
    let vendor = dir.join("_internal/vendor/ffmpeg");
    fs::create_dir_all(&vendor).unwrap();
    fs::write(vendor.join("same.dll"), b"same").unwrap();
    fs::write(dir.join("_internal/same.dll"), b"same").unwrap();
    fs::write(vendor.join("different.dll"), b"vendor").unwrap();
    fs::write(dir.join("_internal/different.dll"), b"top").unwrap();
    // The Rust release is assembled once, not cleaned from a Python freezer
    // tree. Its audit is deliberately read-only: it must never delete a native
    // runtime merely because another DLL shares its filename.
    let audit = run(&["audit", dir.to_str().unwrap()]);
    assert!(audit.status.success());
    assert_eq!(fs::read(dir.join("_internal/same.dll")).unwrap(), b"same");
    assert_eq!(
        fs::read(dir.join("_internal/different.dll")).unwrap(),
        b"top"
    );
    assert_eq!(fs::read(vendor.join("same.dll")).unwrap(), b"same");
    assert_eq!(fs::read(vendor.join("different.dll")).unwrap(), b"vendor");
}

#[test]
fn manifest_filters_components_to_shipped_files() {
    let dir = temp("components");
    fs::write(dir.join("pubg-highlight-trim.exe"), b"app").unwrap();
    let out = run(&["manifest", dir.to_str().unwrap()]);
    assert!(out.status.success());
    let doc: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.join("SUPPLY_CHAIN_MANIFEST.json")).unwrap()).unwrap();
    let names: Vec<&str> = doc["components"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["name"].as_str())
        .collect();
    assert!(names.contains(&"pubg-highlight-trim"));
    assert!(names.contains(&"DirectML"));
    assert!(!names.contains(&"pubg-ocr-engine"));
    assert!(!names.contains(&"FFmpeg / FFprobe"));
}

#[test]
fn real_roi_reports_missing_executable_without_claiming_inference() {
    let dir = temp("real-roi-error");
    let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../rust/ocr_engine/fixtures/real");
    let out = run(&[
        "real-roi",
        "--executable",
        dir.join("missing.exe").to_str().unwrap(),
        "--model-dir",
        dir.to_str().unwrap(),
        "--fixture",
        fixture.to_str().unwrap(),
        "--output",
        dir.join("out.jsonl").to_str().unwrap(),
    ]);
    assert!(!out.status.success());
    assert!(!dir.join("out.jsonl").exists());
}

#[test]
fn compare_reports_classification_mismatch_and_nonzero_exit() {
    let dir = temp("compare");
    let left = dir.join("left.jsonl");
    let right = dir.join("right.jsonl");
    fs::write(&left, "{\"frame_id\":1,\"text\":\"A B\",\"action\":\"kill\",\"event_kind\":\"own\",\"target\":\"x\",\"boxes\":[]}\n").unwrap();
    fs::write(&right, "{\"frame_id\":1,\"text\":\"AB\",\"action\":\"miss\",\"event_kind\":\"own\",\"target\":\"x\",\"boxes\":[]}\n").unwrap();
    let out = run(&[
        "compare",
        left.to_str().unwrap(),
        right.to_str().unwrap(),
        "--allow-missing-boxes",
    ]);
    assert!(!out.status.success());
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["text_match_rate"], 1.0);
    assert_eq!(report["action_match_rate"], 0.0);
}
