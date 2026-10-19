use pubg_ocr_engine::provider_diagnostics::redact;
use std::path::Path;
#[test]
fn shareable_fallback_reason_keeps_failure_details_and_redacts_local_paths() {
    for dir in [r"Z:\synthetic-models", "Z:/synthetic-models"] {
        let reason=format!("directml-init-failed: unsupported operator Conv; model={dir}/det.onnx; device=7; required DLL missing; native=Y:\\operator\\runtime.dll");
        let safe = redact(&reason, Path::new(r"Z:\synthetic-models"));
        assert!(safe.contains("unsupported operator Conv"));
        assert!(safe.contains("device=7"));
        assert!(safe.contains("required DLL missing"));
        assert!(safe.contains("<model-directory>"));
        assert!(!safe.contains("synthetic-models"));
        assert!(!safe.contains("operator\\runtime"));
    }
    assert_eq!(
        redact(
            "directml-init-failed: directml support is not enabled in this build",
            Path::new("models")
        ),
        "directml-init-failed: directml support is not enabled in this build"
    );
    assert!(!redact(
        "loader '/home/operator/runtime/lib.so' missing",
        Path::new("models")
    )
    .contains("/home/operator"));
}
