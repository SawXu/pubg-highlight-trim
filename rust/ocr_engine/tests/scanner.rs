use pubg_ocr_engine::events::{default_language_profile, language_profile};
use pubg_ocr_engine::scanner::{
    detect_events, FrameRead, GateResult, OcrConfig, Recognition, ScanBackend,
};
use serde::Deserialize;

struct RecordingBackend {
    seen: Vec<f64>,
}
impl ScanBackend for RecordingBackend {
    type Frame = f64;
    fn read_frame(&mut self, seconds: f64) -> Result<FrameRead<f64>, String> {
        self.seen.push(seconds);
        Ok(FrameRead {
            frame: Some(seconds),
            seconds: 0.001,
        })
    }
    fn recognize(
        &mut self,
        _frame: &f64,
        _roi: [f64; 4],
        _width: u32,
    ) -> Result<Recognition, String> {
        Ok(Recognition {
            text: "普通HUD文字".into(),
            scores: "0.990".into(),
            seconds: 0.01,
            frame_seconds: 0.0,
        })
    }
    fn gate(&mut self, _frame: &f64, _config: &OcrConfig) -> Result<GateResult, String> {
        Ok(GateResult {
            passed: true,
            reason: "passed".into(),
        })
    }
    fn now(&mut self) -> f64 {
        0.0
    }
}

#[test]
fn non_target_text_does_not_expand_adaptive_schedule() {
    let config = OcrConfig {
        sampling_mode: "adaptive".into(),
        priority_window: vec![],
        ..Default::default()
    };
    let mut backend = RecordingBackend { seen: vec![] };
    let report =
        detect_events(&mut backend, &default_language_profile(), 8.0, &[], &config).unwrap();
    assert_eq!(backend.seen, vec![0.0, 4.0, 8.0]);
    assert_eq!(report.detections.len(), 1);
    assert_eq!(report.detections[0].event_sec, None);
    assert_eq!(report.detections[0].method, "paddle-not-target-text");
    assert_eq!(report.stats.ocr_requests, 3);
}

#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    config: OcrConfig,
    duration: f64,
    candidates: Vec<f64>,
    primary: Vec<TextInterval>,
    assist: Vec<TextInterval>,
    #[serde(default = "yes")]
    gate_passed: bool,
    #[serde(default)]
    failed_frames: Vec<f64>,
    expected_detections: serde_json::Value,
    expected_stats: serde_json::Value,
    expected_decodes: usize,
}
fn yes() -> bool {
    true
}
#[derive(Deserialize)]
struct TextInterval {
    start: f64,
    end: f64,
    text: String,
}
struct FixedBackend<'a> {
    case: &'a Case,
    clock: f64,
    decodes: usize,
}
impl ScanBackend for FixedBackend<'_> {
    type Frame = f64;
    fn read_frame(&mut self, sec: f64) -> Result<FrameRead<f64>, String> {
        self.clock += 0.001;
        self.decodes += 1;
        Ok(FrameRead {
            frame: (!self.case.failed_frames.contains(&sec)).then_some(sec),
            seconds: 0.001,
        })
    }
    fn recognize(&mut self, sec: &f64, roi: [f64; 4], _width: u32) -> Result<Recognition, String> {
        self.clock += 0.012;
        let intervals = if roi == self.case.config.assist_roi {
            &self.case.assist
        } else {
            &self.case.primary
        };
        let text = intervals
            .iter()
            .find(|e| e.start <= *sec && *sec <= e.end)
            .map_or("", |e| &e.text);
        Ok(Recognition {
            text: text.into(),
            scores: "0.990".into(),
            seconds: 0.01,
            frame_seconds: 0.002,
        })
    }
    fn gate(&mut self, _frame: &f64, _config: &OcrConfig) -> Result<GateResult, String> {
        self.clock += 0.003;
        Ok(GateResult {
            passed: self.case.gate_passed,
            reason: if self.case.gate_passed {
                "light-brightness-passed"
            } else {
                "light-low-brightness"
            }
            .into(),
        })
    }
    fn now(&mut self) -> f64 {
        self.clock
    }
}
#[test]
fn multiline_hud_survives_scanner_refinement_and_cache() {
    let mut case = fixtures()
        .cases
        .into_iter()
        .find(|c| c.name == "ocr_cache_shared_by_multiple_subjects")
        .unwrap();
    case.config.target = "self-death".into();
    case.config.language = "zh-Hant".into();
    for interval in &mut case.primary {
        interval.text =
            "HUDNOISE EnemyB\n[TAG]EnemyAlpha以MP5K擊倒您\nOpponentBravo以M416擊倒您".into();
    }
    let mut backend = FixedBackend {
        case: &case,
        clock: 0.0,
        decodes: 0,
    };
    let rules = language_profile("zh-Hant").unwrap();
    let report = detect_events(
        &mut backend,
        &rules,
        case.duration,
        &case.candidates,
        &case.config,
    )
    .unwrap();
    let clean =
        pubg_ocr_engine::events::extract_self_events("[TAG]EnemyAlpha以MP5K擊倒您", Some(&rules));
    assert_eq!(
        report.detections.len(),
        2,
        "different actors must not be merged"
    );
    assert_eq!(report.detections[0].event_key, clean[0].key);
    let other =
        pubg_ocr_engine::events::extract_self_events("OpponentBravo以M416擊倒您", Some(&rules));
    assert_eq!(report.detections[1].event_key, other[0].key);
    assert_eq!(report.detections[0].event_sec, Some(28.0));
    assert_eq!(report.detections[1].event_sec, Some(28.0));
    assert!(
        report.stats.ocr_cache_hits > 0,
        "must exercise cached refinement reads"
    );
}

fn fixtures() -> Fixture {
    serde_json::from_str(include_str!("fixtures/scanner.json")).unwrap()
}
fn assert_value(actual: &serde_json::Value, expected: &serde_json::Value, path: &str) {
    use serde_json::Value;
    match (actual, expected) {
        (Value::Number(a), Value::Number(e)) => assert!(
            (a.as_f64().unwrap() - e.as_f64().unwrap()).abs() < 1e-9,
            "{path}: {a} != {e}"
        ),
        (Value::Object(a), Value::Object(e)) => {
            assert_eq!(
                a.keys().collect::<Vec<_>>(),
                e.keys().collect::<Vec<_>>(),
                "{path}: report fields"
            );
            for (key, value) in a {
                assert_value(value, &e[key], &format!("{path}.{key}"));
            }
        }
        (Value::Array(a), Value::Array(e)) => {
            assert_eq!(a.len(), e.len(), "{path}: array length");
            for (i, (a, e)) in a.iter().zip(e).enumerate() {
                assert_value(a, e, &format!("{path}[{i}]"));
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}
#[test]
fn completed_adaptive_scan_profile_uses_final_work_not_last_detection_snapshot() {
    let case = fixtures()
        .cases
        .into_iter()
        .find(|c| c.name == "adaptive_target_seeds_do_not_recurse")
        .unwrap();
    let mut backend = FixedBackend {
        case: &case,
        clock: 0.0,
        decodes: 0,
    };
    let report = detect_events(
        &mut backend,
        &language_profile(&case.config.language).unwrap(),
        case.duration,
        &case.candidates,
        &case.config,
    )
    .unwrap();
    assert_eq!(report.detections[0].ocr_requests, 2);
    assert!((report.detections[0].ocr_seconds - 0.02).abs() < 1e-9);
    assert_eq!(report.stats.ocr_requests, 7);
    assert!((report.work.ocr_seconds - 0.07).abs() < 1e-9);
    assert!((report.work.detect_seconds - backend.clock).abs() < 1e-9);
    let mut current = pubg_ocr_engine::reports::ScanTelemetry::default();
    current.add_current(&report.stats, &report.work);
    let p = serde_json::to_value(current).unwrap();
    assert_eq!(p["ocr_requests"], 7);
    assert!((p["ocr_predict_seconds_sum"].as_f64().unwrap() - 0.07).abs() < 1e-9);
    assert_eq!(p["refine_calls"], report.stats.refine_calls);
    assert_eq!(p["assist_calls"], report.stats.assist_calls);
}

fn run_case(case: &Case) {
    let mut backend = FixedBackend {
        case,
        clock: 0.0,
        decodes: 0,
    };
    let rules = language_profile(&case.config.language).unwrap();
    let report = detect_events(
        &mut backend,
        &rules,
        case.duration,
        &case.candidates,
        &case.config,
    )
    .unwrap();
    let mut current = pubg_ocr_engine::reports::ScanTelemetry::default();
    current.add_current(&report.stats, &report.work);
    let current = serde_json::to_value(current).unwrap();
    for field in [
        "ocr_requests",
        "ocr_successes",
        "ocr_skipped",
        "ocr_cache_hits",
        "refine_calls",
        "assist_calls",
    ] {
        assert_eq!(
            current[field], case.expected_stats[field],
            "{}.current.{field}",
            case.name
        );
    }
    for (field, expected) in [
        ("refine_ocr_seconds_sum", "refine_ocr_seconds"),
        ("assist_ocr_seconds_sum", "assist_ocr_seconds"),
    ] {
        assert!(
            (current[field].as_f64().unwrap() - case.expected_stats[expected].as_f64().unwrap())
                .abs()
                < 1e-9,
            "{}.current.{field}",
            case.name
        );
    }
    assert_value(
        &serde_json::to_value(report.detections).unwrap(),
        &case.expected_detections,
        &case.name,
    );
    assert_value(
        &serde_json::to_value(report.stats).unwrap(),
        &case.expected_stats,
        &format!("{}.stats", case.name),
    );
    assert_eq!(
        backend.decodes, case.expected_decodes,
        "{} decoded frames",
        case.name
    );
}
macro_rules! baseline {
    ($($name:ident),* $(,)?) => { $(
        #[test]
        fn $name() {
            let fixture = fixtures();
            run_case(fixture.cases.iter().find(|case| case.name == stringify!($name)).unwrap());
        }
    )* };
}
baseline!(
    non_target_adaptive,
    backward_refine,
    multiple_ids_refine,
    bare_assist_is_not_subject_evidence,
    same_subject_assist_suppresses,
    different_subject_assist_keeps,
    adaptive_target_seeds_do_not_recurse,
    own_followup_suppressed,
    own_different_subject_kept,
    self_followup_any_actor_suppressed,
    later_direct_kill_kept,
    english_headshot_followup,
    close_suffix_noise,
    short_prefix_is_distinct,
    max_ocr_calls,
    min_ocr_interval,
    refine_frame_budget,
    refine_time_budget,
    assist_frame_budget,
    assist_time_budget,
    shared_frame_primary_assist,
    ocr_cache_before_budget,
    gate_skips_without_ocr,
    gate_timing_passed,
    single_frame_failure_continues,
    adaptive_candidate_boundary,
    millisecond_refine_rounding,
    ocr_cache_shared_by_multiple_subjects,
    frame_fifo_retains_recent_assist_samples
);

#[test]
fn scanning_same_backend_twice_has_fresh_source_caches_and_budgets() {
    let case = fixtures()
        .cases
        .into_iter()
        .find(|c| c.name == "max_ocr_calls")
        .unwrap();
    let mut backend = FixedBackend {
        case: &case,
        clock: 0.0,
        decodes: 0,
    };
    let rules = language_profile(&case.config.language).unwrap();
    let first = detect_events(&mut backend, &rules, case.duration, &[], &case.config).unwrap();
    let second = detect_events(&mut backend, &rules, case.duration, &[], &case.config).unwrap();
    assert_eq!(first.stats, second.stats);
    assert_eq!(backend.decodes, case.expected_decodes * 2);
}

#[test]
fn invalid_configuration_fails_before_touching_media() {
    let valid = OcrConfig::default();
    let invalid = [
        OcrConfig {
            roi: [0.8, 0.1, 0.2, 0.3],
            ..valid.clone()
        },
        OcrConfig {
            scan_start: 4.0,
            scan_end: Some(4.0),
            ..valid.clone()
        },
        OcrConfig {
            coarse_step: 0.0,
            ..valid.clone()
        },
        OcrConfig {
            refine_max_seconds: Some(f64::NAN),
            ..valid.clone()
        },
        OcrConfig {
            decoded_frame_cache_size: 0,
            ..valid.clone()
        },
        OcrConfig {
            sampling_mode: "unknown".into(),
            ..valid.clone()
        },
        OcrConfig {
            scan_start: f64::NAN,
            ..valid.clone()
        },
        OcrConfig {
            scan_end: Some(f64::INFINITY),
            ..valid.clone()
        },
        OcrConfig {
            priority_window: vec![[1.0, f64::INFINITY]],
            ..valid.clone()
        },
        OcrConfig {
            roi: [0.1, 0.1, f64::NAN, 0.9],
            ..valid.clone()
        },
        OcrConfig {
            candidate_lookback: f64::INFINITY,
            ..valid.clone()
        },
        OcrConfig {
            refine_max_seconds: Some(f64::INFINITY),
            ..valid.clone()
        },
    ];
    let mut backend = RecordingBackend { seen: vec![] };
    for config in invalid {
        assert!(
            detect_events(&mut backend, &default_language_profile(), 8.0, &[], &config).is_err()
        );
    }
    assert!(backend.seen.is_empty());
    assert!(serde_json::from_str::<OcrConfig>(r#"{"ocr_max_calls":-1}"#).is_err());
    assert!(serde_json::from_str::<OcrConfig>(r#"{"ocr_width":-1}"#).is_err());
}

struct FailingBackend {
    stage: &'static str,
}
impl ScanBackend for FailingBackend {
    type Frame = ();
    fn read_frame(&mut self, _sec: f64) -> Result<FrameRead<()>, String> {
        if self.stage == "source" {
            Err("video unreadable".into())
        } else {
            Ok(FrameRead {
                frame: Some(()),
                seconds: 0.0,
            })
        }
    }
    fn recognize(&mut self, _: &(), _: [f64; 4], _: u32) -> Result<Recognition, String> {
        Err("model inference failed".into())
    }
    fn gate(&mut self, _: &(), _: &OcrConfig) -> Result<GateResult, String> {
        Err("gate failed".into())
    }
    fn now(&mut self) -> f64 {
        0.0
    }
}
#[test]
fn source_model_and_gate_errors_are_not_clean_no_event_scans() {
    for (stage, error) in [
        ("source", "video unreadable"),
        ("model", "model inference failed"),
        ("gate", "gate failed"),
    ] {
        let config = OcrConfig {
            brightness_gate: stage == "gate",
            ..Default::default()
        };
        let result = detect_events(
            &mut FailingBackend { stage },
            &default_language_profile(),
            1.0,
            &[],
            &config,
        );
        assert_eq!(result.unwrap_err(), error);
    }
}
