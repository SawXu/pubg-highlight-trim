use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Cpu,
    DirectMl,
}

pub fn provider_name(provider: Provider) -> &'static str {
    match provider {
        Provider::Cpu => "cpu",
        Provider::DirectMl => "directml",
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProviderInfo {
    pub provider: Provider,
    pub requested: Provider,
    pub device_id: Option<i32>,
    pub fallback_reason: Option<String>,
}

impl Default for ProviderInfo {
    fn default() -> Self {
        Self {
            provider: Provider::Cpu,
            requested: Provider::Cpu,
            device_id: None,
            fallback_reason: None,
        }
    }
}

impl ProviderInfo {
    pub fn fell_back(&self) -> bool {
        self.provider != self.requested || self.fallback_reason.is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcrStatus {
    Ok,
    EmptyRoi,
    ModelUnavailable,
    InferenceError,
    ProviderFallback,
    InvalidRoi,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OcrResult {
    pub text: String,
    pub scores: Vec<f32>,
    pub boxes: Vec<[u32; 4]>,
    pub status: OcrStatus,
    pub method: String,
    pub inference_ms: f64,
    pub frame_ms: f64,
    pub provider: Provider,
    pub device_id: Option<i32>,
    pub fallback_reason: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct Roi<'a> {
    pub pixels: &'a [u8],
    pub width: u32,
    pub height: u32,
}

impl OcrResult {
    pub fn empty(status: OcrStatus, method: impl Into<String>) -> Self {
        Self {
            text: String::new(),
            scores: Vec::new(),
            boxes: Vec::new(),
            status,
            method: method.into(),
            inference_ms: 0.0,
            frame_ms: 0.0,
            provider: Provider::Cpu,
            device_id: None,
            fallback_reason: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelManifest {
    pub model_version: String,
    pub detector_file: String,
    pub recognizer_file: String,
    pub dictionary_file: String,
    pub detector_sha256: String,
    pub recognizer_sha256: String,
    pub dictionary_sha256: String,
}

impl ModelManifest {
    pub fn from_json_file(path: &Path) -> Result<Self, EngineError> {
        let raw =
            fs::read_to_string(path).map_err(|_| EngineError::MissingModel(path.to_path_buf()))?;
        let document: ManifestDocument = serde_json::from_str(&raw)
            .map_err(|error| EngineError::InvalidManifest(error.to_string()))?;
        if document.model_version.trim().is_empty() {
            return Err(EngineError::InvalidManifest(
                "model_version must not be empty".into(),
            ));
        }
        Ok(Self {
            model_version: document.model_version,
            detector_file: document.detector.file,
            recognizer_file: document.recognizer.file,
            dictionary_file: document.dictionary.file,
            detector_sha256: document.detector.sha256,
            recognizer_sha256: document.recognizer.sha256,
            dictionary_sha256: document.dictionary.sha256,
        })
    }
}

#[derive(Debug, Deserialize)]
struct ManifestDocument {
    model_version: String,
    detector: ManifestAsset,
    recognizer: ManifestAsset,
    dictionary: ManifestAsset,
}

#[derive(Debug, Deserialize)]
struct ManifestAsset {
    file: String,
    sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OcrEngineConfig {
    pub provider: Provider,
    pub model_dir: PathBuf,
    pub manifest: ModelManifest,
    pub allow_provider_fallback: bool,
}

#[cfg(any(feature = "onnx", test))]
#[derive(Debug, Clone, PartialEq)]
struct ReadingBlock {
    text: String,
    score: f32,
    bbox: [u32; 4],
}

/// Order OCR blocks by visual reading order: rows first, then left-to-right.
/// A row's anchor is never moved by later blocks, preventing a small noise box
/// from chaining two otherwise separate rows together.
#[cfg(any(feature = "onnx", test))]
fn order_reading_blocks(mut blocks: Vec<ReadingBlock>) -> Vec<ReadingBlock> {
    let mut rows: Vec<(u32, u32, Vec<ReadingBlock>)> = Vec::new();
    blocks.sort_by_key(|block| (block.bbox[1], block.bbox[0]));
    for block in blocks {
        let height = block.bbox[3].saturating_sub(block.bbox[1]).max(1);
        let center = block.bbox[1] + height / 2;
        // Use the smaller height so a tall box cannot absorb an adjacent
        // short row. The fixed anchor also prevents noise from chaining rows.
        let row = rows.iter().position(|(anchor, row_height, _)| {
            center.abs_diff(*anchor) <= height.min(*row_height) / 2
        });
        if let Some(index) = row {
            rows[index].2.push(block);
        } else {
            rows.push((center, height, vec![block]));
        }
    }
    rows.sort_by_key(|(anchor, _, _)| *anchor);
    let mut ordered = Vec::with_capacity(rows.iter().map(|row| row.2.len()).sum());
    for (_, _, mut row) in rows {
        row.sort_by_key(|block| block.bbox[0]);
        ordered.extend(row);
    }
    ordered
}

pub trait Recognizer: Send + Sync {
    fn recognize(&self, roi: &[u8], width: u32, height: u32) -> Result<(String, Vec<f32>), String>;

    fn recognize_with_boxes(
        &self,
        roi: &[u8],
        width: u32,
        height: u32,
    ) -> Result<(String, Vec<f32>, Vec<[u32; 4]>), String> {
        self.recognize(roi, width, height)
            .map(|(text, scores)| (text, scores, Vec::new()))
    }

    fn provider_info(&self) -> ProviderInfo {
        ProviderInfo::default()
    }
}

#[cfg(feature = "onnx")]
struct PpOcrState {
    pipeline: ppocr_rs::OcrLite,
    info: ProviderInfo,
}

#[cfg(feature = "onnx")]
pub struct PpOcrRecognizer {
    state: std::sync::Mutex<PpOcrState>,
    detector: PathBuf,
    recognizer: PathBuf,
    dictionary: PathBuf,
    threads: usize,
    allow_provider_fallback: bool,
}

#[cfg(feature = "onnx")]
fn utf8_path(path: &Path) -> Result<&str, String> {
    path.to_str()
        .ok_or_else(|| format!("{} is not UTF-8", path.display()))
}

#[cfg(feature = "onnx")]
fn build_cpu_pipeline(
    detector: &Path,
    recognizer: &Path,
    dictionary: &Path,
    threads: usize,
) -> Result<ppocr_rs::OcrLite, String> {
    let mut pipeline = ppocr_rs::OcrLite::new();
    pipeline
        .init_models_no_angle(
            utf8_path(detector)?,
            utf8_path(recognizer)?,
            utf8_path(dictionary)?,
            threads,
        )
        .map_err(|error| error.to_string())?;
    Ok(pipeline)
}

#[cfg(feature = "onnx")]
fn directml_device_id() -> i32 {
    std::env::var("PUBG_OCR_DIRECTML_DEVICE_ID")
        .ok()
        .and_then(|value| value.parse::<i32>().ok())
        .unwrap_or(0)
}

#[cfg(feature = "directml")]
static DIRECTML_DEVICE_ID: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
#[cfg(feature = "directml")]
static DIRECTML_INTRA_THREADS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(1);

#[cfg(all(feature = "directml", windows))]
fn directml_runtime_available() -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;

    extern "system" {
        fn LoadLibraryW(name: *const u16) -> *mut std::ffi::c_void;
        fn FreeLibrary(handle: *mut std::ffi::c_void) -> i32;
    }

    let name: Vec<u16> = std::ffi::OsStr::new("directml.dll")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let handle = LoadLibraryW(name.as_ptr());
        if handle.is_null() {
            return Err(format!(
                "directml.dll unavailable: {}",
                std::io::Error::last_os_error()
            ));
        }
        FreeLibrary(handle);
    }
    Ok(())
}

#[cfg(all(feature = "directml", not(windows)))]
fn directml_runtime_available() -> Result<(), String> {
    Err("directml is only available on windows".into())
}

#[cfg(feature = "directml")]
fn directml_session_builder(
    session: ort::session::builder::SessionBuilder,
) -> Result<ort::session::builder::SessionBuilder, ort::Error> {
    use ort::execution_providers::DirectMLExecutionProvider;
    use ort::session::builder::GraphOptimizationLevel;
    use std::sync::atomic::Ordering;

    session
        .with_optimization_level(GraphOptimizationLevel::Level2)?
        .with_intra_threads(DIRECTML_INTRA_THREADS.load(Ordering::Relaxed))?
        .with_execution_providers([DirectMLExecutionProvider::default()
            .with_device_id(DIRECTML_DEVICE_ID.load(Ordering::Relaxed))
            .build()])?
        .with_parallel_execution(false)?
        .with_memory_pattern(false)
}

#[cfg(feature = "directml")]
fn build_directml_pipeline(
    detector: &Path,
    recognizer: &Path,
    dictionary: &Path,
    threads: usize,
    device_id: i32,
) -> Result<ppocr_rs::OcrLite, String> {
    use std::sync::atomic::Ordering;

    directml_runtime_available()?;
    DIRECTML_DEVICE_ID.store(device_id, Ordering::Relaxed);
    DIRECTML_INTRA_THREADS.store(threads.max(1), Ordering::Relaxed);
    let mut pipeline = ppocr_rs::OcrLite::new();
    pipeline
        .init_models_no_angle_with_builder(
            utf8_path(detector)?,
            utf8_path(recognizer)?,
            utf8_path(dictionary)?,
            threads,
            Some(directml_session_builder),
        )
        .map_err(|error| error.to_string())?;
    Ok(pipeline)
}

#[cfg(all(feature = "onnx", not(feature = "directml")))]
fn build_directml_pipeline(
    _detector: &Path,
    _recognizer: &Path,
    _dictionary: &Path,
    _threads: usize,
    _device_id: i32,
) -> Result<ppocr_rs::OcrLite, String> {
    Err("directml support is not enabled in this build".into())
}

#[cfg(feature = "onnx")]
impl PpOcrRecognizer {
    pub fn from_paths(
        detector: &Path,
        recognizer: &Path,
        dictionary: &Path,
        threads: usize,
    ) -> Result<Self, String> {
        Self::for_provider(
            detector,
            recognizer,
            dictionary,
            threads,
            Provider::Cpu,
            false,
        )
    }

    pub fn from_model_dir(
        model_dir: &Path,
        manifest: &ModelManifest,
        threads: usize,
    ) -> Result<Self, String> {
        Self::from_model_dir_with_provider(model_dir, manifest, threads, Provider::Cpu, false)
    }

    pub fn from_model_dir_with_provider(
        model_dir: &Path,
        manifest: &ModelManifest,
        threads: usize,
        requested: Provider,
        allow_provider_fallback: bool,
    ) -> Result<Self, String> {
        validate_manifest(model_dir, manifest).map_err(|error| error.to_string())?;
        Self::for_provider(
            &model_dir.join(&manifest.detector_file),
            &model_dir.join(&manifest.recognizer_file),
            &model_dir.join(&manifest.dictionary_file),
            threads,
            requested,
            allow_provider_fallback,
        )
    }

    fn for_provider(
        detector: &Path,
        recognizer: &Path,
        dictionary: &Path,
        threads: usize,
        requested: Provider,
        allow_provider_fallback: bool,
    ) -> Result<Self, String> {
        let (pipeline, info) = match requested {
            Provider::Cpu => (
                build_cpu_pipeline(detector, recognizer, dictionary, threads)?,
                ProviderInfo {
                    provider: Provider::Cpu,
                    requested: Provider::Cpu,
                    device_id: None,
                    fallback_reason: None,
                },
            ),
            Provider::DirectMl => {
                let device_id = directml_device_id();
                match build_directml_pipeline(detector, recognizer, dictionary, threads, device_id)
                {
                    Ok(pipeline) => (
                        pipeline,
                        ProviderInfo {
                            provider: Provider::DirectMl,
                            requested: Provider::DirectMl,
                            device_id: Some(device_id),
                            fallback_reason: None,
                        },
                    ),
                    Err(error) if allow_provider_fallback => (
                        build_cpu_pipeline(detector, recognizer, dictionary, threads)?,
                        ProviderInfo {
                            provider: Provider::Cpu,
                            requested: Provider::DirectMl,
                            device_id: Some(device_id),
                            fallback_reason: Some(format!("directml-init-failed: {error}")),
                        },
                    ),
                    Err(error) => {
                        return Err(format!("directml-init-failed: {error}"));
                    }
                }
            }
        };
        Ok(Self {
            state: std::sync::Mutex::new(PpOcrState { pipeline, info }),
            detector: detector.to_path_buf(),
            recognizer: recognizer.to_path_buf(),
            dictionary: dictionary.to_path_buf(),
            threads,
            allow_provider_fallback,
        })
    }
}

#[cfg(feature = "onnx")]
impl PpOcrRecognizer {
    fn detect_roi(
        &self,
        roi: &[u8],
        width: u32,
        height: u32,
    ) -> Result<(String, Vec<f32>, Vec<[u32; 4]>), String> {
        let image = image::RgbImage::from_raw(width, height, roi.to_vec())
            .ok_or("ROI buffer size does not match dimensions")?;
        let mut state = self.state.lock().map_err(|_| "OCR session lock poisoned")?;
        let result = state
            .pipeline
            .detect(&image, 0, 960, 0.3, 0.6, 1.6, false, false)
            .map_err(|error| error.to_string());
        let result = match result {
            Err(error)
                if state.info.provider == Provider::DirectMl && self.allow_provider_fallback =>
            {
                let pipeline = build_cpu_pipeline(
                    &self.detector,
                    &self.recognizer,
                    &self.dictionary,
                    self.threads,
                )?;
                state.pipeline = pipeline;
                state.info.provider = Provider::Cpu;
                state.info.fallback_reason = Some(format!("directml-inference-failed: {error}"));
                state
                    .pipeline
                    .detect(&image, 0, 960, 0.3, 0.6, 1.6, false, false)
                    .map_err(|error| error.to_string())?
            }
            other => other?,
        };
        let blocks = order_reading_blocks(
            result
                .text_blocks
                .into_iter()
                .map(|block| {
                    let min_x = block
                        .box_points
                        .iter()
                        .map(|point| point.x)
                        .min()
                        .unwrap_or(0);
                    let min_y = block
                        .box_points
                        .iter()
                        .map(|point| point.y)
                        .min()
                        .unwrap_or(0);
                    let max_x = block
                        .box_points
                        .iter()
                        .map(|point| point.x)
                        .max()
                        .unwrap_or(0);
                    let max_y = block
                        .box_points
                        .iter()
                        .map(|point| point.y)
                        .max()
                        .unwrap_or(0);
                    ReadingBlock {
                        text: block.text,
                        score: block.text_score,
                        bbox: [min_x, min_y, max_x, max_y],
                    }
                })
                .collect(),
        );
        let text = blocks
            .iter()
            .map(|block| block.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let scores = blocks.iter().map(|block| block.score).collect();
        let boxes = blocks.iter().map(|block| block.bbox).collect();
        Ok((text, scores, boxes))
    }
}

#[cfg(feature = "onnx")]
impl Recognizer for PpOcrRecognizer {
    fn recognize(&self, roi: &[u8], width: u32, height: u32) -> Result<(String, Vec<f32>), String> {
        self.detect_roi(roi, width, height)
            .map(|(text, scores, _)| (text, scores))
    }

    fn recognize_with_boxes(
        &self,
        roi: &[u8],
        width: u32,
        height: u32,
    ) -> Result<(String, Vec<f32>, Vec<[u32; 4]>), String> {
        self.detect_roi(roi, width, height)
    }

    fn provider_info(&self) -> ProviderInfo {
        self.state
            .lock()
            .map(|state| state.info.clone())
            .unwrap_or_default()
    }
}

pub struct OcrEngine<R> {
    requested: Provider,
    recognizer: R,
}

impl<R: Recognizer> OcrEngine<R> {
    pub fn new(config: OcrEngineConfig, recognizer: R) -> Result<Self, EngineError> {
        validate_manifest(&config.model_dir, &config.manifest)?;
        let info = recognizer.provider_info();
        if config.provider == Provider::DirectMl
            && info.provider == Provider::Cpu
            && info.requested != Provider::DirectMl
            && !config.allow_provider_fallback
        {
            return Err(EngineError::ProviderUnavailable);
        }
        Ok(Self {
            requested: config.provider,
            recognizer,
        })
    }

    pub fn provider(&self) -> Provider {
        self.recognizer.provider_info().provider
    }

    pub fn provider_info(&self) -> ProviderInfo {
        self.recognizer.provider_info()
    }

    fn runtime_info(&self) -> ProviderInfo {
        let mut info = self.recognizer.provider_info();
        info.requested = self.requested;
        if self.requested == Provider::DirectMl
            && info.provider == Provider::Cpu
            && info.fallback_reason.is_none()
        {
            info.fallback_reason = Some("directml-unavailable".into());
        }
        info
    }

    pub fn recognize(&self, roi: Roi<'_>) -> OcrResult {
        let frame_start = Instant::now();
        if roi.width == 0 || roi.height == 0 || roi.pixels.is_empty() {
            let mut result = OcrResult::empty(OcrStatus::EmptyRoi, self.method());
            result.provider = self.provider();
            return result;
        }
        let expected_len = (roi.width as usize)
            .checked_mul(roi.height as usize)
            .and_then(|value| value.checked_mul(3));
        if expected_len != Some(roi.pixels.len()) {
            let mut result = OcrResult::empty(OcrStatus::InvalidRoi, self.method());
            result.provider = self.provider();
            return result;
        }
        let inference_start = Instant::now();
        let outcome = self
            .recognizer
            .recognize_with_boxes(roi.pixels, roi.width, roi.height);
        let info = self.runtime_info();
        match outcome {
            Ok((text, scores, boxes)) => OcrResult {
                text,
                scores,
                boxes,
                status: if info.fell_back() {
                    OcrStatus::ProviderFallback
                } else {
                    OcrStatus::Ok
                },
                method: self.method(),
                inference_ms: inference_start.elapsed().as_secs_f64() * 1000.0,
                frame_ms: frame_start.elapsed().as_secs_f64() * 1000.0,
                provider: info.provider,
                device_id: info.device_id,
                fallback_reason: info.fallback_reason,
            },
            Err(_) => OcrResult {
                text: String::new(),
                scores: Vec::new(),
                boxes: Vec::new(),
                status: OcrStatus::InferenceError,
                method: self.method(),
                inference_ms: inference_start.elapsed().as_secs_f64() * 1000.0,
                frame_ms: frame_start.elapsed().as_secs_f64() * 1000.0,
                provider: info.provider,
                device_id: info.device_id,
                fallback_reason: info.fallback_reason,
            },
        }
    }

    fn method(&self) -> String {
        format!("onnx-{}", provider_name(self.provider()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    MissingModel(PathBuf),
    HashMismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },
    ProviderUnavailable,
    InvalidManifest(String),
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingModel(path) => {
                write!(f, "model file missing or unreadable: {}", path.display())
            }
            Self::HashMismatch {
                path,
                expected,
                actual,
            } => write!(
                f,
                "model file sha256 mismatch for {}: expected {expected}, got {actual}",
                path.display()
            ),
            Self::ProviderUnavailable => write!(f, "requested provider is unavailable"),
            Self::InvalidManifest(reason) => write!(f, "invalid model manifest: {reason}"),
        }
    }
}
impl std::error::Error for EngineError {}

fn validate_manifest(dir: &Path, manifest: &ModelManifest) -> Result<(), EngineError> {
    for (name, expected) in [
        (&manifest.detector_file, &manifest.detector_sha256),
        (&manifest.recognizer_file, &manifest.recognizer_sha256),
        (&manifest.dictionary_file, &manifest.dictionary_sha256),
    ] {
        let path = dir.join(name);
        let bytes = fs::read(&path).map_err(|_| EngineError::MissingModel(path.clone()))?;
        let actual = format!("{:x}", Sha256::digest(bytes));
        if expected.len() != 64 || !expected.eq_ignore_ascii_case(&actual) {
            return Err(EngineError::HashMismatch {
                path,
                expected: expected.clone(),
                actual,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn reading_order_sorts_left_to_right_within_rows() {
        let blocks = vec![
            ReadingBlock {
                text: "B".into(),
                score: 0.8,
                bbox: [100, 20, 130, 40],
            },
            ReadingBlock {
                text: "row2".into(),
                score: 0.8,
                bbox: [5, 60, 80, 80],
            },
            ReadingBlock {
                text: "A".into(),
                score: 0.8,
                bbox: [10, 22, 40, 42],
            },
        ];
        let ordered = order_reading_blocks(blocks);
        assert_eq!(
            ordered
                .iter()
                .map(|block| block.text.as_str())
                .collect::<Vec<_>>(),
            ["A", "B", "row2"]
        );
        assert_eq!(
            ordered.iter().map(|block| block.bbox).collect::<Vec<_>>(),
            [[10, 22, 40, 42], [100, 20, 130, 40], [5, 60, 80, 80]]
        );
    }

    #[test]
    fn tall_box_does_not_absorb_adjacent_short_row() {
        let blocks = vec![
            ReadingBlock {
                text: "top".into(),
                score: 0.9,
                bbox: [100, 0, 140, 100],
            },
            ReadingBlock {
                text: "bottom".into(),
                score: 0.8,
                bbox: [0, 95, 40, 105],
            },
        ];
        let ordered = order_reading_blocks(blocks);
        assert_eq!(
            ordered
                .iter()
                .map(|block| block.text.as_str())
                .collect::<Vec<_>>(),
            ["top", "bottom"]
        );
    }

    #[test]
    fn small_noise_does_not_chain_rows() {
        let blocks = vec![
            ReadingBlock {
                text: "top".into(),
                score: 1.0,
                bbox: [100, 10, 140, 30],
            },
            ReadingBlock {
                text: "noise".into(),
                score: 1.0,
                bbox: [0, 35, 4, 36],
            },
            ReadingBlock {
                text: "bottom".into(),
                score: 1.0,
                bbox: [10, 45, 80, 65],
            },
        ];
        let ordered = order_reading_blocks(blocks);
        assert_eq!(
            ordered
                .iter()
                .map(|block| block.text.as_str())
                .collect::<Vec<_>>(),
            ["top", "noise", "bottom"]
        );
    }
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static TEST_ENGINE_ID: AtomicUsize = AtomicUsize::new(0);
    struct Fake;
    impl Recognizer for Fake {
        fn recognize(&self, _: &[u8], _: u32, _: u32) -> Result<(String, Vec<f32>), String> {
            Ok(("击倒了你 Enemy".into(), vec![0.99]))
        }
    }
    fn config(provider: Provider, allow_provider_fallback: bool) -> OcrEngineConfig {
        static TEST_DIR_ID: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "pubg-ocr-engine-cfg-{}-{}",
            std::process::id(),
            TEST_DIR_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::create_dir_all(&dir);
        for n in ["det.onnx", "rec.onnx", "dict.txt"] {
            fs::write(dir.join(n), b"fixture").unwrap();
        }
        let digest = format!("{:x}", Sha256::digest(b"fixture"));
        OcrEngineConfig {
            provider,
            model_dir: dir,
            manifest: ModelManifest {
                model_version: "test".into(),
                detector_file: "det.onnx".into(),
                recognizer_file: "rec.onnx".into(),
                dictionary_file: "dict.txt".into(),
                detector_sha256: digest.clone(),
                recognizer_sha256: digest.clone(),
                dictionary_sha256: digest,
            },
            allow_provider_fallback,
        }
    }

    fn engine() -> OcrEngine<Fake> {
        let dir = std::env::temp_dir().join(format!(
            "pubg-ocr-engine-test-{}-{}",
            std::process::id(),
            TEST_ENGINE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::create_dir_all(&dir);
        for n in ["det.onnx", "rec.onnx", "dict.txt"] {
            fs::write(dir.join(n), b"fixture").unwrap();
        }
        let digest = format!("{:x}", Sha256::digest(b"fixture"));
        OcrEngine::new(
            OcrEngineConfig {
                provider: Provider::Cpu,
                model_dir: dir,
                manifest: ModelManifest {
                    model_version: "test".into(),
                    detector_file: "det.onnx".into(),
                    recognizer_file: "rec.onnx".into(),
                    dictionary_file: "dict.txt".into(),
                    detector_sha256: digest.clone(),
                    recognizer_sha256: digest.clone(),
                    dictionary_sha256: digest,
                },
                allow_provider_fallback: true,
            },
            Fake,
        )
        .unwrap()
    }
    #[test]
    fn cpu_result_keeps_text_and_timing() {
        let r = engine().recognize(Roi {
            pixels: &[0; 60],
            width: 10,
            height: 2,
        });
        assert_eq!(r.status, OcrStatus::Ok);
        assert_eq!(r.text, "击倒了你 Enemy");
        assert!(r.inference_ms >= 0.0);
    }
    #[test]
    fn empty_roi_is_not_normal_miss() {
        let r = engine().recognize(Roi {
            pixels: &[],
            width: 0,
            height: 0,
        });
        assert_eq!(r.status, OcrStatus::EmptyRoi);
    }
    #[test]
    fn overflowing_roi_dimensions_are_rejected() {
        let r = engine().recognize(Roi {
            pixels: &[0],
            width: u32::MAX,
            height: u32::MAX,
        });
        assert_eq!(r.status, OcrStatus::InvalidRoi);
    }
    #[test]
    fn missing_model_is_explicit() {
        let c = OcrEngineConfig {
            provider: Provider::Cpu,
            model_dir: PathBuf::from("missing"),
            manifest: ModelManifest {
                model_version: "x".into(),
                detector_file: "det.onnx".into(),
                recognizer_file: "rec.onnx".into(),
                dictionary_file: "dict.txt".into(),
                detector_sha256: "0".repeat(64),
                recognizer_sha256: "0".repeat(64),
                dictionary_sha256: "0".repeat(64),
            },
            allow_provider_fallback: false,
        };
        assert!(matches!(
            OcrEngine::new(c, Fake),
            Err(EngineError::MissingModel(_))
        ));
    }

    #[test]
    fn directml_request_falls_back_to_cpu_when_allowed() {
        let engine = OcrEngine::new(config(Provider::DirectMl, true), Fake).unwrap();
        assert_eq!(engine.provider(), Provider::Cpu);
        let result = engine.recognize(Roi {
            pixels: &[0; 60],
            width: 10,
            height: 2,
        });
        assert_eq!(result.status, OcrStatus::ProviderFallback);
        assert_eq!(result.provider, Provider::Cpu);
        assert_eq!(result.method, "onnx-cpu");
        assert!(result.fallback_reason.is_some());
    }

    #[test]
    fn directml_request_without_fallback_is_rejected() {
        assert!(matches!(
            OcrEngine::new(config(Provider::DirectMl, false), Fake),
            Err(EngineError::ProviderUnavailable)
        ));
    }

    #[test]
    fn manifest_loader_reads_pinned_assets() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("models/pp_ocrv6_medium.json");
        let manifest = ModelManifest::from_json_file(&path).unwrap();
        assert_eq!(manifest.model_version, "PP-OCRv6_medium_onnx");
        assert_eq!(manifest.detector_sha256.len(), 64);
    }

    #[cfg(feature = "onnx")]
    #[test]
    #[ignore = "requires downloaded PP-OCRv6 assets and an ORT runtime"]
    fn ppocr_cpu_smoke_test() {
        let model_dir = std::env::var_os("PUBG_OCR_MODEL_DIR").expect("PUBG_OCR_MODEL_DIR");
        let model_dir = PathBuf::from(model_dir);
        let recognizer = PpOcrRecognizer::from_paths(
            &model_dir.join("det.onnx"),
            &model_dir.join("rec.onnx"),
            &model_dir.join("dict.txt"),
            1,
        )
        .unwrap();
        let manifest_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("models/pp_ocrv6_medium.json");
        let manifest = ModelManifest::from_json_file(&manifest_path).unwrap();
        let engine = OcrEngine::new(
            OcrEngineConfig {
                provider: Provider::Cpu,
                model_dir,
                manifest,
                allow_provider_fallback: false,
            },
            recognizer,
        )
        .unwrap();
        let result = engine.recognize(Roi {
            pixels: &[0; 96 * 96 * 3],
            width: 96,
            height: 96,
        });
        assert!(matches!(result.status, OcrStatus::Ok));
        assert!(result.frame_ms >= result.inference_ms);
    }
}
