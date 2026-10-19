//! Public application configuration; CLI syntax remains compatible with the Python application.
use clap::{Parser, ValueEnum};
use serde::{Serialize, Serializer};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Serialize, ValueEnum, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Target {
    SelfDeath,
    OwnKill,
    Both,
}
impl Target {
    pub fn code(self) -> &'static str {
        match self {
            Self::SelfDeath => "self-death",
            Self::OwnKill => "own-kill",
            Self::Both => "both",
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize, ValueEnum, PartialEq, Eq)]
pub enum GameLanguage {
    #[value(name = "auto")]
    #[serde(rename = "auto")]
    Auto,
    #[value(name = "zh-Hans")]
    #[serde(rename = "zh-Hans")]
    SimplifiedChinese,
    #[value(name = "zh-Hant")]
    #[serde(rename = "zh-Hant")]
    TraditionalChinese,
    #[value(name = "en")]
    #[serde(rename = "en")]
    English,
}
impl GameLanguage {
    pub fn code(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::SimplifiedChinese => "zh-Hans",
            Self::TraditionalChinese => "zh-Hant",
            Self::English => "en",
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize, ValueEnum, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ScanMode {
    Auto,
    Fast,
    Full,
}
#[derive(Clone, Copy, Debug, Serialize, ValueEnum, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SamplingMode {
    Fixed,
    Adaptive,
}
#[derive(Clone, Copy, Debug, Serialize, ValueEnum, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum GateMode {
    Full,
    Light,
    None,
}
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum OcrProviderMode {
    Auto,
    Cpu,
    Directml,
    DirectmlOnly,
}
impl OcrProviderMode {
    pub fn code(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Cpu => "cpu",
            Self::Directml => "directml",
            Self::DirectmlOnly => "directml-only",
        }
    }
    pub fn provider(self) -> crate::Provider {
        match self {
            Self::Auto | Self::Cpu => crate::Provider::Cpu,
            Self::Directml | Self::DirectmlOnly => crate::Provider::DirectMl,
        }
    }
    pub fn allows_fallback(self) -> bool {
        self == Self::Directml
    }
}
fn ocr_provider(v: &str) -> Result<OcrProviderMode, String> {
    match v.trim().to_ascii_lowercase().as_str() {
        "auto" => Ok(OcrProviderMode::Auto),
        "cpu" | "default" => Ok(OcrProviderMode::Cpu),
        "directml" | "dml" => Ok(OcrProviderMode::Directml),
        "directml-only" | "dml-only" => Ok(OcrProviderMode::DirectmlOnly),
        _ => Err("expected one of: auto, cpu, directml, directml-only (alias: dml)".into()),
    }
}
fn ocr_device(v: &str) -> Result<i32, String> {
    v.parse::<i32>()
        .ok()
        .filter(|n| *n >= 0)
        .ok_or_else(|| "expected device id in 0..=2147483647".into())
}
fn number(v: &str) -> Result<f64, String> {
    // argparse's float accepts surrounding whitespace, digit separators and
    // signed/case-insensitive NaN/Infinity. Finite checks belong to OcrConfig,
    // not the argument parser: moving them here changes exit 1 into exit 2.
    static FLOAT: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        let digits = r"[0-9](?:_?[0-9])*";
        regex::Regex::new(&format!(r"(?i)^[+-]?(?:inf(?:inity)?|nan|(?:{digits}(?:\.(?:{digits})?)?|\.{digits})(?:e[+-]?{digits})?)$")).unwrap()
    });
    let value = v.trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c));
    if !FLOAT.is_match(value) {
        return Err("expected a number".into());
    }
    value
        .replace('_', "")
        .parse()
        .map_err(|_| "expected a number".into())
}
fn nonnegative(v: &str) -> Result<f64, String> {
    let n = number(v)?;
    if n < 0.0 {
        Err("value must be a non-negative number".into())
    } else {
        Ok(n)
    }
}
fn jobs(v: &str) -> Result<String, String> {
    if v.eq_ignore_ascii_case("auto") {
        return Ok("auto".into());
    };
    match v.parse::<usize>() {
        Ok(n) if n > 0 => Ok(n.to_string()),
        _ => Err("jobs must be auto or a positive integer".into()),
    }
}
fn roi(v: &str) -> Result<[f64; 4], String> {
    let p = v
        .split(',')
        .map(|s| number(s.trim()))
        .collect::<Result<Vec<_>, _>>()?;
    if p.len() == 4
        && 0.0 <= p[0]
        && p[0] < p[2]
        && p[2] <= 1.0
        && 0.0 <= p[1]
        && p[1] < p[3]
        && p[3] <= 1.0
    {
        Ok([p[0], p[1], p[2], p[3]])
    } else {
        Err("ROI values must be x1,y1,x2,y2 ratios in ascending order, between 0 and 1".into())
    }
}
fn window(v: &str) -> Result<[f64; 2], String> {
    let (a, b) = v
        .split_once(':')
        .ok_or("Window must be start:end seconds")?;
    let (a, b) = (number(a.trim())?, number(b.trim())?);
    if a < 0.0 || b <= a {
        Err("Window must satisfy 0 <= start < end".into())
    } else {
        Ok([a, b])
    }
}
fn serialize_merge<S: Serializer>(v: &Option<Option<PathBuf>>, s: S) -> Result<S::Ok, S::Error> {
    match v {
        None => s.serialize_none(),
        Some(None) => s.serialize_bool(true),
        Some(Some(p)) => p.serialize(s),
    }
}
#[derive(Clone, Debug, Parser, Serialize)]
#[command(
    name = "pubg-highlight-trim",
    bin_name = "pubg-highlight-trim",
    version = "0.3.6",
    args_override_self = true,
    about = "Trim PUBG NVIDIA Highlight clips around OCR-detected knock/elimination events."
)]
pub struct Config {
    pub input: Option<PathBuf>,
    #[arg(long,num_args=1..,action=clap::ArgAction::Set)]
    pub files: Vec<PathBuf>,
    #[arg(short = 'o', long)]
    pub output_dir: Option<PathBuf>,
    #[arg(long, value_enum, default_value = "both")]
    pub target: Target,
    #[arg(long, value_enum, default_value = "auto")]
    pub game_lang: GameLanguage,
    #[arg(long,alias="seconds-before",default_value="5",value_parser=number,allow_hyphen_values=true)]
    pub before: f64,
    #[arg(long,alias="seconds-after",default_value="1",value_parser=number,allow_hyphen_values=true)]
    pub after: f64,
    #[arg(long,default_value="2",value_parser=number,allow_hyphen_values=true)]
    pub min_event_sec: f64,
    #[arg(long,default_value="10",value_parser=number,allow_hyphen_values=true)]
    pub molotov_elim_before: f64,
    #[arg(long)]
    pub recursive: bool,
    #[arg(long, alias = "scan-only")]
    pub dry_run: bool,
    #[arg(long,default_value="auto",value_parser=jobs)]
    pub jobs: String,
    #[arg(long,num_args=0..=1,conflicts_with="no_merge")]
    #[serde(serialize_with = "serialize_merge")]
    pub merge: Option<Option<PathBuf>>,
    #[arg(long)]
    pub no_merge: bool,
    #[arg(long)]
    pub split_merge_by_target: bool,
    #[arg(short = 'y', long)]
    pub overwrite: bool,
    #[arg(long)]
    pub move_originals: bool,
    #[arg(long)]
    pub verbose: bool,
    #[arg(long)]
    pub profile: bool,
    #[arg(long, conflicts_with = "no_fast_path")]
    pub fast_path: bool,
    #[arg(long)]
    pub no_fast_path: bool,
    #[arg(long)]
    pub cache_dir: Option<PathBuf>,
    #[arg(long)]
    pub ffmpeg: Option<PathBuf>,
    #[arg(long)]
    pub ffprobe: Option<PathBuf>,
    #[arg(long)]
    pub candidate_csv: Option<PathBuf>,
    #[arg(long)]
    pub no_auto_candidate_csv: bool,
    #[arg(long, value_enum, default_value = "auto")]
    pub scan_mode: ScanMode,
    #[arg(long,value_parser=window)]
    pub priority_window: Vec<[f64; 2]>,
    #[arg(long,default_value="0",value_parser=number,allow_hyphen_values=true)]
    pub scan_start: f64,
    #[arg(long,value_parser=number,allow_hyphen_values=true)]
    pub scan_end: Option<f64>,
    #[arg(long,default_value="4",value_parser=number,allow_hyphen_values=true)]
    pub coarse_step: f64,
    #[arg(long,default_value="8",value_parser=number,allow_hyphen_values=true)]
    pub candidate_lookback: f64,
    #[arg(long,default_value="0.5",value_parser=number,allow_hyphen_values=true)]
    pub candidate_lookahead: f64,
    #[arg(long,default_value="4",value_parser=number,allow_hyphen_values=true)]
    pub candidate_step: f64,
    #[arg(long,default_value="6",value_parser=number,allow_hyphen_values=true)]
    pub refine_before: f64,
    #[arg(long,default_value="0.4",value_parser=number,allow_hyphen_values=true)]
    pub refine_after: f64,
    #[arg(long,default_value="0.5",value_parser=number,allow_hyphen_values=true)]
    pub refine_step: f64,
    #[arg(long, value_enum, default_value = "fixed")]
    pub sampling_mode: SamplingMode,
    #[arg(long,default_value="0.5",value_parser=nonnegative)]
    pub adaptive_step: f64,
    #[arg(long,default_value="2",value_parser=nonnegative)]
    pub adaptive_window: f64,
    #[arg(long)]
    pub ocr_max_calls: Option<usize>,
    #[arg(long,default_value="0",value_parser=nonnegative)]
    pub ocr_min_interval: f64,
    #[arg(long)]
    pub refine_max_frames: Option<usize>,
    #[arg(long)]
    pub assist_max_frames: Option<usize>,
    #[arg(long,value_parser=nonnegative)]
    pub refine_max_seconds: Option<f64>,
    #[arg(long,value_parser=nonnegative)]
    pub assist_max_seconds: Option<f64>,
    #[arg(long,default_value="0.30,0.66,0.70,0.75",value_parser=roi)]
    pub roi: [f64; 4],
    #[arg(long, default_value = "768", allow_hyphen_values = true)]
    pub ocr_width: i32,
    /// OCR inference mode (auto currently uses CPU).
    #[arg(long, env = "PUBG_OCR_PROVIDER", default_value = "cpu", value_parser = ocr_provider)]
    pub ocr_provider: OcrProviderMode,
    /// Inference device index; CLI overrides the legacy environment variable.
    #[arg(long, env = "PUBG_OCR_DIRECTML_DEVICE_ID", default_value = "0", value_parser = ocr_device, allow_hyphen_values = true)]
    pub ocr_device_id: i32,
    #[arg(long)]
    pub no_brightness_gate: bool,
    #[arg(long, alias = "gate-mode", value_enum)]
    pub brightness_gate_mode: Option<GateMode>,
}
impl Config {
    /// argparse's append action retains its default windows when flags are supplied.
    pub fn priority_windows(&self) -> Vec<[f64; 2]> {
        let mut p = vec![[31.0, 43.0], [45.0, 53.0]];
        p.extend_from_slice(&self.priority_window);
        p
    }
    pub fn merge_enabled(&self, single_file: bool) -> bool {
        !self.no_merge && (self.merge.is_some() || !single_file)
    }
    pub fn split_targets(&self) -> bool {
        self.split_merge_by_target && self.target == Target::Both
    }
    pub fn effective_sampling(&self) -> SamplingMode {
        if self.fast_path {
            SamplingMode::Adaptive
        } else if self.no_fast_path {
            SamplingMode::Fixed
        } else {
            self.sampling_mode
        }
    }
}
