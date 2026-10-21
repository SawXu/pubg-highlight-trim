use highlight_shell::RunOptions;
#[test]
fn defaults_and_target_combinations_match_existing_workflow() {
    let mut o = RunOptions::default();
    assert_eq!(
        (
            o.seconds_before,
            o.seconds_after,
            o.minimum_event_seconds,
            o.molotov_seconds_before
        ),
        (5., 1., 2., 10.)
    );
    assert_eq!(o.target(), ("both", false));
    o.self_death = true;
    assert_eq!(o.target(), ("self-death", false));
    o.own_kill = true;
    assert_eq!(o.target(), ("both", true));
    o.input_paths = vec![std::env::temp_dir().to_string_lossy().into()];
    let args = o.arguments().unwrap();
    assert!(args.windows(2).any(|x| x == ["--trim-jobs", "auto"]));
    assert!(args.contains(&"--split-merge-by-target".into()));
    for (jobs, expected) in [("AUTO", "auto"), (" auto ", "auto"), (" 4 ", "4")] {
        o.jobs = jobs.into();
        let args = o.arguments().unwrap();
        assert!(args.windows(2).any(|a| a == ["--jobs", expected]));
    }
    o.seconds_after = f64::INFINITY;
    assert!(o.arguments().is_err());
}
