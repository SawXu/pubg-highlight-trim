use pubg_highlight_trim_core::{storage::Storage, RunOptions};

#[test]
fn old_settings_default_to_sdr_and_hdr_opt_in_round_trips_to_cli() {
    let root = std::env::temp_dir().join(format!("pubg-hdr-options-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let storage = Storage::open(root.clone()).unwrap();
    let source = root.join("source.mp4");
    std::fs::write(&source, []).unwrap();
    std::fs::write(root.join("settings.json"), r#"{"merge":false}"#).unwrap();
    let mut options = storage.load().unwrap();
    assert!(!options.preserve_hdr);
    options.input_paths = vec![source.to_string_lossy().into_owned()];
    assert!(!options
        .arguments()
        .unwrap()
        .iter()
        .any(|a| a == "--preserve-hdr"));
    options.preserve_hdr = true;
    storage.save(&options).unwrap();
    assert_eq!(storage.load().unwrap(), options);
    assert!(storage
        .load()
        .unwrap()
        .arguments()
        .unwrap()
        .iter()
        .any(|a| a == "--preserve-hdr"));
    assert!(options.preview().unwrap().contains("--preserve-hdr"));
    assert!(!RunOptions::default().preserve_hdr);
    std::fs::remove_dir_all(root).unwrap();
}
