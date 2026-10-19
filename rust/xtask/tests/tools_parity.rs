use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};
fn temp() -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "xtask-parity-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&p).unwrap();
    p
}
fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(args)
        .output()
        .unwrap()
}
fn fixture_exe(dir: &Path, code: &str) -> PathBuf {
    let src = dir.join("fake.rs");
    let exe = dir.join("fake.exe");
    fs::write(&src, code).unwrap();
    assert!(Command::new("rustc")
        .args(["--edition=2021", "-o"])
        .arg(&exe)
        .arg(&src)
        .status()
        .unwrap()
        .success());
    exe
}
#[test]
fn smoke_rejects_status_only_and_enforces_timeout() {
    let d = temp();
    let exe = fixture_exe(
        &d,
        r#"fn main(){use std::io::Read; let mut s=String::new(); std::io::stdin().read_to_string(&mut s).unwrap(); if std::env::var_os("FAKE_SLEEP").is_some(){std::thread::sleep(std::time::Duration::from_secs(30));} if std::env::var_os("FAKE_BAD_JSON").is_some(){println!("not-json");return;} println!("{{\"status\":\"ok\",\"provider\":\"cpu\",\"text\":\"\",\"boxes\":[]}}");}"#,
    );
    let out = run(&[
        "onnx-smoke",
        "--model-dir",
        d.to_str().unwrap(),
        "--engine",
        exe.to_str().unwrap(),
    ]);
    assert!(!out.status.success(), "must reject status-only empty OCR");
    let malformed = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .env("FAKE_BAD_JSON", "1")
        .args([
            "onnx-smoke",
            "--model-dir",
            d.to_str().unwrap(),
            "--engine",
            exe.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!malformed.status.success());
    let start = std::time::Instant::now();
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .env("FAKE_SLEEP", "1")
        .args([
            "onnx-smoke",
            "--model-dir",
            d.to_str().unwrap(),
            "--engine",
            exe.to_str().unwrap(),
            "--timeout-secs",
            "1",
        ])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("timed out"));
    assert!(start.elapsed().as_secs() < 10);
}
#[test]
fn verify_rejects_malformed_protobuf_duplicate_and_invalid_utf8_dictionary() {
    use sha2::{Digest, Sha256};
    let d = temp();
    let bad = b"not an ONNX protobuf";
    fs::write(d.join("det.onnx"), bad).unwrap();
    fs::write(d.join("rec.onnx"), bad).unwrap();
    for dict in [b"a\n".repeat(120), vec![255; 120]] {
        fs::write(d.join("dict.txt"), &dict).unwrap();
        let asset = |file: &str, bytes: &[u8]| json!({"file":file,"source":"https://example.invalid/model","sha256":format!("{:x}",Sha256::digest(bytes))});
        let m = json!({"model_version":"negative","detector":asset("det.onnx",bad),"recognizer":asset("rec.onnx",bad),"dictionary":asset("dict.txt",&dict)});
        let manifest = d.join("manifest.json");
        fs::write(&manifest, m.to_string()).unwrap();
        let out = run(&[
            "verify",
            "--model-dir",
            d.to_str().unwrap(),
            "--manifest",
            manifest.to_str().unwrap(),
            "--check-onnx",
        ]);
        assert!(!out.status.success());
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains("protobuf decode failed"));
        assert!(err.contains("dictionary"));
    }
}
#[test]
fn dictionary_uncontrolled_source_does_not_replace_existing_output() {
    let d = temp();
    let out = d.join("dict.txt");
    fs::write(&out, b"existing").unwrap();
    let result = run(&[
        "dictionary",
        "--source",
        "http://127.0.0.1:1/not-allowed",
        "--output",
        out.to_str().unwrap(),
    ]);
    assert!(!result.status.success());
    assert_eq!(fs::read(out).unwrap(), b"existing");
}
