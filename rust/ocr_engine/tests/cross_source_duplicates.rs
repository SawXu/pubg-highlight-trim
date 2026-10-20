//! Synthetic full-resolution frame-source boundary; no private video or OCR mocks.
use pubg_ocr_engine::{
    dedupe::{cross_source_duplicates, VerificationFrames},
    frame::BgrFrame,
    planning::Detection,
};
use std::path::{Path, PathBuf};
fn detection(sec: f64) -> Detection {
    serde_json::from_value(serde_json::json!({"duration_sec":sec+1.,"event_sec":sec,"method":"ocr","detector":"ocr","target":"self-death","event_key":"self-death:knock:enemy","clip_before_sec":5.,"clip_after_sec":1.})).unwrap()
}
struct Frames {
    event: f64,
}
impl VerificationFrames for Frames {
    fn read_at(&mut self, sec: f64) -> Result<Option<BgrFrame>, String> {
        let n = ((sec - self.event + 10.) * 1000.).round() as u64;
        let pixels = (0..48 * 64 * 3)
            .map(|i| ((i as u64 * 73 + n * 97) % 256) as u8)
            .collect();
        Ok(Some(BgrFrame::new(64, 48, pixels).unwrap()))
    }
}
#[test]
fn later_source_with_more_requested_context_wins_before_any_trims() {
    let sources = vec![
        (PathBuf::from("a.mp4"), vec![detection(2.)]),
        (PathBuf::from("b.mp4"), vec![detection(30.5)]),
    ];
    let skipped = cross_source_duplicates(&sources, &mut |path: &Path| {
        Ok(Box::new(Frames {
            event: if path == Path::new("a.mp4") { 2. } else { 30.5 },
        }))
    });
    assert_eq!(skipped, [(0, 0)].into_iter().collect());
}

#[test]
fn dedupe_reports_actual_work_without_changing_conservative_failures() {
    let sources = vec![
        (PathBuf::from("a"), vec![detection(3.)]),
        (PathBuf::from("b"), vec![detection(3.)]),
    ];
    let (skipped, work) = pubg_ocr_engine::dedupe::cross_source_duplicates_profiled(
        &sources,
        &mut |_| Err("unavailable".into()),
        None,
    );
    assert!(skipped.is_empty());
    assert_eq!(work.candidates, 2);
    assert_eq!(work.pair_comparisons, 1);
    assert_eq!(work.verification_windows, 2);
    assert_eq!(work.verification_failures, 2);
    assert_eq!(work.duplicates, 0);
}

struct Evidence {
    event: f64,
    second: bool,
    mode: &'static str,
}
impl VerificationFrames for Evidence {
    fn read_at(&mut self, sec: f64) -> Result<Option<BgrFrame>, String> {
        if self.mode == "decode-error" {
            return Err("decoder unavailable".into());
        }
        if self.mode == "missing" {
            return Ok(None);
        }
        let mut offset = sec - self.event;
        if self.mode == "neighbor" && self.second {
            // Exact pixels only at the tiny +1/60 retry, not the nominal seek.
            offset -= 1. / 60.;
        }
        let mut n = ((offset + 10.) * 1000.).round() as u64;
        if self.second
            && (self.mode == "different-game" || (self.mode == "early-mismatch" && offset < 0.))
        {
            n += 13;
        }
        if matches!(
            self.mode,
            "static" | "hud-only" | "black" | "seven-tiles" | "delta-eight" | "five-percent"
        ) {
            n = 123;
        }
        let mut pixels: Vec<u8> = (0..48 * 64 * 3)
            .map(|i| ((i as u64 * 73 + n * 97) % 256) as u8)
            .collect();
        if self.mode == "black" {
            pixels.fill(0);
        }
        let tick = ((offset + 10.) * 10.).round() as u8;
        if self.mode == "hud-only" {
            pixels[..64 * 4 * 3].fill(tick);
        }
        if matches!(
            self.mode,
            "seven-tiles" | "eight-tiles" | "delta-eight" | "five-percent"
        ) {
            pixels.fill(0);
            let count = if self.mode == "seven-tiles" { 7 } else { 8 };
            for tile in 0..count {
                let (y, x) = (tile / 4, tile % 4);
                let mut changed = 0;
                for yy in 48 * (1 + 2 * y) / 10..48 * (3 + 2 * y) / 10 {
                    for xx in 64 * (1 + 2 * x) / 10..64 * (3 + 2 * x) / 10 {
                        for c in 0..3 {
                            if self.mode != "five-percent" || changed < 1 {
                                pixels[(yy * 64 + xx) * 3 + c] = if self.mode == "delta-eight" {
                                    tick % 2 * 8
                                } else {
                                    tick % 2 * 80
                                };
                            }
                            changed += 1;
                        }
                    }
                }
            }
        }
        let (w, h) = if self.mode == "small" {
            (8, 8)
        } else if self.mode == "shape" && self.second {
            (48, 64)
        } else {
            (64, 48)
        };
        pixels.resize(w * h * 3, 0);
        Ok(Some(BgrFrame::new(w as u32, h as u32, pixels).unwrap()))
    }
}
#[test]
fn useful_upstream_evidence_cases_keep_real_events_and_only_suppress_proven_overlap() {
    for mode in [
        "different-game",
        "black",
        "static",
        "hud-only",
        "missing",
        "decode-error",
        "different-key",
        "missing-key",
        "same-source",
        "extra-event",
        "early-mismatch",
        "neighbor",
        "short",
        "shape",
        "small",
        "seven-tiles",
        "eight-tiles",
        "delta-eight",
        "five-percent",
    ] {
        let mut a = detection(30.5);
        let mut b = detection(2.);
        if mode == "different-key" {
            b.event_key = "self-death:knock:another".into();
        }
        if mode == "missing-key" {
            b.event_key.clear();
        }
        if mode == "short" {
            a.event_sec = Some(0.1);
            b.event_sec = Some(0.1);
        }
        let second_path = if mode == "same-source" {
            "a.mp4"
        } else {
            "b.mp4"
        };
        let mut sources = vec![
            (PathBuf::from("a.mp4"), vec![a]),
            (PathBuf::from(second_path), vec![b]),
        ];
        if mode == "extra-event" {
            let mut d = detection(3.);
            d.event_key = "self-death:knock:unique".into();
            sources[1].1.push(d);
        }
        let skipped = cross_source_duplicates(&sources, &mut |path| {
            Ok(Box::new(Evidence {
                event: if path == Path::new("a.mp4") { 30.5 } else { 2. },
                second: path != Path::new("a.mp4"),
                mode,
            }))
        });
        let expected = if matches!(
            mode,
            "extra-event" | "early-mismatch" | "neighbor" | "eight-tiles"
        ) {
            vec![(1, 0)]
        } else {
            vec![]
        };
        assert_eq!(skipped, expected.into_iter().collect(), "{mode}");
    }
}

#[test]
fn fallback_three_frame_window_can_prove_overlap_when_post_event_window_is_short() {
    let mut a = detection(30.5);
    let mut b = detection(2.);
    a.duration_sec = 30.7;
    b.duration_sec = 2.2;
    let sources = vec![(PathBuf::from("a"), vec![a]), (PathBuf::from("b"), vec![b])];
    assert_eq!(
        cross_source_duplicates(&sources, &mut |p| Ok(Box::new(Frames {
            event: if p == Path::new("a") { 30.5 } else { 2. }
        }))),
        [(1, 0)].into_iter().collect()
    );
}

#[test]
fn target_is_part_of_identity_and_requested_context_not_duration_determines_rank() {
    let mut a = detection(2.);
    a.duration_sec = 100.;
    let mut b = detection(30.5);
    b.clip_before_sec = 10.;
    let sources = vec![
        (PathBuf::from("a"), vec![a.clone()]),
        (PathBuf::from("b"), vec![b.clone()]),
    ];
    let mut factory = |p: &Path| {
        Ok(Box::new(Frames {
            event: if p == Path::new("a") { 2. } else { 30.5 },
        }) as Box<dyn VerificationFrames>)
    };
    assert_eq!(
        cross_source_duplicates(&sources, &mut factory),
        [(0, 0)].into_iter().collect()
    );
    b.target = "own-kill".into();
    assert!(cross_source_duplicates(
        &[(PathBuf::from("a"), vec![a]), (PathBuf::from("b"), vec![b])],
        &mut factory
    )
    .is_empty());
}
