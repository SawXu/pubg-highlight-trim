use highlight_shell::{
    storage::{RuntimeLayout, Storage},
    RunOptions, Shell,
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
#[test]
fn immediate_cancel_and_close_cover_child_birth_and_repeated_run() {
    let d = tempfile::tempdir().unwrap();
    let input = d.path().join("slow.mp4");
    std::fs::write(&input, "").unwrap();
    let shell = Shell::new(
        Storage::open(d.path().join("data")).unwrap(),
        Ok(RuntimeLayout {
            cli: PathBuf::from(env!("CARGO_BIN_EXE_controlled-cli")),
            models: d.path().into(),
        }),
    );
    let o = RunOptions {
        input_paths: vec![input.to_string_lossy().into()],
        ..Default::default()
    };
    for _ in 0..5 {
        shell.start(o.clone(), false).unwrap();
        assert!(shell.start(o.clone(), false).is_err());
        assert!(shell.close(false).is_err());
        assert!(shell.clear_cache().is_err());
        assert!(shell.save(&o).is_err());
        shell.cancel().unwrap();
        let until = Instant::now() + Duration::from_secs(5);
        while shell.state().running {
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(10))
        }
        assert_eq!(shell.state().outcome.as_deref(), Some("cancelled"));
        assert!(shell.state().summary.is_none());
    }
    shell.start(o, false).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    shell.close(true).unwrap();
    assert!(!shell.state().running);
}
