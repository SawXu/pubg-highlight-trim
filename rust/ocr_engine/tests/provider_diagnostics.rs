use pubg_ocr_engine::provider_diagnostics::{directml_initialization_failure, redact};
use std::path::Path;
#[cfg(feature = "directml")]
#[test]
fn real_ppocr_ort_error_conversion_retains_native_cause_and_classification() {
    let error = ppocr_rs::OcrError::Ort(ort::Error::new(
        r"C:\native\dml_provider_factory.cc(123): 887A0002 adapter not found",
    ));
    assert_eq!(error.to_string(), "Ort error");
    let reason = pubg_ocr_engine::provider_diagnostics::error_chain(&error);
    assert!(reason.contains("887A0002 adapter not found"));
    let safe = redact(
        &directml_initialization_failure(&reason),
        Path::new("models"),
    );
    assert!(safe.starts_with("directml-no-device: Ort error:"), "{safe}");
    assert!(safe.contains("887A0002 adapter not found"), "{safe}");
    assert!(!safe.contains("C:\\native"));

    let empty_native = ppocr_rs::OcrError::Ort(ort::Error::new_with_code(
        ort::ErrorCode::InvalidArgument,
        "",
    ));
    let reason = pubg_ocr_engine::provider_diagnostics::error_chain(&empty_native);
    assert!(reason.contains("InvalidArgument"), "{reason}");
    assert!(directml_initialization_failure(&reason).starts_with("directml-init-failed:"));
}

#[test]
fn directml_device_failures_are_distinct_and_preserve_native_details() {
    for reason in [
        "EnumAdapters1 failed: HRESULT 0x887A0002",
        "D3D12CreateDevice failed: 887a0004",
        "ERROR_GRAPHICS_INVALID_DISPLAY_ADAPTER (0xC0262002)",
        "DXGI_ERROR_NOT_FOUND",
        "DXGI_ERROR_UNSUPPORTED",
        "No compatible DirectX 12 device",
        "No available DirectX 12 device",
        "No GPUs or NPUs detected.",
    ] {
        assert_eq!(
            directml_initialization_failure(reason),
            format!("directml-no-device: {reason}")
        );
    }
}

#[test]
fn unrelated_initialization_errors_keep_existing_code_and_details() {
    for reason in [
        "directml support is not enabled in this build",
        "unsupported operator Conv; device=7",
        "E_INVALIDARG: device id must be non-negative",
        "D3D12CreateDevice: E_OUTOFMEMORY (0x8007000E)",
        "DXGI_ERROR_DEVICE_REMOVED (0x887A0005)",
        "directml.dll unavailable: not a valid Win32 application (os error 193)",
        "other.dll missing (os error 126)",
    ] {
        assert_eq!(
            directml_initialization_failure(reason),
            format!("directml-init-failed: {reason}")
        );
    }
}

#[test]
fn missing_runtime_is_distinct_and_redacted_without_losing_loader_reason() {
    let reason = r"directml-dll-missing: directml.dll unavailable: module not found (os error 126); source=C:\Users\operator\runtime.dll";
    let safe = redact(
        &directml_initialization_failure(reason),
        Path::new("models"),
    );
    assert!(safe.starts_with("directml-dll-missing: directml.dll unavailable:"));
    assert!(safe.contains("module not found (os error 126)"));
    assert!(!safe.contains("operator"));
    assert!(safe.contains("<local-path>"));
}

#[test]
fn redaction_preserves_native_source_location_failure_details() {
    for reason in [
        r"D:\build\onnxruntime\dml_provider_factory.cc(123): 887A0004 The specified device interface is not supported",
        r"D:\build\onnxruntime\dml_provider_factory.cc:123: DXGI_ERROR_UNSUPPORTED",
        "/home/operator/onnxruntime/dml_provider_factory.cc(123): 0x887A0004",
    ] {
        let safe = redact(
            &directml_initialization_failure(reason),
            Path::new("models"),
        );
        assert!(safe.starts_with("directml-no-device:"), "{safe}");
        assert!(safe.contains("887A0004") || safe.contains("DXGI_ERROR_UNSUPPORTED"));
        assert!(!safe.contains("dml_provider_factory.cc"), "{safe}");
        assert!(!safe.contains("operator"), "{safe}");
    }
}

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
