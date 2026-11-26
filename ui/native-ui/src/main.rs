#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod windows_app {
    use highlight_core::RunOptions;
    use native_ui::{
        close_action, close_decision, open_session, validate, CloseAction, CloseDecision,
        NativeController,
    };
    use nsh_base::button;
    use nsh_core::{element::desktop as window, prelude::*, Color};
    use nsh_platform_windows::{run_app, AppOptions, AppSource, Root};
    use std::{
        cell::RefCell,
        rc::Rc,
        sync::{Arc, Mutex, OnceLock},
        time::Duration,
    };

    static SESSION: OnceLock<highlight_core::ui_session::UiSession> = OnceLock::new();

    fn session() -> &'static highlight_core::ui_session::UiSession {
        SESSION.get().expect("native session is initialized")
    }

    #[component]
    fn NativeWindow() -> impl IntoElement {
        let session = session();
        let mut loaded = NativeController::new(RunOptions::default());
        let load_message = loaded
            .load_settings(session)
            .map(|_| "已加载设置".to_string())
            .unwrap_or_else(|error| format!("设置加载失败：{error}"));
        let controller = Arc::new(Mutex::new(loaded));
        let initial = controller.lock().unwrap().snapshot(session);
        let status = signal(load_message);
        let details = signal(String::new());
        let inputs = signal(String::new());
        let output = signal(String::new());
        let options = signal(String::new());
        let refresh = {
            let status = status.clone();
            let details = details.clone();
            let controller = Arc::clone(&controller);
            move || {
                let view = controller.lock().unwrap().snapshot(session);
                status.set(view.message);
                let c = controller.lock().unwrap();
                details.set(format!(
                    "运行 ID：{}  阶段：{}  关闭：{:?}\n日志：{}",
                    view.run_id,
                    view.phase.unwrap_or_else(|| "无".into()),
                    c.close_status(session),
                    view.logs.join("\n")
                ));
                inputs.set(if c.options.input_paths.is_empty() {
                    "输入：未选择".into()
                } else {
                    format!("输入：{}", c.options.input_paths.join("；"))
                });
                output.set(format!(
                    "输出：{}",
                    c.options.output_directory.as_deref().unwrap_or("默认目录")
                ));
                options.set(format!("选项：语言={} 扫描={} Trim={} 前后={} / {} 秒；递归={} 合并={} 仅扫描={} 覆盖={} 亮度门={} 移动原片={}", c.options.game_language, c.options.scan_mode, c.options.trim_jobs, c.options.seconds_before, c.options.seconds_after, c.options.recursive, c.options.merge, c.options.scan_only, c.options.overwrite, c.options.brightness_gate, c.options.move_originals));
            }
        };
        let close = window::current_window();
        // NSH 0.6.1 exposes the native WM_CLOSE path as `on_close_press`.
        // Disable its default immediate close so cleanup remains authoritative.
        let mut window_attributes = nsh_core::desktop::WindowAttributes::default();
        window_attributes.close_on_press = Some(false);
        close.set(window_attributes);
        let close_poll: Rc<RefCell<Option<Spawned<()>>>> = Rc::new(RefCell::new(None));
        let pick_files = {
            let status = status.clone();
            let inputs = inputs.clone();
            let controller = Arc::clone(&controller);
            move || match native_ui::pick_inputs(true) {
                Ok(paths) if !paths.is_empty() => {
                    controller.lock().unwrap().set_inputs(paths);
                    status.set("已选择输入文件，请刷新查看".into());
                    inputs.set("输入：已更新（请刷新）".into());
                }
                Ok(_) => status.set("未选择输入文件".into()),
                Err(error) => status.set(format!("选择输入失败：{error}")),
            }
        };
        let pick_folder = {
            let status = status.clone();
            let output = output.clone();
            let controller = Arc::clone(&controller);
            move || match native_ui::pick_output() {
                Ok(path) => {
                    controller.lock().unwrap().set_output(path);
                    status.set("已选择输出目录，请刷新查看".into());
                    output.set("输出：已更新（请刷新）".into());
                }
                Err(error) => status.set(format!("选择输出失败：{error}")),
            }
        };
        let save = {
            let status = status.clone();
            let controller = Arc::clone(&controller);
            move || {
                status.set(
                    controller
                        .lock()
                        .unwrap()
                        .save_settings(session)
                        .map(|_| "设置已保存".into())
                        .unwrap_or_else(|error| format!("设置保存失败：{error}")),
                )
            }
        };
        let edit_options = {
            let status = status.clone();
            let controller = Arc::clone(&controller);
            move || {
                controller.lock().unwrap().toggle_core_options();
                status.set("已切换核心选项（仅扫描、递归、提前秒数）".into());
            }
        };
        let start = {
            let status = status.clone();
            let controller = Arc::clone(&controller);
            move || {
                let mut controller = controller.lock().unwrap();
                let result = if controller.confirmation {
                    controller.start(session, true).map(|_| "已启动".into())
                } else {
                    match controller.prepare_start(session) {
                        Ok(native_ui::StartPrompt::ConfirmationRequired(warnings)) => {
                            Ok(format!("危险选项需确认：{}", warnings.join("；")))
                        }
                        Ok(native_ui::StartPrompt::Ready) => {
                            controller.start(session, false).map(|_| "已启动".into())
                        }
                        Err(error) => Err(error),
                    }
                };
                status.set(result.unwrap_or_else(|error| format!("启动失败：{error}")));
            }
        };
        let cancel = {
            let status = status.clone();
            let controller = Arc::clone(&controller);
            move || {
                let result = controller.lock().unwrap().cancel(session);
                status.set(
                    result
                        .map(|_| "已请求取消，请刷新状态".into())
                        .unwrap_or_else(|error| format!("取消失败：{error}")),
                );
            }
        };
        let confirm_close = {
            let status = status.clone();
            let controller = Arc::clone(&controller);
            let close = close.clone();
            let close_poll = Rc::clone(&close_poll);
            move || {
                let result = controller.lock().unwrap().confirm_close(session);
                if let Err(error) = result {
                    status.set(format!("关闭失败：{error}"));
                    return;
                }
                status.set("正在清理任务".into());
                let status_poll = status.clone();
                let close_poll_for_task = Rc::clone(&close_poll);
                let close_to_poll = close.clone();
                let task = set_interval(Duration::from_millis(50), move || {
                    match close_action(&session.close_status()) {
                        CloseAction::Close => {
                            status_poll.set("清理完成，正在关闭".into());
                            close_to_poll.clone().close();
                            if let Some(task) = close_poll_for_task.borrow_mut().take() {
                                task.cancel();
                            }
                        }
                        CloseAction::ShowError(error) => {
                            status_poll.set(format!("关闭失败：{error}"));
                            if let Some(task) = close_poll_for_task.borrow_mut().take() {
                                task.cancel();
                            }
                        }
                        CloseAction::Wait => {}
                        CloseAction::None => {
                            status_poll.set("未请求关闭".into());
                            if let Some(task) = close_poll_for_task.borrow_mut().take() {
                                task.cancel();
                            }
                        }
                    }
                });
                *close_poll.borrow_mut() = Some(task);
            }
        };
        let request_close = {
            let status = status.clone();
            let close = close.clone();
            move || match close_decision(session.request_close()) {
                CloseDecision::CloseNow => close.close(),
                CloseDecision::RequireConfirmation => {
                    status.set("任务仍在运行，请确认停止并关闭".into())
                }
            }
        };
        // This callback receives system close, Alt+F4, taskbar and NSH chrome
        // close requests—not just presses of the in-app button.
        close.on_close_press({
            let status = status.clone();
            let close = close.clone();
            move |_| match close_decision(session.request_close()) {
                CloseDecision::CloseNow => close.close(),
                CloseDecision::RequireConfirmation => {
                    status.set("任务仍在运行，请确认停止并关闭".into())
                }
            }
        });
        nsh_core::element::view()
            .flex_1()
            .p_6()
            .gap_3()
            .bg(Color(0xff_f7f7f7))
            .child(text("PUBG Highlight Trim").text_size(px(24.0)))
            .child(text(move || status.get()))
            .child(text(move || details.get()))
            .child(text(move || inputs.get()))
            .child(text(move || output.get()))
            .child(text(move || options.get()))
            .child(button().child(text("选择输入 MP4")).on_press(pick_files))
            .child(button().child(text("选择输出目录")).on_press(pick_folder))
            .child(button().child(text("编辑核心选项")).on_press(edit_options))
            .child(button().child(text("保存设置")).on_press(save))
            .child(button().child(text("验证当前参数")).on_press({
                let status = status.clone();
                let controller = Arc::clone(&controller);
                move || status.set(validate(session, &controller.lock().unwrap().options))
            }))
            .child(button().child(text("验证默认参数")).on_press({
                let status = status.clone();
                move || status.set(validate(session, &RunOptions::default()))
            }))
            .child(button().child(text("启动 / 确认危险选项")).on_press(start))
            .child(button().child(text("取消")).on_press(cancel))
            .child(
                button()
                    .child(text("刷新状态 / 日志 / 关闭状态"))
                    .on_press(refresh),
            )
            .child(button().child(text("关闭")).on_press(request_close))
            .child(
                button()
                    .child(text("确认停止并关闭"))
                    .on_press(confirm_close),
            )
    }

    pub fn run() -> std::process::ExitCode {
        let session = match open_session() {
            Ok(session) => session,
            Err(error) => {
                eprintln!("无法启动 native UI: {error}");
                return std::process::ExitCode::FAILURE;
            }
        };
        let _ = SESSION.set(session);
        run_app(
            AppSource::Rust(Root::of::<NativeWindow>()),
            AppOptions {
                title: "PUBG Highlight Trim".into(),
                size: (1180, 840),
                ..Default::default()
            },
        )
    }
}

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    windows_app::run()
}

#[cfg(not(windows))]
fn main() {
    // The product window is Windows-only; state logic remains testable here.
}
