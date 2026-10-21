use std::{
    io::{BufRead, BufReader},
    process::{Command, Stdio},
};
use windows_sys::Win32::{Foundation::*, System::Threading::*};
#[test]
fn abrupt_shell_death_closes_job_and_reaps_entire_live_process_tree() {
    let mut supervisor = Command::new(env!("CARGO_BIN_EXE_controlled-cli"))
        .arg("--supervisor")
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(supervisor.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let pids: Vec<u32> = line
        .strip_prefix("TREE ")
        .unwrap()
        .split_whitespace()
        .map(|n| n.parse().unwrap())
        .collect();
    assert_eq!(pids.len(), 2);
    let handles: Vec<_> = pids
        .into_iter()
        .map(|pid| unsafe {
            let h = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
            assert!(!h.is_null());
            h
        })
        .collect();
    supervisor.kill().unwrap();
    supervisor.wait().unwrap();
    for h in handles {
        unsafe {
            assert_eq!(
                WaitForSingleObject(h, 5000),
                WAIT_OBJECT_0,
                "descendant survived abrupt UI exit"
            );
            CloseHandle(h);
        }
    }
}
