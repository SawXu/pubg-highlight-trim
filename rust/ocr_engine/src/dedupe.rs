//! Conservative port of upstream 50c1177: event identity plus exact, changing
//! full-resolution overlap. Verification failure keeps an event; never player-ID dedupe.
use crate::{frame::BgrFrame, planning::Detection};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

/// Optional verification decoder boundary, independent of OCR sampling/resizing.
pub trait VerificationFrames {
    fn read_at(&mut self, sec: f64) -> Result<Option<BgrFrame>, String>;
}
impl VerificationFrames for crate::media::FfmpegFrameSource {
    fn read_at(&mut self, sec: f64) -> Result<Option<BgrFrame>, String> {
        self.read_at(sec).map_err(|e| e.to_string())
    }
}
type Factory<'a> = dyn FnMut(&Path) -> Result<Box<dyn VerificationFrames>, String> + 'a;

/// Return source/detection positions to skip, ranked by available requested context.
/// Inputs must be minimum-event filtered and context prepared, but not clip-merged.
pub fn cross_source_duplicates(
    sources: &[(PathBuf, Vec<Detection>)],
    factory: &mut Factory<'_>,
) -> HashSet<(usize, usize)> {
    let mut candidates = Vec::new();
    for (s, (_, detections)) in sources.iter().enumerate() {
        for (d, detection) in detections.iter().enumerate() {
            if detection.event_sec.is_some() && !detection.event_key.is_empty() {
                candidates.push((s, d));
            }
        }
    }
    let context = |&(s, d): &(usize, usize)| {
        let detection = &sources[s].1[d];
        let event = detection.event_sec.unwrap();
        event.min(detection.clip_before_sec)
            + (detection.duration_sec - event).min(detection.clip_after_sec)
    };
    // Stable ties retain the original source/event ordering, like Python sorted.
    candidates.sort_by(|a, b| context(b).total_cmp(&context(a)));
    let mut retained: HashMap<(&str, &str), Vec<(usize, usize)>> = HashMap::new();
    let mut skipped = HashSet::new();
    for (s, d) in candidates {
        let (src, detections) = &sources[s];
        let detection = &detections[d];
        let matches = retained
            .entry((&detection.target, &detection.event_key))
            .or_default();
        if matches.iter().any(|&(os, od)| {
            src != &sources[os].0
                && same_recorded_event(src, detection, &sources[os].0, &sources[os].1[od], factory)
        }) {
            skipped.insert((s, d));
        } else {
            matches.push((s, d));
        }
    }
    skipped
}
fn same_recorded_event(
    src: &Path,
    detection: &Detection,
    other_src: &Path,
    other: &Detection,
    factory: &mut Factory<'_>,
) -> bool {
    let event = detection.event_sec.unwrap();
    let other_event = other.event_sec.unwrap();
    let left = (-event).max(-other_event).max(-2.);
    let right = (detection.duration_sec - event)
        .min(other.duration_sec - other_event)
        .min(0.5);
    if right - left < 1. {
        return false;
    }
    let mut windows = Vec::new();
    if right >= 0.5 {
        windows.push((0..6).map(|i| i as f64 / 10.).collect::<Vec<_>>());
    }
    windows.push(vec![left, (left + right) / 2., right]);
    windows.iter().any(|offsets| {
        matching_event_window(
            src,
            event,
            other_src,
            other_event,
            other.duration_sec,
            offsets,
            factory,
        )
        .unwrap_or(false)
    })
}
fn tiles(frame: &BgrFrame) -> Vec<Vec<u8>> {
    let (w, h) = (frame.width() as usize, frame.height() as usize);
    let mut tiles = Vec::new();
    for y in 0..4 {
        for x in 0..4 {
            let mut bytes = Vec::new();
            for yy in (h * (1 + 2 * y) / 10..h * (3 + 2 * y) / 10).step_by((h / 80).max(1)) {
                for xx in (w * (1 + 2 * x) / 10..w * (3 + 2 * x) / 10).step_by((w / 80).max(1)) {
                    bytes.extend_from_slice(
                        &frame.pixels()[(yy * w + xx) * 3..(yy * w + xx) * 3 + 3],
                    );
                }
            }
            tiles.push(bytes);
        }
    }
    tiles
}
fn matching_event_window(
    src: &Path,
    event: f64,
    other_src: &Path,
    other_event: f64,
    other_duration: f64,
    offsets: &[f64],
    factory: &mut Factory<'_>,
) -> Result<bool, String> {
    let mut source = factory(src)?;
    let mut other_source = factory(other_src)?;
    let mut previous: Option<Vec<Vec<u8>>> = None;
    let mut matched_frames = 0;
    for offset in offsets {
        let (Some(frame), Some(other)) = (
            source.read_at(event + offset)?,
            other_source.read_at(other_event + offset)?,
        ) else {
            return Ok(false);
        };
        if (frame.width(), frame.height()) != (other.width(), other.height()) {
            return Ok(false);
        }
        if frame != other {
            let mut matched = false;
            for delta in [-1. / 60., 1. / 60., -1. / 30., 1. / 30.] {
                let timestamp = other_event + offset + delta;
                if timestamp < 0. || timestamp >= other_duration {
                    continue;
                }
                if other_source.read_at(timestamp)?.as_ref() == Some(&frame) {
                    matched = true;
                    break;
                }
            }
            if !matched {
                continue;
            }
        }
        if frame.width() < 16 || frame.height() < 16 {
            return Ok(false);
        }
        let current = tiles(&frame);
        if let Some(previous) = previous {
            let changing = previous
                .iter()
                .zip(&current)
                .filter(|(old, new)| {
                    old.iter()
                        .zip(new.iter())
                        .filter(|(a, b)| a.abs_diff(**b) > 8)
                        .count() as f64
                        > new.len() as f64 * 0.05
                })
                .count();
            if changing < 8 {
                return Ok(false);
            }
        }
        previous = Some(current);
        matched_frames += 1;
        if matched_frames >= 3 {
            return Ok(true);
        }
    }
    Ok(false)
}
