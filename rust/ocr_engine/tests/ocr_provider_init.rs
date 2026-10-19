#![cfg(feature = "onnx")]
use pubg_ocr_engine::{ModelManifest, OcrEngine, OcrEngineConfig, PpOcrRecognizer, Provider};
use std::path::PathBuf;

#[test]
#[ignore = "requires pinned PUBG_OCR_MODEL_DIR and native ONNX runtime"]
fn explicit_device_is_reported_on_fallback_and_strict_initialization_fails() {
    let dir = PathBuf::from(std::env::var_os("PUBG_OCR_MODEL_DIR").unwrap());
    let manifest = ModelManifest::from_json_file(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("models/pp_ocrv6_medium.json"),
    )
    .unwrap();
    let recognizer = PpOcrRecognizer::from_model_dir_with_provider_and_device(
        &dir,
        &manifest,
        1,
        Provider::DirectMl,
        true,
        i32::MAX,
    )
    .unwrap();
    let engine = OcrEngine::new(
        OcrEngineConfig {
            provider: Provider::DirectMl,
            model_dir: dir.clone(),
            manifest: manifest.clone(),
            allow_provider_fallback: true,
        },
        recognizer,
    )
    .unwrap();
    let info = engine.provider_info();
    assert_eq!(info.requested, Provider::DirectMl);
    assert_eq!(info.provider, Provider::Cpu);
    assert_eq!(info.device_id, Some(i32::MAX));
    assert!(info
        .fallback_reason
        .unwrap()
        .contains("directml-init-failed"));
    assert!(PpOcrRecognizer::from_model_dir_with_provider_and_device(
        &dir,
        &manifest,
        1,
        Provider::DirectMl,
        false,
        i32::MAX,
    )
    .err()
    .unwrap()
    .contains("directml-init-failed"));
}
