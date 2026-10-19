use clap::Parser;
use pubg_ocr_engine::config::Config;
use std::process::Command;

#[test]
fn provider_cli_accepts_modes_aliases_case_and_defaults() {
    for mode in ["auto", "CPU", "DiReCtMl", "dml", "directml-only"] {
        let c = Config::try_parse_from(["trim", "--ocr-provider", mode, "--ocr-device-id", "3"])
            .expect("documented OCR options must parse");
        assert_eq!(c.ocr_device_id, 3);
    }
    let c = Config::try_parse_from(["trim"]).unwrap();
    assert_eq!(c.ocr_provider.code(), "cpu");
    assert_eq!(c.ocr_device_id, 0);
}

#[test]
fn invalid_provider_or_device_exits_two_without_summary() {
    for (flag, value) in [
        ("--ocr-provider", "cuda"),
        ("--ocr-provider", ""),
        ("--ocr-provider", " \t "),
        ("--ocr-device-id", "-1"),
        ("--ocr-device-id", "2147483648"),
        ("--ocr-device-id", "x"),
    ] {
        let o = Command::new(env!("CARGO_BIN_EXE_pubg_highlight_trim"))
            .args([flag, value])
            .output()
            .unwrap();
        assert_eq!(o.status.code(), Some(2));
        assert!(!String::from_utf8_lossy(&o.stdout).contains("SUMMARY "));
        let error = String::from_utf8_lossy(&o.stderr);
        assert!(
            error.contains(if flag == "--ocr-provider" {
                "auto, cpu, directml, directml-only"
            } else {
                "0..=2147483647"
            }),
            "{error}"
        );
    }
}

#[test]
fn empty_legacy_provider_environment_is_invalid_but_cli_can_override_it() {
    for value in ["", " \t "] {
        let command = || {
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_pubg_highlight_trim"));
            cmd.env("PUBG_OCR_PROVIDER", value)
                .env_remove("PUBG_OCR_DIRECTML_DEVICE_ID");
            cmd
        };
        let invalid = command().output().unwrap();
        assert_eq!(invalid.status.code(), Some(2));
        assert!(
            String::from_utf8_lossy(&invalid.stderr).contains("auto, cpu, directml, directml-only")
        );
        assert!(!String::from_utf8_lossy(&invalid.stdout).contains("SUMMARY "));
        let valid = command().args(["--ocr-provider", "cpu"]).output().unwrap();
        assert_eq!(valid.status.code(), Some(1)); // accepted syntax, missing input
    }
}

#[test]
fn cli_overrides_legacy_environment_and_invalid_environment_is_syntax_error() {
    let command = || {
        let mut c = Command::new(env!("CARGO_BIN_EXE_pubg_highlight_trim"));
        c.env("PUBG_OCR_PROVIDER", "cuda")
            .env("PUBG_OCR_DIRECTML_DEVICE_ID", "bad");
        c
    };
    let invalid = command().output().unwrap();
    assert_eq!(invalid.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("auto, cpu, directml, directml-only"));
    let valid = command()
        .args(["--ocr-provider", "cpu", "--ocr-device-id", "0"])
        .output()
        .unwrap();
    assert_eq!(valid.status.code(), Some(1)); // syntax accepted, no input selected
    assert!(!String::from_utf8_lossy(&valid.stdout).contains("SUMMARY "));
}
