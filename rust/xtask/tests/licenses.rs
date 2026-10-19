use serde_json::Value;
use std::{fs, path::Path, process::Command};
fn package(root: &Path, name: &str, extra: &str, license: bool) {
    let d = root.join(name);
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/lib.rs"), "pub fn value() {}\n").unwrap();
    fs::write(
        d.join("Cargo.toml"),
        format!(
            "[package]\nname=\"{name}\"\nversion=\"1.0.0\"\nedition=\"2021\"\n{}\n{extra}",
            if license { "license=\"MIT\"" } else { "" }
        ),
    )
    .unwrap();
    fs::write(
        d.join("LICENSE"),
        format!("Copyright {name}\nMIT permission fixture"),
    )
    .unwrap();
}
#[test]
fn licenses_inventory_separates_build_runtime_and_excludes_dev_and_xtask() {
    let d = std::env::temp_dir().join(format!("xtask-licenses-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("Cargo.toml"),"[workspace]\nmembers=[\"pubg-ocr-engine\",\"normal\",\"macros\",\"builder\",\"devonly\",\"toolonly\",\"tool\"]\nresolver=\"2\"\n").unwrap();
    package(&d,"pubg-ocr-engine","[features]\ncli=[]\ndirectml=[]\n[dependencies]\nnormal={path=\"../normal\"}\nmacros={path=\"../macros\"}\n[build-dependencies]\nbuilder={path=\"../builder\"}\n[dev-dependencies]\ndevonly={path=\"../devonly\"}",true);
    package(&d, "normal", "", true);
    package(&d, "macros", "[lib]\nproc-macro=true", true);
    package(&d, "builder", "", true);
    package(&d, "devonly", "", false);
    package(&d, "toolonly", "", false);
    package(
        &d,
        "tool",
        "[dependencies]\ntoolonly={path=\"../toolonly\"}",
        true,
    );
    assert!(Command::new("cargo")
        .arg("generate-lockfile")
        .arg("--manifest-path")
        .arg(d.join("Cargo.toml"))
        .status()
        .unwrap()
        .success());
    let bundle = d.join("bundle");
    fs::create_dir_all(&bundle).unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_xtask"))
            .arg("licenses")
            .arg(&bundle)
            .arg("--manifest-path")
            .arg(d.join("Cargo.toml"))
            .output()
            .unwrap()
    };
    let out = run();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let inv = bundle.join("third_party/rust/INVENTORY.json");
    let first = fs::read(&inv).unwrap();
    let data: Value = serde_json::from_slice(&first).unwrap();
    let crates = data["packages"].as_array().unwrap();
    assert_eq!(crates.len(), 4);
    assert_eq!(
        crates.iter().find(|p| p["name"] == "macros").unwrap()["scope"],
        "build-only"
    );
    assert_eq!(
        crates.iter().find(|p| p["name"] == "normal").unwrap()["scope"],
        "shipped"
    );
    assert_eq!(
        crates.iter().find(|p| p["name"] == "builder").unwrap()["scope"],
        "build-only"
    );
    assert!(bundle
        .join("third_party/rust/normal-1.0.0/LICENSE")
        .is_file());
    assert!(run().status.success());
    assert_eq!(first, fs::read(&inv).unwrap());
    let manifest = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("manifest")
        .arg(&bundle)
        .output()
        .unwrap();
    assert!(manifest.status.success());
    let supply: Value =
        serde_json::from_slice(&fs::read(bundle.join("SUPPLY_CHAIN_MANIFEST.json")).unwrap())
            .unwrap();
    for entry in data["notice_files"].as_array().unwrap() {
        assert!(supply["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["path"] == entry["path"] && f["sha256"] == entry["sha256"]));
    }
    package(&d, "normal", "", false);
    let failed = run();
    assert!(!failed.status.success());
    assert!(
        String::from_utf8_lossy(&failed.stderr).contains("normal 1.0.0: missing license metadata")
    );
    package(&d, "normal", "", true);
    fs::remove_file(d.join("normal/LICENSE")).unwrap();
    let failed = run();
    assert!(
        !failed.status.success(),
        "a distributed runtime dependency needs its actual license text, not only an SPDX tag"
    );
    assert!(String::from_utf8_lossy(&failed.stderr)
        .contains("normal 1.0.0: runtime license text missing"));
    fs::remove_dir_all(d).unwrap();
}
