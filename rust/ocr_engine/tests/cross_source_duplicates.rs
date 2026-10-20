//! Event identity dedupe needs no videos, frame decoding, or image evidence.
use pubg_ocr_engine::{
    dedupe::{cross_source_duplicates, cross_source_duplicates_profiled},
    planning::Detection,
};
use std::path::PathBuf;

fn detection(sec: f64) -> Detection {
    serde_json::from_value(serde_json::json!({
        "duration_sec":sec+1.,"event_sec":sec,"method":"ocr","detector":"ocr",
        "target":"self-death","event_key":"self-death:knock:enemy",
        "clip_before_sec":5.,"clip_after_sec":1.
    }))
    .unwrap()
}

#[test]
fn later_source_with_more_requested_context_wins_before_any_trims() {
    let sources = vec![
        (PathBuf::from("missing-a.mp4"), vec![detection(2.)]),
        (PathBuf::from("missing-b.mp4"), vec![detection(30.5)]),
    ];
    assert_eq!(
        cross_source_duplicates(&sources),
        [(0, 0)].into_iter().collect()
    );
}

#[test]
fn dedupe_reports_identity_comparisons_and_zero_verification_work() {
    let sources = vec![
        (PathBuf::from("a"), vec![detection(3.)]),
        (PathBuf::from("b"), vec![detection(3.)]),
    ];
    let (skipped, work) = cross_source_duplicates_profiled(&sources, None);
    assert_eq!(skipped, [(1, 0)].into_iter().collect());
    assert_eq!(work.candidates, 2);
    assert_eq!(work.pair_comparisons, 1);
    assert_eq!(work.verification_windows, 0);
    assert_eq!(work.verification_failures, 0);
    assert_eq!(work.duplicates, 1);
}

#[test]
fn equal_context_retains_original_source_and_event_order() {
    let sources = vec![
        (PathBuf::from("a"), vec![detection(5.), detection(10.)]),
        (PathBuf::from("b"), vec![detection(20.)]),
        (PathBuf::from("c"), vec![detection(30.)]),
    ];
    assert_eq!(
        cross_source_duplicates(&sources),
        [(1, 0), (2, 0)].into_iter().collect()
    );
}

#[test]
fn same_source_is_not_deduplicated_even_across_source_entries() {
    let sources = vec![
        (PathBuf::from("a"), vec![detection(3.), detection(3.)]),
        (PathBuf::from("a"), vec![detection(3.)]),
    ];
    let (skipped, work) = cross_source_duplicates_profiled(&sources, None);
    assert!(skipped.is_empty());
    assert_eq!(work.pair_comparisons, 0);
}

#[test]
fn empty_keys_and_distinct_identities_are_not_deduplicated() {
    for mode in ["empty-key", "different-key", "different-target", "no-event"] {
        let a = detection(5.);
        let mut b = a.clone();
        match mode {
            "empty-key" => b.event_key.clear(),
            "different-key" => b.event_key = "self-death:knock:another".into(),
            "different-target" => b.target = "own-kill".into(),
            "no-event" => b.event_sec = None,
            _ => unreachable!(),
        }
        let sources = vec![(PathBuf::from("a"), vec![a]), (PathBuf::from("b"), vec![b])];
        assert!(cross_source_duplicates(&sources).is_empty(), "{mode}");
    }
}

#[test]
fn short_event_context_does_not_require_a_verification_window() {
    let mut a = detection(0.1);
    a.duration_sec = 0.2;
    let sources = vec![
        (PathBuf::from("a"), vec![a.clone()]),
        (PathBuf::from("b"), vec![a]),
    ];
    assert_eq!(
        cross_source_duplicates(&sources),
        [(1, 0)].into_iter().collect()
    );
}

#[test]
fn requested_context_not_duration_determines_rank() {
    let mut a = detection(2.);
    a.duration_sec = 100.;
    let mut b = detection(30.5);
    b.clip_before_sec = 10.;
    let sources = vec![(PathBuf::from("a"), vec![a]), (PathBuf::from("b"), vec![b])];
    assert_eq!(
        cross_source_duplicates(&sources),
        [(0, 0)].into_iter().collect()
    );
}

#[test]
fn available_post_event_context_also_affects_rank() {
    let mut a = detection(10.);
    a.clip_after_sec = 5.;
    let mut b = a.clone();
    b.duration_sec = 20.;
    let sources = vec![(PathBuf::from("a"), vec![a]), (PathBuf::from("b"), vec![b])];
    assert_eq!(
        cross_source_duplicates(&sources),
        [(0, 0)].into_iter().collect()
    );
}
