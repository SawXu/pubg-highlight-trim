//! Stable UTF-8 JSON and UTF-8-BOM/CRLF CSV application report contract.
use crate::planning::Detection;
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Write},
    path::Path,
};
pub const RECORD_COLUMNS: &[&str] = &[
    "Index",
    "Name",
    "Target",
    "DurationSec",
    "Status",
    "EventSec",
    "EventSecs",
    "EventCount",
    "EventKeys",
    "EventWeapon",
    "ContextRule",
    "BeforeSec",
    "AfterSec",
    "KeepStartSec",
    "KeepEndSec",
    "KeepDurationSec",
    "Method",
    "Detector",
    "PaddleText",
    "PaddleScores",
    "OcrSeconds",
    "SampledFrames",
    "DetectSeconds",
    "OcrFrameSeconds",
    "OcrCoarseFrames",
    "OcrRefineFrames",
    "OcrGateSkippedFrames",
    "OcrGateReason",
    "OcrRequests",
    "OcrSuccesses",
    "OcrCacheHits",
    "OcrSkipped",
    "RefineBudgetUsed",
    "AssistBudgetUsed",
    "TerminationReason",
    "RefineOcrSeconds",
    "AssistOcrSeconds",
    "ProbeSeconds",
    "TrimSeconds",
    "TotalSeconds",
    "Encoder",
    "Output",
];
pub const CANDIDATE_COLUMNS: &[&str] = &[
    "SchemaVersion",
    "RunId",
    "Name",
    "Status",
    "Target",
    "StartSec",
    "EndSec",
    "EventSec",
    "EventSecs",
    "Score",
    "GateStatus",
    "GateReason",
    "SamplingDensity",
    "SampledFrames",
    "OcrRequests",
    "OcrSuccesses",
    "OcrSkipped",
    "OcrCacheHits",
    "OcrCoarseFrames",
    "OcrRefineFrames",
    "RefineBudgetUsed",
    "AssistBudgetUsed",
    "RefineOcrSeconds",
    "AssistOcrSeconds",
    "CacheStatus",
    "TerminationReason",
    "Method",
    "Detector",
];
pub type Record = BTreeMap<String, String>;
fn put(row: &mut Record, k: &str, v: impl ToString) {
    row.insert(k.into(), v.to_string());
}
fn fixed(n: f64) -> String {
    format!("{n:.3}")
}
fn event(n: Option<f64>) -> String {
    n.map(fixed).unwrap_or_default()
}
pub fn blank_record(
    index: usize,
    path: &Path,
    duration: f64,
    status: &str,
    method: &str,
    detector: &str,
) -> Record {
    let mut r = RECORD_COLUMNS
        .iter()
        .map(|k| (k.to_string(), String::new()))
        .collect::<Record>();
    for (k, v) in [
        ("Index", index.to_string()),
        (
            "Name",
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
        ),
        ("DurationSec", fixed(duration)),
        ("Status", status.into()),
        ("Method", method.into()),
        ("Detector", detector.into()),
    ] {
        put(&mut r, k, v)
    }
    for k in [
        "OcrSeconds",
        "DetectSeconds",
        "OcrFrameSeconds",
        "RefineOcrSeconds",
        "AssistOcrSeconds",
        "ProbeSeconds",
        "TrimSeconds",
        "TotalSeconds",
    ] {
        put(&mut r, k, "0.000")
    }
    for k in [
        "EventCount",
        "SampledFrames",
        "OcrCoarseFrames",
        "OcrRefineFrames",
        "OcrGateSkippedFrames",
        "OcrRequests",
        "OcrSuccesses",
        "OcrCacheHits",
        "OcrSkipped",
        "RefineBudgetUsed",
        "AssistBudgetUsed",
    ] {
        put(&mut r, k, "0")
    }
    r
}
#[derive(Default, Clone, Copy)]
pub struct Timing {
    pub probe: f64,
    pub trim: f64,
    pub total: f64,
}
fn detection_fields(row: &mut Record, d: &Detection, t: Timing) {
    for (k, v) in [
        ("Target", d.target.clone()),
        ("EventSec", event(d.event_sec)),
        ("EventSecs", d.event_secs.clone()),
        ("EventKeys", d.event_key.clone()),
        ("EventWeapon", d.event_weapon.clone()),
        ("ContextRule", d.context_rule.clone()),
        ("PaddleText", d.text.clone()),
        ("PaddleScores", d.scores.clone()),
        ("OcrGateReason", d.gate_reason.clone()),
        ("TerminationReason", d.termination_reason.clone()),
    ] {
        put(row, k, v)
    }
    for (k, v) in [
        ("OcrSeconds", d.ocr_seconds),
        ("DetectSeconds", d.detect_seconds),
        ("OcrFrameSeconds", d.ocr_frame_seconds),
        ("RefineOcrSeconds", d.refine_ocr_seconds),
        ("AssistOcrSeconds", d.assist_ocr_seconds),
        ("ProbeSeconds", t.probe),
        ("TrimSeconds", t.trim),
        ("TotalSeconds", t.total),
    ] {
        put(row, k, fixed(v))
    }
    for (k, v) in [
        ("SampledFrames", d.sampled_count),
        ("OcrCoarseFrames", d.ocr_coarse_frames),
        ("OcrRefineFrames", d.ocr_refine_frames),
        ("OcrGateSkippedFrames", d.ocr_gate_skipped_frames),
        ("OcrRequests", d.ocr_requests),
        ("OcrSuccesses", d.ocr_successes),
        ("OcrCacheHits", d.ocr_cache_hits),
        ("OcrSkipped", d.ocr_skipped),
        ("RefineBudgetUsed", d.refine_budget_used),
        ("AssistBudgetUsed", d.assist_budget_used),
    ] {
        put(row, k, v)
    }
}
pub fn skipped_record(index: usize, path: &Path, d: &Detection, t: Timing) -> Record {
    let mut r = blank_record(
        index,
        path,
        d.duration_sec,
        "skipped",
        &d.method,
        &d.detector,
    );
    detection_fields(&mut r, d, t);
    put(
        &mut r,
        "EventCount",
        if d.event_sec.is_some() {
            d.event_count.as_str()
        } else {
            "0"
        },
    );
    if d.clip_before_sec != 0.0 {
        put(&mut r, "BeforeSec", fixed(d.clip_before_sec))
    }
    if d.clip_after_sec != 0.0 {
        put(&mut r, "AfterSec", fixed(d.clip_after_sec))
    }
    r
}
pub fn included_record(
    index: usize,
    path: &Path,
    d: &Detection,
    window: [f64; 2],
    encoder: &str,
    output: &str,
    t: Timing,
) -> Record {
    let mut r = blank_record(
        index,
        path,
        d.duration_sec,
        "included",
        &d.method,
        &d.detector,
    );
    detection_fields(&mut r, d, t);
    put(&mut r, "EventCount", &d.event_count);
    put(
        &mut r,
        "EventSecs",
        if d.event_secs.is_empty() {
            event(d.event_sec)
        } else {
            d.event_secs.clone()
        },
    );
    for (k, v) in [
        ("BeforeSec", d.clip_before_sec.max(0.0)),
        ("AfterSec", d.clip_after_sec.max(0.0)),
        ("KeepStartSec", window[0]),
        ("KeepEndSec", window[1]),
        ("KeepDurationSec", window[1] - window[0]),
    ] {
        put(&mut r, k, fixed(v))
    }
    put(&mut r, "Encoder", encoder);
    put(&mut r, "Output", output);
    r
}
fn get<'a>(row: &'a Record, key: &str) -> &'a str {
    row.get(key).map(String::as_str).unwrap_or("")
}
fn candidate_record(run_id: &str, row: &Record, cache_status: &str) -> Record {
    let mut r = Record::new();
    for k in CANDIDATE_COLUMNS {
        put(&mut r, k, get(row, k))
    }
    for (k, v) in [
        ("SchemaVersion", "1"),
        ("RunId", run_id),
        ("StartSec", get(row, "KeepStartSec")),
        ("EndSec", get(row, "KeepEndSec")),
        (
            "Score",
            get(row, "PaddleScores")
                .split(';')
                .next()
                .unwrap_or("")
                .trim(),
        ),
        ("GateReason", get(row, "OcrGateReason")),
        ("CacheStatus", cache_status),
    ] {
        put(&mut r, k, v)
    }
    let coarse = get(row, "OcrCoarseFrames").parse::<u64>().unwrap_or(0);
    let refine = get(row, "OcrRefineFrames").parse::<u64>().unwrap_or(0);
    put(
        &mut r,
        "SamplingDensity",
        if coarse + refine == 0 {
            "none"
        } else if refine > 0 {
            "dense"
        } else {
            "coarse"
        },
    );
    put(
        &mut r,
        "GateStatus",
        if get(row, "OcrGateReason") == "disabled" {
            "disabled"
        } else if !matches!(get(row, "OcrGateSkippedFrames"), "" | "0") {
            "skipped"
        } else {
            "passed"
        },
    );
    r
}
pub fn write_csv(path: &Path, columns: &[&str], records: &[Record]) -> io::Result<()> {
    let mut file = io::BufWriter::new(fs::File::create(path)?);
    file.write_all(b"\xef\xbb\xbf")?;
    let mut writer = csv::WriterBuilder::new()
        .terminator(csv::Terminator::CRLF)
        .from_writer(file);
    writer.write_record(columns)?;
    for r in records {
        writer.write_record(columns.iter().map(|k| get(r, k)))?
    }
    writer.flush()
}
pub fn write_records(
    directory: &Path,
    run_id: &str,
    records: &[Record],
    cache_status: &BTreeMap<String, String>,
) -> io::Result<()> {
    write_csv(
        &directory.join("检测与裁剪记录.csv"),
        RECORD_COLUMNS,
        records,
    )?;
    let candidates = records
        .iter()
        .filter(|r| !get(r, "EventSec").is_empty())
        .map(|r| {
            candidate_record(
                run_id,
                r,
                cache_status
                    .get(get(r, "Name"))
                    .map(String::as_str)
                    .unwrap_or("miss"),
            )
        })
        .collect::<Vec<_>>();
    write_csv(
        &directory.join("candidate_events.csv"),
        CANDIDATE_COLUMNS,
        &candidates,
    )
}
/// Preserve the two historical Python JSON numeric extensions only when the
/// user explicitly supplies positive infinity. Ordinary runs remain strict JSON;
/// silently changing Infinity to serde_json's null would change field types.
/// No source/path text can match the unescaped, fixed object-key token.
#[cfg(feature = "onnx")]
pub(crate) fn encode_summary(
    value: &serde_json::Value,
    minimum: f64,
    molotov: f64,
) -> io::Result<String> {
    let mut text = serde_json::to_string(value).map_err(io::Error::other)?;
    for (field, number) in [("min_event_sec", minimum), ("molotov_elim_before", molotov)] {
        if number == f64::INFINITY {
            if !value[field].is_null() {
                return Err(io::Error::other("infinite summary metadata mismatch"));
            }
            let token = format!("\"{field}\":null");
            if !text.contains(&token) {
                return Err(io::Error::other("infinite summary field missing"));
            }
            text = text.replacen(&token, &format!("\"{field}\":Infinity"), 1);
        }
    }
    Ok(text)
}
pub fn write_json(path: &Path, value: &serde_json::Value) -> io::Result<()> {
    fs::write(
        path,
        serde_json::to_vec_pretty(value).map_err(io::Error::other)?,
    )
}
pub fn counts(records: &[Record], field: &str) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for row in records {
        *counts.entry(get(row, field).to_owned()).or_default() += 1;
    }
    counts
}
pub fn source_total(records: &[Record], field: &str) -> f64 {
    let mut totals: BTreeMap<&str, f64> = BTreeMap::new();
    for row in records {
        let n = get(row, field).parse::<f64>().unwrap_or(0.0);
        let value = totals.entry(get(row, "Name")).or_default();
        *value = value.max(n);
    }
    totals.values().sum()
}
pub fn round3(n: f64) -> f64 {
    (n * 1000.0).round_ties_even() / 1000.0
}
