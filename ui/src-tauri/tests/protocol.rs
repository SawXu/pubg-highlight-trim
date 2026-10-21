use highlight_shell::RunState;
#[test]
fn streams_legacy_unknown_and_terminal_semantics() {
    let mut s = RunState::default();
    s.line(
        r#"PROGRESS {"phase":"dedupe","event":"start","total":5}"#,
        false,
    );
    assert_eq!(s.phases["dedupe"].total, 5);
    assert_eq!(s.phases["dedupe"].current, 0);
    s.line(
        "PROGRESS {\"phase\":\"trim\",\"current\":1,\"total\":2,\"workers\":2}",
        false,
    );
    s.line(
        "PROGRESS {\"phase\":\"future\",\"current\":0,\"total\":1}",
        false,
    );
    s.line("[1/2] INCLUDE a.mp4", false);
    assert_eq!(s.active_phase.as_deref(), Some("trim"));
    assert_eq!(s.included, 1);
    s.line("SUMMARY {\"included_count\":20}", true);
    assert!(s.summary.is_none());
    s.line(
        "SUMMARY {\"source_count\":2,\"included_count\":1,\"unreadable_count\":1,\"unknown\":42}",
        false,
    );
    s.finish(2, false, None);
    assert_eq!(s.outcome.as_deref(), Some("partial-unreadable"));
    s.finish(0, true, None);
    assert_eq!(s.outcome.as_deref(), Some("cancelled"));
    s.finish(0, true, Some("job still active".into()));
    assert_eq!(s.outcome.as_deref(), Some("cleanup-failed"));
}
