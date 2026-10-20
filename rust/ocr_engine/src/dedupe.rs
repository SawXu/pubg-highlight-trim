//! Cross-source event deduplication based on the existing target and event key.
//! No media decoding or frame verification is performed.
use crate::planning::Detection;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

#[derive(Default, serde::Serialize)]
pub struct DedupeWork {
    pub candidates: usize,
    pub pair_comparisons: usize,
    /// Retained for telemetry/schema compatibility; verification is no longer performed.
    pub verification_windows: usize,
    /// Retained for telemetry/schema compatibility; verification is no longer performed.
    pub verification_failures: usize,
    pub duplicates: usize,
}

/// Return source/detection positions to skip, ranked by requested context.
/// Events are duplicates only when target and event_key match across different sources.
/// Empty keys and events from the same source are never deduplicated.
pub fn cross_source_duplicates(sources: &[(PathBuf, Vec<Detection>)]) -> HashSet<(usize, usize)> {
    cross_source_duplicates_profiled(sources, None).0
}

/// Like [`cross_source_duplicates`] but also returns work telemetry and accepts an optional
/// progress callback invoked as `(current, total)` after each candidate is processed.
pub fn cross_source_duplicates_profiled(
    sources: &[(PathBuf, Vec<Detection>)],
    progress: Option<&dyn Fn(usize, usize)>,
) -> (HashSet<(usize, usize)>, DedupeWork) {
    let mut work = DedupeWork::default();
    let mut candidates = Vec::new();
    for (s, (_, detections)) in sources.iter().enumerate() {
        for (d, detection) in detections.iter().enumerate() {
            if detection.event_sec.is_some() && !detection.event_key.is_empty() {
                candidates.push((s, d));
            }
        }
    }
    work.candidates = candidates.len();
    let context = |&(s, d): &(usize, usize)| {
        let detection = &sources[s].1[d];
        let event = detection.event_sec.unwrap();
        event.min(detection.clip_before_sec)
            + (detection.duration_sec - event).min(detection.clip_after_sec)
    };
    // Stable ties retain source/event ordering.
    candidates.sort_by(|a, b| context(b).total_cmp(&context(a)));
    let mut retained: HashMap<(&str, &str), Vec<(usize, usize)>> = HashMap::new();
    let mut skipped = HashSet::new();
    let total = work.candidates;
    for (processed, (s, d)) in candidates.into_iter().enumerate() {
        if crate::cancellation::is_cancelled() {
            break;
        }
        let (src, detections) = &sources[s];
        let detection = &detections[d];
        let matches = retained
            .entry((&detection.target, &detection.event_key))
            .or_default();
        if matches.iter().any(|&(os, _)| {
            if src == &sources[os].0 {
                return false;
            }
            work.pair_comparisons += 1;
            true
        }) {
            skipped.insert((s, d));
        } else {
            matches.push((s, d));
        }
        if let Some(f) = progress {
            f(processed + 1, total);
        }
    }
    work.duplicates = skipped.len();
    (skipped, work)
}
