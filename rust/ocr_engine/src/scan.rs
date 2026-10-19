//! Pure OCR scan schedule construction.
//!
//! This module intentionally has no frame/video dependencies.  Times are
//! quantized to milliseconds while being assembled, matching the Python
//! scheduler's de-duplication and ordering semantics.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScanConfig {
    pub priority_window: Vec<[f64; 2]>,
    pub scan_start: f64,
    pub scan_end: Option<f64>,
    pub coarse_step: f64,
    pub candidate_lookback: f64,
    pub candidate_lookahead: f64,
    pub candidate_step: f64,
    pub no_full_scan: bool,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            priority_window: vec![[31.0, 43.0], [45.0, 53.0]],
            scan_start: 0.0,
            scan_end: None,
            coarse_step: 4.0,
            candidate_lookback: 8.0,
            candidate_lookahead: 0.5,
            candidate_step: 4.0,
            no_full_scan: false,
        }
    }
}

/// Build an inclusive range, with the same one-microsecond tolerance used by
/// the Python implementation. Invalid/non-positive steps produce no samples.
fn time_range(start: f64, end: f64, step: f64) -> impl Iterator<Item = f64> {
    let mut t = start;
    std::iter::from_fn(move || {
        if !t.is_finite() || !end.is_finite() || step <= 0.0 || t > end + 1e-6 {
            return None;
        }
        // Python round(t, 3) rounds the original binary value to decimal,
        // not a rounded intermediate t*1000 (which can land on a false tie).
        // Rust's fixed precision formatter uses the same ties-to-even decimal
        // rounding; parsing restores the corresponding binary f64.
        let result = format!("{t:.3}")
            .parse::<f64>()
            .expect("finite decimal time");
        t += step;
        Some(result)
    })
}

// Python's round(x) uses ties-to-even (unlike f64::round, which rounds
// halfway cases away from zero). All scheduler values are non-negative.
fn round_even(value: f64) -> i64 {
    let lower = value.floor();
    let fraction = value - lower;
    let rounded = if fraction < 0.5 {
        lower
    } else if fraction > 0.5 {
        lower + 1.0
    } else if (lower as i64) % 2 == 0 {
        lower
    } else {
        lower + 1.0
    };
    rounded as i64
}

fn key(seconds: f64) -> i64 {
    round_even(seconds * 1000.0)
}

fn finish(times: BTreeSet<i64>) -> Vec<f64> {
    times.into_iter().map(|ms| ms as f64 / 1000.0).collect()
}

fn end_limit(duration: f64, config: &ScanConfig) -> f64 {
    config.scan_end.map_or(duration, |end| duration.min(end))
}

/// Build the fixed/text-priority scan schedule.
///
/// Candidate and priority windows are inclusive. Duplicate timestamps are
/// removed at millisecond precision and output is sorted chronologically.
pub fn build_text_priority_scan_times(
    duration: f64,
    candidate_times: &[f64],
    config: &ScanConfig,
) -> Vec<f64> {
    let end = end_limit(duration, config);
    let mut times = BTreeSet::new();
    if !config.no_full_scan {
        for t in time_range(config.scan_start, end, config.coarse_step) {
            times.insert(key(t));
        }
    }
    for &[start, stop] in &config.priority_window {
        let lo = config.scan_start.max(0.0).max(start);
        let hi = end.min(stop);
        if hi >= lo {
            for t in time_range(lo, hi, config.candidate_step) {
                times.insert(key(t));
            }
        }
    }
    for &candidate in candidate_times {
        let lo = config
            .scan_start
            .max(0.0)
            .max(candidate - config.candidate_lookback);
        let hi = end.min(candidate + config.candidate_lookahead);
        if hi >= lo {
            for t in time_range(lo, hi, config.candidate_step) {
                times.insert(key(t));
            }
        }
    }
    finish(times)
}

/// Build the adaptive discovery schedule. Dense samples are added by the
/// caller after a seed identifies an event; this function only builds seeds.
pub fn build_adaptive_scan_times(
    duration: f64,
    candidate_times: &[f64],
    config: &ScanConfig,
) -> Vec<f64> {
    let end = end_limit(duration, config);
    let mut times = BTreeSet::new();
    if !config.no_full_scan {
        for t in time_range(config.scan_start, end, config.coarse_step) {
            times.insert(key(t));
        }
    }
    for &[start, stop] in &config.priority_window {
        let lo = config.scan_start.max(0.0).max(start);
        let hi = end.min(stop);
        if hi >= lo {
            for t in time_range(lo, hi, config.candidate_step) {
                times.insert(key(t));
            }
            times.insert(key(lo));
            times.insert(key(hi));
        }
    }
    for &candidate in candidate_times {
        let lo = config
            .scan_start
            .max(0.0)
            .max(candidate - config.candidate_lookback);
        let hi = end.min(candidate + config.candidate_lookahead);
        if hi >= lo {
            for t in time_range(lo, hi, config.candidate_step) {
                times.insert(key(t));
            }
            times.insert(key(lo));
            times.insert(key(hi));
            if lo <= candidate && candidate <= hi {
                times.insert(key(candidate));
            }
        }
    }
    finish(times)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_match_python_schedule() {
        let c = ScanConfig::default();
        assert_eq!(
            build_text_priority_scan_times(10.0, &[], &c),
            vec![0.0, 4.0, 8.0]
        );
    }
    #[test]
    fn adaptive_adds_window_anchors_and_dedupes() {
        let c = ScanConfig {
            priority_window: vec![[2.0, 5.0]],
            no_full_scan: true,
            candidate_lookback: 1.0,
            candidate_step: 4.0,
            ..Default::default()
        };
        assert_eq!(
            build_adaptive_scan_times(10.0, &[3.0, 3.0], &c),
            vec![2.0, 3.0, 3.5, 5.0]
        );
    }
    #[test]
    fn config_round_trips_json() {
        let c = ScanConfig::default();
        let decoded: ScanConfig =
            serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert_eq!(c, decoded);
    }
}
