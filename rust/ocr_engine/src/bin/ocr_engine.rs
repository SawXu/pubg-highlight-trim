#![cfg(feature = "cli")]

use base64::Engine;
use pubg_ocr_engine::{
    ModelManifest, OcrEngine, OcrEngineConfig, OcrStatus, PpOcrRecognizer, Provider, Roi,
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let model_dir = PathBuf::from(
        args.next()
            .ok_or("usage: ocr_engine <model-dir> [threads]")?,
    );
    let threads = args
        .next()
        .and_then(|value| value.to_str().and_then(|text| text.parse().ok()))
        .unwrap_or(1);
    let manifest = ModelManifest::from_json_file(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("models/pp_ocrv6_medium.json"),
    )?;
    let engine = match PpOcrRecognizer::from_model_dir(&model_dir, &manifest, threads).and_then(
        |recognizer| {
            OcrEngine::new(
                OcrEngineConfig {
                    provider: Provider::Cpu,
                    model_dir,
                    manifest,
                    allow_provider_fallback: false,
                },
                recognizer,
            )
            .map_err(|error| error.to_string())
        },
    ) {
        Ok(engine) => Some(engine),
        Err(error) => {
            eprintln!("OCR model unavailable: {error}");
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
                    method: "onnx-cpu",
                    inference_ms: 0.0,
                    frame_ms: 0.0,
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
            },
        )?;
        output.write_all(b"\n")?;
        output.flush()?;
    }
    Ok(())
}
