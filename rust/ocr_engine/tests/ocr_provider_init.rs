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
    let fallback_reason = info.fallback_reason.unwrap();
    let strict_reason = PpOcrRecognizer::from_model_dir_with_provider_and_device(
        &dir,
        &manifest,
        1,
        Provider::DirectMl,
        false,
        i32::MAX,
    )
    .err()
    .unwrap();
    for reason in [&fallback_reason, &strict_reason] {
        #[cfg(all(feature = "directml", windows))]
        if !reason.starts_with("directml-dll-missing:") {
            assert!(reason.contains("Ort error: "), "{reason}");
            if reason.contains("887A0002") || reason.contains("887a0002") {
                assert!(reason.starts_with("directml-no-device:"), "{reason}");
            } else {
                // Some native ORT builds return an empty message. Retain their
                // error code rather than falsely claim hardware is absent.
                assert!(reason.contains("Error { code:"), "{reason}");
                assert!(reason.starts_with("directml-init-failed:"), "{reason}");
            }
        }
        assert!(
            reason.starts_with("directml-init-failed:")
                || reason.starts_with("directml-dll-missing:")
                || reason.starts_with("directml-no-device:"),
            "{reason}"
        );
    }
}
