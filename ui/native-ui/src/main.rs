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

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum ThemeMode {
        Light,
        Dark,
    }

    #[derive(Clone, Copy)]
    struct ThemePalette {
        topbar_bg: Color,
        topbar_border: Color,
        brand_gold: Color,
        brand_gold_text: Color,
        topbar_text: Color,
        topbar_subtext: Color,
        canvas_bg: Color,
        surface_main: Color,
        surface_subtle: Color,
        border_main: Color,
        border_input: Color,
        text_primary: Color,
        text_secondary: Color,
        text_muted: Color,
        primary_bg: Color,
        primary_hover: Color,
        primary_text: Color,
        accent_text: Color,
        btn_sec_bg: Color,
        btn_sec_hover: Color,
        btn_sec_border_hover: Color,
        status_pill_bg: Color,
        status_pill_border: Color,
        status_pill_text: Color,
        status_dot: Color,
        danger_bg: Color,
        danger_bg_hover: Color,
        danger_text: Color,
        danger_border: Color,
        log_bg: Color,
        log_header_bg: Color,
        log_border: Color,
        log_text: Color,
        log_muted: Color,
        log_dot: Color,
        footer_bg: Color,
        footer_text: Color,
    }

    impl ThemeMode {
        fn toggle(self) -> Self {
            match self {
                Self::Light => Self::Dark,
                Self::Dark => Self::Light,
            }
        }

        fn toggle_label(self) -> &'static str {
            match self {
                Self::Light => "深色主题",
                Self::Dark => "浅色主题",
            }
        }

        fn palette(self) -> ThemePalette {
            match self {
                Self::Light => ThemePalette {
                    topbar_bg: Color(0xff_ffff_ff),
                    topbar_border: Color(0xd9_e1dc_ff),
                    brand_gold: Color(0xe8_b862_ff),
                    brand_gold_text: Color(0x17_251e_ff),
                    topbar_text: Color(0x1b_2d24_ff),
                    topbar_subtext: Color(0x64_756b_ff),
                    canvas_bg: Color(0xf3_f6f4_ff),
                    surface_main: Color(0xff_ffff_ff),
                    surface_subtle: Color(0xf8_faf9_ff),
                    border_main: Color(0xdc_e4df_ff),
                    border_input: Color(0xcb_d5cf_ff),
                    text_primary: Color(0x21_342b_ff),
                    text_secondary: Color(0x4b_5e54_ff),
                    text_muted: Color(0x6b_7c71_ff),
                    primary_bg: Color(0x24_734b_ff),
                    primary_hover: Color(0x19_5936_ff),
                    primary_text: Color(0xff_ffff_ff),
                    accent_text: Color(0x24_734b_ff),
                    btn_sec_bg: Color(0xff_ffff_ff),
                    btn_sec_hover: Color(0xed_f4f0_ff),
                    btn_sec_border_hover: Color(0x63_8773_ff),
                    status_pill_bg: Color(0xea_f4ed_ff),
                    status_pill_border: Color(0xc4_dfd0_ff),
                    status_pill_text: Color(0x1d_6340_ff),
                    status_dot: Color(0x24_734b_ff),
                    danger_bg: Color(0xff_eae5_ff),
                    danger_bg_hover: Color(0xf9_d3c9_ff),
                    danger_text: Color(0x8a_2e23_ff),
                    danger_border: Color(0xe6_b3a8_ff),
                    log_bg: Color(0xfb_fdfb_ff),
                    log_header_bg: Color(0xf1_f5f3_ff),
                    log_border: Color(0xd2_dcd6_ff),
                    log_text: Color(0x21_342b_ff),
                    log_muted: Color(0x6b_7c71_ff),
                    log_dot: Color(0x24_734b_ff),
                    footer_bg: Color(0xe9_efeb_ff),
                    footer_text: Color(0x5f_7065_ff),
                },
                Self::Dark => ThemePalette {
                    topbar_bg: Color(0x14_1e19_ff),
                    topbar_border: Color(0x24_352c_ff),
                    brand_gold: Color(0xe8_b862_ff),
                    brand_gold_text: Color(0x14_1e19_ff),
                    topbar_text: Color(0xee_f5f0_ff),
                    topbar_subtext: Color(0x8e_a397_ff),
                    canvas_bg: Color(0x0f_1713_ff),
                    surface_main: Color(0x16_221c_ff),
                    surface_subtle: Color(0x1c_2a23_ff),
                    border_main: Color(0x26_382e_ff),
                    border_input: Color(0x31_473b_ff),
                    text_primary: Color(0xe6_f0ea_ff),
                    text_secondary: Color(0xc2_d4ca_ff),
                    text_muted: Color(0x87_9e90_ff),
                    primary_bg: Color(0x2a_8557_ff),
                    primary_hover: Color(0x22_6e48_ff),
                    primary_text: Color(0xff_ffff_ff),
                    accent_text: Color(0x63_cf97_ff),
                    btn_sec_bg: Color(0x1d_2c24_ff),
                    btn_sec_hover: Color(0x26_392f_ff),
                    btn_sec_border_hover: Color(0x4e_7360_ff),
                    status_pill_bg: Color(0x1c_2e25_ff),
                    status_pill_border: Color(0x2e_4c3d_ff),
                    status_pill_text: Color(0x8f_d9ac_ff),
                    status_dot: Color(0x63_cf97_ff),
                    danger_bg: Color(0x38_1e1b_ff),
                    danger_bg_hover: Color(0x4a_2723_ff),
                    danger_text: Color(0xf4_aba0_ff),
                    danger_border: Color(0x6c_352e_ff),
                    log_bg: Color(0x12_1c17_ff),
                    log_header_bg: Color(0x18_261f_ff),
                    log_border: Color(0x26_382e_ff),
                    log_text: Color(0xd6_e7dc_ff),
                    log_muted: Color(0x7e_9989_ff),
                    log_dot: Color(0xe8_b862_ff),
                    footer_bg: Color(0x13_1d18_ff),
                    footer_text: Color(0x7e_9688_ff),
                },
            }
        }
    }

    static SESSION: OnceLock<highlight_core::ui_session::UiSession> = OnceLock::new();
    fn session() -> &'static highlight_core::ui_session::UiSession {
        SESSION.get().expect("native session is initialized")
    }

    fn close_label(status: &highlight_core::ui_session::CloseStatus) -> &'static str {
        match status {
            highlight_core::ui_session::CloseStatus::NotRequested => "待命",
            highlight_core::ui_session::CloseStatus::Waiting { .. } => "等待确认",
            highlight_core::ui_session::CloseStatus::Completed => "已完成",
            highlight_core::ui_session::CloseStatus::Failed(_) => "清理失败",
        }
    }

    #[component]
    fn NativeWindow() -> impl IntoElement {
        let session = session();
        let mut loaded = NativeController::new(RunOptions::default());
        let load_message = loaded
            .load_settings(session)
            .map(|_| "已加载设置".to_string())
            .unwrap_or_else(|e| format!("设置加载失败：{e}"));
        let controller = Arc::new(Mutex::new(loaded));
        let status = signal(load_message);
        let inputs = signal(String::new());
        let output = signal(String::new());
        let phase = signal(String::new());
        let run_id = signal(String::new());
        let close_state = signal(String::new());
        let logs_text = signal(String::new());
        let language = signal(String::new());
        let scan_mode = signal(String::new());
        let trim_jobs = signal(String::new());
        let trim_seconds = signal(String::new());
        let recursive = signal(String::new());
        let merge = signal(String::new());
        let scan_only = signal(String::new());
        let overwrite = signal(String::new());
        let summary = signal(String::new());
        let theme_mode = signal(ThemeMode::Light);

        let sync_view = {
            let controller = Arc::clone(&controller);
            let inputs = inputs.clone();
            let output = output.clone();
            let phase = phase.clone();
            let run_id = run_id.clone();
            let close_state = close_state.clone();
            let logs_text = logs_text.clone();
            let language = language.clone();
            let scan_mode = scan_mode.clone();
            let trim_jobs = trim_jobs.clone();
            let trim_seconds = trim_seconds.clone();
            let recursive = recursive.clone();
            let merge = merge.clone();
            let scan_only = scan_only.clone();
            let overwrite = overwrite.clone();
            let summary = summary.clone();
            let status = status.clone();
            move || {
                let c = controller.lock().unwrap();
                let view = c.snapshot(session);
                status.set(view.message.clone());
                inputs.set(if c.options.input_paths.is_empty() {
                    "尚未选择输入文件".into()
                } else {
                    c.options.input_paths.join("；")
                });
                output.set(
                    c.options
                        .output_directory
                        .as_deref()
                        .unwrap_or("使用默认目录")
                        .into(),
                );
                phase.set(view.phase.clone().unwrap_or_else(|| "待命".into()));
                run_id.set(if view.run_id == 0 {
                    "—".into()
                } else {
                    view.run_id.to_string()
                });
                close_state.set(close_label(&c.close_status(session)).into());
                logs_text.set(if view.logs.is_empty() {
                    "> 等待任务启动...\n> 请在左侧选择输入 MP4 文件，然后点击右上角「开始处理」。"
                        .into()
                } else {
                    view.logs.join("\n")
                });
                language.set(c.options.game_language.clone());
                scan_mode.set(c.options.scan_mode.clone());
                trim_jobs.set(c.options.trim_jobs.clone());
                trim_seconds.set(format!(
                    "{} / {} 秒",
                    c.options.seconds_before, c.options.seconds_after
                ));
                recursive.set(
                    if c.options.recursive {
                        "开启"
                    } else {
                        "关闭"
                    }
                    .into(),
                );
                merge.set(if c.options.merge { "开启" } else { "关闭" }.into());
                scan_only.set(
                    if c.options.scan_only {
                        "开启"
                    } else {
                        "关闭"
                    }
                    .into(),
                );
                overwrite.set(
                    if c.options.overwrite {
                        "开启"
                    } else {
                        "关闭"
                    }
                    .into(),
                );
                summary.set(format!(
                    "{} · {}",
                    view.message,
                    view.phase.as_deref().unwrap_or("待命")
                ));
            }
        };
        sync_view();
        let close = window::current_window();
        let mut attrs = nsh_core::desktop::WindowAttributes::default();
        attrs.close_on_press = Some(false);
        close.set(attrs);
        let close_poll: Rc<RefCell<Option<Spawned<()>>>> = Rc::new(RefCell::new(None));

        let pick_files = {
            let c = Arc::clone(&controller);
            let sync = sync_view.clone();
            let status = status.clone();
            move || match native_ui::pick_inputs(true) {
                Ok(paths) if !paths.is_empty() => {
                    c.lock().unwrap().set_inputs(paths);
                    sync();
                }
                Ok(_) => status.set("未选择输入文件".into()),
                Err(e) => status.set(format!("选择输入失败：{e}")),
            }
        };
        let pick_folder = {
            let c = Arc::clone(&controller);
            let sync = sync_view.clone();
            let status = status.clone();
            move || match native_ui::pick_output() {
                Ok(path) => {
                    c.lock().unwrap().set_output(path);
                    sync();
                }
                Err(e) => status.set(format!("选择输出失败：{e}")),
            }
        };
        let refresh = {
            let sync = sync_view.clone();
            move || sync()
        };
        let save = {
            let c = Arc::clone(&controller);
            let sync = sync_view.clone();
            let status = status.clone();
            move || {
                let result = c
                    .lock()
                    .unwrap()
                    .save_settings(session)
                    .map(|_| "设置已保存".into())
                    .unwrap_or_else(|e| format!("设置保存失败：{e}"));
                sync();
                status.set(result);
            }
        };
        let edit_options = {
            let c = Arc::clone(&controller);
            let sync = sync_view.clone();
            move || {
                c.lock().unwrap().toggle_core_options();
                sync();
            }
        };
        let validate_current = {
            let c = Arc::clone(&controller);
            let sync = sync_view.clone();
            let status = status.clone();
            move || {
                let result = validate(session, &c.lock().unwrap().options);
                sync();
                status.set(result);
            }
        };
        let start = {
            let c = Arc::clone(&controller);
            let sync = sync_view.clone();
            let status = status.clone();
            move || {
                let mut c = c.lock().unwrap();
                let result = if c.confirmation {
                    c.start(session, true).map(|_| "已启动".into())
                } else {
                    match c.prepare_start(session) {
                        Ok(native_ui::StartPrompt::ConfirmationRequired(w)) => {
                            Ok(format!("危险选项需确认：{}", w.join("；")))
                        }
                        Ok(native_ui::StartPrompt::Ready) => {
                            c.start(session, false).map(|_| "已启动".into())
                        }
                        Err(e) => Err(e),
                    }
                };
                drop(c);
                sync();
                if let Ok(message) = result {
                    status.set(message);
                } else if let Err(e) = result {
                    status.set(format!("启动失败：{e}"));
                }
            }
        };
        let cancel = {
            let c = Arc::clone(&controller);
            let sync = sync_view.clone();
            let status = status.clone();
            move || {
                let result = c.lock().unwrap().cancel(session);
                sync();
                status.set(
                    result
                        .map(|_| "已请求取消".into())
                        .unwrap_or_else(|e| format!("取消失败：{e}")),
                );
            }
        };
        let confirm_close = {
            let status = status.clone();
            let c = Arc::clone(&controller);
            let close = close.clone();
            let poll = Rc::clone(&close_poll);
            move || {
                if let Err(e) = c.lock().unwrap().confirm_close(session) {
                    status.set(format!("关闭失败：{e}"));
                    return;
                }
                status.set("正在清理任务".into());
                let s = status.clone();
                let p = Rc::clone(&poll);
                let w = close.clone();
                let task = set_interval(Duration::from_millis(50), move || {
                    match close_action(&session.close_status()) {
                        CloseAction::Close => {
                            s.set("清理完成，正在关闭".into());
                            w.clone().close();
                            if let Some(t) = p.borrow_mut().take() {
                                t.cancel();
                            }
                        }
                        CloseAction::ShowError(e) => {
                            s.set(format!("关闭失败：{e}"));
                            if let Some(t) = p.borrow_mut().take() {
                                t.cancel();
                            }
                        }
                        CloseAction::Wait => {}
                        CloseAction::None => {
                            s.set("未请求关闭".into());
                            if let Some(t) = p.borrow_mut().take() {
                                t.cancel();
                            }
                        }
                    }
                });
                *poll.borrow_mut() = Some(task);
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

        let toggle_theme = {
            let theme_mode = theme_mode.clone();
            move || theme_mode.set(theme_mode.get().toggle())
        };

        nsh_core::element::view().flex_1().flex_col().child(
            nsh_core::element::dynamic(move || {
                let mode = theme_mode.get();
                let t = mode.palette();
                let secondary = move |label: &'static str| {
                    button()
                        .px_3()
                        .py_2()
                        .rounded_md()
                        .bg(t.btn_sec_bg)
                        .border_1()
                        .border_color(t.border_input)
                        .text_size(px(13.0))
                        .text_color(t.text_secondary)
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(move |s| s.bg(t.btn_sec_hover).border_color(t.btn_sec_border_hover))
                        .child(text(label))
                };
                let value_card = move |label: &'static str, value: Signal<String>| {
                    nsh_core::element::view()
                        .flex_1()
                        .flex_col()
                        .gap_1()
                        .bg(t.surface_subtle)
                        .border_1()
                        .border_color(t.border_main)
                        .rounded_md()
                        .px_3()
                        .py_2()
                        .child(text(label).text_size(px(11.0)).text_color(t.text_muted))
                        .child(
                            text(move || value.get())
                                .text_size(px(13.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(t.text_primary),
                        )
                };
                let metric = move |label: &'static str, value: Signal<String>, color: Color| {
                    nsh_core::element::view()
                        .flex_1()
                        .flex_col()
                        .gap_1()
                        .bg(t.surface_main)
                        .border_1()
                        .border_color(t.border_main)
                        .rounded_lg()
                        .px_4()
                        .py_3()
                        .shadow_sm()
                        .child(text(label).text_size(px(12.0)).text_color(t.text_muted))
                        .child(
                            text(move || value.get())
                                .text_size(px(20.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(color),
                        )
                };

                let status = status.clone();
                let inputs = inputs.clone();
                let output = output.clone();
                let phase = phase.clone();
                let run_id = run_id.clone();
                let close_state = close_state.clone();
                let logs_text = logs_text.clone();
                let language = language.clone();
                let scan_mode = scan_mode.clone();
                let trim_jobs = trim_jobs.clone();
                let trim_seconds = trim_seconds.clone();
                let recursive = recursive.clone();
                let merge = merge.clone();
                let scan_only = scan_only.clone();
                let overwrite = overwrite.clone();
                let summary = summary.clone();

                let pick_files = pick_files.clone();
                let pick_folder = pick_folder.clone();
                let refresh = refresh.clone();
                let save = save.clone();
                let edit_options = edit_options.clone();
                let validate_current = validate_current.clone();
                let start = start.clone();
                let cancel = cancel.clone();
                let request_close = request_close.clone();
                let confirm_close = confirm_close.clone();
                let toggle_theme = toggle_theme.clone();

                nsh_core::element::view()
                    .flex_1()
                    .flex_col()
                    .bg(t.canvas_bg)
                    .child(
                        nsh_core::element::view()
                            .px_6()
                            .py_4()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .bg(t.topbar_bg)
                            .border_b_1()
                            .border_color(t.topbar_border)
                            .child(
                                nsh_core::element::view()
                                    .flex_row()
                                    .items_center()
                                    .gap_3()
                                    .child(
                                        nsh_core::element::view()
                                            .w(px(36.0))
                                            .h(px(36.0))
                                            .rounded_lg()
                                            .bg(t.brand_gold)
                                            .items_center()
                                            .justify_center()
                                            .child(
                                                text("P")
                                                    .text_size(px(18.0))
                                                    .font_weight(FontWeight::BOLD)
                                                    .text_color(t.brand_gold_text),
                                            ),
                                    )
                                    .child(
                                        nsh_core::element::view()
                                            .flex_col()
                                            .gap_1()
                                            .child(
                                                text("PUBG Highlight Trim")
                                                    .text_size(px(18.0))
                                                    .font_weight(FontWeight::BOLD)
                                                    .text_color(t.topbar_text),
                                            )
                                            .child(
                                                text("OCR 视频剪辑工作台")
                                                    .text_size(px(12.0))
                                                    .text_color(t.topbar_subtext),
                                            ),
                                    ),
                            )
                            .child(
                                nsh_core::element::view()
                                    .flex_row()
                                    .items_center()
                                    .gap_3()
                                    .child(
                                        nsh_core::element::view()
                                            .px_3()
                                            .py_1()
                                            .rounded_full()
                                            .bg(t.status_pill_bg)
                                            .border_1()
                                            .border_color(t.status_pill_border)
                                            .flex_row()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                text("●")
                                                    .text_size(px(10.0))
                                                    .text_color(t.status_dot),
                                            )
                                            .child(
                                                text(move || status.get())
                                                    .text_size(px(12.0))
                                                    .text_color(t.status_pill_text),
                                            ),
                                    )
                                    .child(secondary(mode.toggle_label()).on_press(toggle_theme)),
                            ),
                    )
                    .child(
                        nsh_core::element::view()
                            .flex_1()
                            .flex_row()
                            .overflow_hidden()
                            .child(
                                nsh_core::element::view()
                                    .w(px(390.0))
                                    .bg(t.surface_main)
                                    .border_r_1()
                                    .border_color(t.border_main)
                                    .flex_col()
                                    .overflow_hidden()
                                    .child(
                                        nsh_core::element::scroll().flex_1().child(
                                            nsh_core::element::view()
                                                .p_5()
                                                .flex_col()
                                                .gap_5()
                                                .child(
                                                    nsh_core::element::view()
                                                        .flex_col()
                                                        .gap_2()
                                                        .child(
                                                            text("输入与输出")
                                                                .text_size(px(15.0))
                                                                .font_weight(FontWeight::SEMIBOLD)
                                                                .text_color(t.text_primary),
                                                        )
                                                        .child(
                                                            text("输入视频")
                                                                .text_size(px(12.0))
                                                                .text_color(t.text_muted),
                                                        )
                                                        .child(
                                                            nsh_core::element::view()
                                                                .bg(t.surface_subtle)
                                                                .border_1()
                                                                .border_color(t.border_input)
                                                                .rounded_md()
                                                                .px_3()
                                                                .py_2()
                                                                .child(
                                                                    text(move || inputs.get())
                                                                        .text_size(px(13.0))
                                                                        .text_color(t.text_primary),
                                                                ),
                                                        )
                                                        .child(
                                                            text("输出目录")
                                                                .text_size(px(12.0))
                                                                .text_color(t.text_muted),
                                                        )
                                                        .child(
                                                            nsh_core::element::view()
                                                                .bg(t.surface_subtle)
                                                                .border_1()
                                                                .border_color(t.border_input)
                                                                .rounded_md()
                                                                .px_3()
                                                                .py_2()
                                                                .child(
                                                                    text(move || output.get())
                                                                        .text_size(px(13.0))
                                                                        .text_color(t.text_primary),
                                                                ),
                                                        )
                                                        .child(
                                                            nsh_core::element::view()
                                                                .pt_1()
                                                                .flex_row()
                                                                .gap_2()
                                                                .child(
                                                                    secondary("选择输入 MP4")
                                                                        .flex_1()
                                                                        .on_press(pick_files),
                                                                )
                                                                .child(
                                                                    secondary("选择输出目录")
                                                                        .flex_1()
                                                                        .on_press(pick_folder),
                                                                ),
                                                        ),
                                                )
                                                .child(
                                                    nsh_core::element::view()
                                                        .h(px(1.0))
                                                        .bg(t.border_main),
                                                )
                                                .child(
                                                    nsh_core::element::view()
                                                        .flex_col()
                                                        .gap_2()
                                                        .child(
                                                            text("检测与剪辑规则")
                                                                .text_size(px(15.0))
                                                                .font_weight(FontWeight::SEMIBOLD)
                                                                .text_color(t.text_primary),
                                                        )
                                                        .child(
                                                            nsh_core::element::view()
                                                                .flex_row()
                                                                .gap_2()
                                                                .child(value_card("语言", language))
                                                                .child(value_card(
                                                                    "扫描模式",
                                                                    scan_mode,
                                                                )),
                                                        )
                                                        .child(
                                                            nsh_core::element::view()
                                                                .flex_row()
                                                                .gap_2()
                                                                .child(value_card(
                                                                    "前后剪辑秒数",
                                                                    trim_seconds,
                                                                ))
                                                                .child(value_card(
                                                                    "Trim 并行度",
                                                                    trim_jobs,
                                                                )),
                                                        )
                                                        .child(
                                                            nsh_core::element::view()
                                                                .flex_row()
                                                                .gap_2()
                                                                .child(value_card(
                                                                    "仅扫描",
                                                                    scan_only,
                                                                ))
                                                                .child(value_card(
                                                                    "递归",
                                                                    recursive,
                                                                )),
                                                        )
                                                        .child(
                                                            nsh_core::element::view()
                                                                .flex_row()
                                                                .gap_2()
                                                                .child(value_card(
                                                                    "合并输出",
                                                                    merge,
                                                                ))
                                                                .child(value_card(
                                                                    "覆盖",
                                                                    overwrite,
                                                                )),
                                                        )
                                                        .child(
                                                            nsh_core::element::view()
                                                                .pt_2()
                                                                .flex_row()
                                                                .gap_2()
                                                                .child(
                                                                    secondary("切换常用选项")
                                                                        .flex_1()
                                                                        .on_press(edit_options),
                                                                )
                                                                .child(
                                                                    secondary("保存设置")
                                                                        .on_press(save),
                                                                )
                                                                .child(
                                                                    secondary("检查参数")
                                                                        .on_press(validate_current),
                                                                ),
                                                        ),
                                                ),
                                        ),
                                    ),
                            )
                            .child(
                                nsh_core::element::view()
                                    .flex_1()
                                    .flex_col()
                                    .bg(t.canvas_bg)
                                    .overflow_hidden()
                                    .child(
                                        nsh_core::element::view()
                                            .bg(t.surface_main)
                                            .border_b_1()
                                            .border_color(t.border_main)
                                            .px_6()
                                            .py_4()
                                            .flex_row()
                                            .items_center()
                                            .justify_between()
                                            .child(
                                                nsh_core::element::view()
                                                    .flex_col()
                                                    .gap_1()
                                                    .child(
                                                        text("任务控制")
                                                            .text_size(px(16.0))
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .text_color(t.text_primary),
                                                    )
                                                    .child(
                                                        text(move || summary.get())
                                                            .text_size(px(12.0))
                                                            .text_color(t.text_muted),
                                                    ),
                                            )
                                            .child(
                                                nsh_core::element::view()
                                                    .flex_row()
                                                    .gap_2()
                                                    .items_center()
                                                    .child(
                                                        button()
                                                            .px_5()
                                                            .py_2()
                                                            .rounded_md()
                                                            .bg(t.primary_bg)
                                                            .text_size(px(13.0))
                                                            .text_color(t.primary_text)
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .items_center()
                                                            .justify_center()
                                                            .hover(move |s| s.bg(t.primary_hover))
                                                            .cursor_pointer()
                                                            .child(text("开始处理"))
                                                            .on_press(start),
                                                    )
                                                    .child(secondary("取消任务").on_press(cancel))
                                                    .child(secondary("刷新状态").on_press(refresh)),
                                            ),
                                    )
                                    .child(
                                        nsh_core::element::view()
                                            .flex_1()
                                            .flex_col()
                                            .p_6()
                                            .gap_4()
                                            .overflow_hidden()
                                            .child(
                                                nsh_core::element::view()
                                                    .flex_row()
                                                    .gap_3()
                                                    .child(metric(
                                                        "当前阶段",
                                                        phase,
                                                        t.accent_text,
                                                    ))
                                                    .child(metric(
                                                        "运行 ID",
                                                        run_id,
                                                        t.text_primary,
                                                    ))
                                                    .child(metric(
                                                        "关闭状态",
                                                        close_state,
                                                        t.text_primary,
                                                    )),
                                            )
                                            .child(
                                                nsh_core::element::view()
                                                    .flex_1()
                                                    .flex_col()
                                                    .bg(t.log_bg)
                                                    .rounded_lg()
                                                    .border_1()
                                                    .border_color(t.log_border)
                                                    .shadow_sm()
                                                    .overflow_hidden()
                                                    .child(
                                                        nsh_core::element::view()
                                                            .bg(t.log_header_bg)
                                                            .border_b_1()
                                                            .border_color(t.log_border)
                                                            .px_4()
                                                            .py_2()
                                                            .flex_row()
                                                            .justify_between()
                                                            .items_center()
                                                            .child(
                                                                nsh_core::element::view()
                                                                    .flex_row()
                                                                    .items_center()
                                                                    .gap_2()
                                                                    .child(
                                                                        text("●")
                                                                            .text_size(px(10.0))
                                                                            .text_color(t.log_dot),
                                                                    )
                                                                    .child(
                                                                        text("运行日志")
                                                                            .text_size(px(12.0))
                                                                            .font_weight(
                                                                                FontWeight::SEMIBOLD,
                                                                            )
                                                                            .text_color(
                                                                                t.text_primary,
                                                                            ),
                                                                    ),
                                                            )
                                                            .child(
                                                                text("等宽输出 · 可选中复制")
                                                                    .text_size(px(11.0))
                                                                    .text_color(t.log_muted),
                                                            ),
                                                    )
                                                    .child(
                                                        nsh_core::element::scroll()
                                                            .flex_1()
                                                            .p_4()
                                                            .child(
                                                                text(move || logs_text.get())
                                                                    .font_family("ui-monospace")
                                                                    .text_size(px(12.5))
                                                                    .text_color(t.log_text)
                                                                    .selectable(true),
                                                            ),
                                                    ),
                                            ),
                                    ),
                            ),
                    )
                    .child(
                        nsh_core::element::view()
                            .bg(t.footer_bg)
                            .border_t_1()
                            .border_color(t.border_main)
                            .px_6()
                            .py_2()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .child(
                                text("PUBG Highlight Trim · Native UI (NSH)")
                                    .text_size(px(11.0))
                                    .text_color(t.footer_text),
                            )
                            .child(
                                nsh_core::element::view()
                                    .flex_row()
                                    .gap_2()
                                    .child(secondary("关闭窗口").on_press(request_close))
                                    .child(
                                        button()
                                            .px_3()
                                            .py_2()
                                            .rounded_md()
                                            .bg(t.danger_bg)
                                            .border_1()
                                            .border_color(t.danger_border)
                                            .text_size(px(13.0))
                                            .text_color(t.danger_text)
                                            .items_center()
                                            .justify_center()
                                            .hover(move |s| s.bg(t.danger_bg_hover))
                                            .cursor_pointer()
                                            .child(text("确认停止并关闭"))
                                            .on_press(confirm_close),
                                    ),
                            ),
                    )
            }),
        )
    }

    pub fn run() -> std::process::ExitCode {
        let session = match open_session() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("无法启动 native UI: {e}");
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
fn main() {}
