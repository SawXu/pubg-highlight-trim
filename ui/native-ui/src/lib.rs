//! Native Windows UI boundary. The stateful workflow remains in `UiSession`.

use highlight_core::{
    ui_session::{CloseRequest, CloseStatus, FrozenRun, SessionStatus, UiSession},
    RunOptions, RunState, Shell,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetMode {
    Combined,
    Separate,
    SelfDeath,
    OwnKill,
}

impl TargetMode {
    pub fn from_options(options: &RunOptions) -> Self {
        match (options.self_death, options.own_kill) {
            (false, false) => Self::Combined,
            (true, true) => Self::Separate,
            (true, false) => Self::SelfDeath,
            (false, true) => Self::OwnKill,
        }
    }

    pub fn apply(self, options: &mut RunOptions) {
        (options.self_death, options.own_kill) = match self {
            Self::Combined => (false, false),
            Self::Separate => (true, true),
            Self::SelfDeath => (true, false),
            Self::OwnKill => (false, true),
        };
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionView {
    pub running: bool,
    pub run_id: u64,
    pub message: String,
    pub cancelling: bool,
    pub phase: Option<String>,
    pub logs: Vec<String>,
    pub included: usize,
    pub skipped: usize,
    pub exit_code: Option<i32>,
    pub output_dir: Option<String>,
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
        self.cancel_confirmation();
    }

    pub fn set_output(&mut self, path: Option<String>) {
        self.options.output_directory = path;
        self.cancel_confirmation();
    }

    pub fn close_status(&self, session: &UiSession) -> CloseStatus {
        session.close_status()
    }

    pub fn reset_defaults(&mut self) {
        self.options = RunOptions::default();
        self.cancel_confirmation();
    }

    pub fn cancel_confirmation(&mut self) {
        self.pending = None;
        self.confirmation = false;
    }

    pub fn command_preview(&self) -> Result<String, String> {
        self.options.preview()
    }

    pub fn warnings(&self) -> Vec<String> {
        self.options.warnings()
    }

    pub fn clear_logs(&self, session: &UiSession) -> Result<(), String> {
        session.clear_logs()
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
    let message = state
        .error
        .or_else(|| {
            state.outcome.map(|s| match s.as_str() {
                "success" => "处理完成".into(),
                "no-events" => "没有检测到事件".into(),
                "partial-unreadable" => "部分视频无法读取".into(),
                "all-unreadable" => "所有视频无法读取".into(),
                "failed" => "处理失败".into(),
                "cancelled" => "已取消".into(),
                "cleanup-failed" => "清理失败".into(),
                _ => s,
            })
        })
        .unwrap_or_else(|| {
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
        phase: state.active_phase.map(|s| match s.as_str() {
            "scan" => "OCR 扫描".into(),
            "dedupe" => "事件去重".into(),
            "trim" => "裁剪片段".into(),
            "merge" => "合并输出".into(),
            _ => s,
        }),
        included: state.included as usize,
        skipped: state.skipped as usize,
        exit_code: state.exit_code,
        output_dir: state
            .summary
            .as_ref()
            .and_then(|s| s.get("output_dir"))
            .and_then(|v| v.as_str())
            .map(str::to_owned),
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
            included: 0,
            skipped: 0,
            exit_code: None,
            output_dir: None,
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
            included: 0,
            skipped: 0,
            exit_code: None,
            output_dir: None,
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

/// Presentation state for the title-bar close prompt, independent of start warnings.
/// Cleanup and task lifecycle remain owned by UiSession.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ClosePrompt {
    #[default]
    Hidden,
    Confirming,
    Waiting,
    Failed(String),
}

impl ClosePrompt {
    pub fn request(&mut self, request: CloseRequest) -> CloseAction {
        if *self == Self::Waiting {
            return CloseAction::Wait;
        }
        match close_decision(request) {
            CloseDecision::CloseNow => CloseAction::Close,
            CloseDecision::RequireConfirmation => {
                // Repeated title-bar clicks must not discard a displayed error.
                if *self == Self::Hidden {
                    *self = Self::Confirming;
                }
                CloseAction::None
            }
        }
    }

    pub fn dismiss(&mut self) {
        if *self != Self::Waiting {
            *self = Self::Hidden;
        }
    }

    pub fn confirm(
        &mut self,
        request: CloseRequest,
        cleanup: impl FnOnce() -> Result<(), String>,
    ) -> CloseAction {
        if self.request(request) == CloseAction::Close {
            // The task may have finished naturally while the prompt was open.
            return CloseAction::Close;
        }
        if *self == Self::Waiting {
            return CloseAction::Wait;
        }
        match cleanup() {
            Ok(()) => {
                *self = Self::Waiting;
                CloseAction::Wait
            }
            Err(error) => {
                *self = Self::Failed(error.clone());
                CloseAction::ShowError(error)
            }
        }
    }

    pub fn poll(&mut self, status: &CloseStatus) -> CloseAction {
        if *self != Self::Waiting {
            return CloseAction::None;
        }
        let action = close_action(status);
        if let CloseAction::ShowError(error) = &action {
            *self = Self::Failed(error.clone());
        }
        action
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

pub fn open_output_dir(path: &str) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err("没有输出目录".into());
    }
    #[cfg(windows)]
    {
        std::process::Command::new("explorer.exe")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("无法打开输出目录：{e}"))
    }
    #[cfg(not(windows))]
    Err("打开输出目录仅支持 Windows".into())
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
    fn target_cards_round_trip_and_preview_cli_flags() {
        use super::{NativeController, TargetMode};
        use highlight_core::RunOptions;
        let mut c = NativeController::new(RunOptions::default());
        c.options.input_paths = vec![std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .into_owned()];
        for (mode, flags, target, split) in [
            (TargetMode::Combined, (false, false), "both", false),
            (TargetMode::Separate, (true, true), "both", true),
            (TargetMode::SelfDeath, (true, false), "self-death", false),
            (TargetMode::OwnKill, (false, true), "own-kill", false),
        ] {
            mode.apply(&mut c.options);
            assert_eq!((c.options.self_death, c.options.own_kill), flags);
            assert_eq!(TargetMode::from_options(&c.options), mode);
            let preview = c.command_preview().unwrap();
            assert!(preview.contains(&format!("--target {target}")), "{preview}");
            assert_eq!(preview.contains("--split-merge-by-target"), split);
        }
        c.options.input_paths.clear();
        assert!(c.command_preview().is_err());
    }

    #[test]
    fn reset_defaults_discards_settings_and_start_confirmation() {
        use highlight_core::{storage::Storage, ui_session::UiSession, RunOptions, Shell};
        let root = std::env::temp_dir().join(format!(
            "native-controller-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let storage = Storage::open(root.clone()).unwrap();
        let session = UiSession::new(Shell::new(storage, Err("test: no CLI needed".into())));
        let mut c = super::NativeController::new(RunOptions::default());
        c.options.overwrite = true;
        c.options.seconds_before = 42.0;
        c.set_inputs(vec![root.to_string_lossy().into_owned()]);
        assert_eq!(
            c.prepare_start(&session).unwrap(),
            super::StartPrompt::ConfirmationRequired(vec![
                "覆盖现有输出文件，此操作不可撤销。".into()
            ])
        );
        assert!(c.confirmation);
        c.reset_defaults();
        assert_eq!(c.options, RunOptions::default());
        assert!(!c.confirmation);
        assert!(c.command_preview().is_err());
        assert_eq!(c.start(&session, true), Err("请先验证参数".into()));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn clearing_logs_is_visible_through_the_session_snapshot() {
        use highlight_core::{storage::Storage, ui_session::UiSession, RunOptions, Shell};
        let root = std::env::temp_dir().join(format!(
            "native-logs-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let storage = Storage::open(root.clone()).unwrap();
        storage.append("existing log line").unwrap();
        let session = UiSession::new(Shell::new(storage, Err("test: no CLI needed".into())));
        let c = super::NativeController::new(RunOptions::default());
        assert_eq!(c.snapshot(&session).logs, vec!["existing log line"]);
        c.clear_logs(&session).unwrap();
        assert!(c.snapshot(&session).logs.is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn view_translates_outcomes_and_phases_and_preserves_metrics() {
        for (code, label) in [
            ("success", "处理完成"),
            ("no-events", "没有检测到事件"),
            ("partial-unreadable", "部分视频无法读取"),
            ("all-unreadable", "所有视频无法读取"),
            ("failed", "处理失败"),
            ("cancelled", "已取消"),
            ("cleanup-failed", "清理失败"),
        ] {
            let mut state = RunState::default();
            state.outcome = Some(code.into());
            assert_eq!(view(state).message, label);
        }
        for (code, label) in [
            ("scan", "OCR 扫描"),
            ("dedupe", "事件去重"),
            ("trim", "裁剪片段"),
            ("merge", "合并输出"),
        ] {
            let mut state = RunState::default();
            state.active_phase = Some(code.into());
            state.included = 3;
            state.skipped = 2;
            state.exit_code = Some(0);
            state.line(
                r#"SUMMARY {"output_dir":"C:/clips","included_count":3,"skipped_count":2}"#,
                false,
            );
            let v = view(state);
            assert_eq!(v.phase.as_deref(), Some(label));
            assert_eq!((v.included, v.skipped, v.exit_code), (3, 2, Some(0)));
            assert_eq!(v.output_dir.as_deref(), Some("C:/clips"));
        }
    }

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
    fn close_prompt_is_conditional_and_can_be_dismissed_without_cleanup() {
        use super::{CloseAction, ClosePrompt};
        let mut prompt = ClosePrompt::default();
        assert_eq!(prompt, ClosePrompt::Hidden);
        assert_eq!(prompt.request(CloseRequest::Allowed), CloseAction::Close);
        assert_eq!(prompt, ClosePrompt::Hidden);
        for _ in 0..2 {
            assert_eq!(
                prompt.request(CloseRequest::ConfirmationRequired),
                CloseAction::None
            );
            assert_eq!(prompt, ClosePrompt::Confirming);
        }
        prompt.dismiss();
        assert_eq!(prompt, ClosePrompt::Hidden);
    }

    #[test]
    fn close_prompt_waits_for_cleanup_and_ignores_repeated_operations() {
        use super::{CloseAction, ClosePrompt};
        use highlight_core::ui_session::CloseStatus;
        let mut prompt = ClosePrompt::Confirming;
        let mut calls = 0;
        assert_eq!(
            prompt.confirm(CloseRequest::ConfirmationRequired, || {
                calls += 1;
                Ok(())
            }),
            CloseAction::Wait
        );
        assert_eq!(calls, 1);
        assert_eq!(prompt, ClosePrompt::Waiting);
        // Even if the task now looks idle, cleanup must finish before closing.
        assert_eq!(prompt.request(CloseRequest::Allowed), CloseAction::Wait);
        assert_eq!(
            prompt.confirm(CloseRequest::Allowed, || panic!("duplicate cleanup")),
            CloseAction::Wait
        );
        prompt.dismiss();
        assert_eq!(prompt, ClosePrompt::Waiting);
        assert_eq!(
            prompt.poll(&CloseStatus::Waiting { run_id: None }),
            CloseAction::Wait
        );
        assert_eq!(prompt.poll(&CloseStatus::Completed), CloseAction::Close);
    }

    #[test]
    fn close_prompt_handles_natural_completion_and_cleanup_failures() {
        use super::{CloseAction, ClosePrompt};
        use highlight_core::ui_session::CloseStatus;
        let mut prompt = ClosePrompt::Confirming;
        assert_eq!(
            prompt.confirm(CloseRequest::Allowed, || panic!(
                "finished task needs no cancellation"
            )),
            CloseAction::Close
        );
        assert_eq!(
            prompt.confirm(CloseRequest::ConfirmationRequired, || Err(
                "cancel failed".into()
            )),
            CloseAction::ShowError("cancel failed".into())
        );
        assert_eq!(prompt, ClosePrompt::Failed("cancel failed".into()));
        assert_eq!(
            prompt.confirm(CloseRequest::ConfirmationRequired, || Ok(())),
            CloseAction::Wait
        );
        assert_eq!(
            prompt.poll(&CloseStatus::Failed("cleanup failed".into())),
            CloseAction::ShowError("cleanup failed".into())
        );
        assert_eq!(prompt, ClosePrompt::Failed("cleanup failed".into()));
        assert_eq!(
            prompt.request(CloseRequest::ConfirmationRequired),
            CloseAction::None
        );
        assert_eq!(prompt, ClosePrompt::Failed("cleanup failed".into()));
        prompt.dismiss();
        assert_eq!(
            prompt.poll(&CloseStatus::Failed("old failure".into())),
            CloseAction::None
        );
        assert_eq!(prompt, ClosePrompt::Hidden);
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
                included: 0,
                skipped: 0,
                exit_code: None,
                output_dir: None,
            }
        );
    }
}
