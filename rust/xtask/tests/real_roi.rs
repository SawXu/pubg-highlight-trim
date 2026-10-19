use serde_json::{json, Value};
use std::{fs, path::PathBuf, process::Command};
struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn real_roi_classifies_actual_events_and_rejects_subject_or_target_regressions() {
    let root = Temp(std::env::temp_dir().join(format!(
            "pubg-roi-gate-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )));
    fs::create_dir(&root.0).unwrap();
    image::RgbImage::new(16, 16)
        .save(root.0.join("frame.png"))
        .unwrap();
    let baseline = json!({"frame_id":1,"image":"frame.png","language":"zh-Hans","text":"你用M416击倒了EnemyA","action":"knock","event_kind":"knock","target":"own-kill","boxes":[]});
    fs::write(root.0.join("baseline.jsonl"), format!("{baseline}\n")).unwrap();
    let program = root.0.join("engine.rs");
    fs::write(&program,r#"use std::io::{self,BufRead,Write};fn main(){for _ in io::stdin().lock().lines(){println!("{}",std::env::var("MOCK_OCR_RESPONSE").unwrap());io::stdout().flush().unwrap();}}"#).unwrap();
    let engine = root
        .0
        .join(format!("engine{}", std::env::consts::EXE_SUFFIX));
    let compile = Command::new("rustc")
        .arg(&program)
        .arg("-o")
        .arg(&engine)
        .output()
        .unwrap();
    assert!(
        compile.status.success(),
        "{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    for (text, ok, target) in [
        ("你用M416击倒了EnemyB", false, "own-kill"),
        ("EnemyA用M416击倒了你", false, "self-death"),
        ("你 用M416 击倒了EnemyA", true, "own-kill"),
    ] {
        let output = root.0.join("report.jsonl");
        let response = json!({"text":text,"status":"ok","provider":"cpu","boxes":[[0,0,12,12]],"scores":[0.99]});
        let run = Command::new(env!("CARGO_BIN_EXE_xtask"))
            .args(["real-roi", "--executable"])
            .arg(&engine)
            .arg("--model-dir")
            .arg(&root.0)
            .arg("--fixture")
            .arg(&root.0)
            .arg("--output")
            .arg(&output)
            .env("MOCK_OCR_RESPONSE", response.to_string())
            .output()
            .unwrap();
        assert_eq!(
            run.status.success(),
            ok,
            "text={text}; {}",
            String::from_utf8_lossy(&run.stderr)
        );
        let row: Value =
            serde_json::from_str(fs::read_to_string(&output).unwrap().lines().next().unwrap())
                .unwrap();
        assert_eq!(row["actual_target"], target);
        assert_eq!(row["semantic_match"], ok);
    }
}
