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
}

#[cfg(feature = "onnx")]
pub struct PpOcrRecognizer {
    pipeline: std::sync::Mutex<ppocr_rs::OcrLite>,
}

#[cfg(feature = "onnx")]
impl PpOcrRecognizer {
    pub fn from_paths(
        detector: &Path,
        recognizer: &Path,
        dictionary: &Path,
        threads: usize,
    ) -> Result<Self, String> {
        let mut pipeline = ppocr_rs::OcrLite::new();
        pipeline
            .init_models_no_angle(
                detector.to_str().ok_or("detector path is not UTF-8")?,
                recognizer.to_str().ok_or("recognizer path is not UTF-8")?,
                dictionary.to_str().ok_or("dictionary path is not UTF-8")?,
                threads,
            )
            .map_err(|error| error.to_string())?;
        Ok(Self {
            pipeline: std::sync::Mutex::new(pipeline),
        })
    }

    pub fn from_model_dir(
        model_dir: &Path,
        manifest: &ModelManifest,
        threads: usize,
    ) -> Result<Self, String> {
        Self::from_paths(
            &model_dir.join(&manifest.detector_file),
            &model_dir.join(&manifest.recognizer_file),
            &model_dir.join(&manifest.dictionary_file),
            threads,
        )
    }

    #[cfg(feature = "directml")]
    pub fn from_paths_directml(
        detector: &Path,
        classifier: &Path,
        recognizer: &Path,
        dictionary: &Path,
        threads: usize,
    ) -> Result<Self, String> {
        fn builder(
            session: ort::session::builder::SessionBuilder,
        ) -> Result<ort::session::builder::SessionBuilder, ort::Error> {
            let device_id = std::env::var("PUBG_OCR_DIRECTML_DEVICE_ID")
                .ok()
                .and_then(|value| value.parse::<i32>().ok())
                .unwrap_or(0);
            session.with_execution_providers([
                ort::execution_providers::DirectMLExecutionProvider::default()
                    .with_device_id(device_id)
                    .build(),
            ])
        }
        let mut pipeline = ppocr_rs::OcrLite::new();
        pipeline
            .init_models_with_dict_and_builder(
                detector.to_str().ok_or("detector path is not UTF-8")?,
                classifier.to_str().ok_or("classifier path is not UTF-8")?,
                recognizer.to_str().ok_or("recognizer path is not UTF-8")?,
                dictionary.to_str().ok_or("dictionary path is not UTF-8")?,
                threads,
                Some(builder),
            )
            .map_err(|error| error.to_string())?;
        Ok(Self {
            pipeline: std::sync::Mutex::new(pipeline),
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
        let mut pipeline = self
            .pipeline
            .lock()
            .map_err(|_| "OCR session lock poisoned")?;
        let result = pipeline
            .detect(&image, 0, 960, 0.3, 0.6, 1.6, false, false)
            .map_err(|error| error.to_string())?;
        let mut blocks = result.text_blocks;
        blocks.sort_by_key(|block| {
            let min_y = block
                .box_points
                .iter()
                .map(|point| point.y)
                .min()
                .unwrap_or(0);
            let min_x = block
                .box_points
                .iter()
                .map(|point| point.x)
                .min()
                .unwrap_or(0);
            (min_y, min_x)
        });
        let text = blocks
            .iter()
            .map(|block| block.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let scores = blocks.iter().map(|block| block.text_score).collect();

        let boxes = blocks
            .iter()
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
                [min_x, min_y, max_x, max_y]
            })
            .collect();
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
}

pub struct OcrEngine<R> {
    provider: Provider,
    recognizer: R,
    provider_fallback: bool,
}

impl<R: Recognizer> OcrEngine<R> {
    pub fn new(config: OcrEngineConfig, recognizer: R) -> Result<Self, EngineError> {
        validate_manifest(&config.model_dir, &config.manifest)?;
        let (provider, fallback) = match config.provider {
            Provider::Cpu => (Provider::Cpu, false),
            Provider::DirectMl if directml_available() => (Provider::DirectMl, false),
            Provider::DirectMl if config.allow_provider_fallback => (Provider::Cpu, true),
            Provider::DirectMl => return Err(EngineError::ProviderUnavailable),
        };
        Ok(Self {
            provider,
            recognizer,
            provider_fallback: fallback,
        })
    }

    pub fn provider(&self) -> Provider {
        self.provider
    }

    pub fn recognize(&self, roi: Roi<'_>) -> OcrResult {
        let frame_start = Instant::now();
        if roi.width == 0 || roi.height == 0 || roi.pixels.is_empty() {
            return OcrResult::empty(OcrStatus::EmptyRoi, self.method());
        }
        let expected_len = (roi.width as usize)
            .checked_mul(roi.height as usize)
            .and_then(|value| value.checked_mul(3));
        if expected_len != Some(roi.pixels.len()) {
            return OcrResult::empty(OcrStatus::InvalidRoi, self.method());
        }
        let inference_start = Instant::now();
        match self
            .recognizer
            .recognize_with_boxes(roi.pixels, roi.width, roi.height)
        {
            Ok((text, scores, boxes)) => OcrResult {
                text,
                scores,
                boxes,
                status: if self.provider_fallback {
                    OcrStatus::ProviderFallback
                } else {
                    OcrStatus::Ok
                },
                method: self.method(),
                inference_ms: inference_start.elapsed().as_secs_f64() * 1000.0,
                frame_ms: frame_start.elapsed().as_secs_f64() * 1000.0,
            },
            Err(_) => OcrResult {
                text: String::new(),
                scores: Vec::new(),
                boxes: Vec::new(),
                status: OcrStatus::InferenceError,
                method: self.method(),
                inference_ms: inference_start.elapsed().as_secs_f64() * 1000.0,
                frame_ms: frame_start.elapsed().as_secs_f64() * 1000.0,
            },
        }
    }

    fn method(&self) -> String {
        format!(
            "onnx-{}",
            match self.provider {
                Provider::Cpu => "cpu",
                Provider::DirectMl => "directml",
            }
        )
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
        write!(f, "{self:?}")
    }
}
impl std::error::Error for EngineError {}

#[cfg(feature = "directml")]
fn directml_available() -> bool {
    use ort::execution_providers::{DirectMLExecutionProvider, ExecutionProvider};

    cfg!(target_os = "windows")
        && std::env::var_os("PUBG_OCR_DIRECTML").is_some()
        && DirectMLExecutionProvider::default()
            .is_available()
            .unwrap_or(false)
}

#[cfg(not(feature = "directml"))]
fn directml_available() -> bool {
    false
}

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
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static TEST_ENGINE_ID: AtomicUsize = AtomicUsize::new(0);
    struct Fake;
    impl Recognizer for Fake {
        fn recognize(&self, _: &[u8], _: u32, _: u32) -> Result<(String, Vec<f32>), String> {
            Ok(("击倒了你 Enemy".into(), vec![0.99]))
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
