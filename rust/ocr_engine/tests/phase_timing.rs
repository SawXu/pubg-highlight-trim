use pubg_ocr_engine::reports::{Phase, PhaseTimings};
#[test]
fn sequential_wall_clock_accumulates_split_planning_and_freezes_reporting_boundary() {
    let mut t = PhaseTimings::default();
    t.enter(Phase::Scan, 2.0);
    t.enter(Phase::Planning, 7.0);
    t.enter(Phase::Dedupe, 10.0);
    t.enter(Phase::Planning, 14.0);
    t.enter(Phase::Trim, 16.0);
    t.enter(Phase::Merge, 22.0);
    t.enter(Phase::Archive, 30.0);
    t.enter(Phase::Reporting, 31.0);
    let stages = t.finish(35.0);
    assert_eq!(
        stages,
        serde_json::json!({"setup":2.0,"scan":5.0,"planning":5.0,"dedupe":4.0,"trim":6.0,"merge":8.0,"archive":1.0,"reporting":4.0,"total":35.0})
    );
}
#[test]
fn csv_reporting_gate_before_merge_is_not_charged_to_media_phases() {
    let mut t = PhaseTimings::default();
    t.enter(Phase::Trim, 2.0);
    t.enter(Phase::Reporting, 5.0);
    t.enter(Phase::Merge, 9.0);
    t.enter(Phase::Archive, 15.0);
    t.enter(Phase::Reporting, 16.0);
    assert_eq!(
        t.finish(18.0),
        serde_json::json!({"setup":2.0,"scan":0.0,"planning":0.0,"dedupe":0.0,"trim":3.0,"merge":6.0,"archive":1.0,"reporting":6.0,"total":18.0})
    );
}

#[test]
fn empty_phases_and_rounding_do_not_reuse_other_stage_measurements() {
    let mut t = PhaseTimings::default();
    t.enter(Phase::Scan, 0.0004);
    t.enter(Phase::Planning, 0.0056);
    t.enter(Phase::Dedupe, 0.0056);
    t.enter(Phase::Planning, 0.0056);
    t.enter(Phase::Trim, 0.0056);
    t.enter(Phase::Merge, 0.0056);
    t.enter(Phase::Archive, 0.0056);
    t.enter(Phase::Reporting, 0.0056);
    assert_eq!(
        t.finish(0.0066),
        serde_json::json!({"setup":0.0,"scan":0.005,"planning":0.0,"dedupe":0.0,"trim":0.0,"merge":0.0,"archive":0.0,"reporting":0.001,"total":0.007})
    );
}
