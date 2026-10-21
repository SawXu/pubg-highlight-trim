use highlight_shell::{
    storage::{RuntimeLayout, Storage},
    RunOptions, Shell,
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
fn wait(shell: &Shell) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while shell.state().running {
        assert!(Instant::now() < deadline, "{:?}", shell.state());
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn public_shell_runs_persists_resets_and_rejects_unconfirmed_danger() {
    let d = tempfile::tempdir().unwrap();
    let input = d.path().join("a.mp4");
    std::fs::write(&input, "").unwrap();
    let storage = Storage::open(d.path().join("data")).unwrap();
    let cli = PathBuf::from(env!("CARGO_BIN_EXE_controlled-cli"));
    let shell = Shell::new(
        storage.clone(),
        Ok(RuntimeLayout {
            cli,
            models: d.path().into(),
        }),
    );
    let mut options = RunOptions {
        input_paths: vec![input.to_string_lossy().into()],
        ..Default::default()
    };
    options.move_originals = true;
    assert!(shell.start(options.clone(), false).is_err());
    options.move_originals = false;
    assert_eq!(shell.start(options.clone(), false).unwrap(), 1);
    wait(&shell);
    assert_eq!(shell.state().included, 1);
    assert_eq!(shell.state().outcome.as_deref(), Some("success"));
    assert_eq!(storage.load().unwrap().input_paths, options.input_paths);
    assert_eq!(shell.start(options, false).unwrap(), 2);
    wait(&shell);
    assert_eq!(shell.state().included, 1);
    assert!(shell
        .logs()
        .unwrap()
        .iter()
        .any(|l| l.contains("ERR SUMMARY")));
    shell.clear_logs().unwrap();
    assert!(shell.logs().unwrap().is_empty());
    shell.clear_cache().unwrap();
}
#[test]
fn storage_is_explicit_bounded_and_never_falls_back() {
    assert!(Storage::open(PathBuf::from("relative-data")).is_err());
    let d = tempfile::tempdir().unwrap();
    let s = Storage::open(d.path().join("data")).unwrap();
    std::fs::write(s.root.join("run.log"), "a".repeat(2 * 1024 * 1024)).unwrap();
    s.append(&"x".repeat(100000)).unwrap();
    assert!(std::fs::metadata(s.root.join("run.log")).unwrap().len() <= 1024 * 1024);
    assert!(s.logs().unwrap().iter().all(|l| l.len() <= 16384));
}
