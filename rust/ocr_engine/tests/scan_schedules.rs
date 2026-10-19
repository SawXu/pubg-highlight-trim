use pubg_ocr_engine::scan::{
    build_adaptive_scan_times, build_text_priority_scan_times, ScanConfig,
};
use serde::Deserialize;
#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    config: Config,
    duration: f64,
    candidates: Vec<f64>,
    expected: Vec<f64>,
}
#[derive(Deserialize)]
struct Config {
    #[serde(flatten)]
    schedule: ScanConfig,
    sampling_mode: String,
}
#[test]
fn decimal_and_1500_random_schedules_match_frozen_python() {
    let fixture: Fixture =
        serde_json::from_str(include_str!("fixtures/scan_schedules.json")).unwrap();
    let mut mismatches = vec![];
    for case in fixture.cases {
        let actual = if case.config.sampling_mode == "adaptive" {
            build_adaptive_scan_times(case.duration, &case.candidates, &case.config.schedule)
        } else {
            build_text_priority_scan_times(case.duration, &case.candidates, &case.config.schedule)
        };
        if actual != case.expected {
            mismatches.push((case.name, actual, case.expected));
        }
    }
    assert!(
        mismatches.is_empty(),
        "{} mismatches; first: {:?}",
        mismatches.len(),
        mismatches.first()
    );
}
