use serde_json::Value;
use std::{collections::BTreeSet, path::Path};

fn rows(input: &str) -> Vec<Value> {
    input
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn pinned_manifest_and_text_baseline_keep_languages_events_and_sources() {
    let manifest: Value =
        serde_json::from_str(include_str!("../models/pp_ocrv6_medium.json")).unwrap();
    assert_eq!(
        manifest["contract"]["languages"],
        serde_json::json!(["zh-Hans", "zh-Hant", "en"])
    );
    for role in ["detector", "recognizer", "dictionary"] {
        let asset = &manifest[role];
        assert!(asset["source"].as_str().unwrap().starts_with("https://"));
        assert!(asset["source"]
            .as_str()
            .unwrap()
            .contains(asset["revision"].as_str().unwrap()));
        let hash = asset["sha256"].as_str().unwrap();
        assert_eq!(hash.len(), 64);
        assert!(hash.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }
    let baseline = rows(include_str!("../fixtures/pubg_ocr_baseline.jsonl"));
    let languages: BTreeSet<_> = baseline
        .iter()
        .map(|row| row["language"].as_str().unwrap())
        .collect();
    assert_eq!(languages, BTreeSet::from(["zh-Hans", "zh-Hant", "en"]));
    let actions: BTreeSet<_> = baseline
        .iter()
        .map(|row| row["action"].as_str().unwrap())
        .collect();
    assert!(actions.is_superset(&BTreeSet::from(["knock", "eliminate", "ignored"])));
}

#[test]
fn public_real_roi_metadata_matches_images_and_semantic_coverage() {
    let baseline = rows(include_str!("../fixtures/real/baseline.jsonl"));
    let manifest: Value =
        serde_json::from_str(include_str!("../fixtures/real/manifest.json")).unwrap();
    assert_eq!(baseline.len(), 18);
    assert_eq!(manifest["count"].as_u64().unwrap(), baseline.len() as u64);
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/real");
    let mut ids = BTreeSet::new();
    for row in &baseline {
        assert!(ids.insert(row["frame_id"].as_u64().unwrap()));
        assert!(directory.join(row["image"].as_str().unwrap()).is_file());
        assert!(!row["source"].as_str().unwrap().is_empty());
        assert!(row["event_sec"].as_f64().unwrap() >= 0.0);
    }
    // Historical annotations remain for provenance, not as the actual event oracle.
    let reference: Value =
        serde_json::from_str(include_str!("../fixtures/real/reference.json")).unwrap();
    assert_eq!(
        reference["source_git_sha"],
        "a070076526f918b0ef7f1848994d69363d5c1291"
    );
    assert_eq!(manifest["reference"], "reference.json");
    let frames = reference["frames"].as_array().unwrap();
    assert_eq!(frames.len(), baseline.len());
    let mut reference_ids = BTreeSet::new();
    for frame in frames {
        assert!(reference_ids.insert(frame["frame_id"].as_u64().unwrap()));
        let row = baseline
            .iter()
            .find(|row| row["frame_id"] == frame["frame_id"])
            .unwrap();
        let image = std::fs::read(directory.join(row["image"].as_str().unwrap())).unwrap();
        use sha2::{Digest, Sha256};
        assert_eq!(
            frame["image_sha256"],
            format!("{:x}", Sha256::digest(&image))
        );
        assert!(frame["event_signatures"].as_array().is_some());
        assert_eq!(frame["normalized_ocr_sha256"].as_str().unwrap().len(), 64);
    }
    assert_eq!(reference_ids, ids);
    for (language, expected) in [("zh-Hans", 8), ("zh-Hant", 4), ("en", 6)] {
        let count = frames
            .iter()
            .filter(|frame| frame["language"] == language)
            .count();
        assert_eq!(count, expected);
        assert_eq!(
            manifest["languages"][language].as_u64().unwrap(),
            count as u64
        );
        for target in ["self-death", "own-kill"] {
            assert!(frames.iter().any(|frame| frame["language"] == language
                && frame["event_signatures"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|event| event["target"] == target)));
        }
    }
    for id in [10, 12] {
        let frame = frames.iter().find(|frame| frame["frame_id"] == id).unwrap();
        assert_eq!(
            frame["language"], "zh-Hans",
            "audited image language, not its stale annotation"
        );
    }
    let actions: BTreeSet<_> = frames
        .iter()
        .flat_map(|frame| frame["event_signatures"].as_array().unwrap())
        .map(|event| event["action"].as_str().unwrap())
        .collect();
    assert!(actions.is_superset(&BTreeSet::from(["knock", "eliminate"])));
}
