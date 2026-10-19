use std::{fs, process::Command};
#[test]
fn builder_samples_local_csv_prefers_originals_and_writes_cropped_baseline() {
    let d = std::env::temp_dir().join(format!("xtask-builder-{}", std::process::id()));
    fs::create_dir_all(d.join("media/original")).unwrap();
    fs::create_dir_all(d.join("media/output")).unwrap();
    fs::write(d.join("media/original/sample.mp4"), b"original").unwrap();
    fs::write(d.join("media/output/sample.mp4"), b"derived").unwrap();
    let record = d.join("record.csv");
    fs::write(&record,"\u{feff}Status,PaddleText,Target,Name,EventSec,EventKeys\r\nincluded,你用M416击倒了敌人,own-kill,sample.mp4,4.5,\r\nincluded,你获得助攻,own-kill,sample.mp4,5.5,\r\nexcluded,noise,self-death,sample.mp4,6,\r\n").unwrap();
    let src = d.join("fake.rs");
    let exe = d.join("fake.exe");
    fs::write(&src,r#"fn main(){use std::io::Write; let args:Vec<_>=std::env::args().collect(); let path=&args[args.iter().position(|x|x=="-i").unwrap()+1]; assert!(path.contains("original")); std::io::stdout().write_all(b"P6\n100 100\n255\n").unwrap(); std::io::stdout().write_all(&vec![128;30000]).unwrap();}"#).unwrap();
    assert!(Command::new("rustc")
        .arg(src)
        .arg("-o")
        .arg(&exe)
        .status()
        .unwrap()
        .success());
    let output = d.join("roi");
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args([
            "build-real-roi",
            "--record",
            record.to_str().unwrap(),
            "--media-root",
            d.join("media").to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--limit-per-language",
            "2",
            "--ffmpeg",
            exe.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let row: serde_json::Value = serde_json::from_str(
        fs::read_to_string(output.join("baseline.jsonl"))
            .unwrap()
            .trim(),
    )
    .unwrap();
    assert!(row.get("text").is_none());
    assert_eq!(row["schema"], "reference-v2");
    assert_eq!(
        row["normalized_ocr_sha256"],
        "31536ddf6c6c787c9fc761a07328317b8eb417358f211536809bf05cef8f38c8"
    );
    assert_eq!(row["event_signatures"], serde_json::json!([]));
    assert_eq!(row["event_kind"], "assist");
    assert_eq!(row["event_sec"], 5.5);
    assert_eq!(row["source"], "source_001");
    assert_eq!(
        row["source_sha256"],
        "0682c5f2076f099c34cfdd15a9e063849ed437a49677e6fcc5b4198c76575be5"
    );
    let metadata = fs::read_to_string(output.join("baseline.jsonl")).unwrap();
    for private in ["sample.mp4", "你获得助攻", d.to_str().unwrap()] {
        assert!(!metadata.contains(private));
        assert!(!String::from_utf8_lossy(&out.stdout).contains(private));
        assert!(!String::from_utf8_lossy(&out.stderr).contains(private));
    }
    let image = image::open(output.join("frame_001.jpg")).unwrap();
    assert_eq!((image.width(), image.height()), (40, 9));
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(output.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["count"], 1);
    assert_eq!(manifest["languages"]["zh-Hans"], 1);
    assert_eq!(
        fs::read(d.join("media/original/sample.mp4")).unwrap(),
        b"original"
    );
}

#[test]
fn builder_v2_event_fingerprints_keep_subject_semantic_checks_without_raw_identity() {
    let d = std::env::temp_dir().join(format!("xtask-builder-v2-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(d.join("media")).unwrap();
    fs::write(d.join("media/private-source.mp4"), b"synthetic-media").unwrap();
    let record = d.join("record.csv");
    fs::write(
        &record,
        "Status,PaddleText,Target,Name,EventSec,EventKeys
included,你用M416击倒了EnemyA,own-kill,private-source.mp4,4.5,
",
    )
    .unwrap();
    let src = d.join("helper.rs");
    let helper = d.join("helper.exe");
    fs::write(&src,r#"fn main(){use std::io::{BufRead,Write};if std::env::args().any(|a|a=="image2pipe"){std::io::stdout().write_all(b"P6
100 100
255
").unwrap();std::io::stdout().write_all(&vec![128;30000]).unwrap();}else{for _ in std::io::stdin().lock().lines(){println!("{}",std::env::var("RESPONSE").unwrap());}}}"#).unwrap();
    assert!(Command::new("rustc")
        .arg(&src)
        .arg("-o")
        .arg(&helper)
        .status()
        .unwrap()
        .success());
    let fixture = d.join("fixture");
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["build-real-roi", "--record"])
        .arg(record)
        .arg("--media-root")
        .arg(d.join("media"))
        .arg("--output")
        .arg(&fixture)
        .arg("--ffmpeg")
        .arg(&helper)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let baseline = fs::read_to_string(fixture.join("baseline.jsonl")).unwrap();
    let row: serde_json::Value = serde_json::from_str(baseline.trim()).unwrap();
    assert_eq!(row["event_signatures"].as_array().unwrap().len(), 1);
    assert!(!baseline.contains("EnemyA"));
    assert!(!baseline.contains("private-source"));
    for (text, ok) in [
        ("你用M416击倒了EnemyA", true),
        ("你用M416击倒了EnemyB", false),
    ] {
        let response =
            serde_json::json!({"text":text,"status":"ok","boxes":[[0,0,12,12]],"scores":[0.99]});
        let run = Command::new(env!("CARGO_BIN_EXE_xtask"))
            .args(["real-roi", "--executable"])
            .arg(&helper)
            .arg("--model-dir")
            .arg(&d)
            .arg("--fixture")
            .arg(&fixture)
            .arg("--output")
            .arg(d.join("result.jsonl"))
            .env("RESPONSE", response.to_string())
            .output()
            .unwrap();
        assert_eq!(
            run.status.success(),
            ok,
            "{}",
            String::from_utf8_lossy(&run.stderr)
        );
    }
    fs::remove_dir_all(d).unwrap();
}

#[test]
fn builder_decode_failure_reports_synthetic_sample_not_private_stderr() {
    let d = std::env::temp_dir().join(format!("xtask-builder-errors-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(d.join("media")).unwrap();
    fs::write(d.join("media/private-source.mp4"), b"bad").unwrap();
    fs::write(
        d.join("record.csv"),
        "Status,PaddleText,Target,Name,EventSec,EventKeys
included,你用M416击倒了EnemyA,own-kill,private-source.mp4,4.5,
",
    )
    .unwrap();
    fs::write(d.join("helper.rs"),r#"fn main(){eprintln!("private-source.mp4: private decoder path and EnemyA"); std::process::exit(1);}"#).unwrap();
    assert!(Command::new("rustc")
        .arg(d.join("helper.rs"))
        .arg("-o")
        .arg(d.join("helper.exe"))
        .status()
        .unwrap()
        .success());
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["build-real-roi", "--record"])
        .arg(d.join("record.csv"))
        .arg("--media-root")
        .arg(d.join("media"))
        .arg("--output")
        .arg(d.join("fixture"))
        .arg("--ffmpeg")
        .arg(d.join("helper.exe"))
        .output()
        .unwrap();
    assert!(!out.status.success());
    let log = String::from_utf8_lossy(&out.stderr);
    assert!(log.contains("skip sample 1: decoder failed"));
    for private in ["private-source.mp4", "EnemyA", d.to_str().unwrap()] {
        assert!(!log.contains(private));
    }
    fs::remove_dir_all(d).unwrap();
}
