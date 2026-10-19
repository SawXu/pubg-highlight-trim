#![cfg(any(feature = "cli", feature = "highlight-cli"))]

use pubg_ocr_engine::planning::PlanningResult;
use serde_json::Value;
use std::io::Write;
use std::process::{Command, Output, Stdio};

fn invoke(input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_highlight_plan"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start highlight_plan");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .expect("write request");
    child.wait_with_output().expect("read response")
}

fn fixture_cases() -> Vec<Value> {
    serde_json::from_str(include_str!("../fixtures/planning.json")).unwrap()
}

fn request(case: &Value) -> Value {
    case["request"].clone()
}

#[test]
fn legacy_single_json_matches_every_fixture() {
    for case in fixture_cases() {
        let output = invoke(&format!("{}\n", request(&case)));
        assert!(
            output.status.success(),
            "{}: {}",
            case["name"],
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        assert!(output.stdout.ends_with(b"\n"));
        assert_eq!(
            serde_json::from_slice::<PlanningResult>(&output.stdout).unwrap(),
            serde_json::from_value::<PlanningResult>(case["expected"].clone()).unwrap(),
            "{}",
            case["name"]
        );
    }
}

#[test]
fn jsonl_processes_multiple_requests_in_order() {
    let cases = fixture_cases();
    let input = cases
        .iter()
        .map(|case| serde_json::to_string(&request(case)).unwrap())
        .collect::<Vec<_>>()
        .join("\r\n");
    let output = invoke(&input);
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
    assert_eq!(responses.len(), cases.len());
    for (response, case) in responses.iter().zip(&cases) {
        assert_eq!(
            *response,
            serde_json::from_value::<PlanningResult>(case["expected"].clone()).unwrap(),
            "{}",
            case["name"]
        );
    }
}

#[test]
fn malformed_later_request_keeps_prior_jsonl_response() {
    let case = fixture_cases().remove(0);
    let input = format!("{}\nnot json\n", request(&case));
    let output = invoke(&input);
    assert!(!output.status.success());
    assert!(!output.stderr.is_empty());
    assert_eq!(
        serde_json::from_slice::<PlanningResult>(&output.stdout).unwrap(),
        serde_json::from_value::<PlanningResult>(case["expected"].clone()).unwrap()
    );
}

#[test]
fn invalid_request_exits_nonzero_without_machine_output() {
    for input in [
        "",
        "not json\n",
        "{}\n",
        "{\"config\":{\"duration_ms\":1000,\"before_ms\":0,\"after_ms\":1,\"dedup_ms\":0},\"samples\":[{\"at_ms\":-1,\"text\":\"YOU killed enemy\"}]}\n",
    ] {
        let output = invoke(input);
        assert!(!output.status.success(), "unexpected success for {input}");
        assert!(output.stdout.is_empty(), "diagnostics must not leak to stdout");
        assert!(!output.stderr.is_empty(), "errors must explain the failure");
    }
}
