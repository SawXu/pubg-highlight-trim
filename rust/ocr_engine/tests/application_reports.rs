use pubg_ocr_engine::{
    planning::Detection,
    reports::{self, Timing, RECORD_COLUMNS},
};
use std::{collections::BTreeMap, fs, path::Path};
fn detection() -> Detection {
    serde_json::from_value(serde_json::json!({"duration_sec":12.25,"event_sec":6.5,"method":"paddle-own-kill-text","detector":"ocr","target":"own-kill","event_key":"own-kill:eliminate:enemy","event_weapon":"燃烧瓶","text":"你 淘汰了 Enemy, \"hello\"","scores":"0.980;0.900","ocr_seconds":0.125,"sampled_count":3,"ocr_requests":2,"ocr_successes":2,"ocr_cache_hits":1,"ocr_coarse_frames":2,"ocr_refine_frames":1,"gate_reason":"disabled","context_rule":"molotov-elim-context","clip_before_sec":10.0,"clip_after_sec":1.0})).unwrap()
}
#[test]
fn included_and_skipped_rows_keep_historical_types_fields_and_rounding() {
    let d = detection();
    let row = reports::included_record(
        1,
        Path::new("录像.mp4"),
        &d,
        [0.0, 7.5],
        "libx264",
        "clips/001_own-kill_录像.mp4",
        Timing {
            probe: 0.1,
            trim: 0.25,
            total: 0.75,
        },
    );
    assert_eq!(row.len(), 42);
    for (k, v) in [
        ("Index", "1"),
        ("Name", "录像.mp4"),
        ("Status", "included"),
        ("DurationSec", "12.250"),
        ("EventSec", "6.500"),
        ("EventSecs", "6.500"),
        ("EventCount", "1"),
        ("KeepDurationSec", "7.500"),
        ("BeforeSec", "10.000"),
        ("AfterSec", "1.000"),
        ("PaddleScores", "0.980;0.900"),
        ("OcrSeconds", "0.125"),
        ("SampledFrames", "3"),
        ("OcrRequests", "2"),
        ("ProbeSeconds", "0.100"),
        ("TrimSeconds", "0.250"),
        ("TotalSeconds", "0.750"),
    ] {
        assert_eq!(row[k], v, "{k}")
    }
    let skip = reports::skipped_record(1, Path::new("录像.mp4"), &d, Timing::default());
    assert_eq!(skip["Status"], "skipped");
    assert_eq!(skip["KeepStartSec"], "");
    assert_eq!(skip["BeforeSec"], "10.000");
    assert_eq!(skip["EventSecs"], "");
    assert_eq!(skip["Output"], "");
}
#[test]
fn reports_use_bom_crlf_complete_order_and_candidate_projection() {
    let dir = std::env::temp_dir().join(format!("pubg-reports-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let row = reports::included_record(
        1,
        Path::new("录像.mp4"),
        &detection(),
        [0.0, 7.5],
        "",
        "",
        Timing::default(),
    );
    let blank = reports::blank_record(
        2,
        Path::new("bad.mp4"),
        0.0,
        "unreadable",
        "video-unreadable",
        "ocr",
    );
    reports::write_records(
        &dir,
        "fixed-run",
        &[row, blank],
        &BTreeMap::from([("录像.mp4".into(), "hit".into())]),
    )
    .unwrap();
    let bytes = fs::read(dir.join("检测与裁剪记录.csv")).unwrap();
    assert!(bytes.starts_with(b"\xef\xbb\xbf"));
    let text = String::from_utf8(bytes[3..].to_vec()).unwrap();
    assert!(text.starts_with(&(RECORD_COLUMNS.join(",") + "\r\n")));
    assert!(text.contains("\"你 淘汰了 Enemy, \"\"hello\"\"\""));
    let candidate_bytes = fs::read(dir.join("candidate_events.csv")).unwrap();
    let mut reader = csv::Reader::from_reader(&candidate_bytes[3..]);
    let headers = reader.headers().unwrap().clone();
    assert_eq!(
        headers.iter().collect::<Vec<_>>(),
        reports::CANDIDATE_COLUMNS
    );
    let rows = reader.records().map(Result::unwrap).collect::<Vec<_>>();
    assert_eq!(rows.len(), 1);
    let values = headers
        .iter()
        .zip(rows[0].iter())
        .collect::<BTreeMap<_, _>>();
    for (k, v) in [
        ("SchemaVersion", "1"),
        ("RunId", "fixed-run"),
        ("Score", "0.980"),
        ("GateStatus", "disabled"),
        ("SamplingDensity", "dense"),
        ("CacheStatus", "hit"),
        ("StartSec", "0.000"),
        ("EndSec", "7.500"),
    ] {
        assert_eq!(values[k], v, "{k}")
    }
    fs::remove_dir_all(dir).unwrap();
}
