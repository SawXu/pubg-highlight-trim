//! UI-facing seam for the native desktop shell.
//!
//! This module deliberately contains no windowing code. A native UI can use this
//! seam without owning a second task state machine.
use crate::{RunOptions, RunState, Shell};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Clone, Debug)]
pub struct FrozenRun {
    options: RunOptions,
}

impl FrozenRun {
    pub fn options(&self) -> &RunOptions {
        &self.options
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CloseRequest {
    Allowed,
    ConfirmationRequired,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CloseStatus {
    NotRequested,
    Waiting { run_id: Option<u64> },
    Completed,
    Failed(String),
}

#[derive(Clone, Debug)]
pub enum SessionStatus {
    Starting,
    Running(RunState),
    Finished(RunState),
    Failed(String),
}

#[derive(Clone)]
pub struct UiSession {
    shell: Shell,
    inner: Arc<Mutex<SessionState>>,
}

struct SessionState {
    starting: bool,
    active_run: Option<u64>,
    cancel_requested: Option<u64>,
    close: CloseStatus,
    startup_error: Option<(u64, String)>,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            starting: false,
            active_run: None,
            cancel_requested: None,
            close: CloseStatus::NotRequested,
            startup_error: None,
        }
    }
}

fn failure_message(run: &RunState) -> String {
    run.error.clone().unwrap_or_else(|| "清理失败".into())
}

fn close_status(run: &RunState) -> CloseStatus {
    if run.outcome.as_deref() == Some("cleanup-failed") {
        CloseStatus::Failed(failure_message(run))
    } else {
        CloseStatus::Completed
    }
}

fn session_status(run: RunState) -> SessionStatus {
    if run.outcome.as_deref() == Some("cleanup-failed") {
        SessionStatus::Failed(failure_message(&run))
    } else if run.running {
        SessionStatus::Running(run)
    } else {
        SessionStatus::Finished(run)
    }
}

impl UiSession {
    pub fn new(shell: Shell) -> Self {
        Self {
            shell,
            inner: Arc::new(Mutex::new(SessionState::default())),
        }
    }

    pub fn bootstrap(&self) -> RunState {
        self.shell.state()
    }

    pub fn load(&self) -> Result<RunOptions, String> {
        self.shell.load()
    }

    pub fn save(&self, options: &RunOptions) -> Result<(), String> {
        self.shell.save(options)
    }

    pub fn validate(&self, options: &RunOptions) -> Result<(), String> {
        options.arguments().map(|_| ())
    }

    pub fn freeze(&self, options: RunOptions) -> Result<FrozenRun, String> {
        self.validate(&options)?;
        Ok(FrozenRun { options })
    }

    pub fn state(&self) -> RunState {
        self.shell.state()
    }

    /// Reserve the run id and dispatch startup. The potentially blocking Shell
    /// operation always runs off the caller's (UI) thread.
    pub fn start(&self, snapshot: FrozenRun, confirmed: bool) -> Result<u64, String> {
        let run_id = self.shell.state().run_id + 1;
        let mut state = self.inner.lock().unwrap();
        if state.starting || state.active_run.is_some() || self.shell.state().running {
            return Err("任务已在运行或启动中".into());
        }
        state.starting = true;
        state.active_run = Some(run_id);
        state.startup_error = None;
        drop(state);

        let shell = self.shell.clone();
        let inner = Arc::clone(&self.inner);
        std::thread::spawn(move || {
            let result = shell.start(snapshot.options, confirmed);
            let cancel = {
                let mut state = inner.lock().unwrap();
                state.starting = false;
                match result {
                    Ok(actual_id) => {
                        debug_assert_eq!(actual_id, run_id);
                        state.cancel_requested == Some(run_id)
                    }
                    Err(error) => {
                        if state.active_run == Some(run_id) {
                            state.active_run = None;
                            state.cancel_requested = None;
                            state.startup_error = Some((run_id, error));
                            if matches!(state.close, CloseStatus::Waiting { .. }) {
                                state.close = CloseStatus::Completed;
                            }
                        }
                        false
                    }
                }
            };
            if cancel {
                let result = shell.cancel();
                let mut current = shell.state();
                if result.is_ok() {
                    while current.running && current.outcome.as_deref() != Some("cleanup-failed") {
                        std::thread::sleep(Duration::from_millis(10));
                        current = shell.state();
                    }
                }
                let mut state = inner.lock().unwrap();
                if state.active_run == Some(run_id)
                    && (!current.running || current.outcome.as_deref() == Some("cleanup-failed"))
                {
                    state.active_run = None;
                    state.cancel_requested = None;
                    if matches!(state.close, CloseStatus::Waiting { .. }) {
                        state.close = close_status(&current);
                    }
                } else if let Err(error) = result {
                    if matches!(state.close, CloseStatus::Waiting { .. }) {
                        state.close = CloseStatus::Failed(error);
                    }
                }
            }
        });
        Ok(run_id)
    }

    /// Return authoritative state only for the current run. Old readers cannot
    /// overwrite a newer run's view.
    pub fn observe(&self, run_id: u64) -> Option<RunState> {
        let state = self.inner.lock().unwrap();
        if state.active_run != Some(run_id) {
            return None;
        }
        let current = self.shell.state();
        if (!current.running || current.outcome.as_deref() == Some("cleanup-failed"))
            && !state.starting
        {
            drop(state);
            let mut state = self.inner.lock().unwrap();
            if state.active_run == Some(run_id) {
                state.active_run = None;
                state.cancel_requested = None;
                if matches!(state.close, CloseStatus::Waiting { .. }) {
                    state.close = close_status(&current);
                }
            }
        }
        Some(current)
    }

    /// A polling seam for startup errors and terminal cleanup outcomes.
    pub fn status(&self, run_id: u64) -> Option<SessionStatus> {
        let state = self.inner.lock().unwrap();
        if state
            .active_run
            .is_some_and(|active_id| active_id != run_id)
            || (state.active_run.is_none()
                && state.startup_error.as_ref().map(|(id, _)| *id) != Some(run_id)
                && self.shell.state().run_id != run_id)
        {
            return None;
        }
        if state.starting {
            return Some(SessionStatus::Starting);
        }
        if let Some((error_run_id, error)) = &state.startup_error {
            if *error_run_id == run_id {
                return Some(SessionStatus::Failed(error.clone()));
            }
        }
        let run = self.shell.state();
        if (!run.running || run.outcome.as_deref() == Some("cleanup-failed"))
            && state.active_run == Some(run_id)
        {
            drop(state);
            let mut state = self.inner.lock().unwrap();
            if state.active_run == Some(run_id) {
                state.active_run = None;
                state.cancel_requested = None;
                if matches!(state.close, CloseStatus::Waiting { .. }) {
                    state.close = close_status(&run);
                }
            }
        }
        Some(session_status(run))
    }

    /// Cancellation is single-flight. The lock remains held logically until
    /// Shell reports a terminal state, including cleanup failure.
    pub fn cancel(&self, run_id: u64) -> Result<(), String> {
        let mut state = self.inner.lock().unwrap();
        if state.active_run != Some(run_id) {
            return Err("运行 ID 已过期".into());
        }
        if state.cancel_requested.is_some() {
            return Err("取消已请求，仍在等待终态".into());
        }
        state.cancel_requested = Some(run_id);
        if state.starting {
            return Ok(());
        }
        drop(state);

        let shell = self.shell.clone();
        let inner = Arc::clone(&self.inner);
        std::thread::spawn(move || {
            let result = shell.cancel();
            let mut current = shell.state();
            if result.is_ok() {
                while current.running && current.outcome.as_deref() != Some("cleanup-failed") {
                    std::thread::sleep(Duration::from_millis(10));
                    current = shell.state();
                }
            }
            let mut state = inner.lock().unwrap();
            if state.active_run == Some(run_id)
                && (!current.running || current.outcome.as_deref() == Some("cleanup-failed"))
            {
                state.active_run = None;
                state.cancel_requested = None;
                if matches!(state.close, CloseStatus::Waiting { .. }) {
                    state.close = close_status(&current);
                }
            } else if let Err(error) = result {
                if matches!(state.close, CloseStatus::Waiting { .. }) {
                    state.close = CloseStatus::Failed(error);
                }
            }
        });
        Ok(())
    }

    pub fn request_close(&self) -> CloseRequest {
        if self.shell.state().running || self.inner.lock().unwrap().starting {
            CloseRequest::ConfirmationRequired
        } else {
            CloseRequest::Allowed
        }
    }

    /// Dispatch cleanup and expose its eventual result through `close_status`.
    pub fn confirm_close(&self, run_id: Option<u64>) -> Result<(), String> {
        let active = {
            let mut state = self.inner.lock().unwrap();
            if matches!(state.close, CloseStatus::Waiting { .. }) {
                return Ok(());
            }
            let active = run_id.or(state.active_run);
            if let Some(id) = active {
                if state.active_run != Some(id) {
                    let error = "运行 ID 已过期".to_string();
                    state.close = CloseStatus::Failed(error.clone());
                    return Err(error);
                }
            }
            state.close = CloseStatus::Waiting { run_id: active };
            active
        };
        if let Some(id) = active {
            if let Err(error) = self.cancel(id) {
                self.inner.lock().unwrap().close = CloseStatus::Failed(error.clone());
                return Err(error);
            }
        } else {
            let mut state = self.inner.lock().unwrap();
            state.close = CloseStatus::Completed;
        }
        Ok(())
    }

    pub fn close_status(&self) -> CloseStatus {
        self.inner.lock().unwrap().close.clone()
    }

    pub fn close_requested(&self) -> bool {
        !matches!(self.close_status(), CloseStatus::NotRequested)
    }

    /// Read the bounded persistent log without exposing Shell or its lock to a UI.
    pub fn logs(&self) -> Result<Vec<String>, String> {
        self.shell.logs()
    }
}

#[cfg(test)]
mod tests {
    use super::{close_status, session_status, CloseStatus, SessionStatus};
    use crate::RunState;

    #[test]
    fn cleanup_failure_is_failed_for_status_and_close() {
        let run = RunState {
            running: true,
            outcome: Some("cleanup-failed".into()),
            error: Some("child process remained".into()),
            ..Default::default()
        };

        assert_eq!(
            close_status(&run),
            CloseStatus::Failed("child process remained".into())
        );
        assert!(matches!(
            session_status(run),
            SessionStatus::Failed(error) if error == "child process remained"
        ));
    }
}
