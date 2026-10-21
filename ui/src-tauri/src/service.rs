use crate::{
    process::{ControlledChild, Job},
    storage::{RuntimeLayout, Storage},
    RunOptions, RunState,
};
use std::{
    io::Read,
    sync::{Arc, Mutex},
    time::Duration,
};
#[derive(Clone)]
pub struct Shell {
    inner: Arc<Mutex<Inner>>,
    pub storage: Storage,
    pub layout: Result<RuntimeLayout, String>,
}
struct Inner {
    state: RunState,
    job: Option<Arc<Job>>,
}
impl Shell {
    pub fn new(storage: Storage, layout: Result<RuntimeLayout, String>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                state: RunState::default(),
                job: None,
            })),
            storage,
            layout,
        }
    }
    pub fn state(&self) -> RunState {
        self.inner.lock().unwrap().state.clone()
    }
    pub fn save(&self, options: &RunOptions) -> Result<(), String> {
        let i = self.inner.lock().unwrap();
        if i.state.running {
            return Err("运行中不能修改设置".into());
        }
        self.storage.save(options)
    }
    pub fn start(&self, options: RunOptions, confirmed: bool) -> Result<u64, String> {
        let args = options.arguments()?;
        if !options.warnings().is_empty() && !confirmed {
            return Err("危险操作尚未确认".into());
        }
        let layout = self.layout.as_ref().map_err(Clone::clone)?;
        let mut inner = self.inner.lock().unwrap();
        if inner.state.running {
            return Err("任务已在运行".into());
        }
        self.storage.save(&options)?;
        let id = inner.state.run_id + 1;
        inner.state = RunState {
            run_id: id,
            running: true,
            ..Default::default()
        };
        let spawned = ControlledChild::spawn(&layout.cli, &args, Some(&layout.models));
        let mut controlled = match spawned {
            Ok(c) => c,
            Err(e) => {
                inner.state.finish(-1, false, None);
                inner.state.error = Some(e.clone());
                return Err(e);
            }
        };
        inner.job = Some(controlled.job.clone());
        drop(inner);
        let stdout = controlled.child.stdout.take().unwrap();
        let stderr = controlled.child.stderr.take().unwrap();
        let s = self.clone();
        std::thread::spawn(move || {
            let out = s.reader(stdout, false, id);
            let err = s.reader(stderr, true, id);
            let status = controlled.child.wait();
            // Parent exit alone is not completion: kill and verify every remaining descendant.
            let cleanup = controlled.job.cleanup(Duration::from_secs(10)).err();
            if let Some(error) = cleanup {
                let mut i = s.inner.lock().unwrap();
                if i.state.run_id != id {
                    return;
                }
                let cancelled = i.state.cancelling;
                i.state.finish(
                    status.as_ref().ok().and_then(|s| s.code()).unwrap_or(-1),
                    cancelled,
                    Some(error),
                );
                // Readers may still be held open by surviving children. Do not hang
                // the completion monitor or report cancellation as successful.
                return;
            }
            let _ = out.join();
            let _ = err.join();
            let mut i = s.inner.lock().unwrap();
            if i.state.run_id != id {
                return;
            }
            let cancelled = i.state.cancelling;
            let code = status.as_ref().ok().and_then(|s| s.code()).unwrap_or(-1);
            i.state.finish(code, cancelled, None);
            if let Err(e) = status {
                i.state.error = Some(e.to_string())
            }
            if !i.state.running {
                i.job = None;
            }
        });
        Ok(id)
    }
    fn reader<R: Read + Send + 'static>(
        &self,
        mut reader: R,
        stderr: bool,
        run_id: u64,
    ) -> std::thread::JoinHandle<()> {
        let s = self.clone();
        std::thread::spawn(move || {
            // Bound individual records even if a faulty CLI never emits a newline.
            let mut buf = [0; 4096];
            let mut line = Vec::new();
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        for &b in &buf[..n] {
                            if b == b'\n' {
                                s.line(&String::from_utf8_lossy(&line), stderr, run_id);
                                line.clear()
                            } else if line.len() < 65536 {
                                line.push(b)
                            }
                        }
                    }
                    Err(e) => {
                        s.line(&format!("ERROR stream: {e}"), true, run_id);
                        break;
                    }
                }
            }
            if !line.is_empty() {
                s.line(&String::from_utf8_lossy(&line), stderr, run_id)
            }
        })
    }
    fn line(&self, line: &str, stderr: bool, run_id: u64) {
        let line = line.trim_end_matches('\r');
        let mut i = self.inner.lock().unwrap();
        // Cleanup recovery may allow a new run before an old pipe reader unwinds.
        // Never let that reader's buffered output or summary overwrite the new run.
        if i.state.run_id != run_id {
            return;
        }
        i.state.line(line, stderr);
        if let Err(e) = self.storage.append(&if stderr {
            format!("ERR {line}")
        } else {
            line.into()
        }) {
            i.state.error = Some(e)
        }
    }
    pub fn cancel(&self) -> Result<(), String> {
        let (job, run_id) = {
            let mut i = self.inner.lock().unwrap();
            if !i.state.running {
                return Ok(());
            }
            i.state.cancelling = true;
            (i.job.clone(), i.state.run_id)
        };
        if let Some(j) = job {
            if let Err(e) = j.cleanup(Duration::from_secs(10)) {
                let mut i = self.inner.lock().unwrap();
                if i.state.run_id == run_id {
                    i.state.finish(-1, true, Some(e.clone()));
                }
                return Err(e);
            }
            let mut i = self.inner.lock().unwrap();
            if i.state.run_id == run_id && i.state.outcome.as_deref() == Some("cleanup-failed") {
                let code = i.state.exit_code.unwrap_or(-1);
                i.state.finish(code, true, None);
                i.job = None;
            }
        }
        Ok(())
    }
    pub fn close(&self, confirmed: bool) -> Result<(), String> {
        if self.state().running && !confirmed {
            return Err("任务仍在运行，需要确认停止并退出".into());
        }
        self.cancel()?;
        let until = std::time::Instant::now() + Duration::from_secs(15);
        while self.state().running {
            if std::time::Instant::now() >= until {
                return Err("等待进程树清理超时；拒绝正常退出".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }
    pub fn clear_cache(&self) -> Result<(), String> {
        let i = self.inner.lock().unwrap();
        if i.state.running {
            return Err("运行时禁止清理缓存".into());
        }
        self.storage.clear_cache()
    }
    pub fn clear_logs(&self) -> Result<(), String> {
        let _lock = self.inner.lock().unwrap();
        self.storage.clear_logs()
    }
    pub fn logs(&self) -> Result<Vec<String>, String> {
        let _lock = self.inner.lock().unwrap();
        self.storage.logs()
    }
}
