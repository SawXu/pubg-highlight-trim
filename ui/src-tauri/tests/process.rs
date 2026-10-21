use highlight_shell::process::ControlledChild;
use std::{
    io::{BufRead, BufReader},
    path::Path,
    time::Duration,
};
fn helper() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_controlled-cli"))
}
#[test]
fn suspended_assignment_covers_immediate_descendant_and_parent_exit() {
    let mut p = ControlledChild::spawn(helper(), &["--tree".into()], None).unwrap();
    let mut line = String::new();
    BufReader::new(p.child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    assert!(line.starts_with("READY "));
    assert!(p.job.active().unwrap() >= 2);
    assert!(
        p.job.verify_quiescent(Duration::ZERO).is_err(),
        "must not claim clean while descendants live"
    );
    p.job.cleanup(Duration::from_secs(5)).unwrap();
    p.child.wait().unwrap();
    assert_eq!(p.job.active().unwrap(), 0);
    let mut p = ControlledChild::spawn(helper(), &["--orphan".into()], None).unwrap();
    p.child.wait().unwrap();
    assert!(p.job.active().unwrap() > 0);
    p.job.cleanup(Duration::from_secs(5)).unwrap();
    assert_eq!(p.job.active().unwrap(), 0);
}
#[test]
fn launch_inside_existing_job_is_nested_or_safely_refused() {
    let result = std::process::Command::new(helper())
        .arg("--outer-job")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
