use pubg_ocr_engine::persistent_cache::{cache_key, read, write};
use serde_json::json;
use std::{fs, path::PathBuf};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "pubg-cache-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn cache_identity_tracks_contents_configuration_and_models_not_filename() {
    let temp = Temp::new();
    let source = temp.0.join("原片.mp4");
    let duplicate = temp.0.join("copy.mp4");
    fs::write(&source, b"first media").unwrap();
    fs::write(&duplicate, b"first media").unwrap();
    let config = json!({"target":"both", "scan_step":4.0});
    let key = cache_key(&source, &config, "model-hash-v1").unwrap();
    assert_eq!(
        key,
        cache_key(&duplicate, &config, "model-hash-v1").unwrap()
    );
    fs::write(&source, b"other media").unwrap();
    assert_ne!(key, cache_key(&source, &config, "model-hash-v1").unwrap());
    assert_ne!(
        key,
        cache_key(&duplicate, &json!({"target":"own-kill"}), "model-hash-v1").unwrap()
    );
    assert_ne!(
        key,
        cache_key(&duplicate, &config, "model-hash-v2").unwrap()
    );
    let candidates_a = json!({"config":config,"candidate_times":[1.0,2.0]});
    let candidates_b = json!({"config":config,"candidate_times":[1.0,3.0]});
    assert_ne!(
        cache_key(&duplicate, &candidates_a, "model-hash-v1").unwrap(),
        cache_key(&duplicate, &candidates_b, "model-hash-v1").unwrap(),
        "candidate CSV changes must invalidate cached detections"
    );
}

#[test]
fn cache_commits_complete_results_and_corruption_is_a_miss() {
    let temp = Temp::new();
    let dir = temp.0.join("cache");
    let key = "a".repeat(64);
    let payload = json!({"events":[{"time":1.25}], "duration":30.0});
    assert!(read::<serde_json::Value>(&dir, &key).is_none());
    write(&dir, &key, &payload).unwrap();
    assert_eq!(read::<serde_json::Value>(&dir, &key), Some(payload));
    let path = dir.join(format!("{key}.json"));
    let mut envelope: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    envelope["payload"]["duration"] = json!(99);
    fs::write(&path, serde_json::to_vec(&envelope).unwrap()).unwrap();
    assert!(read::<serde_json::Value>(&dir, &key).is_none());
    fs::write(&path, b"{partial").unwrap();
    assert!(read::<serde_json::Value>(&dir, &key).is_none());
    write(&dir, &key, &json!([])).unwrap();
    assert_eq!(read::<serde_json::Value>(&dir, &key), Some(json!([])));
    assert_eq!(
        fs::read_dir(&dir).unwrap().count(),
        1,
        "no incomplete temporary files"
    );
}

#[test]
fn cache_rejects_foreign_envelopes_and_traversal_without_writing_outside_cache() {
    let temp = Temp::new();
    let dir = temp.0.join("cache");
    assert!(write(&dir, "../source", &json!({})).is_err());
    assert!(!dir.exists());
    let key = "b".repeat(64);
    write(&dir, &key, &json!({})).unwrap();
    let path = dir.join(format!("{key}.json"));
    let good = fs::read(&path).unwrap();
    for field in ["schema", "key"] {
        let mut envelope: serde_json::Value = serde_json::from_slice(&good).unwrap();
        envelope[field] = json!("wrong");
        fs::write(&path, serde_json::to_vec(&envelope).unwrap()).unwrap();
        assert!(read::<serde_json::Value>(&dir, &key).is_none());
    }
}
