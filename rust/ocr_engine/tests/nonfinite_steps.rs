use pubg_ocr_engine::{
    scan::{build_text_priority_scan_times, ScanConfig},
    scanner::OcrConfig,
};
#[test]
fn python_nonfinite_positive_step_yields_first_sample_then_stops() {
    // Frozen Python time_range: round(start,3) is yielded before t+=NaN/Inf;
    // the next inclusive-end comparison is false. Negative/zero are rejected.
    for step in [f64::NAN, f64::INFINITY] {
        let config = OcrConfig {
            coarse_step: step,
            candidate_step: step,
            refine_step: step,
            refine_search_step: step,
            assist_step: step,
            adaptive_step: step,
            ..Default::default()
        };
        assert!(config.validate().is_ok());
        let schedule = ScanConfig {
            priority_window: vec![],
            scan_start: 1.0,
            coarse_step: step,
            ..Default::default()
        };
        assert_eq!(
            build_text_priority_scan_times(10.0, &[], &schedule),
            vec![1.0]
        );
    }
    for step in [0.0, -1.0, f64::NEG_INFINITY] {
        assert!(OcrConfig {
            coarse_step: step,
            ..Default::default()
        }
        .validate()
        .is_err());
    }
}
