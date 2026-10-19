use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "xtask-retirement-{name}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}
fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn comparator_preserves_timing_box_order_and_missing_frame_checks() {
    let dir = temp("compare");
    let left = dir.join("left.jsonl");
    let right = dir.join("right.jsonl");
    fs::write(
        &left,
        "{\"frame_id\":1,\"text\":\"YOU KNOCKED OUT\",\"boxes\":[[1,2]],\"inference_ms\":10}\n",
    )
    .unwrap();
    fs::write(
        &right,
        "{\"frame_id\":1,\"text\":\"YOU   KNOCKED OUT\",\"boxes\":[[1,2]],\"inference_ms\":12}\n",
    )
    .unwrap();
    let output = run(&["compare", left.to_str().unwrap(), right.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["text_match_rate"], 1.0);
    assert_eq!(report["box_order_match_rate"], 1.0);
    assert_eq!(report["mean_inference_delta_ms"], 2.0);
    fs::write(&right, "").unwrap();
    let output = run(&["compare", left.to_str().unwrap(), right.to_str().unwrap()]);
    assert!(!output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["missing_from_right"], json!(["1"]));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
#[ignore = "requires HTTPS access to pinned model source; mandatory after model provisioning in CI"]
fn bad_https_download_preserves_existing_asset_and_leaves_no_partial_file() {
    let dir = temp("download");
    // Use the already-controlled pinned upstream asset. HTTP test servers are
    // deliberately rejected before networking and cannot exercise hash failure.
    let pinned: Value =
        serde_json::from_str(include_str!("../../ocr_engine/models/pp_ocrv6_medium.json")).unwrap();
    let asset =
        json!({"file":"det.onnx","source":pinned["dictionary"]["source"],"sha256":"0".repeat(64)});
    let manifest = dir.join("manifest.json");
    fs::write(&manifest, serde_json::to_vec(&json!({"model_version":"fixture","detector":asset,"recognizer":asset,"dictionary":asset})).unwrap()).unwrap();
    let models = dir.join("models");
    fs::create_dir(&models).unwrap();
    fs::write(models.join("det.onnx"), b"existing verified content").unwrap();
    let output = run(&[
        "download",
        "--output-dir",
        models.to_str().unwrap(),
        "--manifest",
        manifest.to_str().unwrap(),
    ]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("sha256 mismatch"));
    assert_eq!(
        fs::read(models.join("det.onnx")).unwrap(),
        b"existing verified content"
    );
    assert!(!models.join("det.onnx.part").exists());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn audit_checks_nested_zip_modules_and_manifest_hashes_actual_files() {
    let dir = temp("bundle");
    fs::write(dir.join("pubg-highlight-trim.exe"), b"app").unwrap();
    fs::create_dir(dir.join("z-models")).unwrap();
    fs::write(dir.join("z-models/rec.onnx"), b"model").unwrap();
    let output = run(&["manifest", dir.to_str().unwrap()]);
    assert!(output.status.success());
    let manifest: Value =
        serde_json::from_slice(&fs::read(dir.join("SUPPLY_CHAIN_MANIFEST.json")).unwrap()).unwrap();
    assert_eq!(manifest["files"][0]["bytes"], 3);
    assert_eq!(
        manifest["files"][0]["sha256"],
        "a172cedcae47474b615c54d510a5d84a8dea3032e958587430b413538be3f333"
    );
    assert_eq!(manifest["files"][1]["path"], "z-models/rec.onnx");
    let names: Vec<_> = manifest["components"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["name"].as_str().unwrap())
        .collect();
    assert!(!names.iter().any(|name| [
        "CPython",
        "NumPy",
        "OpenCV contrib",
        "PyInstaller bootloader"
    ]
    .contains(name)));
    let archive = fs::File::create(dir.join("modules.zip")).unwrap();
    let mut zip = zip::ZipWriter::new(archive);
    zip.start_file(
        "paddlex/__init__.pyc",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    zip.write_all(b"bad").unwrap();
    zip.finish().unwrap();
    let output = run(&["audit", dir.to_str().unwrap()]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("modules.zip!paddlex/__init__.pyc"));
    fs::remove_dir_all(dir).unwrap();
}
