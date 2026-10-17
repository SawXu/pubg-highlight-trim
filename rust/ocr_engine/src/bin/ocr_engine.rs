#![cfg(feature = "cli")]

use base64::Engine;
use pubg_ocr_engine::{
    provider_name, ModelManifest, OcrEngine, OcrEngineConfig, OcrStatus, PpOcrRecognizer, Provider,
    Roi,
};
use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
struct Request {
    pixels_b64: String,
    width: u32,
    height: u32,
}

#[derive(Debug, Serialize)]
struct Response<'a> {
    text: &'a str,
    scores: &'a [f32],
    boxes: &'a [[u32; 4]],
    status: &'static str,
    method: &'a str,
    inference_ms: f64,
    frame_ms: f64,
    provider: &'static str,
    device_id: Option<i32>,
    fallback_reason: Option<&'a str>,
    model_version: &'a str,
}

#[derive(Debug, Serialize)]
struct InitDiagnostic<'a> {
    event: &'static str,
    requested_provider: &'static str,
    provider: &'static str,
    device_id: Option<i32>,
    model_version: &'a str,
    fallback_reason: Option<&'a str>,
    error: Option<&'a str>,
}

fn status_name(status: OcrStatus) -> &'static str {
    match status {
        OcrStatus::Ok => "ok",
        OcrStatus::EmptyRoi => "empty-roi",
        OcrStatus::InvalidRoi => "invalid-roi",
        OcrStatus::InferenceError => "inference-error",
        OcrStatus::ProviderFallback => "provider-fallback",
        OcrStatus::ModelUnavailable => "model-unavailable",
    }
}

fn resolve_manifest_path(model_dir: &std::path::Path) -> PathBuf {
    let bundled = model_dir.join("pp_ocrv6_medium.json");
    if bundled.is_file() {
        return bundled;
    }
    let fallback = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("models/pp_ocrv6_medium.json");
    if fallback.is_file() {
        return fallback;
    }
    bundled
}

fn parse_provider(value: Option<&str>) -> Result<(Provider, bool), String> {
    let value = value
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .unwrap_or("cpu")
        .to_ascii_lowercase();
    match value.as_str() {
        "cpu" | "default" | "auto" => Ok((Provider::Cpu, false)),
        "directml" | "dml" => Ok((Provider::DirectMl, true)),
        "directml-only" | "dml-only" => Ok((Provider::DirectMl, false)),
        other => Err(format!(
            "unknown provider {other:?}; expected cpu, directml or directml-only"
        )),
    }
}

fn emit_init_diagnostic(diagnostic: &InitDiagnostic<'_>) {
    if let Ok(line) = serde_json::to_string(diagnostic) {
        eprintln!("{line}");
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let model_dir = PathBuf::from(
        args.next()
            .ok_or("usage: ocr_engine <model-dir> [threads] [provider]")?,
    );
    let threads = args
        .next()
        .and_then(|value| value.to_str().and_then(|text| text.parse().ok()))
        .unwrap_or(1);
    let provider_arg = args
        .next()
        .and_then(|value| value.to_str().map(str::to_owned))
        .or_else(|| std::env::var("PUBG_OCR_PROVIDER").ok());
    let (requested_provider, allow_provider_fallback) = parse_provider(provider_arg.as_deref())?;
    let manifest = ModelManifest::from_json_file(&resolve_manifest_path(&model_dir))
        .map_err(|error| error.to_string())?;
    let model_version = manifest.model_version.clone();
    let engine = match PpOcrRecognizer::from_model_dir_with_provider(
        &model_dir,
        &manifest,
        threads,
        requested_provider,
        allow_provider_fallback,
    )
    .and_then(|recognizer| {
        OcrEngine::new(
            OcrEngineConfig {
                provider: requested_provider,
                model_dir,
                manifest,
                allow_provider_fallback,
            },
            recognizer,
        )
        .map_err(|error| error.to_string())
    }) {
        Ok(engine) => {
            let info = engine.provider_info();
            emit_init_diagnostic(&InitDiagnostic {
                event: "engine-init",
                requested_provider: provider_name(requested_provider),
                provider: provider_name(info.provider),
                device_id: info.device_id,
                model_version: &model_version,
                fallback_reason: info.fallback_reason.as_deref(),
                error: None,
            });
            Some(engine)
        }
        Err(error) => {
            emit_init_diagnostic(&InitDiagnostic {
                event: "engine-init",
                requested_provider: provider_name(requested_provider),
                provider: "none",
                device_id: None,
                model_version: &model_version,
                fallback_reason: None,
                error: Some(&error),
            });
            None
        }
    };
    let mut output = io::stdout().lock();
    for line in io::stdin().lock().lines() {
        let request: Request = serde_json::from_str(&line?)?;
        if engine.is_none() {
            serde_json::to_writer(
                &mut output,
                &Response {
                    text: "",
                    scores: &[],
                    boxes: &[],
                    status: "model-unavailable",
                    method: "onnx-unavailable",
                    inference_ms: 0.0,
                    frame_ms: 0.0,
                    provider: "none",
                    device_id: None,
                    fallback_reason: None,
                    model_version: &model_version,
                },
            )?;
            output.write_all(b"\n")?;
            output.flush()?;
            continue;
        }
        let pixels = base64::engine::general_purpose::STANDARD.decode(request.pixels_b64)?;
        let result = engine.as_ref().unwrap().recognize(Roi {
            pixels: &pixels,
            width: request.width,
            height: request.height,
        });
        serde_json::to_writer(
            &mut output,
            &Response {
                text: &result.text,
                scores: &result.scores,
                boxes: &result.boxes,
                status: status_name(result.status),
                method: &result.method,
                inference_ms: result.inference_ms,
                frame_ms: result.frame_ms,
                provider: provider_name(result.provider),
                device_id: result.device_id,
                fallback_reason: result.fallback_reason.as_deref(),
                model_version: &model_version,
            },
        )?;
        output.write_all(b"\n")?;
        output.flush()?;
    }
    Ok(())
}
