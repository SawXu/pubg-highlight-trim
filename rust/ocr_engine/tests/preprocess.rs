#[cfg(feature = "onnx")]
mod common;
use pubg_ocr_engine::frame::{brightness_gate, crop_for_ocr, BgrFrame, GateMode, Interpolation};
use serde_json::Value;
use sha2::{Digest, Sha256};
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/preprocess.json")).unwrap()
}
fn bytes(v: &Value) -> Vec<u8> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_u64().unwrap() as u8)
        .collect()
}
#[cfg(feature = "onnx")]
#[test]
#[ignore = "requires PUBG_OCR_MODEL_DIR and native ONNX runtime"]
fn real_roi_preprocessing_keeps_recognized_subjects() {
    use pubg_ocr_engine::{PpOcrRecognizer, Recognizer};
    let dir = std::path::PathBuf::from(
        std::env::var_os("PUBG_OCR_MODEL_DIR").expect("PUBG_OCR_MODEL_DIR"),
    );
    let recognizer = PpOcrRecognizer::from_paths(
        &dir.join("det.onnx"),
        &dir.join("rec.onnx"),
        &dir.join("dict.txt"),
        1,
    )
    .unwrap();
    let cases: Value =
        serde_json::from_str(include_str!("../fixtures/missed_events.json")).unwrap();
    for case in cases["frames"].as_array().unwrap() {
        let name = case["image"].as_str().unwrap();
        let raw = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/missed_event_ocr")
                .join(name),
        )
        .unwrap();
        let rgb = image::load_from_memory(&raw).unwrap().to_rgb8();
        let frame = BgrFrame::from_rgb(rgb.width(), rgb.height(), rgb.into_raw()).unwrap();
        let crop = crop_for_ocr(&frame, [0., 0., 1., 1.], 768).unwrap();
        let (text, _) = recognizer
            .recognize(&crop.to_rgb(), crop.width(), crop.height())
            .unwrap();
        use pubg_ocr_engine::events::{extract_text_events, language_profile, Target};
        let events = extract_text_events(
            &text,
            Target::Both,
            Some(&language_profile("zh-Hant").unwrap()),
        );
        assert_eq!(events.len(), 1, "{name}: expected one event");
        assert_eq!(events[0].target, case["target"].as_str().unwrap());
        assert_eq!(events[0].action, case["action"].as_str().unwrap());
        assert!(
            common::contains_identity(&events[0].key, case),
            "{name}: recognized key identity changed"
        );
        assert!(
            common::contains_identity(&events[0].subject, case),
            "{name}: recognized subject identity changed"
        );
    }
}
#[test]
fn color_grid_matches_all_frozen_opencv_color_conversions() {
    let f = fixture();
    let mut pixels = Vec::new();
    for g in 0..256 {
        for b in 0..256 {
            pixels.extend([b as u8, g as u8, ((b * 13 + g * 71) % 256) as u8]);
        }
    }
    let frame = BgrFrame::new(256, 256, pixels).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(frame.to_gray())),
        f["color_grid"]["gray"].as_str().unwrap()
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(frame.to_hsv())),
        f["color_grid"]["hsv"].as_str().unwrap()
    );
}
#[test]
fn full_frame_fractional_rois_match_production_768_area_inputs() {
    let f = fixture();
    let raw = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/missed_event_ocr/23.04.41.02_32.png"),
    )
    .unwrap();
    let rgb = image::load_from_memory(&raw).unwrap().to_rgb8();
    let roi = BgrFrame::from_rgb(rgb.width(), rgb.height(), rgb.into_raw()).unwrap();
    let mut pixels = Vec::with_capacity(2560 * 1440 * 3);
    for y in 0..1440 {
        for x in 0..2560 {
            let offset =
                ((y % roi.height() as usize) * roi.width() as usize + x % roi.width() as usize) * 3;
            pixels.extend_from_slice(&roi.pixels()[offset..offset + 3]);
        }
    }
    let frame = BgrFrame::new(2560, 1440, pixels).unwrap();
    for expected in f["canvas"]["crop"].as_array().unwrap() {
        let roi = std::array::from_fn(|i| expected["roi"][i].as_f64().unwrap());
        let crop = crop_for_ocr(&frame, roi, 768).unwrap();
        assert_eq!(
            (crop.width(), crop.height()),
            (
                expected["width"].as_u64().unwrap() as u32,
                expected["height"].as_u64().unwrap() as u32
            )
        );
        assert_eq!(
            format!("{:x}", Sha256::digest(crop.pixels())),
            expected["sha256"].as_str().unwrap()
        );
    }
    for expected in f["canvas"]["gates"].as_array().unwrap() {
        let mode = if expected["mode"] == "light" {
            GateMode::Light
        } else {
            GateMode::Full
        };
        let result = brightness_gate(&frame, [0.26, 0.635, 0.74, 0.725], 768, mode).unwrap();
        assert_eq!(result.reason, expected["reason"].as_str().unwrap());
        assert_eq!(result.passed, expected["passed"].as_bool().unwrap());
    }
}
#[test]
fn gate_reasons_cover_flat_hud_noise_and_text_structure() {
    let f = fixture();
    for expected in f["synthetic_gates"].as_array().unwrap() {
        let name = expected["name"].as_str().unwrap();
        let frame = match name {
            "black" => BgrFrame::new(768, 100, vec![0; 768 * 100 * 3]).unwrap(),
            "white" => BgrFrame::new(768, 100, vec![255; 768 * 100 * 3]).unwrap(),
            "bar" => {
                let mut p = vec![0; 768 * 100 * 3];
                p[30 * 768 * 3..40 * 768 * 3].fill(255);
                BgrFrame::new(768, 100, p).unwrap()
            }
            _ => {
                let rgb = image::load_from_memory(
                    &std::fs::read(
                        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                            .join(format!("tests/fixtures/gate_{name}.png")),
                    )
                    .unwrap(),
                )
                .unwrap()
                .to_rgb8();
                BgrFrame::from_rgb(rgb.width(), rgb.height(), rgb.into_raw()).unwrap()
            }
        };
        let mode = if expected["mode"] == "light" {
            GateMode::Light
        } else {
            GateMode::Full
        };
        let result = brightness_gate(&frame, [0., 0., 1., 1.], 768, mode).unwrap();
        assert_eq!(
            result.reason,
            expected["reason"].as_str().unwrap(),
            "{name} {}",
            expected["mode"]
        );
        assert_eq!(result.passed, expected["passed"].as_bool().unwrap());
    }
}
#[test]
fn integer_area_noninteger_area_and_linear_edge_shapes_match_opencv() {
    let f = fixture();
    for expected in f["resizes"].as_array().unwrap() {
        let sw = expected["source"][0].as_u64().unwrap() as u32;
        let sh = expected["source"][1].as_u64().unwrap() as u32;
        let dw = expected["dest"][0].as_u64().unwrap() as u32;
        let dh = expected["dest"][1].as_u64().unwrap() as u32;
        let pixels = (0..sw * sh * 3)
            .map(|i| ((i * 37 + i / 7 * 13) % 256) as u8)
            .collect();
        let frame = BgrFrame::new(sw, sh, pixels).unwrap();
        let mode = if expected["mode"] == "area" {
            Interpolation::Area
        } else {
            Interpolation::Linear
        };
        let resized = frame.resize(dw, dh, mode).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(resized.pixels())),
            expected["sha256"].as_str().unwrap(),
            "{sw}x{sh} -> {dw}x{dh} {}",
            expected["mode"]
        );
    }
}
#[test]
fn invalid_or_empty_rois_do_not_panic_or_run_ocr() {
    let frame = BgrFrame::new(1, 1, vec![0, 0, 0]).unwrap();
    assert!(crop_for_ocr(&frame, [0., 0., f64::NAN, 1.], 768).is_err());
    assert!(crop_for_ocr(&frame, [0., 0., 0.1, 0.1], 768).is_err());
    let gate = brightness_gate(&frame, [0., 0., 0.1, 0.1], 768, GateMode::Full).unwrap();
    assert!(gate.passed);
    assert_eq!(gate.reason, "empty-roi");
}
#[test]
fn fixed_pixels_preserve_bgr_rgb_and_opencv_area_crop() {
    let f = fixture();
    let frame = BgrFrame::new(13, 7, bytes(&f["fixed"]["bgr"])).unwrap();
    assert_eq!(&frame.to_rgb()[..3], &[74, 37, 0]);
    let crop = crop_for_ocr(&frame, [0., 0., 1., 1.], 5).unwrap();
    assert_eq!((crop.width(), crop.height()), (5, 2)); // OCR truncates, not rounds, height.
    let resized = frame.resize(5, 3, Interpolation::Area).unwrap();
    assert_eq!(resized.pixels(), bytes(&f["fixed"]["area"]));
}
#[test]
fn fixed_pixels_preserve_opencv_linear_gray_and_hsv() {
    let f = fixture();
    let frame = BgrFrame::new(13, 7, bytes(&f["fixed"]["bgr"])).unwrap();
    assert_eq!(
        frame
            .resize(19, 11, Interpolation::Linear)
            .unwrap()
            .pixels(),
        bytes(&f["fixed"]["linear"])
    );
    assert_eq!(frame.to_gray(), bytes(&f["fixed"]["gray"]));
    assert_eq!(frame.to_hsv(), bytes(&f["fixed"]["hsv"]));
}
#[test]
fn public_real_rois_match_opencv_ocr_input_bytes_and_gate_decisions() {
    let f = fixture();
    for row in f["real"].as_array().unwrap() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/missed_event_ocr")
            .join(row["name"].as_str().unwrap());
        let raw = std::fs::read(path).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(&raw)),
            row["sha256"].as_str().unwrap()
        );
        let rgb = image::load_from_memory(&raw).unwrap().to_rgb8();
        let frame = BgrFrame::from_rgb(rgb.width(), rgb.height(), rgb.into_raw()).unwrap();
        for expected in row["crops"].as_array().unwrap() {
            let crop = crop_for_ocr(
                &frame,
                [0., 0., 1., 1.],
                expected["width"].as_u64().unwrap() as u32,
            )
            .unwrap();
            assert_eq!(crop.height(), expected["height"].as_u64().unwrap() as u32);
            assert_eq!(
                format!("{:x}", Sha256::digest(crop.pixels())),
                expected["bgr_sha256"].as_str().unwrap(),
                "{} crop {}",
                row["name"],
                expected["width"]
            );
        }
        for expected in row["gates"].as_array().unwrap() {
            let mode = match expected["mode"].as_str().unwrap() {
                "none" => GateMode::None,
                "light" => GateMode::Light,
                _ => GateMode::Full,
            };
            let gate = brightness_gate(
                &frame,
                [0., 0., 1., 1.],
                expected["width"].as_u64().unwrap() as u32,
                mode,
            )
            .unwrap();
            assert_eq!(
                gate.passed,
                expected["passed"].as_bool().unwrap(),
                "{} gate {} {}",
                row["name"],
                expected["mode"],
                expected["width"]
            );
            assert_eq!(gate.reason, expected["reason"].as_str().unwrap());
        }
    }
}
