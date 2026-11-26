//! Native Windows UI boundary. The stateful workflow remains in `UiSession`.

use highlight_core::{
    ui_session::{CloseRequest, CloseStatus, FrozenRun, SessionStatus, UiSession},
    RunOptions, RunState, Shell,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionView {
    pub running: bool,
    pub run_id: u64,
    pub message: String,
    pub cancelling: bool,
    pub phase: Option<String>,
    pub logs: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartPrompt {
    Ready,
    ConfirmationRequired(Vec<String>),
}

/// Small native-UI controller. It owns presentation state only; execution and
/// cancellation remain in UiSession, which is the single workflow state machine.
pub struct NativeController {
    pub options: RunOptions,
    pending: Option<FrozenRun>,
    pub confirmation: bool,
    pub run_id: Option<u64>,
}

impl NativeController {
    pub fn new(options: RunOptions) -> Self {
        Self {
            options,
            pending: None,
            confirmation: false,
            run_id: None,
        }
    }

    pub fn load_settings(&mut self, session: &UiSession) -> Result<(), String> {
        self.options = session.load()?;
        self.pending = None;
        self.confirmation = false;
        Ok(())
    }

    pub fn save_settings(&self, session: &UiSession) -> Result<(), String> {
        session.save(&self.options)
    }

    pub fn set_inputs(&mut self, paths: Vec<String>) {
        self.options.input_paths = paths;
        self.pending = None;
    }

    pub fn set_output(&mut self, path: Option<String>) {
        self.options.output_directory = path;
        self.pending = None;
    }

    pub fn close_status(&self, session: &UiSession) -> CloseStatus {
        session.close_status()
    }

    pub fn toggle_core_options(&mut self) {
        self.options.scan_only = !self.options.scan_only;
        self.options.recursive = !self.options.recursive;
        self.options.seconds_before += 1.0;
        self.pending = None;
    }

    pub fn prepare_start(&mut self, session: &UiSession) -> Result<StartPrompt, String> {
        let snapshot = session.freeze(self.options.clone())?;
        let warnings = snapshot.options().warnings();
        self.pending = Some(snapshot);
        self.confirmation = !warnings.is_empty();
        Ok(if self.confirmation {
            StartPrompt::ConfirmationRequired(warnings)
        } else {
            StartPrompt::Ready
        })
    }

    pub fn start(&mut self, session: &UiSession, confirmed: bool) -> Result<u64, String> {
        if self.confirmation && !confirmed {
            return Err("启动尚未确认".into());
        }
        let snapshot = self.pending.take().ok_or("请先验证参数")?;
        let id = session.start(snapshot, confirmed)?;
        self.confirmation = false;
        self.run_id = Some(id);
        Ok(id)
    }

    pub fn cancel(&self, session: &UiSession) -> Result<(), String> {
        session.cancel(self.run_id.ok_or("没有运行中的任务")?)
    }

    pub fn request_close(&self, session: &UiSession) -> CloseRequest {
        session.request_close()
    }

    pub fn confirm_close(&self, session: &UiSession) -> Result<(), String> {
        session.confirm_close(self.run_id)
    }

    pub fn status(&self, session: &UiSession) -> Option<SessionStatus> {
        self.run_id.and_then(|id| session.status(id))
    }

    pub fn logs(&self, session: &UiSession) -> Result<Vec<String>, String> {
        session.logs()
    }

    /// Build the complete presentation snapshot from the authoritative session.
    /// The native window calls this after each user action (and its refresh action).
    pub fn snapshot(&self, session: &UiSession) -> SessionView {
        let logs = self.logs(session).unwrap_or_else(|error| vec![error]);
        match self.status(session) {
            Some(status) => project_status(status, logs),
            None => view_with_logs(session.bootstrap(), logs),
        }
    }
}

pub fn view(state: RunState) -> SessionView {
    view_with_logs(state, Vec::new())
}

fn view_with_logs(state: RunState, logs: Vec<String>) -> SessionView {
    let message = state.error.or(state.outcome).unwrap_or_else(|| {
        if state.running {
            "运行中".into()
        } else {
            "就绪".into()
        }
    });
    SessionView {
        running: state.running,
        run_id: state.run_id,
        message,
        cancelling: state.cancelling,
        phase: state.active_phase,
        logs,
    }
}

/// Project the session seam into data that is safe for a view to render.
pub fn project_status(status: SessionStatus, logs: Vec<String>) -> SessionView {
    match status {
        SessionStatus::Starting => SessionView {
            running: true,
            run_id: 0,
            message: "正在启动".into(),
            cancelling: false,
            phase: None,
            logs,
        },
        SessionStatus::Running(state) | SessionStatus::Finished(state) => {
            view_with_logs(state, logs)
        }
        SessionStatus::Failed(error) => SessionView {
            running: false,
            run_id: 0,
            message: error,
            cancelling: false,
            phase: None,
            logs,
        },
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CloseAction {
    None,
    Wait,
    Close,
    ShowError(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseDecision {
    CloseNow,
    RequireConfirmation,
}

/// Decide what a native close request should do before touching the window.
pub fn close_decision(request: CloseRequest) -> CloseDecision {
    match request {
        CloseRequest::Allowed => CloseDecision::CloseNow,
        CloseRequest::ConfirmationRequired => CloseDecision::RequireConfirmation,
    }
}

pub fn close_action(status: &CloseStatus) -> CloseAction {
    match status {
        CloseStatus::NotRequested => CloseAction::None,
        CloseStatus::Waiting { .. } => CloseAction::Wait,
        CloseStatus::Completed => CloseAction::Close,
        CloseStatus::Failed(error) => CloseAction::ShowError(error.clone()),
    }
}

/// The platform-independent part of the native slice. Tests can exercise this
/// without creating a Win32 window or loading NSH.
pub fn validate(session: &UiSession, options: &RunOptions) -> String {
    match session.validate(options) {
        Ok(()) => "参数有效".into(),
        Err(error) => error,
    }
}

/// Native file dialogs are deliberately isolated from the portable controller.
/// rfd delegates to the Windows common-dialog API on Windows; tests on other
/// platforms can still exercise validation and state transitions.
#[cfg(windows)]
pub fn pick_inputs(files: bool) -> Result<Vec<String>, String> {
    let dialog = rfd::FileDialog::new();
    let paths = if files {
        dialog
            .add_filter("MP4", &["mp4"])
            .pick_files()
            .unwrap_or_default()
    } else {
        dialog.pick_folder().into_iter().collect()
    };
    Ok(paths
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect())
}

#[cfg(windows)]
pub fn pick_output() -> Result<Option<String>, String> {
    Ok(rfd::FileDialog::new()
        .pick_folder()
        .map(|p| p.to_string_lossy().into_owned()))
}

#[cfg(not(windows))]
pub fn pick_inputs(_files: bool) -> Result<Vec<String>, String> {
    Err("文件对话框仅支持 Windows；请在测试中直接提供路径".into())
}

#[cfg(not(windows))]
pub fn pick_output() -> Result<Option<String>, String> {
    Err("文件对话框仅支持 Windows；请在测试中直接提供路径".into())
}

#[cfg(windows)]
pub fn open_session() -> Result<UiSession, String> {
    use highlight_core::storage::{RuntimeLayout, Storage};
    use std::path::PathBuf;

    let root = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("无法定位应用目录")?
        .to_path_buf();
    let data = std::env::var_os("PUBG_HIGHLIGHT_TRIM_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("data"));
    let storage = Storage::open(data)?;
    Ok(UiSession::new(Shell::new(
        storage,
        RuntimeLayout::from_root(&root),
    )))
}

#[cfg(test)]
mod tests {
    use super::{close_decision, view, CloseDecision};
    use highlight_core::{ui_session::CloseRequest, RunState};

    #[test]
    fn close_decision_requires_confirmation_only_for_active_work() {
        assert_eq!(
            close_decision(CloseRequest::Allowed),
            CloseDecision::CloseNow
        );
        assert_eq!(
            close_decision(CloseRequest::ConfirmationRequired),
            CloseDecision::RequireConfirmation
        );
    }

    #[test]
    fn view_exposes_authoritative_terminal_state() {
        let mut state = RunState::default();
        state.run_id = 4;
        state.error = Some("启动失败".into());
        assert_eq!(
            view(state),
            super::SessionView {
                running: false,
                run_id: 4,
                message: "启动失败".into(),
                cancelling: false,
                phase: None,
                logs: Vec::new(),
            }
        );
    }
}
