use serde::{Deserialize, Serialize};
use std::path::Path;
#[derive(Serialize)]
pub struct Choice {
    pub value: &'static str,
    pub label: &'static str,
}
pub const GAME_LANGUAGES: &[Choice] = &[
    Choice {
        value: "auto",
        label: "自动识别",
    },
    Choice {
        value: "zh-Hans",
        label: "简体中文",
    },
    Choice {
        value: "zh-Hant",
        label: "繁体中文",
    },
    Choice {
        value: "en",
        label: "English",
    },
];
pub const SCAN_MODES: &[Choice] = &[
    Choice {
        value: "auto",
        label: "自动",
    },
    Choice {
        value: "fast",
        label: "快速",
    },
    Choice {
        value: "full",
        label: "完整",
    },
];
pub const TRIM_JOBS: &[Choice] = &[
    Choice {
        value: "auto",
        label: "自动",
    },
    Choice {
        value: "1",
        label: "1",
    },
    Choice {
        value: "2",
        label: "2",
    },
];
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RunOptions {
    pub input_paths: Vec<String>,
    pub output_directory: Option<String>,
    pub self_death: bool,
    pub own_kill: bool,
    pub game_language: String,
    pub seconds_before: f64,
    pub seconds_after: f64,
    pub minimum_event_seconds: f64,
    pub molotov_seconds_before: f64,
    pub scan_mode: String,
    pub jobs: String,
    pub trim_jobs: String,
    pub recursive: bool,
    pub merge: bool,
    pub scan_only: bool,
    pub overwrite: bool,
    pub brightness_gate: bool,
    pub move_originals: bool,
}
impl Default for RunOptions {
    fn default() -> Self {
        Self {
            input_paths: vec![],
            output_directory: None,
            self_death: false,
            own_kill: false,
            game_language: "auto".into(),
            seconds_before: 5.,
            seconds_after: 1.,
            minimum_event_seconds: 2.,
            molotov_seconds_before: 10.,
            scan_mode: "auto".into(),
            jobs: "auto".into(),
            trim_jobs: "auto".into(),
            recursive: false,
            merge: true,
            scan_only: false,
            overwrite: false,
            brightness_gate: true,
            move_originals: false,
        }
    }
}
impl RunOptions {
    pub fn target(&self) -> (&'static str, bool) {
        (
            if self.self_death == self.own_kill {
                "both"
            } else if self.self_death {
                "self-death"
            } else {
                "own-kill"
            },
            self.self_death && self.own_kill,
        )
    }
    pub fn arguments(&self) -> Result<Vec<String>, String> {
        if self.input_paths.is_empty()
            || self.input_paths.iter().any(|p| {
                p.trim().is_empty()
                    || !Path::new(p).exists()
                    || (!Path::new(p).is_dir() && !p.to_lowercase().ends_with(".mp4"))
            })
        {
            return Err("请选择存在的 MP4 文件或文件夹".into());
        }
        if self.input_paths.len() > 1 && self.input_paths.iter().any(|p| !Path::new(p).is_file()) {
            return Err("多选输入只能包含 MP4 文件".into());
        }
        for n in [
            self.seconds_before,
            self.seconds_after,
            self.minimum_event_seconds,
            self.molotov_seconds_before,
        ] {
            if !n.is_finite() || n < 0. {
                return Err("秒数必须是有限非负数字".into());
            }
        }
        if !GAME_LANGUAGES.iter().any(|c| c.value == self.game_language)
            || !SCAN_MODES.iter().any(|c| c.value == self.scan_mode)
            || !TRIM_JOBS.iter().any(|c| c.value == self.trim_jobs)
        {
            return Err("无效的语言、扫描模式或 Trim 并行度".into());
        }
        let jobs = self.jobs.trim().to_ascii_lowercase();
        if jobs != "auto" && jobs.parse::<i32>().map_or(true, |n| n < 1) {
            return Err("OCR 并行度必须为 auto 或正整数".into());
        }
        if let Some(p) = &self.output_directory {
            if !p.trim().is_empty() && Path::new(p).exists() && !Path::new(p).is_dir() {
                return Err("输出路径不是文件夹".into());
            }
        }
        let mut a = Vec::new();
        if self.input_paths.len() > 1 {
            a.push("--files".into())
        }
        a.extend(
            self.input_paths
                .iter()
                .map(|p| {
                    Path::new(p)
                        .canonicalize()
                        .map(|p| p.to_string_lossy().into_owned())
                        .map_err(|e| e.to_string())
                })
                .collect::<Result<Vec<_>, _>>()?,
        );
        for (k, v) in [
            ("--target", self.target().0.into()),
            ("--game-lang", self.game_language.clone()),
            ("--before", number(self.seconds_before)),
            ("--after", number(self.seconds_after)),
            ("--min-event-sec", number(self.minimum_event_seconds)),
            ("--molotov-elim-before", number(self.molotov_seconds_before)),
            ("--scan-mode", self.scan_mode.clone()),
            ("--jobs", jobs),
            ("--trim-jobs", self.trim_jobs.clone()),
        ] {
            a.extend([k.into(), v]);
        }
        if let Some(p) = &self.output_directory {
            if !p.trim().is_empty() {
                a.extend(["--output-dir".into(), p.trim().into()]);
            }
        }
        a.push(if self.merge { "--merge" } else { "--no-merge" }.into());
        for (yes, flag) in [
            (self.scan_mode == "fast", "--fast-path"),
            (self.merge && self.target().1, "--split-merge-by-target"),
            (self.recursive, "--recursive"),
            (self.scan_only, "--scan-only"),
            (self.overwrite, "--overwrite"),
            (!self.brightness_gate, "--no-brightness-gate"),
            (self.move_originals, "--move-originals"),
        ] {
            if yes {
                a.push(flag.into())
            }
        }
        Ok(a)
    }
    pub fn warnings(&self) -> Vec<String> {
        let mut w = vec![];
        if self.overwrite {
            w.push("覆盖现有输出文件，此操作不可撤销。".into())
        }
        if self.move_originals {
            w.push("将归档移动原片，原始位置会改变。".into())
        }
        w
    }
}
fn number(n: f64) -> String {
    let s = format!("{n:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}
pub fn preview(exe: &str, args: &[String]) -> String {
    std::iter::once(exe)
        .chain(args.iter().map(String::as_str))
        .map(|s| {
            if s.is_empty() || s.chars().any(char::is_whitespace) || s.contains('"') {
                format!("\"{}\"", s.replace('"', "\\\""))
            } else {
                s.into()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
