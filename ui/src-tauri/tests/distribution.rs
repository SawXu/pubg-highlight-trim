use highlight_shell::distribution::{Distribution, RuntimeVariant};
use std::fs;

#[test]
fn installed_system_variant_keeps_data_at_install_root_and_requires_compatible_runtime() {
    let d = tempfile::tempdir().unwrap();
    fs::write(
        d.path().join("distribution.json"),
        r#"{"variant":"system","minimum_webview2_major":120}"#,
    )
    .unwrap();
    let policy = Distribution::from_root(d.path()).unwrap().unwrap();
    assert_eq!(policy.variant, RuntimeVariant::System);
    assert_eq!(policy.data_directory(d.path()), d.path().join("data"));
    assert!(policy
        .check_version("119.0.0.0")
        .unwrap_err()
        .contains("内置版"));
    assert!(policy.check_version("120.0.2210.61").is_ok());
    assert!(policy.browser_directory(d.path()).unwrap().is_none());
    assert!(policy.check_version("150.0.0.0 dev").is_err());
}

#[test]
fn fixed_variant_never_falls_back_and_invalid_install_policy_is_not_development() {
    let d = tempfile::tempdir().unwrap();
    assert!(Distribution::from_root(d.path()).unwrap().is_none());
    fs::create_dir(d.path().join(".installer")).unwrap();
    assert!(Distribution::from_root(d.path()).is_err());
    fs::remove_dir(d.path().join(".installer")).unwrap();
    fs::write(d.path().join("distribution.json"), "{}").unwrap();
    assert!(Distribution::from_root(d.path()).is_err());
    fs::write(
        d.path().join("distribution.json"),
        r#"{"variant":"fixed","minimum_webview2_major":120}"#,
    )
    .unwrap();
    let policy = Distribution::from_root(d.path()).unwrap().unwrap();
    assert!(policy
        .browser_directory(d.path())
        .unwrap_err()
        .contains("拒绝使用系统运行时"));
    let folder = d.path().join("WebView2Runtime");
    fs::create_dir(&folder).unwrap();
    for name in [
        "msedgewebview2.exe",
        "msedge.dll",
        "icudtl.dat",
        "resources.pak",
    ] {
        fs::write(folder.join(name), "fixture").unwrap();
    }
    assert_eq!(policy.browser_directory(d.path()).unwrap(), Some(folder));
    assert!(policy.check_version("invalid").is_err());
    let udf = d.path().join("data/webview");
    fs::create_dir_all(&udf).unwrap();
    let browser = d.path().join("WebView2Runtime/msedgewebview2.exe");
    assert!(policy.verify_loaded_paths(d.path(), &browser, &udf).is_ok());
    let other = d.path().join("shared-msedgewebview2.exe");
    fs::write(&other, "fixture").unwrap();
    assert!(policy
        .verify_loaded_paths(d.path(), &other, &udf)
        .unwrap_err()
        .contains("拒绝回退"));
    assert!(policy
        .verify_loaded_paths(d.path(), &browser, d.path())
        .unwrap_err()
        .contains("数据路径"));
}
