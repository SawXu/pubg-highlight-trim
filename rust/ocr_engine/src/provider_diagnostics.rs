//! Shareable provider failure reasons retain actionable diagnostics, not local paths.
use std::path::Path;

/// Third-party Display implementations may show only a category ("Ort error").
/// Keep their source chain so provider classification and reports see the native cause.
pub fn error_chain(error: &(dyn std::error::Error + 'static)) -> String {
    let mut reason = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        reason.push_str(": ");
        let message = cause.to_string();
        if message.is_empty() {
            // Some native runtimes return an empty message but retain an error
            // code in Debug. Do not invent a device-availability diagnosis.
            reason.push_str(&format!("{cause:?}"));
        } else {
            reason.push_str(&message);
        }
        source = cause.source();
    }
    reason
}

/// Classify only concrete device-availability failures from real DirectML session
/// initialization. Generic unsupported operators, invalid arguments, and driver
/// failures must retain the existing initialization-failure code.
pub fn directml_initialization_failure(reason: &str) -> String {
    if reason.starts_with("directml-dll-missing:") {
        return reason.to_owned();
    }
    let lower = reason.to_ascii_lowercase();
    let no_device = [
        "887a0002", // DXGI_ERROR_NOT_FOUND (selected adapter does not exist)
        "887a0004", // DXGI_ERROR_UNSUPPORTED (no compatible D3D12 device)
        "c0262002", // ERROR_GRAPHICS_INVALID_DISPLAY_ADAPTER (software adapter)
        "dxgi_error_not_found",
        "dxgi_error_unsupported",
        "error_graphics_invalid_display_adapter",
        "no compatible directx 12 device",
        "no available directx 12 device",
        "no gpus or npus detected",
    ]
    .iter()
    .any(|marker| lower.contains(marker));
    let code = if no_device {
        "directml-no-device"
    } else {
        "directml-init-failed"
    };
    format!("{code}: {reason}")
}

pub fn redact(reason: &str, model_dir: &Path) -> String {
    let mut text = reason.to_owned();
    let mut roots = vec![model_dir.to_path_buf()];
    if let Ok(canonical) = model_dir.canonicalize() {
        roots.push(canonical);
    }
    for root in roots {
        let value = root.to_string_lossy();
        for variant in [
            value.to_string(),
            value.replace('\\', "/"),
            value.replace('/', "\\"),
        ] {
            if !variant.is_empty() {
                text = text.replace(&variant, "<model-directory>");
            }
        }
    }
    // Native WIL/ORT diagnostics append the HRESULT after a source location.
    // Redact that location first so the general Windows-path matcher does not
    // swallow the actionable error text (Windows paths may contain spaces).
    let source_locations = regex::Regex::new(
        r#"(?i)(?:[a-z]:[\\/]|\\\\|/)[^\r\n\"',;]*?\.(?:cc|cpp|c|h|hpp)(\(\d+\)|:\d+:)"#,
    )
    .unwrap();
    text = source_locations
        .replace_all(&text, "<local-path>$1")
        .into_owned();
    // Unknown operator paths can also occur in native loader/inference errors.
    let paths = regex::Regex::new(
        r#"(?i)[a-z]:[\\/][^\r\n\"',;]*|(?:\\\\|//)[^\s\"',;]+|/(?:[^/\s:]+/)+[^\s\"',;]*"#,
    )
    .unwrap();
    paths.replace_all(&text, "<local-path>").into_owned()
}
