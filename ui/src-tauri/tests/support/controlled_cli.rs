use std::{io::Write, time::Duration};
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(String::as_str).unwrap_or("");
    match mode {
        "--sleep" => loop {
            std::thread::sleep(Duration::from_secs(1))
        },
        "--tree" | "--orphan" => {
            let c = std::process::Command::new(std::env::current_exe().unwrap())
                .arg("--sleep")
                .spawn()
                .unwrap();
            println!("READY {}", c.id());
            std::io::stdout().flush().unwrap();
            if mode == "--tree" {
                loop {
                    std::thread::sleep(Duration::from_secs(1))
                }
            }
        }
        "--supervisor" => {
            use std::io::{BufRead, BufReader};
            let exe = std::env::current_exe().unwrap();
            let mut c =
                highlight_shell::process::ControlledChild::spawn(&exe, &["--tree".into()], None)
                    .unwrap();
            let mut line = String::new();
            BufReader::new(c.child.stdout.take().unwrap())
                .read_line(&mut line)
                .unwrap();
            println!(
                "TREE {} {}",
                c.child.id(),
                line.trim_start_matches("READY ").trim()
            );
            std::io::stdout().flush().unwrap();
            loop {
                std::thread::sleep(Duration::from_secs(1))
            }
        }
        "--outer-job" => {
            use windows_sys::Win32::{
                Foundation::*,
                System::{JobObjects::*, Threading::*},
            };
            unsafe {
                let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                assert!(!job.is_null());
                assert_ne!(AssignProcessToJobObject(job, GetCurrentProcess()), 0);
                let exe = std::env::current_exe().unwrap();
                match highlight_shell::process::ControlledChild::spawn(
                    &exe,
                    &["--tree".into()],
                    None,
                ) {
                    Ok(mut c) => {
                        c.job.cleanup(Duration::from_secs(5)).unwrap();
                        c.child.wait().unwrap();
                        assert_eq!(c.job.active().unwrap(), 0)
                    }
                    Err(e) => assert!(e.contains("AssignProcessToJobObject"), "{e}"),
                };
                CloseHandle(job);
            }
        }
        _ => {
            if args.iter().any(|a| a.contains("slow.mp4")) {
                let c = std::process::Command::new(std::env::current_exe().unwrap())
                    .arg("--sleep")
                    .spawn()
                    .unwrap();
                println!("READY {}", c.id());
                std::io::stdout().flush().unwrap();
                loop {
                    std::thread::sleep(Duration::from_secs(1))
                }
            }
            println!("PROGRESS {{\"phase\":\"scan\",\"current\":1,\"total\":1,\"workers\":1}}");
            eprintln!("SUMMARY {{\"included_count\":999}}");
            println!("[1/1] INCLUDE fixture.mp4");
            println!("SUMMARY {{\"source_count\":1,\"included_count\":1,\"skipped_count\":0}}");
        }
    }
}
