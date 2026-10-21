use highlight_shell::{RunOptions, RunState};
#[test]
fn independent_ocr_trim_jobs_and_all_switches_reach_cli() {
    let d = tempfile::tempdir().unwrap();
    let a = d.path().join("a.mp4");
    let b = d.path().join("b.MP4");
    std::fs::write(&a, "").unwrap();
    std::fs::write(&b, "").unwrap();
    let o = RunOptions {
        input_paths: vec![a.to_string_lossy().into(), b.to_string_lossy().into()],
        output_directory: Some(d.path().join("output space").to_string_lossy().into()),
        game_language: "zh-Hant".into(),
        scan_mode: "fast".into(),
        jobs: "4".into(),
        trim_jobs: "2".into(),
        self_death: true,
        own_kill: true,
        recursive: true,
        merge: true,
        scan_only: true,
        overwrite: true,
        brightness_gate: false,
        move_originals: true,
        ..Default::default()
    };
    let args = o.arguments().unwrap();
    assert_eq!(args[0], "--files");
    for pair in [
        ["--target", "both"],
        ["--game-lang", "zh-Hant"],
        ["--jobs", "4"],
        ["--trim-jobs", "2"],
        ["--before", "5"],
        ["--after", "1"],
        ["--min-event-sec", "2"],
        ["--molotov-elim-before", "10"],
    ] {
        assert!(args.windows(2).any(|a| a == pair), "{pair:?}")
    }
    for f in [
        "--fast-path",
        "--split-merge-by-target",
        "--recursive",
        "--scan-only",
        "--overwrite",
        "--no-brightness-gate",
        "--move-originals",
        "--merge",
    ] {
        assert!(args.iter().any(|a| a == f), "{f}")
    }
    assert_eq!(o.warnings().len(), 2);
    let mut invalid = o.clone();
    invalid.trim_jobs = "3".into();
    assert!(invalid.arguments().is_err());
    invalid = o.clone();
    invalid.jobs = "0".into();
    assert!(invalid.arguments().is_err());
    invalid = o.clone();
    invalid.seconds_before = -1.;
    assert!(invalid.arguments().is_err());
}
#[test]
fn exit_codes_require_summary_and_cancellation_has_priority() {
    for (code, summary, expected) in [
        (0, None, "failed"),
        (2, None, "failed"),
        (1, Some(r#"{"included_count":2}"#), "failed"),
        (0, Some(r#"{}"#), "no-events"),
        (
            2,
            Some(r#"{"source_count":2,"unreadable_count":2}"#),
            "all-unreadable",
        ),
        (0, Some(r#"{"included_count":3}"#), "success"),
    ] {
        let mut s = RunState::default();
        if let Some(v) = summary {
            s.line(&format!("SUMMARY {v}"), false)
        }
        s.finish(code, false, None);
        assert_eq!(s.outcome.as_deref(), Some(expected));
        s.finish(code, true, None);
        assert_eq!(s.outcome.as_deref(), Some("cancelled"));
    }
}
#[test]
fn phase_lifecycles_and_malformed_records_do_not_corrupt_known_activity() {
    let mut s = RunState::default();
    for line in [
        r#"PROGRESS {"phase":"scan","current":1,"total":1,"workers":1}"#,
        r#"PROGRESS {"phase":"dedupe","event":"done","current":0,"total":0,"workers":0,"future":true}"#,
        r#"PROGRESS {"phase":"trim","event":"skipped","current":0,"total":0,"workers":0,"reason":"dry-run","planned":2}"#,
    ] {
        s.line(line, false)
    }
    assert_eq!(s.phases["scan"].event.as_deref(), Some("done"));
    assert_eq!(s.phases["trim"].planned, Some(2));
    assert_eq!(s.phases["trim"].workers, Some(0));
    for bad in [
        "PROGRESS []",
        "PROGRESS {",
        "SUMMARY nope",
        r#"PROGRESS {"phase":"merge","current":99,"total":1}"#,
        r#"SUMMARY {"included_count":"x"}"#,
    ] {
        s.line(bad, false)
    }
    assert_eq!(s.active_phase.as_deref(), Some("trim"));
    assert!(s.summary.is_none());
}
