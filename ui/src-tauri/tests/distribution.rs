use highlight_shell::distribution::{Distribution, RuntimeVariant};
use std::fs;

#[test]
fn installer_probe_clears_all_loader_environment_overrides_before_runtime_discovery() {
    // This integration-test executable has no other environment-mutating tests.
    // Missing Fixed resources stop before the native loader: no runtime or
    // registry mutation is needed to observe the public probe's environment.
    let keys = [
        "WEBVIEW2_BROWSER_EXECUTABLE_FOLDER",
        "WEBVIEW2_USER_DATA_FOLDER",
        "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
        "WEBVIEW2_CHANNEL_SEARCH_KIND",
        "WEBVIEW2_RELEASE_CHANNELS",
        "WEBVIEW2_WAIT_FOR_SCRIPT_DEBUGGER",
        "WEBVIEW2_PIPE_FOR_SCRIPT_DEBUGGER",
    ];
    struct RestoreEnvironment(Vec<(&'static str, Option<std::ffi::OsString>)>);
    impl Drop for RestoreEnvironment {
        fn drop(&mut self) {
            for (key, value) in &self.0 {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }
    let _restore = RestoreEnvironment(
        keys.iter()
            .map(|key| (*key, std::env::var_os(key)))
            .collect(),
    );
    for key in keys {
        std::env::set_var(key, "untrusted-override");
    }
    let d = tempfile::tempdir().unwrap();
    fs::write(
        d.path().join("distribution.json"),
        r#"{"variant":"fixed","minimum_webview2_major":120}"#,
    )
    .unwrap();
    assert!(highlight_shell::distribution::check_runtime(d.path(), "fixed").is_err());
    for key in keys {
        let expected = match key {
            "WEBVIEW2_CHANNEL_SEARCH_KIND" | "WEBVIEW2_RELEASE_CHANNELS" => Some("0"),
            _ => None,
        };
        assert_eq!(std::env::var(key).ok().as_deref(), expected, "{key}");
    }
    assert!(!d.path().join("data").exists());
}

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
