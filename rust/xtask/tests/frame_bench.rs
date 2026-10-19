use serde_json::Value;
use std::{fs, process::Command};
#[test]
fn benchmark_preserves_backward_refinement_warmup_and_best_repeat_metrics() {
    let d = std::env::temp_dir().join(format!("xtask-frame-{}", std::process::id()));
    fs::create_dir_all(&d).unwrap();
    let src = d.join("fixture.rs");
    let exe = d.join("fixture.exe");
    fs::write(&src,r#"fn main(){let a:Vec<_>=std::env::args().collect(); if a.iter().any(|x|x=="stream=width,height"){println!("1,1");} else if a.iter().any(|x|x=="format=duration"){println!("26.9");} else {use std::io::{Write,stdout}; let p=std::env::var("FRAME_LOG").unwrap(); let sec=&a[a.iter().position(|x|x=="-ss").unwrap()+1]; writeln!(std::fs::OpenOptions::new().create(true).append(true).open(p).unwrap(),"{sec}").unwrap(); stdout().write_all(&[0,0,0]).unwrap();}}"#).unwrap();
    assert!(Command::new("rustc")
        .arg(&src)
        .arg("-o")
        .arg(&exe)
        .status()
        .unwrap()
        .success());
    let log = d.join("log.txt");
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .env("FRAME_LOG", &log)
        .args([
            "frame-bench",
            "--ffmpeg",
            exe.to_str().unwrap(),
            "--ffprobe",
            exe.to_str().unwrap(),
            "--av1",
            "AV1.mp4",
            "--h264",
            "H264.mp4",
            "--repeat",
            "2",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    let row = &report[0];
    assert_eq!(row["n"], 15);
    assert_eq!(row["bad"], 0);
    assert!(row["total_ms"].as_f64().unwrap() > 0.0);
    assert_eq!(row["repeat"], 2);
    assert_eq!(row["warmup_reads"], 1);
    let log = fs::read_to_string(log).unwrap();
    let times: Vec<_> = log.lines().take(16).collect();
    assert_eq!(
        times,
        vec![
            "4.000", "4.000", "8.000", "12.000", "16.000", "20.000", "10.000", "8.000", "6.000",
            "7.500", "7.000", "8.500", "12.000", "13.000", "13.500", "14.000"
        ]
    );
}
