//! Separate integration-test process: the real global cancellation flag never
//! leaks into other tests and needs no production-only reset API.
use pubg_ocr_engine::{cancellation, dedupe, planning::Detection};
use std::{cell::RefCell, path::PathBuf};

#[test]
fn request_during_progress_stops_remaining_candidates_and_application() {
    let detection: Detection = serde_json::from_value(serde_json::json!({
        "duration_sec":20.0,"event_sec":5.0,"method":"ocr","detector":"ocr",
        "target":"self-death","event_key":"self-death:knock:enemy"
    }))
    .unwrap();
    let sources = vec![
        (PathBuf::from("a.mp4"), vec![detection.clone()]),
        (PathBuf::from("b.mp4"), vec![detection.clone()]),
        (PathBuf::from("c.mp4"), vec![detection]),
    ];
    let calls = RefCell::new(Vec::new());
    let (_, work) = dedupe::cross_source_duplicates_profiled(
        &sources,
        Some(&|current, total| {
            calls.borrow_mut().push((current, total));
            cancellation::request();
        }),
    );
    assert_eq!(work.candidates, 3);
    assert_eq!(*calls.borrow(), vec![(1, 3)]);
    assert!(cancellation::check().is_err());
    // The production application checks the same flag before any scan/trim/output work.
    #[cfg(feature = "onnx")]
    {
        use clap::Parser;
        let config = pubg_ocr_engine::config::Config::parse_from(["trim", "nonexistent.mp4"]);
        assert!(pubg_ocr_engine::application::run(config)
            .unwrap_err()
            .contains("cancel"));
    }
}
