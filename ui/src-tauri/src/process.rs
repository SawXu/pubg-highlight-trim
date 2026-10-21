//! Windows-only controlled process boundary. The child is born suspended; it never
//! executes user code before membership in our kill-on-close Job is established.
use std::{
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    System::{Diagnostics::ToolHelp::*, JobObjects::*, Threading::*},
};
const CREATE_SUSPENDED_FLAG: u32 = 4;
pub struct Job(HANDLE);
unsafe impl Send for Job {}
unsafe impl Sync for Job {}
impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
fn os_error(context: &str) -> String {
    format!("{context}: {}", std::io::Error::last_os_error())
}
impl Job {
    pub fn new() -> Result<Arc<Self>, String> {
        unsafe {
            let h = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if h.is_null() {
                return Err(os_error("CreateJobObject"));
            }
            let job = Arc::new(Self(h));
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                h,
                JobObjectExtendedLimitInformation,
                &info as *const _ as _,
                std::mem::size_of_val(&info) as u32,
            ) == 0
            {
                return Err(os_error("SetInformationJobObject"));
            }
            Ok(job)
        }
    }
    pub fn terminate(&self) -> Result<(), String> {
        unsafe {
            if TerminateJobObject(self.0, 1) == 0 {
                Err(os_error("TerminateJobObject"))
            } else {
                Ok(())
            }
        }
    }
    pub fn active(&self) -> Result<u32, String> {
        unsafe {
            let mut i: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = std::mem::zeroed();
            if QueryInformationJobObject(
                self.0,
                JobObjectBasicAccountingInformation,
                &mut i as *mut _ as _,
                std::mem::size_of_val(&i) as u32,
                std::ptr::null_mut(),
            ) == 0
            {
                Err(os_error("QueryInformationJobObject"))
            } else {
                Ok(i.ActiveProcesses)
            }
        }
    }
    pub fn cleanup(&self, timeout: Duration) -> Result<(), String> {
        self.terminate()?;
        self.verify_quiescent(timeout)
    }
    pub fn verify_quiescent(&self, timeout: Duration) -> Result<(), String> {
        let until = Instant::now() + timeout;
        loop {
            if self.active()? == 0 {
                return Ok(());
            }
            if Instant::now() >= until {
                return Err("进程树清理超时，不能确认任务已停止".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
pub struct ControlledChild {
    pub child: Child,
    pub job: Arc<Job>,
}
impl ControlledChild {
    pub fn spawn(
        executable: &Path,
        args: &[String],
        models: Option<&Path>,
    ) -> Result<Self, String> {
        let job = Job::new()?;
        let mut command = Command::new(executable);
        command
            .args(args)
            .current_dir(executable.parent().ok_or("CLI 缺少父目录")?)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(CREATE_SUSPENDED_FLAG | CREATE_NO_WINDOW);
        command.env("PYTHONIOENCODING", "utf-8");
        if let Some(m) = models {
            command.env("PUBG_OCR_MODEL_DIR", m);
        }
        // Never discover ffmpeg through the caller's current directory or PATH.
        if let Some(dir) = executable.parent() {
            let system = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
            command.env(
                "PATH",
                std::env::join_paths([
                    dir.to_path_buf(),
                    dir.join("_internal/vendor/ffmpeg"),
                    PathBuf::from(system).join("System32"),
                ])
                .map_err(|e| e.to_string())?,
            );
        }
        let child = command.spawn().map_err(|e| format!("启动 CLI: {e}"))?;
        use std::os::windows::io::AsRawHandle;
        let assigned = unsafe { AssignProcessToJobObject(job.0, child.as_raw_handle() as HANDLE) };
        if assigned == 0 {
            let e = os_error("AssignProcessToJobObject（已有 Job 不兼容时安全拒绝启动）");
            let cleanup = stop_failed_launch(child, job);
            return Err(format!("{e}; {cleanup}"));
        }
        if let Err(e) = resume(child.id()) {
            let cleanup = stop_failed_launch(child, job);
            return Err(format!("{e}; {cleanup}"));
        }
        Ok(Self { child, job })
    }
}
// A failed launch must not block IPC forever on a still-suspended process.
// Retain the native handles in a retry guard when Windows cannot confirm cleanup.
fn stop_failed_launch(mut child: Child, job: Arc<Job>) -> String {
    use std::os::windows::io::AsRawHandle;
    let _ = job.terminate();
    let killed = child.kill();
    let waited = unsafe { WaitForSingleObject(child.as_raw_handle() as HANDLE, 5000) };
    if waited == WAIT_OBJECT_0 {
        let reaped = child.try_wait();
        return format!("failed-launch cleanup confirmed; kill={killed:?}, reap={reaped:?}");
    }
    let message = format!(
        "未能确认失败启动进程已停止；后台保留安全句柄并重试；kill={killed:?}, wait={waited}"
    );
    std::thread::spawn(move || loop {
        let _ = job.terminate();
        let _ = child.kill();
        let status = unsafe { WaitForSingleObject(child.as_raw_handle() as HANDLE, 1000) };
        if status == WAIT_OBJECT_0 {
            let _ = child.try_wait();
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    });
    message
}
fn resume(pid: u32) -> Result<(), String> {
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(os_error("CreateToolhelp32Snapshot"));
        }
        let mut entry: THREADENTRY32 = std::mem::zeroed();
        entry.dwSize = std::mem::size_of_val(&entry) as u32;
        let mut more = Thread32First(snapshot, &mut entry);
        let mut result = Err("无法找到挂起的 CLI 主线程".into());
        while more != 0 {
            if entry.th32OwnerProcessID == pid {
                let thread = OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID);
                if thread.is_null() {
                    result = Err(os_error("OpenThread"))
                } else {
                    let count = ResumeThread(thread);
                    result = if count == u32::MAX {
                        Err(os_error("ResumeThread"))
                    } else {
                        Ok(())
                    };
                    CloseHandle(thread);
                }
                break;
            }
            more = Thread32Next(snapshot, &mut entry);
        }
        CloseHandle(snapshot);
        result
    }
}
