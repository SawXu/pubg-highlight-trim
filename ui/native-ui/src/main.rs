#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod windows_app {
    use highlight_core::{ui_session::UiSession, RunOptions};
    use native_ui::{
        open_session, CloseAction, ClosePrompt, NativeController, SessionView, StartPrompt,
        TargetMode,
    };
    use nsh_base::{button, checkbox};
    use nsh_core::{
        element::{desktop as window, input},
        prelude::*,
        Color,
    };
    use nsh_platform_windows::{run_app, AppOptions, AppSource, Root};
    use std::{cell::RefCell, path::PathBuf, rc::Rc, sync::OnceLock, time::Duration};

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

    static SESSION: OnceLock<UiSession> = OnceLock::new();
    fn session() -> &'static UiSession {
        SESSION.get().expect("native session is initialized")
    }

    /// Reactive presentation only. Running/cancellation always come from UiSession.
    #[derive(Clone)]
    struct Presentation {
        controller: Rc<RefCell<NativeController>>,
        options: Signal<RunOptions>,
        seconds: Signal<Vec<String>>,
        view: Signal<SessionView>,
        preview: Signal<String>,
        validation: Signal<String>,
        pending_confirmation: Signal<Vec<String>>,
        close_prompt: Signal<ClosePrompt>,
        message: Signal<String>,
    }

    fn seconds_text(o: &RunOptions) -> Vec<String> {
        [
            o.seconds_before,
            o.seconds_after,
            o.minimum_event_seconds,
            o.molotov_seconds_before,
        ]
        .iter()
        .map(ToString::to_string)
        .collect()
    }

    impl Presentation {
        fn sync_view(&self) {
            let next = self.controller.borrow().snapshot(session());
            // Do not invalidate selectable log text when a poll changes nothing.
            if self.view.get_untracked() != next {
                self.view.set(next);
            }
        }
        fn sync_options(&self) {
            let c = self.controller.borrow();
            self.options.set(c.options.clone());
            match c.command_preview() {
                Ok(preview) => {
                    self.preview.set(preview);
                    self.validation.set(String::new());
                }
                Err(error) => {
                    self.preview.set(format!("参数错误：{error}"));
                    self.validation.set(error);
                }
            }
            self.pending_confirmation.set(if c.confirmation {
                c.warnings()
            } else {
                Vec::new()
            });
        }
        fn edit(&self, update: impl FnOnce(&mut RunOptions)) {
            if self.controller.borrow().snapshot(session()).running {
                return;
            }
            {
                let mut c = self.controller.borrow_mut();
                update(&mut c.options);
                c.cancel_confirmation();
            }
            self.sync_options();
        }
        fn result(&self, result: Result<(), String>, success: &str) {
            self.message
                .set(result.map(|_| success.to_string()).unwrap_or_else(|e| e));
            self.sync_view();
        }
        fn update_close(
            &self,
            update: impl FnOnce(&mut ClosePrompt) -> CloseAction,
        ) -> CloseAction {
            let mut prompt = self.close_prompt.get_untracked();
            let action = update(&mut prompt);
            if self.close_prompt.get_untracked() != prompt {
                self.close_prompt.set(prompt);
            }
            if let CloseAction::ShowError(error) = &action {
                self.message.set(format!("关闭失败：{error}"));
            }
            action
        }
        fn start(&self, confirmed: bool) {
            if self.close_prompt.get_untracked() == ClosePrompt::Waiting {
                return;
            }
            let result = {
                let mut c = self.controller.borrow_mut();
                if c.snapshot(session()).running {
                    return;
                }
                if confirmed {
                    c.start(session(), true).map(|_| ())
                } else {
                    match c.prepare_start(session()) {
                        Ok(StartPrompt::Ready) => c.start(session(), false).map(|_| ()),
                        Ok(StartPrompt::ConfirmationRequired(_)) => Ok(()),
                        Err(e) => Err(e),
                    }
                }
            };
            self.sync_options();
            self.result(result, "");
        }
    }

    fn secondary(t: ThemePalette, label: &str) -> nsh_base::ButtonElement {
        button()
            .px_3()
            .py_2()
            .rounded_md()
            .bg(t.btn_sec_bg)
            .border_1()
            .border_color(t.border_input)
            .text_size(px(12.0))
            .text_color(t.text_secondary)
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(move |s| s.bg(t.btn_sec_hover).border_color(t.btn_sec_border_hover))
            .disabled_style(|s| s.opacity(0.45))
            .child(text(label.to_string()))
    }

    fn heading(t: ThemePalette, label: &str) -> impl IntoElement {
        text(label.to_string())
            .text_size(px(15.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(t.text_primary)
    }

    fn field(
        p: Presentation,
        t: ThemePalette,
        placeholder: &str,
        multiline: bool,
        read: impl Fn(&RunOptions) -> String + 'static,
        write: impl Fn(&mut RunOptions, &str) + 'static,
    ) -> impl IntoElement {
        let options = p.options.clone();
        let state = p.view.clone();
        input()
            .value(move || read(&options.get()))
            .placeholder(placeholder.to_string())
            .multiline(multiline)
            .h(px(if multiline { 84.0 } else { 36.0 }))
            .readonly(move || state.get().running)
            .bg(t.surface_subtle)
            .border_1()
            .border_color(t.border_input)
            .rounded_md()
            .px_3()
            .py_2()
            .text_size(px(13.0))
            .text_color(t.text_primary)
            .on_change(move |value: &str| p.edit(|o| write(o, value)))
    }

    fn seconds_field(
        p: Presentation,
        t: ThemePalette,
        index: usize,
        label: &str,
    ) -> impl IntoElement {
        let values = p.seconds.clone();
        let state = p.view.clone();
        view()
            .flex_1()
            .flex_col()
            .gap_2()
            .child(
                text(label.to_string())
                    .text_size(px(12.0))
                    .text_color(t.text_muted),
            )
            .child(
                input()
                    .value(move || values.get()[index].clone())
                    .readonly(move || state.get().running)
                    .h(px(36.0))
                    .px_3()
                    .py_2()
                    .bg(t.surface_subtle)
                    .border_1()
                    .border_color(t.border_input)
                    .rounded_md()
                    .text_size(px(13.0))
                    .text_color(t.text_primary)
                    .on_change(move |value: &str| {
                        if p.view.get_untracked().running {
                            return;
                        }
                        let mut values = p.seconds.get_untracked();
                        values[index] = value.into();
                        p.seconds.set(values);
                        // Keep invalid text editable, but never silently run an old valid value.
                        let number = value.trim().parse().unwrap_or(f64::NAN);
                        p.edit(|o| match index {
                            0 => o.seconds_before = number,
                            1 => o.seconds_after = number,
                            2 => o.minimum_event_seconds = number,
                            _ => o.molotov_seconds_before = number,
                        });
                    }),
            )
    }

    fn segments(
        p: Presentation,
        t: ThemePalette,
        label: &str,
        choices: &'static [highlight_core::options::Choice],
        read: fn(&RunOptions) -> &str,
        write: fn(&mut RunOptions, String),
    ) -> impl IntoElement {
        let mut row = view().flex_row().gap_1();
        for choice in choices {
            let selected = p.options.clone();
            let state = p.view.clone();
            let edit = p.clone();
            row = row.child(
                secondary(t, choice.label)
                    .flex_1()
                    .disabled(move || state.get().running)
                    .bg(move || {
                        if read(&selected.get()) == choice.value {
                            t.status_pill_bg
                        } else {
                            t.btn_sec_bg
                        }
                    })
                    .border_color({
                        let options = p.options.clone();
                        move || {
                            if read(&options.get()) == choice.value {
                                t.accent_text
                            } else {
                                t.border_input
                            }
                        }
                    })
                    .on_press(move || edit.edit(|o| write(o, choice.value.into()))),
            );
        }
        view()
            .flex_col()
            .gap_2()
            .child(
                text(label.to_string())
                    .text_size(px(12.0))
                    .text_color(t.text_muted),
            )
            .child(row)
    }

    fn check(
        p: Presentation,
        t: ThemePalette,
        label: &str,
        danger: bool,
        read: fn(&RunOptions) -> bool,
        write: fn(&mut RunOptions, bool),
    ) -> impl IntoElement {
        let options = p.options.clone();
        let mark = p.options.clone();
        let state = p.view.clone();
        checkbox()
            .checked(move || read(&options.get()))
            .disabled(move || state.get().running)
            .flex_row()
            .items_center()
            .gap_2()
            .py_1()
            .cursor_pointer()
            .indicator(
                view()
                    .w(px(18.0))
                    .h(px(18.0))
                    .rounded_md()
                    .border_1()
                    .border_color(t.accent_text)
                    .bg(t.status_pill_bg)
                    .items_center()
                    .justify_center()
                    .child(
                        text(move || if read(&mark.get()) { "✓" } else { "" })
                            .text_color(t.accent_text),
                    ),
            )
            .child(
                text(label.to_string())
                    .text_size(px(13.0))
                    .text_color(if danger {
                        t.danger_text
                    } else {
                        t.text_secondary
                    }),
            )
            .on_change(move |value: &bool| p.edit(|o| write(o, *value)))
    }

    fn sidebar(p: Presentation, t: ThemePalette, expert: Signal<bool>) -> impl IntoElement {
        let inputs = field(
            p.clone(),
            t,
            "每行一个 MP4 文件或文件夹",
            true,
            |o| o.input_paths.join("\n"),
            |o, s| {
                o.input_paths = s
                    .lines()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect()
            },
        );
        let output = field(
            p.clone(),
            t,
            "留空由 CLI 自动创建",
            false,
            |o| o.output_directory.clone().unwrap_or_default(),
            |o, s| {
                o.output_directory = if s.trim().is_empty() {
                    None
                } else {
                    Some(s.into())
                }
            },
        );
        let mut pickers = view().flex_row().gap_2();
        for (files, label) in [(false, "选择文件夹"), (true, "选择 MP4（可多选）")] {
            let p = p.clone();
            let state = p.view.clone();
            pickers = pickers.child(
                secondary(t, label)
                    .disabled(move || state.get().running)
                    .on_press(move || match native_ui::pick_inputs(files) {
                        Ok(paths) if !paths.is_empty() => p.edit(|o| o.input_paths = paths),
                        Ok(_) => {}
                        Err(e) => p.message.set(e),
                    }),
            );
        }
        let browse = {
            let p = p.clone();
            move || match native_ui::pick_output() {
                Ok(Some(path)) => p.edit(|o| o.output_directory = Some(path)),
                Ok(None) => {}
                Err(e) => p.message.set(e),
            }
        };
        let clear = {
            let p = p.clone();
            move || p.edit(|o| o.output_directory = None)
        };
        let locked = p.view.clone();
        let locked_clear = p.view.clone();
        let mut targets = view().flex_col().gap_2();
        for pair in [
            [
                (TargetMode::Combined, "全部合在一起", "两类事件 → 一个视频"),
                (
                    TargetMode::Separate,
                    "按目标分别合并",
                    "两类事件 → 各一个视频",
                ),
            ],
            [
                (TargetMode::SelfDeath, "仅自己倒地", ""),
                (TargetMode::OwnKill, "仅我的击杀", ""),
            ],
        ] {
            let mut row = view().flex_row().gap_2();
            for (mode, label, subtext) in pair {
                let bg = p.options.clone();
                let border = p.options.clone();
                let state = p.view.clone();
                let p = p.clone();
                row = row.child(
                    secondary(t, label)
                        .flex_1()
                        .flex_col()
                        .gap_1()
                        .bg(move || {
                            if TargetMode::from_options(&bg.get()) == mode {
                                t.status_pill_bg
                            } else {
                                t.surface_subtle
                            }
                        })
                        .border_color(move || {
                            if TargetMode::from_options(&border.get()) == mode {
                                t.accent_text
                            } else {
                                t.border_input
                            }
                        })
                        .disabled(move || state.get().running)
                        .child(text(subtext).text_size(px(10.0)).text_color(t.text_muted))
                        .on_press(move || p.edit(|o| mode.apply(o))),
                );
            }
            targets = targets.child(row);
        }
        let merge = p.options.clone();
        let archive = p.options.clone();
        let expert_toggle = expert.clone();
        let expert_label = expert.clone();
        let expert_p = p.clone();
        let expert_locked = p.view.clone();
        let save = {
            let p = p.clone();
            move || {
                let result = p.controller.borrow().save_settings(session());
                p.result(result, "设置已保存");
            }
        };
        let reset = {
            let p = p.clone();
            move || {
                if p.view.get_untracked().running {
                    return;
                }
                p.controller.borrow_mut().reset_defaults();
                p.seconds.set(seconds_text(&p.controller.borrow().options));
                p.sync_options();
                p.message.set("已恢复默认设置".into());
            }
        };
        let validate = {
            let p = p.clone();
            move || {
                let result = session().validate(&p.controller.borrow().options);
                p.result(result, "参数有效");
            }
        };
        let validation = p.validation.clone();
        let save_locked = p.view.clone();
        let reset_locked = p.view.clone();
        let validate_locked = p.view.clone();
        view().w(px(390.0)).bg(t.surface_main).border_r_1().border_color(t.border_main)
            .flex_col().overflow_hidden().child(scroll().flex_1().child(
                view().p_5().flex_col().gap_4()
                    .child(heading(t, "输入与输出"))
                    .child(inputs).child(pickers)
                    .child(text("输出目录").text_size(px(12.0)).text_color(t.text_muted))
                    .child(output)
                    .child(view().flex_row().gap_2()
                        .child(secondary(t, "浏览").disabled(move || locked.get().running).on_press(browse))
                        .child(secondary(t, "清除").disabled(move || locked_clear.get().running).on_press(clear)))
                    .child(heading(t, "检测目标与输出方式"))
                    .child(targets)
                    .child(text(move || if merge.get().merge {
                        "仅包含检测到的片段；某类没有匹配事件时，不生成该类视频。"
                    } else { "已关闭合并输出：只生成独立片段，不生成合并视频。" })
                        .text_size(px(11.0)).text_color(t.text_muted))
                    .child(heading(t, "检测与剪辑"))
                    .child(segments(p.clone(), t, "游戏语言", highlight_core::options::GAME_LANGUAGES,
                        |o| &o.game_language, |o, s| o.game_language = s))
                    .child(segments(p.clone(), t, "扫描模式", highlight_core::options::SCAN_MODES,
                        |o| &o.scan_mode, |o, s| o.scan_mode = s))
                    .child(view().flex_row().gap_2()
                        .child(seconds_field(p.clone(), t, 0, "事件前（秒）"))
                        .child(seconds_field(p.clone(), t, 1, "事件后（秒）")))
                    .child(view().flex_col().gap_1()
                        .child(check(p.clone(), t, "合并输出", false, |o| o.merge, |o, b| o.merge = b))
                        .child(check(p.clone(), t, "递归扫描子文件夹", false, |o| o.recursive, |o, b| o.recursive = b))
                        .child(check(p.clone(), t, "保留 HDR · 默认转为 SDR BT.709", false, |o| o.preserve_hdr, |o, b| o.preserve_hdr = b))
                        .child(check(p.clone(), t, "仅扫描", false, |o| o.scan_only, |o, b| o.scan_only = b))
                        .child(check(p.clone(), t, "覆盖输出 · 会替换已有输出", true, |o| o.overwrite, |o, b| o.overwrite = b))
                        .child(check(p.clone(), t, "归档原片 · 会移走源文件", true, |o| o.move_originals, |o, b| o.move_originals = b)))
                    .child(dynamic(move || if archive.get().move_originals {
                        view().p_3().rounded_md().bg(t.danger_bg).border_1().border_color(t.danger_border)
                            .child(text("产出过片段的源 MP4 将从原目录移到输出目录 originals 子目录，并写入 moved_sources.csv。请确认后再开始。")
                                .text_size(px(12.0)).text_color(t.danger_text)).into_element()
                    } else { view().into_element() }))
                    .child(secondary(t, "").disabled(move || expert_locked.get().running)
                        .child(text(move || if expert_label.get() { "▾ 专家设置" } else { "▸ 专家设置" }))
                        .on_press(move || expert_toggle.set(!expert_toggle.get())))
                    .child(dynamic(move || if expert.get() {
                        view().flex_col().gap_3()
                            .child(seconds_field(expert_p.clone(), t, 2, "忽略开场事件（秒）"))
                            .child(seconds_field(expert_p.clone(), t, 3, "燃烧瓶前置（秒）"))
                            .child(text("OCR 并行度").text_size(px(12.0)).text_color(t.text_muted))
                            .child(field(expert_p.clone(), t, "auto 或正整数", false,
                                |o| o.jobs.clone(), |o, s| o.jobs = s.into()))
                            .child(segments(expert_p.clone(), t, "Trim 并行度", highlight_core::options::TRIM_JOBS,
                                |o| &o.trim_jobs, |o, s| o.trim_jobs = s))
                            .child(check(expert_p.clone(), t, "启用亮色描边文字预筛选", false,
                                |o| o.brightness_gate, |o, b| o.brightness_gate = b)).into_element()
                    } else { view().into_element() }))
                    .child(view().flex_row().gap_1()
                        .child(secondary(t, "保存设置").disabled(move || save_locked.get().running).on_press(save))
                        .child(secondary(t, "恢复默认").disabled(move || reset_locked.get().running).on_press(reset))
                        .child(secondary(t, "检查参数").disabled(move || validate_locked.get().running).on_press(validate)))
                    .child(text(move || validation.get()).text_size(px(12.0)).text_color(t.danger_text))
            ))
    }

    fn metric(
        t: ThemePalette,
        label: &str,
        value: impl Fn() -> String + 'static,
    ) -> impl IntoElement {
        view()
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
            .child(
                text(label.to_string())
                    .text_size(px(12.0))
                    .text_color(t.text_muted),
            )
            .child(
                text(value)
                    .text_size(px(20.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(t.accent_text),
            )
    }

    fn workspace(p: Presentation, t: ThemePalette) -> impl IntoElement {
        let summary = p.view.clone();
        let message = p.message.clone();
        let start_p = p.clone();
        let start_locked = p.view.clone();
        let start_close = p.close_prompt.clone();
        let cancel_close = p.close_prompt.clone();
        let invalid = p.validation.clone();
        let pending = p.pending_confirmation.clone();
        let cancel_p = p.clone();
        let cancel_state = p.view.clone();
        let open_p = p.clone();
        let open_state = p.view.clone();
        let open_options = p.options.clone();
        let confirmation = p.pending_confirmation.clone();
        let confirm_p = p.clone();
        let reject_p = p.clone();
        let phase = p.view.clone();
        let included = p.view.clone();
        let skipped = p.view.clone();
        let run_id = p.view.clone();
        let logs = p.view.clone();
        let clear_p = p.clone();
        let preview = p.preview.clone();
        view()
            .flex_1()
            .flex_col()
            .overflow_hidden()
            .bg(t.canvas_bg)
            .child(
                view()
                    .bg(t.surface_main)
                    .border_b_1()
                    .border_color(t.border_main)
                    .px_6()
                    .py_4()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        view()
                            .flex_1()
                            .min_w(px(0.0))
                            .mr_4()
                            .flex_col()
                            .gap_1()
                            .overflow_hidden()
                            .child(heading(t, "任务控制"))
                            .child(
                                text(move || {
                                    let s = summary.get();
                                    let note = message.get();
                                    format!(
                                        "{}{}{}",
                                        s.message,
                                        if note.is_empty() { "" } else { " · " },
                                        note
                                    )
                                })
                                .text_size(px(12.0))
                                .text_color(t.text_muted),
                            ),
                    )
                    .child(
                        view()
                            .flex_row()
                            .gap_2()
                            .child(
                                button()
                                    .px_5()
                                    .py_2()
                                    .rounded_md()
                                    .bg(t.primary_bg)
                                    .text_color(t.primary_text)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .hover(move |s| s.bg(t.primary_hover))
                                    .disabled_style(|s| s.opacity(0.45))
                                    .disabled(move || {
                                        start_locked.get().running
                                            || start_close.get() == ClosePrompt::Waiting
                                            || !invalid.get().is_empty()
                                            || !pending.get().is_empty()
                                    })
                                    .child(text("开始处理"))
                                    .on_press(move || start_p.start(false)),
                            )
                            .child(
                                secondary(t, "停止任务")
                                    .disabled(move || {
                                        let s = cancel_state.get();
                                        !s.running || s.cancelling
                                            || cancel_close.get() == ClosePrompt::Waiting
                                    })
                                    .on_press(move || {
                                        let result = cancel_p.controller.borrow().cancel(session());
                                        cancel_p.result(result, "已请求停止，等待任务清理");
                                    }),
                            )
                            .child(
                                secondary(t, "打开输出")
                                    .disabled(move || {
                                        let s = open_state.get();
                                        s.running
                                            || s.output_dir
                                                .or(open_options.get().output_directory)
                                                .is_none_or(|s| s.trim().is_empty())
                                    })
                                    .on_press(move || {
                                        let path = open_p
                                            .view
                                            .get_untracked()
                                            .output_dir
                                            .or(open_p.options.get_untracked().output_directory);
                                        if let Some(path) = path {
                                            open_p.result(
                                                native_ui::open_output_dir(&path),
                                                "已打开输出目录",
                                            );
                                        }
                                    }),
                            ),
                    ),
            )
            .child(dynamic(move || {
                let warnings = confirmation.get();
                if warnings.is_empty() {
                    return view().into_element();
                }
                let accept = confirm_p.clone();
                let reject = reject_p.clone();
                view()
                    .m_4()
                    .p_4()
                    .bg(t.danger_bg)
                    .border_1()
                    .border_color(t.danger_border)
                    .rounded_lg()
                    .flex_col()
                    .gap_2()
                    .child(
                        text(format!("危险操作确认\n{}", warnings.join("\n")))
                            .text_color(t.danger_text)
                            .text_size(px(13.0)),
                    )
                    .child(
                        view()
                            .flex_row()
                            .gap_2()
                            .child(
                                secondary(t, "确认开始处理")
                                    .disabled(move || confirm_p.close_prompt.get() == ClosePrompt::Waiting)
                                    .on_press(move || accept.start(true)),
                            )
                            .child(secondary(t, "取消").on_press(move || {
                                reject.controller.borrow_mut().cancel_confirmation();
                                reject.sync_options();
                            })),
                    )
                    .into_element()
            }))
            .child(
                view()
                    .flex_1()
                    .flex_col()
                    .p_6()
                    .gap_4()
                    .overflow_hidden()
                    .child(
                        view()
                            .flex_row()
                            .gap_3()
                            .child(metric(t, "当前阶段", move || {
                                phase.get().phase.unwrap_or_else(|| "待命".into())
                            }))
                            .child(metric(t, "已保留", move || {
                                included.get().included.to_string()
                            }))
                            .child(metric(t, "已跳过", move || {
                                skipped.get().skipped.to_string()
                            }))
                            .child(metric(t, "运行 ID", move || {
                                let id = run_id.get().run_id;
                                if id == 0 {
                                    "—".into()
                                } else {
                                    id.to_string()
                                }
                            })),
                    )
                    .child(
                        view()
                            .flex_1()
                            .flex_col()
                            .bg(t.log_bg)
                            .rounded_lg()
                            .border_1()
                            .border_color(t.log_border)
                            .shadow_sm()
                            .overflow_hidden()
                            .child(
                                view()
                                    .bg(t.log_header_bg)
                                    .border_b_1()
                                    .border_color(t.log_border)
                                    .px_4()
                                    .py_2()
                                    .flex_row()
                                    .justify_between()
                                    .items_center()
                                    .child(
                                        view()
                                            .flex_row()
                                            .gap_2()
                                            .items_center()
                                            .child(text("●").text_size(px(10.0)).text_color(t.log_dot))
                                            .child(
                                                text("运行日志")
                                                    .text_size(px(12.0))
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .text_color(t.text_primary),
                                            ),
                                    )
                                    .child(secondary(t, "清空日志").on_press(move || {
                                        let result =
                                            clear_p.controller.borrow().clear_logs(session());
                                        clear_p.result(result, "日志已清空");
                                    })),
                            )
                            .child(
                                scroll().flex_1().p_4().child(
                                    text(move || {
                                        let lines = logs.get().logs;
                                        if lines.is_empty() {
                                            "> 等待任务启动…\n> 请在左侧选择输入视频或文件夹，然后点击右上角「开始处理」。"
                                                .into()
                                        } else {
                                            lines.join("\n")
                                        }
                                    })
                                    .font_family("ui-monospace")
                                    .text_size(px(12.5))
                                    .text_color(t.log_text)
                                    .selectable(true),
                                ),
                            ),
                    )
                    .child(
                        view()
                            .flex_col()
                            .h(px(140.0))
                            .bg(t.log_bg)
                            .border_1()
                            .border_color(t.log_border)
                            .rounded_lg()
                            .shadow_sm()
                            .overflow_hidden()
                            .child(
                                view()
                                    .px_4()
                                    .py_2()
                                    .bg(t.log_header_bg)
                                    .border_b_1()
                                    .border_color(t.log_border)
                                    .child(
                                        text("命令预览 · 实时 CLI 参数")
                                            .text_size(px(12.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(t.log_muted),
                                    ),
                            )
                            .child(
                                scroll().flex_1().p_4().child(
                                    text(move || preview.get())
                                        .font_family("ui-monospace")
                                        .text_size(px(12.0))
                                        .text_color(t.log_text)
                                        .selectable(true),
                                ),
                            ),
                    ),
            )
    }

    #[component]
    fn NativeWindow() -> impl IntoElement {
        let mut controller = NativeController::new(RunOptions::default());
        let message = controller
            .load_settings(session())
            .map(|_| "已加载设置".into())
            .unwrap_or_else(|e| format!("设置加载失败：{e}"));
        let p = Presentation {
            options: signal(controller.options.clone()),
            seconds: signal(seconds_text(&controller.options)),
            view: signal(controller.snapshot(session())),
            preview: signal(String::new()),
            validation: signal(String::new()),
            pending_confirmation: signal(Vec::new()),
            close_prompt: signal(ClosePrompt::Hidden),
            message: signal(message),
            controller: Rc::new(RefCell::new(controller)),
        };
        p.sync_options();
        let theme = signal(ThemeMode::Light);
        let expert = signal(false);
        let close = window::current_window();
        let mut attrs = nsh_core::desktop::WindowAttributes::default();
        attrs.close_on_press = Some(false);
        close.set(attrs);
        let close_p = p.clone();
        let close_window = close.clone();
        close.on_close_press(move |_| {
            if close_p.update_close(|prompt| prompt.request(session().request_close()))
                == CloseAction::Close
            {
                close_window.close();
            }
        });
        let poll_p = p.clone();
        let poll_window = close.clone();
        set_interval(Duration::from_millis(500), move || {
            poll_p.sync_view();
            if poll_p.update_close(|prompt| prompt.poll(&session().close_status()))
                == CloseAction::Close
            {
                poll_window.clone().close();
            }
        });
        view().flex_1().flex_col().child(dynamic(move || {
            let mode = theme.get();
            let t = mode.palette();
            let toggle = theme.clone();
            let status = p.view.clone();
            let status_message = p.message.clone();
            let close_prompt = p.close_prompt.clone();
            let confirm_window = close.clone();
            let confirm_p = p.clone();
            let continue_p = p.clone();
            view()
                .flex_1()
                .flex_col()
                .bg(t.canvas_bg)
                .child(
                    view()
                        .px_6()
                        .py_4()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .bg(t.topbar_bg)
                        .border_b_1()
                        .border_color(t.topbar_border)
                        .child(
                            view()
                                .flex_row()
                                .items_center()
                                .gap_3()
                                .child(
                                    view()
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
                                    view()
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
                            view()
                                .flex_row()
                                .items_center()
                                .gap_3()
                                .child(
                                    view()
                                        .px_3()
                                        .py_1()
                                        .rounded_full()
                                        .bg(t.status_pill_bg)
                                        .border_1()
                                        .border_color(t.status_pill_border)
                                        .flex_row()
                                        .gap_2()
                                        .child(text("●").text_color(t.status_dot))
                                        .child(
                                            text(move || {
                                                let view = status.get();
                                                let message = status_message.get();
                                                if view.running {
                                                    "运行中".to_string()
                                                } else if view.message.chars().count() > 12
                                                    || [message.as_str(), view.message.as_str()]
                                                        .iter()
                                                        .any(|s| {
                                                            s.contains("错误")
                                                                || s.contains("失败")
                                                                || s.contains("缺少")
                                                                || s.to_ascii_lowercase()
                                                                    .contains("error")
                                                        })
                                                {
                                                    "错误".to_string()
                                                } else {
                                                    view.message
                                                }
                                            })
                                            .text_size(px(12.0))
                                            .text_color(t.status_pill_text),
                                        ),
                                )
                                .child(
                                    secondary(t, mode.toggle_label())
                                        .on_press(move || toggle.set(toggle.get().toggle())),
                                ),
                        ),
                )
                .child(dynamic(move || {
                    let prompt = close_prompt.get();
                    if prompt == ClosePrompt::Hidden {
                        return view().into_element();
                    }
                    let waiting = prompt == ClosePrompt::Waiting;
                    let message = match prompt {
                        ClosePrompt::Waiting => "正在停止并清理任务，完成后退出…".into(),
                        ClosePrompt::Failed(error) => format!("关闭失败：{error}"),
                        _ => "任务仍在运行或启动中，停止并退出？".into(),
                    };
                    let accept = confirm_p.clone();
                    let reject = continue_p.clone();
                    let window = confirm_window.clone();
                    view()
                        .m_4()
                        .p_4()
                        .bg(t.danger_bg)
                        .border_1()
                        .border_color(t.danger_border)
                        .rounded_lg()
                        .flex_col()
                        .gap_2()
                        .child(text(message).text_color(t.danger_text).text_size(px(13.0)))
                        .child(
                            view()
                                .flex_row()
                                .gap_2()
                                .child(secondary(t, "继续运行").disabled(waiting).on_press(
                                    move || {
                                        reject.update_close(|prompt| {
                                            prompt.dismiss();
                                            CloseAction::None
                                        });
                                    },
                                ))
                                .child(
                                    secondary(t, "停止并退出")
                                        .bg(t.danger_bg)
                                        .border_color(t.danger_border)
                                        .text_color(t.danger_text)
                                        .hover(move |s| s.bg(t.danger_bg_hover))
                                        .disabled(waiting)
                                        .on_press(move || {
                                            let action = accept.update_close(|prompt| {
                                                prompt.confirm(session().request_close(), || {
                                                    accept
                                                        .controller
                                                        .borrow()
                                                        .confirm_close(session())
                                                })
                                            });
                                            if action == CloseAction::Close {
                                                window.close();
                                            }
                                        }),
                                ),
                        )
                        .into_element()
                }))
                .child(
                    view()
                        .flex_1()
                        .flex_row()
                        .overflow_hidden()
                        .child(sidebar(p.clone(), t, expert.clone()))
                        .child(workspace(p.clone(), t)),
                )
                .child(
                    view()
                        .bg(t.footer_bg)
                        .border_t_1()
                        .border_color(t.border_main)
                        .px_6()
                        .py_2()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .child(
                            text("v0.4.1 · PUBG Highlight Trim · Native UI (NSH)")
                                .text_size(px(11.0))
                                .text_color(t.footer_text),
                        ),
                )
        }))
    }

    fn icon_path() -> Option<PathBuf> {
        let exe = std::env::current_exe().ok();
        let mut candidates = Vec::new();
        if let Some(parent) = exe.as_deref().and_then(|p| p.parent()) {
            candidates.push(parent.join("icon.ico"));
        }
        candidates.push(PathBuf::from("ui/native-ui/icons/icon.ico"));
        candidates
            .push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("icons/icon.ico"));
        candidates
            .into_iter()
            .find_map(|path| path.canonicalize().ok())
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
                size: (1280, 900),
                icon: icon_path(),
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
