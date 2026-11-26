use highlight_shell::{
    storage::{RuntimeLayout, Storage},
    ui_session::{CloseRequest, CloseStatus, SessionStatus, UiSession},
    RunOptions, Shell,
};
use std::{path::PathBuf, thread, time::Duration};

fn session() -> (UiSession, tempfile::TempDir, RunOptions) {
    let d = tempfile::tempdir().unwrap();
    let input = d.path().join("input.mp4");
    std::fs::write(&input, "").unwrap();
    let shell = Shell::new(
        Storage::open(d.path().join("data")).unwrap(),
        Ok(RuntimeLayout {
            cli: PathBuf::from(env!("CARGO_BIN_EXE_controlled-cli")),
            models: d.path().into(),
        }),
    );
    let options = RunOptions {
        input_paths: vec![input.to_string_lossy().into()],
        ..Default::default()
    };
    (UiSession::new(shell), d, options)
}

fn wait_done(ui: &UiSession, run_id: u64) {
    for _ in 0..500 {
        if matches!(ui.status(run_id), Some(SessionStatus::Finished(_))) {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("run did not finish")
}

#[test]
fn freeze_uses_the_confirmed_snapshot_even_if_caller_mutates_options() {
    let (ui, _dir, mut options) = session();
    let snapshot = ui.freeze(options.clone()).unwrap();
    options.overwrite = true;
    assert!(!snapshot.options().overwrite);
    let id = ui.start(snapshot, false).unwrap();
    wait_done(&ui, id);
}

#[test]
fn duplicate_start_and_stale_run_id_are_rejected() {
    let (ui, _dir, options) = session();
    let first = ui
        .start(ui.freeze(options.clone()).unwrap(), false)
        .unwrap();
    assert!(ui.start(ui.freeze(options).unwrap(), false).is_err());
    assert!(ui.observe(first + 1).is_none());
    wait_done(&ui, first);
}

#[test]
fn duplicate_cancel_is_rejected_until_the_run_reaches_a_terminal_state() {
    let (ui, _dir, options) = session();
    let id = ui.start(ui.freeze(options).unwrap(), false).unwrap();
    ui.cancel(id).unwrap();
    assert!(ui.cancel(id).is_err());
    wait_done(&ui, id);
    assert!(matches!(ui.status(id), Some(SessionStatus::Finished(_))));
}

#[test]
fn startup_failure_is_reported_without_blocking_start() {
    let d = tempfile::tempdir().unwrap();
    let shell = Shell::new(
        Storage::open(d.path().join("data")).unwrap(),
        Err("missing runtime".into()),
    );
    let ui = UiSession::new(shell);
    let input = d.path().join("input.mp4");
    std::fs::write(&input, "").unwrap();
    let options = RunOptions {
        input_paths: vec![input.to_string_lossy().into()],
        ..Default::default()
    };
    let id = ui.start(ui.freeze(options).unwrap(), false).unwrap();
    for _ in 0..100 {
        if matches!(ui.status(id), Some(SessionStatus::Failed(_))) {
            assert!(ui.status(id + 1).is_none());
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("startup failure was not reported")
}

#[test]
fn startup_failure_does_not_cancel_a_following_run() {
    let (ui, _dir, mut options) = session();
    options.overwrite = true;
    let failed = ui
        .start(ui.freeze(options.clone()).unwrap(), false)
        .unwrap();
    ui.cancel(failed).unwrap();
    for _ in 0..100 {
        if matches!(ui.status(failed), Some(SessionStatus::Failed(_))) {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    options.overwrite = false;
    let next = ui.start(ui.freeze(options).unwrap(), false).unwrap();
    assert!(matches!(ui.status(next), Some(SessionStatus::Starting)));
    wait_done(&ui, next);
    assert!(matches!(ui.status(next), Some(SessionStatus::Finished(_))));
}

#[test]
fn stale_close_request_does_not_remain_waiting() {
    let (ui, _dir, _options) = session();

    assert!(ui.confirm_close(Some(42)).is_err());
    assert_eq!(
        ui.close_status(),
        CloseStatus::Failed("运行 ID 已过期".into())
    );
}

#[test]
fn close_waits_for_cleanup_and_reports_completion() {
    let (ui, _dir, options) = session();
    let id = ui.start(ui.freeze(options).unwrap(), false).unwrap();
    assert_eq!(ui.request_close(), CloseRequest::ConfirmationRequired);
    ui.confirm_close(Some(id)).unwrap();
    assert_eq!(ui.close_status(), CloseStatus::Waiting { run_id: Some(id) });
    ui.confirm_close(Some(id)).unwrap();
    wait_done(&ui, id);
    for _ in 0..100 {
        if ui.close_status() == CloseStatus::Completed {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(ui.close_status(), CloseStatus::Completed);
    assert!(ui.close_requested());
    assert_eq!(ui.request_close(), CloseRequest::Allowed);
}
