use pubg_ocr_engine::planning::{plan_clips, run_jsonl, PlanningRequest, PlanningResult};
use serde::Deserialize;

#[derive(Deserialize)]
struct Case {
    name: String,
    request: PlanningRequest,
    expected: PlanningResult,
}

#[test]
fn jsonl_preserves_request_order_and_prior_responses_on_error() {
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../fixtures/planning.json")).unwrap();
    let mut input = String::new();
    for case in &cases {
        input.push_str(&format!("{}\r\n", case["request"]));
    }
    input.push_str("not json\n");
    let mut output = Vec::new();
    assert!(run_jsonl(input.as_bytes(), &mut output).is_err());
    let responses: Vec<PlanningResult> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let expected: Vec<PlanningResult> = cases
        .iter()
        .map(|case| serde_json::from_value(case["expected"].clone()).unwrap())
        .collect();
    assert_eq!(responses, expected);
}

#[test]
fn json_transport_preserves_python_float_precision() {
    let input = r#"{"config":{"seconds_before":0,"seconds_after":0.40933575603594796,"min_event_sec":0},"detections":[{"duration_sec":60,"event_sec":21.381554704208238,"method":"ocr","detector":"ocr"}]}"#;
    let request: PlanningRequest = serde_json::from_str(input).unwrap();
    let result = plan_clips(request);
    assert_eq!(
        result.clips[0].detection.event_sec,
        Some(21.381554704208238_f64)
    );
    assert_eq!(
        result.clips[0].detection.clip_after_sec,
        0.40933575603594796_f64
    );
}

#[test]
fn empty_or_invalid_requests_never_look_like_no_events() {
    for input in [
        "",
        "not json\n",
        "{}\n",
        "{\"config\":{},\"detections\":[]}\n",
    ] {
        let mut output = Vec::new();
        assert!(run_jsonl(input.as_bytes(), &mut output).is_err(), "{input}");
        assert!(output.is_empty());
    }
}

#[cfg(any(feature = "cli", feature = "highlight-cli"))]
#[test]
fn canonical_planner_runs_without_models_and_keeps_stdout_clean() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../fixtures/planning.json")).unwrap();
    let input = cases
        .iter()
        .map(|case| format!("{}\n", case["request"]))
        .collect::<String>();
    let mut child = Command::new(env!("CARGO_BIN_EXE_highlight_plan"))
        .env("PUBG_OCR_MODEL_DIR", "nonexistent-models")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let writer = std::thread::spawn(move || stdin.write_all(input.as_bytes()).unwrap());
    let output = child.wait_with_output().unwrap();
    writer.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let responses: Vec<PlanningResult> = stdout
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let expected: Vec<PlanningResult> = cases
        .iter()
        .map(|case| serde_json::from_value(case["expected"].clone()).unwrap())
        .collect();
    assert_eq!(responses, expected);
}

#[test]
fn frozen_production_clip_planning_contract() {
    let cases: Vec<Case> = serde_json::from_str(include_str!("../fixtures/planning.json")).unwrap();
    for case in cases {
        assert_eq!(plan_clips(case.request), case.expected, "{}", case.name);
    }
}
