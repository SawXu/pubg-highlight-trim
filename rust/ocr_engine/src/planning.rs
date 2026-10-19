//! Production clip planning, frozen from pipeline.py at 75916e2.
//! No event parsing, scanning or media I/O lives here. The JSONL adapter is a
//! temporary migration boundary for the remaining Python application shell.
use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, Write};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Detection {
    pub duration_sec: f64,
    pub event_sec: Option<f64>,
    pub method: String,
    pub detector: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub scores: String,
    #[serde(default)]
    pub ocr_seconds: f64,
    #[serde(default)]
    pub sampled_count: u64,
    #[serde(default)]
    pub detect_seconds: f64,
    #[serde(default)]
    pub ocr_frame_seconds: f64,
    #[serde(default)]
    pub ocr_coarse_frames: u64,
    #[serde(default)]
    pub ocr_refine_frames: u64,
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub event_key: String,
    #[serde(default = "one")]
    pub event_count: String,
    #[serde(default)]
    pub event_secs: String,
    #[serde(default)]
    pub event_weapon: String,
    #[serde(default)]
    pub context_rule: String,
    #[serde(default)]
    pub clip_before_sec: f64,
    #[serde(default)]
    pub clip_after_sec: f64,
    #[serde(default)]
    pub ocr_gate_skipped_frames: u64,
    #[serde(default)]
    pub ocr_requests: u64,
    #[serde(default)]
    pub ocr_successes: u64,
    #[serde(default)]
    pub ocr_cache_hits: u64,
    #[serde(default)]
    pub ocr_skipped: u64,
    #[serde(default)]
    pub refine_budget_used: u64,
    #[serde(default)]
    pub assist_budget_used: u64,
    #[serde(default)]
    pub termination_reason: String,
    #[serde(default)]
    pub gate_reason: String,
    #[serde(default)]
    pub refine_ocr_seconds: f64,
    #[serde(default)]
    pub assist_ocr_seconds: f64,
}
fn one() -> String {
    "1".into()
}
fn default_minimum() -> Option<f64> {
    Some(2.0)
}
fn default_molotov() -> Option<f64> {
    Some(10.0)
}
fn default_language() -> String {
    "zh-Hans".into()
}

#[derive(Debug, Deserialize)]
pub struct PlanningConfig {
    pub seconds_before: f64,
    pub seconds_after: f64,
    #[serde(default = "default_minimum")]
    pub min_event_sec: Option<f64>,
    #[serde(default = "default_molotov")]
    pub molotov_elim_before: Option<f64>,
    #[serde(default = "default_language")]
    pub game_lang: String,
    #[serde(default)]
    pub split_targets: bool,
}

#[derive(Debug, Deserialize)]
pub struct PlanningRequest {
    pub config: PlanningConfig,
    pub detections: Vec<Detection>,
}
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct ClipPlan {
    pub detection: Detection,
    pub start_sec: f64,
    pub end_sec: f64,
}
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct PlanningResult {
    pub clips: Vec<ClipPlan>,
    pub skipped: Vec<Detection>,
}

// Python re's Unicode whitespace includes four information separators in
// addition to Unicode White_Space (Rust's is_whitespace).
fn python_whitespace(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}
fn molotov_weapon(weapon: &str, language: &str) -> bool {
    let normalized: String = weapon.chars().filter(|&c| !python_whitespace(c)).collect();
    // Python re.IGNORECASE additionally equates these Unicode characters with
    // ASCII i/s/k; ordinary Rust/Unicode lowercase alone is not equivalent.
    let weapon = if language == "en" {
        normalized
            .chars()
            .map(|c| match c {
                'İ' | 'ı' => 'i',
                'ſ' => 's',
                'K' => 'k',
                _ => c.to_ascii_lowercase(),
            })
            .collect()
    } else {
        normalized
    };
    let terms: &[&str] = match language {
        "en" => &["molotov", "firebomb", "fire-bomb", "gascan", "c4"],
        "zh-Hant" => &["燃燒彈", "汽油彈", "molotov"],
        _ => &["燃烧弹", "燃燒彈", "燃烧瓶", "燃燒瓶", "molotov"],
    };
    terms.iter().any(|term| weapon.contains(term))
}
fn apply_context(d: &mut Detection, config: &PlanningConfig) {
    d.clip_before_sec = config.seconds_before.max(0.0);
    d.clip_after_sec = config.seconds_after.max(0.0);
    d.context_rule = "default".into();
    let molotov = config.molotov_elim_before.unwrap_or(0.0).max(0.0);
    let eliminate = d
        .event_key
        .split(';')
        .any(|key| key.splitn(3, ':').nth(1) == Some("eliminate"));
    if molotov > 0.0
        && d.event_sec.is_some()
        && eliminate
        && molotov_weapon(&d.event_weapon, &config.game_lang)
    {
        d.clip_before_sec = d.clip_before_sec.max(molotov);
        d.context_rule = "molotov-elim-context".into();
    }
}
fn unique_join<'a>(values: impl Iterator<Item = &'a str>, separator: &str) -> String {
    let mut unique = Vec::new();
    for value in values.filter(|v| !v.is_empty()) {
        if !unique.contains(&value) {
            unique.push(value);
        }
    }
    unique.join(separator)
}
fn merge_group(group: &[Detection]) -> Detection {
    // All counters/timings and detector come from the LAST observation, not
    // sums. Duration/timestamp come from the first. Even a singleton is rebuilt.
    let first = &group[0];
    let mut merged = group.last().unwrap().clone();
    merged.duration_sec = first.duration_sec;
    merged.event_sec = first.event_sec;
    merged.method = unique_join(group.iter().map(|d| d.method.as_str()), "+");
    if merged.method.is_empty() {
        merged.method = first.method.clone();
    }
    merged.text = unique_join(group.iter().map(|d| d.text.as_str()), " || ");
    merged.scores = group
        .iter()
        .map(|d| d.scores.as_str())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(";");
    let targets = unique_join(group.iter().map(|d| d.target.as_str()), ";");
    let target_count = group
        .iter()
        .map(|d| d.target.as_str())
        .filter(|s| !s.is_empty())
        .collect::<std::collections::HashSet<_>>()
        .len();
    merged.target = if target_count == 1 {
        targets
    } else {
        "both".into()
    };
    merged.event_key = group
        .iter()
        .map(|d| d.event_key.as_str())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(";");
    merged.event_count = group.len().to_string();
    merged.event_secs = group
        .iter()
        .filter_map(|d| d.event_sec)
        .map(|t| format!("{t:.3}"))
        .collect::<Vec<_>>()
        .join(";");
    merged.event_weapon = unique_join(group.iter().map(|d| d.event_weapon.as_str()), ";");
    merged.context_rule = unique_join(
        group
            .iter()
            .map(|d| d.context_rule.as_str())
            .filter(|r| *r != "default"),
        "+",
    );
    if merged.context_rule.is_empty() {
        merged.context_rule = "default".into();
    }
    merged.clip_before_sec = group
        .iter()
        .map(|d| d.clip_before_sec.max(0.0))
        .fold(0.0, f64::max);
    merged.clip_after_sec = group
        .iter()
        .map(|d| d.clip_after_sec.max(0.0))
        .fold(0.0, f64::max);
    merged
}
fn merged_plans(mut detections: Vec<Detection>, config: &PlanningConfig) -> Vec<ClipPlan> {
    detections.retain(|d| d.event_sec.is_some());
    detections.sort_by(|a, b| {
        a.event_sec
            .unwrap()
            .partial_cmp(&b.event_sec.unwrap())
            .unwrap()
    });
    let mut plans = Vec::new();
    let mut group = Vec::new();
    let mut group_start = 0.0;
    let mut group_end = 0.0;
    for d in detections {
        // Python's `max(0, clip_before) or before` deliberately falls back for
        // zero, including raw negative config values. Preserve that behavior.
        let before = if d.clip_before_sec > 0.0 {
            d.clip_before_sec
        } else {
            config.seconds_before
        };
        let after = if d.clip_after_sec > 0.0 {
            d.clip_after_sec
        } else {
            config.seconds_after
        };
        let event = d.event_sec.unwrap();
        let start = (event - before).max(0.0);
        let mut end = (event + after).min(d.duration_sec);
        if end <= start {
            end = (start + 0.1).min(d.duration_sec);
        }
        if !group.is_empty() && start <= group_end + 1e-6 {
            group.push(d);
            group_end = group_end.max(end);
            // Historical behavior: do NOT rewind group_start for a later
            // event with a longer before window. See fixture known-defect note.
            continue;
        }
        if !group.is_empty() {
            plans.push(ClipPlan {
                detection: merge_group(&group),
                start_sec: group_start,
                end_sec: group_end,
            });
        }
        group = vec![d];
        group_start = start;
        group_end = end;
    }
    if !group.is_empty() {
        plans.push(ClipPlan {
            detection: merge_group(&group),
            start_sec: group_start,
            end_sec: group_end,
        });
    }
    plans
}

/// Canonical planning boundary: minimum-event partition, language-dependent
/// context, optional target split, stable chronological merge and report metadata.
/// Unlike the previous simplified planner, no new duration/negative-time filter
/// or synthetic match-end event/window is introduced.
pub fn plan_clips(request: PlanningRequest) -> PlanningResult {
    let (kept, skipped) = prepare_detections(request.detections, &request.config);
    PlanningResult {
        clips: plan_prepared(kept, &request.config),
        skipped,
    }
}

/// Apply the existing minimum-event partition and context rules before optional
/// cross-source verification. No clip grouping has happened yet.
pub fn prepare_detections(
    detections: Vec<Detection>,
    config: &PlanningConfig,
) -> (Vec<Detection>, Vec<Detection>) {
    let minimum = config.min_event_sec.unwrap_or(0.0).max(0.0);
    let mut kept = Vec::new();
    let mut skipped = Vec::new();
    for mut d in detections {
        if d.event_sec.is_some_and(|t| t < minimum) {
            d.method = "skipped-before-min-event-sec".into();
            skipped.push(d);
        } else {
            apply_context(&mut d, config);
            kept.push(d);
        }
    }
    (kept, skipped)
}

/// Merge already prepared, verified events without recomputing context.
pub fn plan_prepared(kept: Vec<Detection>, config: &PlanningConfig) -> Vec<ClipPlan> {
    if config.split_targets {
        ["self-death", "own-kill"]
            .into_iter()
            .flat_map(|target| {
                merged_plans(
                    kept.iter()
                        .filter(|d| d.target == target)
                        .cloned()
                        .collect(),
                    &config,
                )
            })
            .collect()
    } else {
        merged_plans(kept, &config)
    }
}

/// One request/response per line, no OCR/model initialization or diagnostics on
/// stdout. Errors fail the subprocess rather than masquerading as no clips.
pub fn run_jsonl(
    reader: impl BufRead,
    mut writer: impl Write,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut count = 0;
    for line in reader.lines() {
        let line = line?;
        let request: PlanningRequest = serde_json::from_str(&line)?;
        serde_json::to_writer(&mut writer, &plan_clips(request))?;
        writer.write_all(b"\n")?;
        writer.flush()?;
        count += 1;
    }
    if count == 0 {
        return Err(
            io::Error::new(io::ErrorKind::InvalidInput, "expected a planning request").into(),
        );
    }
    Ok(())
}
