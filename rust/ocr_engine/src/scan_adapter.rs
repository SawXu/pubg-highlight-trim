//! Production media/OCR adapter. Scheduling and event semantics live in scanner/events.
use crate::{
    config::{Config, GateMode, SamplingMode, ScanMode},
    scanner::{self, OcrConfig},
    source_files::Source,
};

pub fn source_config(
    c: &Config,
    source: &Source,
    candidate_csv: Option<&std::path::Path>,
) -> OcrConfig {
    let gate = c.brightness_gate_mode.unwrap_or(GateMode::Light);
    OcrConfig {
        target: c.target.code().into(),
        language: source.language.code().into(),
        priority_window: c.priority_windows(),
        scan_start: c.scan_start,
        scan_end: c.scan_end,
        coarse_step: c.coarse_step,
        candidate_lookback: c.candidate_lookback,
        candidate_lookahead: c.candidate_lookahead,
        candidate_step: c.candidate_step,
        refine_before: c.refine_before,
        refine_after: c.refine_after,
        refine_step: c.refine_step,
        no_full_scan: !source.force_full_scan
            && match c.scan_mode {
                ScanMode::Fast => true,
                ScanMode::Full => false,
                ScanMode::Auto => candidate_csv.is_some_and(std::path::Path::exists),
            },
        roi: c.roi,
        ocr_width: c.ocr_width.max(0) as u32,
        brightness_gate: !source.match_end && !c.no_brightness_gate && gate != GateMode::None,
        brightness_gate_mode: match gate {
            GateMode::Full => "full",
            GateMode::Light => "light",
            GateMode::None => "none",
        }
        .into(),
        sampling_mode: match c.effective_sampling() {
            SamplingMode::Fixed => "fixed",
            SamplingMode::Adaptive => "adaptive",
        }
        .into(),
        adaptive_step: c.adaptive_step,
        adaptive_window: c.adaptive_window,
        ocr_max_calls: c.ocr_max_calls,
        ocr_min_interval: c.ocr_min_interval,
        refine_max_frames: c.refine_max_frames,
        assist_max_frames: c.assist_max_frames,
        refine_max_seconds: c.refine_max_seconds,
        assist_max_seconds: c.assist_max_seconds,
        ..OcrConfig::default()
    }
}
/// Planning's data model is a strict superset of scan detections; serde keeps the
/// boundary explicit and ensures every telemetry field survives the conversion.
pub fn planning_detections(
    d: Vec<scanner::EventDetection>,
) -> Result<Vec<crate::planning::Detection>, String> {
    serde_json::from_value(serde_json::to_value(d).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

#[cfg(feature = "onnx")]
mod production {
    use super::*;
    use crate::{
        frame::BgrFrame,
        media::{FfmpegFrameSource, FfmpegTools, MediaError},
        scanner::{FrameRead, GateResult, Recognition, ScanBackend},
        ModelManifest, OcrEngine, OcrEngineConfig, OcrStatus, PpOcrRecognizer, Provider, Roi,
    };
    use std::{path::PathBuf, time::Instant};
    pub struct EngineSettings {
        pub model_dir: PathBuf,
        pub manifest: ModelManifest,
        pub provider: Provider,
        pub fallback: bool,
        pub threads: usize,
        pub identity: String,
    }
    impl EngineSettings {
        pub fn discover() -> Result<Self, String> {
            let mut roots = Vec::new();
            if let Ok(exe) = std::env::current_exe() {
                if let Some(p) = exe.parent() {
                    roots.push(p.to_path_buf());
                }
            }
            if let Ok(cwd) = std::env::current_dir() {
                roots.push(cwd)
            }
            roots.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."));
            let model_dir=std::env::var_os("PUBG_OCR_MODEL_DIR").map(PathBuf::from).or_else(||roots.iter().map(|r|r.join("ocr_models")).find(|p|p.join("det.onnx").is_file()&&p.join("rec.onnx").is_file())).ok_or("Rust OCR ocr_models directory not found; set PUBG_OCR_MODEL_DIR or use the offline release bundle.")?;
            let bundled = model_dir.join("pp_ocrv6_medium.json");
            let manifest_path = if bundled.is_file() {
                bundled
            } else {
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("models/pp_ocrv6_medium.json")
            };
            let manifest =
                ModelManifest::from_json_file(&manifest_path).map_err(|e| e.to_string())?;
            // Validation is required even for cache-only runs; names alone are not identity.
            crate::validate_manifest(&model_dir, &manifest).map_err(|e| e.to_string())?;
            let raw = std::env::var("PUBG_OCR_PROVIDER").unwrap_or_default();
            let p = raw.trim().to_lowercase();
            let (provider, fallback) = match p.as_str() {
                "" | "cpu" | "default" | "auto" => (Provider::Cpu, false),
                "dml" | "directml" => (Provider::DirectMl, true),
                "dml-only" | "directml-only" => (Provider::DirectMl, false),
                _ => {
                    return Err(format!(
                        "unknown OCR provider {raw:?}; expected cpu, directml or directml-only"
                    ))
                }
            };
            let threads = std::env::var("PUBG_OCR_THREADS")
                .map(|s| {
                    s.parse::<usize>()
                        .map_err(|_| "PUBG_OCR_THREADS must be an integer".to_string())
                })
                .unwrap_or(Ok(1))?
                .max(1);
            let identity = format!(
                "application-scanner-v2:reading-order-v2:{}:{}:{}:{}:{p}:{threads}",
                manifest.model_version,
                manifest.detector_sha256,
                manifest.recognizer_sha256,
                manifest.dictionary_sha256
            );
            Ok(Self {
                model_dir,
                manifest,
                provider,
                fallback,
                threads,
                identity,
            })
        }
        pub fn engine(&self) -> Result<OcrEngine<PpOcrRecognizer>, String> {
            let recognizer = PpOcrRecognizer::from_model_dir_with_provider(
                &self.model_dir,
                &self.manifest,
                self.threads,
                self.provider,
                self.fallback,
            )?;
            OcrEngine::new(
                OcrEngineConfig {
                    provider: self.provider,
                    model_dir: self.model_dir.clone(),
                    manifest: self.manifest.clone(),
                    allow_provider_fallback: self.fallback,
                },
                recognizer,
            )
            .map_err(|e| e.to_string())
        }
    }
    pub struct Backend<'a> {
        pub frames: FfmpegFrameSource,
        pub engine: &'a OcrEngine<PpOcrRecognizer>,
        pub started: Instant,
        failure_kind: scanner::ScanFailureKind,
    }
    impl ScanBackend for Backend<'_> {
        type Frame = BgrFrame;
        fn failure_kind(&self) -> scanner::ScanFailureKind {
            self.failure_kind
        }
        fn read_frame(&mut self, seconds: f64) -> Result<FrameRead<BgrFrame>, String> {
            crate::cancellation::check()?;
            let started = Instant::now();
            self.failure_kind = scanner::ScanFailureKind::Runtime;
            let frame = self.frames.read_at(seconds).map_err(|e| {
                self.failure_kind = match e {
                    MediaError::VideoUnreadable(_) => scanner::ScanFailureKind::SourceUnreadable,
                    _ => scanner::ScanFailureKind::Runtime,
                };
                e.to_string()
            })?;
            Ok(FrameRead {
                frame,
                seconds: started.elapsed().as_secs_f64(),
            })
        }
        fn recognize(
            &mut self,
            frame: &BgrFrame,
            roi: [f64; 4],
            width: u32,
        ) -> Result<Recognition, String> {
            crate::cancellation::check()?;
            let preprocess_started = Instant::now();
            let crop = crate::frame::crop_for_ocr(frame, roi, width)?;
            let pixels = crop.to_rgb();
            let frame_seconds = preprocess_started.elapsed().as_secs_f64();
            let started = Instant::now();
            let result = self.engine.recognize(Roi {
                pixels: &pixels,
                width: crop.width(),
                height: crop.height(),
            });
            crate::cancellation::check()?;
            if !matches!(result.status, OcrStatus::Ok | OcrStatus::ProviderFallback) {
                return Err(format!(
                    "Rust OCR failed: {:?}: {}",
                    result.status,
                    result.error.unwrap_or_default()
                ));
            }
            Ok(Recognition {
                text: result.text,
                scores: result
                    .scores
                    .iter()
                    .map(|s| format!("{s:.3}"))
                    .collect::<Vec<_>>()
                    .join(";"),
                seconds: started.elapsed().as_secs_f64(),
                frame_seconds,
            })
        }
        fn gate(&mut self, frame: &BgrFrame, config: &OcrConfig) -> Result<GateResult, String> {
            crate::cancellation::check()?;
            let r = crate::frame::brightness_gate(
                frame,
                config.brightness_gate_roi,
                config.brightness_gate_width,
                match config.brightness_gate_mode.as_str() {
                    "full" => crate::frame::GateMode::Full,
                    "none" => crate::frame::GateMode::None,
                    _ => crate::frame::GateMode::Light,
                },
            )?;
            Ok(GateResult {
                passed: r.passed,
                reason: r.reason.into(),
            })
        }
        fn now(&mut self) -> f64 {
            self.started.elapsed().as_secs_f64()
        }
    }
    pub fn backend<'a>(
        source: &Source,
        tools: &FfmpegTools,
        engine: &'a OcrEngine<PpOcrRecognizer>,
    ) -> Result<Backend<'a>, MediaError> {
        let hw = std::env::var("PUBG_HIGHLIGHT_TRIM_FFMPEG_HWACCEL").ok();
        Ok(Backend {
            frames: FfmpegFrameSource::open(&source.path, tools, hw.as_deref())?,
            engine,
            started: Instant::now(),
            failure_kind: scanner::ScanFailureKind::Runtime,
        })
    }
}
#[cfg(feature = "onnx")]
pub use production::*;
