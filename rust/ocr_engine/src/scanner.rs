//! OCR scanning, refinement and cross-frame event lifecycle.
//!
//! The backend owns only media/image/inference IO and a monotonic clock. The
//! scanner owns per-source caches, budgets and the Python scan state machine.
use crate::scan::{build_adaptive_scan_times, build_text_priority_scan_times, ScanConfig};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OcrConfig {
    pub target: String,
    pub language: String,
    pub priority_window: Vec<[f64; 2]>,
    pub scan_start: f64,
    pub scan_end: Option<f64>,
    pub coarse_step: f64,
    pub candidate_lookback: f64,
    pub candidate_lookahead: f64,
    pub candidate_step: f64,
    pub refine_before: f64,
    pub refine_after: f64,
    pub refine_step: f64,
    pub refine_search_step: f64,
    pub event_dedupe_seconds: f64,
    pub no_full_scan: bool,
    pub roi: [f64; 4],
    pub assist_roi: [f64; 4],
    pub assist_after: f64,
    pub assist_step: f64,
    pub ocr_width: u32,
    pub brightness_gate: bool,
    pub brightness_gate_mode: String,
    pub brightness_gate_roi: [f64; 4],
    pub brightness_gate_width: u32,
    pub sampling_mode: String,
    pub adaptive_step: f64,
    pub adaptive_window: f64,
    pub ocr_max_calls: Option<usize>,
    pub ocr_min_interval: f64,
    pub refine_max_frames: Option<usize>,
    pub assist_max_frames: Option<usize>,
    pub refine_max_seconds: Option<f64>,
    pub assist_max_seconds: Option<f64>,
    pub decoded_frame_cache_size: usize,
}
impl Default for OcrConfig {
    fn default() -> Self {
        Self {
            target: "self-death".into(),
            language: "zh-Hans".into(),
            priority_window: vec![[31.0, 43.0], [45.0, 53.0]],
            scan_start: 0.0,
            scan_end: None,
            coarse_step: 4.0,
            candidate_lookback: 8.0,
            candidate_lookahead: 0.5,
            candidate_step: 4.0,
            refine_before: 6.0,
            refine_after: 0.4,
            refine_step: 0.5,
            refine_search_step: 2.0,
            event_dedupe_seconds: 5.0,
            no_full_scan: false,
            roi: [0.30, 0.66, 0.70, 0.75],
            assist_roi: [0.25, 0.64, 0.75, 0.86],
            assist_after: 1.2,
            assist_step: 0.5,
            ocr_width: 768,
            brightness_gate: false,
            brightness_gate_mode: "light".into(),
            brightness_gate_roi: [0.26, 0.635, 0.74, 0.725],
            brightness_gate_width: 768,
            sampling_mode: "fixed".into(),
            adaptive_step: 0.5,
            adaptive_window: 2.0,
            ocr_max_calls: None,
            ocr_min_interval: 0.0,
            refine_max_frames: None,
            assist_max_frames: None,
            refine_max_seconds: None,
            assist_max_seconds: None,
            decoded_frame_cache_size: 8,
        }
    }
}
impl OcrConfig {
    pub fn validate(&self) -> Result<(), String> {
        for (name, roi) in [
            ("roi", self.roi),
            ("assist_roi", self.assist_roi),
            ("brightness_gate_roi", self.brightness_gate_roi),
        ] {
            if !roi.iter().all(|x| x.is_finite())
                || !(0.0 <= roi[0]
                    && roi[0] < roi[2]
                    && roi[2] <= 1.0
                    && 0.0 <= roi[1]
                    && roi[1] < roi[3]
                    && roi[3] <= 1.0)
            {
                return Err(format!(
                    "{name} values must be ratios in ascending order, between 0 and 1"
                ));
            }
        }
        if !["full", "light", "none"].contains(&self.brightness_gate_mode.as_str()) {
            return Err("brightness_gate_mode must be full, light, or none".into());
        }
        if !["fixed", "adaptive"].contains(&self.sampling_mode.as_str()) {
            return Err("sampling_mode must be fixed or adaptive".into());
        }
        if !self.scan_start.is_finite() || self.scan_start < 0.0 {
            return Err("scan_start must be a finite non-negative number".into());
        }
        if self
            .scan_end
            .is_some_and(|end| !end.is_finite() || end <= self.scan_start)
        {
            return Err("scan_end must be greater than scan_start".into());
        }
        if self.priority_window.iter().any(|&[start, end]| {
            !start.is_finite() || !end.is_finite() || start < 0.0 || end <= start
        }) {
            return Err("priority windows must satisfy 0 <= start < end".into());
        }
        for (name, value) in [
            ("coarse_step", self.coarse_step),
            ("candidate_step", self.candidate_step),
            ("refine_step", self.refine_step),
            ("refine_search_step", self.refine_search_step),
            ("assist_step", self.assist_step),
            ("adaptive_step", self.adaptive_step),
        ] {
            // Match the legacy positive-step check exactly. NaN/+infinity
            // produce the initial sample and terminate the range on the next
            // comparison; negative/zero still fail before media is touched.
            if value <= 0.0 {
                return Err(format!("{name} must be positive"));
            }
        }
        for (name, value) in [
            ("candidate_lookback", self.candidate_lookback),
            ("candidate_lookahead", self.candidate_lookahead),
            ("refine_before", self.refine_before),
            ("refine_after", self.refine_after),
            ("assist_after", self.assist_after),
            ("adaptive_window", self.adaptive_window),
            ("ocr_min_interval", self.ocr_min_interval),
        ] {
            if !value.is_finite() || value < 0.0 {
                return Err(format!("{name} must be a finite non-negative number"));
            }
        }
        for (name, value) in [
            ("refine_max_seconds", self.refine_max_seconds),
            ("assist_max_seconds", self.assist_max_seconds),
        ] {
            if value.is_some_and(|value| !value.is_finite() || value < 0.0) {
                return Err(format!("{name} must be finite and non-negative or None"));
            }
        }
        if self.decoded_frame_cache_size < 1 {
            return Err("decoded_frame_cache_size must be positive".into());
        }
        Ok(())
    }
    fn schedule(&self) -> ScanConfig {
        ScanConfig {
            priority_window: self.priority_window.clone(),
            scan_start: self.scan_start,
            scan_end: self.scan_end,
            coarse_step: self.coarse_step,
            candidate_lookback: self.candidate_lookback,
            candidate_lookahead: self.candidate_lookahead,
            candidate_step: self.candidate_step,
            no_full_scan: self.no_full_scan,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OcrRuntimeStats {
    pub ocr_requests: usize,
    pub ocr_successes: usize,
    pub ocr_cache_hits: usize,
    pub ocr_skipped: usize,
    pub gate_skipped: usize,
    pub refine_budget_used: usize,
    pub assist_budget_used: usize,
    pub refine_calls: usize,
    pub assist_calls: usize,
    pub refine_ocr_seconds: f64,
    pub assist_ocr_seconds: f64,
    pub last_gate_reason: String,
    pub gate_reasons: Vec<String>,
    pub last_skip_reason: String,
    pub last_cache_hit: bool,
    pub last_request_sec: Option<f64>,
    pub termination_reason: String,
}

pub struct FrameRead<F> {
    pub frame: Option<F>,
    pub seconds: f64,
}
pub struct Recognition {
    pub text: String,
    pub scores: String,
    pub seconds: f64,
    pub frame_seconds: f64,
}
pub struct GateResult {
    pub passed: bool,
    pub reason: String,
}
/// Media/inference boundary. `None` means a failed single-frame read; `Err`
/// means a source/model/inference failure and must never become a clean miss.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanFailureKind {
    SourceUnreadable,
    Runtime,
}
pub trait ScanBackend {
    type Frame;
    /// Classification is independent of the human-readable error returned by a read.
    fn failure_kind(&self) -> ScanFailureKind {
        ScanFailureKind::Runtime
    }
    fn read_frame(&mut self, seconds: f64) -> Result<FrameRead<Self::Frame>, String>;
    fn recognize(
        &mut self,
        frame: &Self::Frame,
        roi: [f64; 4],
        width: u32,
    ) -> Result<Recognition, String>;
    fn gate(&mut self, frame: &Self::Frame, config: &OcrConfig) -> Result<GateResult, String>;
    fn now(&mut self) -> f64;
}

pub use crate::events::TextEvent;
/// Language rules are implemented by the event module, not by a second scan
/// parser. All fuzzy equivalence remains in that one implementation.
pub trait EventRules {
    fn extract(&self, text: &str, target: &str, allow_assist: bool) -> Vec<TextEvent>;
    fn same_event(&self, left: &TextEvent, right: &TextEvent) -> bool;
    fn same_noisy_event(&self, left: &TextEvent, right: &TextEvent) -> bool;
    fn same_followup_subject(&self, left: &TextEvent, right: &TextEvent) -> bool;
    fn has_assist(&self, text: &str) -> bool;
    fn is_assist(&self, text: &str) -> bool;
    fn is_delayed(&self, text: &str) -> bool;
    fn has_own_candidate(&self, text: &str) -> bool;
}
impl EventRules for crate::events::LanguageProfile {
    fn extract(&self, text: &str, target: &str, allow_assist: bool) -> Vec<TextEvent> {
        use crate::events::{extract_own_kill_events, extract_text_events, Target};
        if allow_assist {
            return extract_own_kill_events(text, true, Some(self));
        }
        let target = match target {
            "own-kill" => Target::OwnKill,
            "both" => Target::Both,
            _ => Target::SelfDeath,
        };
        extract_text_events(text, target, Some(self))
    }
    fn same_event(&self, left: &TextEvent, right: &TextEvent) -> bool {
        crate::events::same_text_event(left, right)
    }
    fn same_noisy_event(&self, left: &TextEvent, right: &TextEvent) -> bool {
        crate::events::same_noisy_close_event(left, right)
    }
    fn same_followup_subject(&self, left: &TextEvent, right: &TextEvent) -> bool {
        crate::events::same_followup_subject(left, right, Some(self))
    }
    fn has_assist(&self, text: &str) -> bool {
        crate::events::has_assist_text(text, Some(self))
    }
    fn is_assist(&self, text: &str) -> bool {
        crate::events::is_assist_own_kill_text(text, Some(self))
    }
    fn is_delayed(&self, text: &str) -> bool {
        crate::events::is_delayed_own_elim_text(text, Some(self))
    }
    fn has_own_candidate(&self, text: &str) -> bool {
        crate::events::has_own_kill_candidate(text, Some(self))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EventDetection {
    pub duration_sec: f64,
    pub event_sec: Option<f64>,
    pub method: String,
    pub detector: String,
    pub text: String,
    pub scores: String,
    pub ocr_seconds: f64,
    pub sampled_count: usize,
    pub detect_seconds: f64,
    pub ocr_frame_seconds: f64,
    pub ocr_coarse_frames: usize,
    pub ocr_refine_frames: usize,
    pub target: String,
    pub event_key: String,
    pub event_count: String,
    pub event_secs: String,
    pub event_weapon: String,
    pub context_rule: String,
    pub clip_before_sec: f64,
    pub clip_after_sec: f64,
    pub ocr_gate_skipped_frames: usize,
    pub ocr_requests: usize,
    pub ocr_successes: usize,
    pub ocr_cache_hits: usize,
    pub ocr_skipped: usize,
    pub refine_budget_used: usize,
    pub assist_budget_used: usize,
    pub termination_reason: String,
    pub gate_reason: String,
    pub refine_ocr_seconds: f64,
    pub assist_ocr_seconds: f64,
}
/// Completed scan measurements, distinct from event-time detection snapshots.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ScanWork {
    pub ocr_seconds: f64,
    pub frame_seconds: f64,
    pub detect_seconds: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanReport {
    pub detections: Vec<EventDetection>,
    pub stats: OcrRuntimeStats,
    #[serde(default)]
    pub work: ScanWork,
}

#[derive(Clone, Default)]
struct ReadResult {
    text: String,
    scores: String,
    seconds: f64,
    frame_seconds: f64,
    method: String,
    skipped_reason: String,
}
struct Scanner<'a, B: ScanBackend, R: EventRules> {
    backend: &'a mut B,
    rules: &'a R,
    config: &'a OcrConfig,
    stats: OcrRuntimeStats,
    frames: VecDeque<(i64, B::Frame)>,
    cache: BTreeMap<(bool, i64), (String, String)>,
}
fn key(t: f64) -> i64 {
    (t * 1000.0).round_ties_even() as i64
}
fn rounded(t: f64) -> f64 {
    // Python round(t, 3) rounds the binary value to a decimal grid, not
    // round(t * 1000) / 1000. For example round(0.0025, 3) is 0.003.
    format!("{t:.3}")
        .parse()
        .expect("formatted finite timestamp")
}
fn times(start: f64, end: f64, step: f64) -> Vec<f64> {
    let mut t = start;
    let mut result = vec![];
    while t <= end + 1e-6 {
        result.push(rounded(t));
        t += step;
    }
    result
}
fn normalize(text: &str) -> String {
    crate::events::normalize_ocr_text(text)
}

impl<B: ScanBackend, R: EventRules> Scanner<'_, B, R> {
    fn read(
        &mut self,
        sec: f64,
        assist: bool,
        gate: bool,
        target: &str,
    ) -> Result<ReadResult, String> {
        self.stats.last_skip_reason.clear();
        self.stats.last_gate_reason.clear();
        self.stats.last_cache_hit = false;
        let frame_key = key(sec.max(0.0));
        let cache_key = (assist, frame_key);
        let mut result = ReadResult::default();
        if let Some((text, scores)) = self.cache.get(&cache_key) {
            self.stats.ocr_cache_hits += 1;
            self.stats.last_cache_hit = true;
            result.text = text.clone();
            result.scores = scores.clone();
        } else {
            let skip = if self
                .config
                .ocr_max_calls
                .is_some_and(|n| self.stats.ocr_requests >= n)
            {
                self.stats.termination_reason = "ocr-budget-exhausted".into();
                "ocr-budget-exhausted"
            } else if self.config.ocr_min_interval > 0.0
                && self
                    .stats
                    .last_request_sec
                    .is_some_and(|last| (sec - last).abs() < self.config.ocr_min_interval - 1e-6)
            {
                "ocr-min-interval"
            } else {
                ""
            };
            if !skip.is_empty() {
                self.stats.ocr_skipped += 1;
                self.stats.last_skip_reason = skip.into();
                result.method = skip.into();
                result.skipped_reason = skip.into();
                return Ok(result);
            }
            let frame_started = self.backend.now();
            if !self.frames.iter().any(|(k, _)| *k == frame_key) {
                let read = self.backend.read_frame(sec)?;
                result.frame_seconds = read.seconds;
                let Some(frame) = read.frame else {
                    self.stats.ocr_skipped += 1;
                    self.stats.last_skip_reason = "frame-read-failed".into();
                    result.method = "frame-read-failed".into();
                    result.skipped_reason = "frame-read-failed".into();
                    return Ok(result);
                };
                self.frames.push_back((frame_key, frame));
                if self.frames.len() > self.config.decoded_frame_cache_size {
                    self.frames.pop_front();
                }
            }
            let frame = &self.frames.iter().find(|(k, _)| *k == frame_key).unwrap().1;
            if gate && self.config.brightness_gate && self.config.brightness_gate_mode != "none" {
                let gate_started = self.backend.now();
                let outcome = self.backend.gate(frame, self.config)?;
                result.frame_seconds += self.backend.now() - gate_started;
                self.stats.last_gate_reason = outcome.reason.clone();
                if !self.stats.gate_reasons.contains(&outcome.reason) {
                    self.stats.gate_reasons.push(outcome.reason.clone());
                }
                if !outcome.passed {
                    self.stats.ocr_skipped += 1;
                    self.stats.gate_skipped += 1;
                    self.stats.last_skip_reason = outcome.reason;
                    result.frame_seconds =
                        result.frame_seconds.max(self.backend.now() - frame_started);
                    result.method = "opencv-no-bright-event-text".into();
                    return Ok(result);
                }
            }
            let roi = if assist {
                self.config.assist_roi
            } else {
                self.config.roi
            };
            self.stats.ocr_requests += 1;
            self.stats.last_request_sec = Some(sec);
            let recognition = self.backend.recognize(frame, roi, self.config.ocr_width)?;
            self.stats.ocr_successes += 1;
            result.frame_seconds += recognition.frame_seconds;
            result.text = normalize(&recognition.text);
            result.scores = recognition.scores;
            result.seconds = recognition.seconds;
            self.cache
                .insert(cache_key, (result.text.clone(), result.scores.clone()));
        }
        result.method = if assist {
            if self.rules.has_assist(&result.text) {
                "paddle-own-kill-assist-skipped"
            } else {
                "paddle-not-assist-text"
            }
            .into()
        } else if result.text.is_empty() {
            "paddle-not-target-text".into()
        } else if ["own-kill", "both"].contains(&target)
            && self.rules.has_own_candidate(&result.text)
            && self
                .rules
                .extract(&result.text, "own-kill", false)
                .is_empty()
            && self.rules.is_delayed(&result.text)
        {
            "paddle-own-kill-delayed-elim-skipped".into()
        } else if ["own-kill", "both"].contains(&target)
            && self.rules.has_own_candidate(&result.text)
            && self
                .rules
                .extract(&result.text, "own-kill", false)
                .is_empty()
            && self.rules.is_assist(&result.text)
        {
            "paddle-own-kill-assist-skipped".into()
        } else {
            self.rules
                .extract(&result.text, target, false)
                .first()
                .map_or_else(|| "paddle-not-target-text".into(), |e| e.method.clone())
        };
        Ok(result)
    }

    fn matches(&self, result: &ReadResult, event: &TextEvent) -> bool {
        self.rules
            .extract(&result.text, &event.target, false)
            .iter()
            .any(|candidate| self.rules.same_event(candidate, event))
    }
    fn refine_limit(&mut self, used: usize, started: f64) -> bool {
        if self
            .config
            .refine_max_frames
            .is_some_and(|limit| used >= limit)
        {
            self.stats.termination_reason = "refine-budget-exhausted".into();
            true
        } else if self
            .config
            .refine_max_seconds
            .is_some_and(|limit| self.backend.now() - started >= limit)
        {
            self.stats.termination_reason = "refine-time-budget-exhausted".into();
            true
        } else {
            false
        }
    }
    fn refine_read(
        &mut self,
        sec: f64,
        event: &TextEvent,
        cost: &mut Cost,
    ) -> Result<ReadResult, String> {
        self.stats.refine_budget_used += 1;
        self.stats.refine_calls += 1;
        let result = self.read(sec, false, false, &event.target)?;
        self.stats.refine_ocr_seconds += result.seconds;
        cost.add(&result);
        Ok(result)
    }
    fn refine(
        &mut self,
        coarse_sec: f64,
        event: &TextEvent,
        coarse: &ReadResult,
    ) -> Result<(f64, ReadResult, Cost), String> {
        let lo = (coarse_sec - self.config.refine_before).max(0.0);
        let mut cost = Cost::default();
        let mut used = 0;
        let mut hit_sec = coarse_sec;
        let mut hit = coarse.clone();
        if !self.matches(coarse, event) {
            if self.config.refine_max_frames == Some(0) {
                self.stats.termination_reason = "refine-budget-exhausted".into();
                return Ok((
                    coarse_sec,
                    ReadResult {
                        method: "paddle-refine-budget-exhausted".into(),
                        ..Default::default()
                    },
                    cost,
                ));
            }
            hit = self.refine_read(coarse_sec, event, &mut cost)?;
            used += 1;
            if !self.matches(&hit, event) {
                return Ok((
                    coarse_sec,
                    ReadResult {
                        method: "paddle-refine-missed".into(),
                        ..Default::default()
                    },
                    cost,
                ));
            }
        }
        let step = self.config.refine_search_step.max(self.config.refine_step);
        let started = self.backend.now();
        let mut t = rounded(hit_sec - step);
        let mut miss = None;
        while t >= lo - 1e-6 {
            if self.refine_limit(used, started) {
                break;
            }
            let result = self.refine_read(t, event, &mut cost)?;
            used += 1;
            if self.matches(&result, event) {
                hit_sec = t;
                hit = result;
                t = rounded(t - step);
            } else {
                miss = Some(t);
                break;
            }
        }
        if let Some(mut miss_sec) = miss {
            let tolerance = self.config.refine_step.max(0.001);
            while hit_sec - miss_sec > tolerance + 1e-6 {
                let mid = rounded((hit_sec + miss_sec) / 2.0);
                if mid <= miss_sec + 1e-6 || mid >= hit_sec - 1e-6 {
                    break;
                }
                if self.refine_limit(used, started) {
                    break;
                }
                let result = self.refine_read(mid, event, &mut cost)?;
                used += 1;
                if self.matches(&result, event) {
                    hit_sec = mid;
                    hit = result;
                } else {
                    miss_sec = mid;
                }
            }
        }
        Ok((hit_sec, hit, cost))
    }
    fn assist(
        &mut self,
        event_sec: f64,
        duration: f64,
        event: &TextEvent,
    ) -> Result<(Option<ReadResult>, Cost), String> {
        let mut cost = Cost::default();
        let started = self.backend.now();
        for (used, sec) in times(
            event_sec,
            duration.min(event_sec + self.config.assist_after),
            self.config.assist_step,
        )
        .into_iter()
        .enumerate()
        {
            if self
                .config
                .assist_max_frames
                .is_some_and(|limit| used >= limit)
            {
                self.stats.termination_reason = "assist-budget-exhausted".into();
                break;
            }
            if self
                .config
                .assist_max_seconds
                .is_some_and(|limit| self.backend.now() - started >= limit)
            {
                self.stats.termination_reason = "assist-time-budget-exhausted".into();
                break;
            }
            self.stats.assist_budget_used += 1;
            self.stats.assist_calls += 1;
            let result = self.read(sec, true, false, &event.target)?;
            self.stats.assist_ocr_seconds += result.seconds;
            cost.add(&result);
            if self.rules.is_assist(&result.text)
                && self
                    .rules
                    .extract(&result.text, "own-kill", true)
                    .iter()
                    .any(|candidate| self.rules.same_event(candidate, event))
            {
                return Ok((Some(result), cost));
            }
        }
        Ok((None, cost))
    }
    fn followup(
        &self,
        event: &TextEvent,
        event_sec: f64,
        seen: &[(TextEvent, f64)],
    ) -> Option<&'static str> {
        if event.action != "eliminate" {
            return None;
        }
        if event.target == "self-death"
            && seen.iter().any(|(prior, sec)| {
                prior.target == "self-death" && prior.action == "knock" && *sec <= event_sec + 1e-6
            })
        {
            return Some("paddle-followup-self-elimination-skipped-after-knock");
        }
        if seen.iter().any(|(prior, sec)| {
            prior.action == "knock"
                && *sec <= event_sec + 1e-6
                && (event.target != "own-kill"
                    || event_sec - sec <= self.config.event_dedupe_seconds)
                && self.rules.same_followup_subject(event, prior)
        }) {
            return Some("paddle-followup-elimination-skipped");
        }
        None
    }
}
#[derive(Default)]
struct Cost {
    sampled: usize,
    ocr: f64,
    frame: f64,
}
impl Cost {
    fn add(&mut self, result: &ReadResult) {
        self.sampled += 1;
        self.ocr += result.seconds;
        self.frame += result.frame_seconds;
    }
    fn merge(&mut self, other: &Cost) {
        self.sampled += other.sampled;
        self.ocr += other.ocr;
        self.frame += other.frame;
    }
}

/// Scan one source using fixed OCR results or a real video/OCR backend. No
/// model/frame failure is converted to a successful no-event report. All
/// caches and counters are scoped to this call, including repeated calls on
/// the same backend. Detection telemetry is snapshotted when each hit is
/// accepted, matching the historical reports; `stats` is the final runtime.
pub fn detect_events<B: ScanBackend, R: EventRules>(
    backend: &mut B,
    rules: &R,
    duration: f64,
    candidate_times: &[f64],
    config: &OcrConfig,
) -> Result<ScanReport, String> {
    config.validate()?;
    let started = backend.now();
    let mut scanner = Scanner {
        backend,
        rules,
        config,
        stats: OcrRuntimeStats::default(),
        frames: VecDeque::new(),
        cache: BTreeMap::new(),
    };
    let schedule = config.schedule();
    let initial = if config.sampling_mode == "adaptive" {
        build_adaptive_scan_times(duration, candidate_times, &schedule)
    } else {
        build_text_priority_scan_times(duration, candidate_times, &schedule)
    };
    let seeds: BTreeSet<_> = if config.sampling_mode == "adaptive" {
        initial.iter().map(|t| key(*t)).collect()
    } else {
        BTreeSet::new()
    };
    let mut scheduled: BTreeSet<_> = initial.iter().map(|t| key(*t)).collect();
    let mut pending = scheduled.clone();
    let mut cost = Cost::default();
    let mut coarse_count = 0;
    let mut refine_count = 0;
    let mut gate_count = 0;
    let mut last = ReadResult {
        method: "not-scanned".into(),
        ..Default::default()
    };
    let mut seen: Vec<(TextEvent, f64)> = vec![];
    let mut detections = vec![];
    while let Some(frame_key) = pending.pop_first() {
        let sec = frame_key as f64 / 1000.0;
        let seed = seeds.contains(&frame_key);
        if seed || config.sampling_mode != "adaptive" {
            coarse_count += 1;
        }
        let result = scanner.read(sec, false, config.brightness_gate, &config.target)?;
        cost.add(&result);
        if result.method == "opencv-no-bright-event-text" {
            gate_count += 1;
        }
        if result.skipped_reason == "ocr-budget-exhausted" {
            break;
        }
        if !result.text.is_empty() {
            last = result.clone();
        }
        let events = rules.extract(&result.text, &config.target, false);
        if seed && !events.is_empty() {
            for t in times(
                config.scan_start.max(sec),
                duration.min(sec + config.adaptive_window),
                config.adaptive_step,
            ) {
                let k = key(t);
                if scheduled.insert(k) {
                    pending.insert(k);
                }
            }
        }
        for event in events {
            if seen.iter().any(|(prior, t)| {
                rules.same_event(&event, prior)
                    || ((sec - t).abs() <= config.event_dedupe_seconds
                        && rules.same_noisy_event(&event, prior))
            }) {
                continue;
            }
            let (event_sec, refined, refine_cost) = scanner.refine(sec, &event, &result)?;
            refine_count += refine_cost.sampled;
            cost.merge(&refine_cost);
            let refined_event = rules
                .extract(&refined.text, &event.target, false)
                .into_iter()
                .find(|candidate| rules.same_event(candidate, &event))
                .unwrap_or_else(|| event.clone());
            if seen.iter().any(|(prior, t)| {
                (event_sec - t).abs() <= config.event_dedupe_seconds
                    && rules.same_noisy_event(&refined_event, prior)
            }) {
                continue;
            }
            if let Some(method) = scanner.followup(&refined_event, event_sec, &seen) {
                last = ReadResult {
                    text: fallback(&refined.text, &result.text),
                    scores: fallback(&refined.scores, &result.scores),
                    method: method.into(),
                    ..Default::default()
                };
                seen.push((refined_event, event_sec));
                continue;
            }
            if event.target == "own-kill" {
                let (assist, assist_cost) = scanner.assist(event_sec, duration, &refined_event)?;
                cost.merge(&assist_cost);
                if let Some(assist) = assist {
                    last = ReadResult {
                        text: normalize(&format!(
                            "{} {}",
                            fallback(&refined.text, &result.text),
                            assist.text
                        )),
                        scores: [fallback(&refined.scores, &result.scores), assist.scores]
                            .into_iter()
                            .filter(|s| !s.is_empty())
                            .collect::<Vec<_>>()
                            .join(";"),
                        method: "paddle-own-kill-assist-skipped".into(),
                        ..Default::default()
                    };
                    seen.push((event, event_sec));
                    continue;
                }
            }
            let mut detection = snapshot(
                &mut scanner,
                &cost,
                duration,
                coarse_count,
                refine_count,
                gate_count,
                started,
            );
            detection.event_sec = Some(event_sec);
            detection.method = refined_event.method.clone();
            detection.target = event.target;
            detection.text = fallback(&refined.text, &result.text);
            detection.scores = fallback(&refined.scores, &result.scores);
            detection.event_key = refined_event.key.clone();
            detection.event_secs = format!("{event_sec:.3}");
            detection.event_weapon = refined_event.weapon.clone();
            detections.push(detection);
            seen.push((refined_event, event_sec));
        }
    }
    if detections.is_empty() {
        let mut detection = snapshot(
            &mut scanner,
            &cost,
            duration,
            coarse_count,
            refine_count,
            gate_count,
            started,
        );
        detection.method = if last.method == "not-scanned" {
            "paddle-no-target-text-found".into()
        } else {
            last.method
        };
        detection.text = last.text;
        detection.scores = last.scores;
        detection.target = config.target.clone();
        detections.push(detection);
    }
    let work = ScanWork {
        ocr_seconds: cost.ocr,
        frame_seconds: cost.frame,
        detect_seconds: scanner.backend.now() - started,
    };
    Ok(ScanReport {
        detections,
        stats: scanner.stats,
        work,
    })
}
fn fallback(preferred: &str, other: &str) -> String {
    if preferred.is_empty() {
        other
    } else {
        preferred
    }
    .into()
}
fn snapshot<B: ScanBackend, R: EventRules>(
    scanner: &mut Scanner<'_, B, R>,
    cost: &Cost,
    duration: f64,
    coarse: usize,
    refine: usize,
    gates: usize,
    started: f64,
) -> EventDetection {
    EventDetection {
        duration_sec: duration,
        detector: "ocr".into(),
        event_count: "1".into(),
        ocr_seconds: cost.ocr,
        sampled_count: cost.sampled,
        detect_seconds: scanner.backend.now() - started,
        ocr_frame_seconds: cost.frame,
        ocr_coarse_frames: coarse,
        ocr_refine_frames: refine,
        ocr_gate_skipped_frames: gates,
        ocr_requests: scanner.stats.ocr_requests,
        ocr_successes: scanner.stats.ocr_successes,
        ocr_cache_hits: scanner.stats.ocr_cache_hits,
        ocr_skipped: scanner.stats.ocr_skipped,
        refine_budget_used: scanner.stats.refine_budget_used,
        assist_budget_used: scanner.stats.assist_budget_used,
        termination_reason: scanner.stats.termination_reason.clone(),
        gate_reason: if scanner.stats.gate_reasons.is_empty() && !scanner.config.brightness_gate {
            "disabled".into()
        } else {
            scanner.stats.gate_reasons.join(";")
        },
        refine_ocr_seconds: scanner.stats.refine_ocr_seconds,
        assist_ocr_seconds: scanner.stats.assist_ocr_seconds,
        ..Default::default()
    }
}
