//! The production CLI contract comes from cli.py/pipeline.py, not the retired probe scaffold.
use clap::Parser;
use pubg_ocr_engine::config::Config;
use std::process::{Command, Output};
fn invoke(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pubg_highlight_trim"))
        .args(args)
        .output()
        .expect("start Rust application CLI")
}
fn config(args: &[&str]) -> serde_json::Value {
    serde_json::to_value(
        Config::try_parse_from(std::iter::once("pubg_highlight_trim").chain(args.iter().copied()))
            .unwrap(),
    )
    .unwrap()
}
#[test]
fn help_and_version_work_without_inputs_or_ocr_runtime() {
    for flag in ["--help", "-h", "--version", "-V"] {
        let o = invoke(&[flag]);
        assert!(o.status.success());
        assert!(o.stderr.is_empty());
        let text = String::from_utf8(o.stdout).unwrap();
        assert!(text.contains("pubg-highlight-trim"));
        if flag == "--help" || flag == "-h" {
            for arg in [
                "--files",
                "--output-dir",
                "--target",
                "--game-lang",
                "--before",
                "--after",
                "--dry-run",
                "--jobs",
                "--merge",
                "--verbose",
                "--profile",
                "--candidate-csv",
                "--split-merge-by-target",
                "--move-originals",
                "--roi",
                "--refine-max-seconds",
            ] {
                assert!(text.contains(arg), "missing {arg}")
            }
        } else {
            assert_eq!(text.trim(), "pubg-highlight-trim 0.3.5")
        }
    }
}
#[test]
fn invalid_arguments_exit_two_without_a_fabricated_run_summary() {
    for args in [
        vec!["video.mp4", "--unknown"],
        vec!["--files"],
        vec!["video.mp4", "--jobs", "0"],
        vec!["video.mp4", "--jobs", "-1"],
        vec!["video.mp4", "--jobs", "many"],
        vec!["video.mp4", "--target", "other"],
        vec!["video.mp4", "--game-lang", "fr"],
        vec!["video.mp4", "--after", "oops"],
        vec!["video.mp4", "--output-dir"],
        vec!["video.mp4", "--merge", "--no-merge"],
        vec!["one.mp4", "two.mp4"],
    ] {
        let o = invoke(&args);
        assert_eq!(o.status.code(), Some(2), "{args:?}");
        assert!(!o.stderr.is_empty());
        assert!(o.stdout.is_empty())
    }
}
#[test]
fn default_input_is_current_directory_and_runtime_errors_exit_one() {
    let o = invoke(&["a-definitely-missing-input.mp4"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stderr).starts_with("ERROR "));
    assert!(!String::from_utf8_lossy(&o.stdout).contains("SUMMARY "));
    assert!(config(&[])["input"].is_null());
}
#[test]
fn defaults_and_optional_merge_preserve_python_cli_semantics() {
    let c = config(&["video.mp4"]);
    assert_eq!(c["input"], "video.mp4");
    assert_eq!(c["files"], serde_json::json!([]));
    for (k, v) in [
        ("target", serde_json::json!("both")),
        ("game_lang", serde_json::json!("auto")),
        ("before", serde_json::json!(5.0)),
        ("after", serde_json::json!(1.0)),
        ("jobs", serde_json::json!("auto")),
        ("min_event_sec", serde_json::json!(2.0)),
        ("molotov_elim_before", serde_json::json!(10.0)),
        ("coarse_step", serde_json::json!(4.0)),
        ("refine_step", serde_json::json!(0.5)),
        ("roi", serde_json::json!([0.3, 0.66, 0.7, 0.75])),
    ] {
        assert_eq!(c[k], v, "{k}")
    }
    for k in [
        "dry_run",
        "verbose",
        "profile",
        "no_merge",
        "overwrite",
        "recursive",
        "fast_path",
        "no_fast_path",
        "move_originals",
    ] {
        assert_eq!(c[k], false)
    }
    assert!(c["output_dir"].is_null());
    assert!(c["merge"].is_null());
    assert_eq!(config(&["video.mp4", "--merge"])["merge"], true);
    assert_eq!(config(&["video.mp4", "--no-merge"])["no_merge"], true);
}
#[test]
fn removed_legacy_flags_are_rejected_and_never_advertised() {
    let help = String::from_utf8(invoke(&["--help"]).stdout).unwrap();
    for flag in [
        "--montage",
        "--final",
        "--detector",
        "--include-view-replays",
        "--allow-starts-downed",
        "--opening-check-start",
        "--opening-check-end",
        "--opening-check-fps",
        "--opening-red-threshold",
        "--full-scan",
        "--fast-scan",
        "--no-full-scan",
    ] {
        assert!(!help.contains(flag), "{flag}");
        let out = invoke(&["missing.mp4", flag]);
        assert_eq!(out.status.code(), Some(2), "{flag}");
        assert!(out.stdout.is_empty());
    }
}
#[test]
fn explicit_options_keep_unicode_paths_and_independent_diagnostics() {
    let c = config(&[
        "--files",
        "录像 二.mp4",
        "first.mp4",
        "-o",
        "输出 clips",
        "--target",
        "own-kill",
        "--game-lang",
        "zh-Hant",
        "--before",
        "0",
        "--after",
        "2.5",
        "--dry-run",
        "--jobs",
        "3",
        "--merge",
        "merged movie.mp4",
        "--profile",
    ]);
    assert_eq!(c["files"], serde_json::json!(["录像 二.mp4", "first.mp4"]));
    assert_eq!(c["output_dir"], "输出 clips");
    assert_eq!(c["target"], "own-kill");
    assert_eq!(c["game_lang"], "zh-Hant");
    assert_eq!(c["before"], 0.0);
    assert_eq!(c["after"], 2.5);
    assert_eq!(c["jobs"], "3");
    assert_eq!(c["merge"], "merged movie.mp4");
    assert_eq!(c["profile"], true);
    assert_eq!(c["verbose"], false);
    let c = config(&[
        "input",
        "--verbose",
        "--seconds-before",
        "3",
        "--seconds-after",
        "4",
        "--scan-only",
        "--jobs",
        "AUTO",
    ]);
    assert_eq!(c["verbose"], true);
    assert_eq!(c["profile"], false);
    assert_eq!(c["before"], 3.0);
    assert_eq!(c["after"], 4.0);
    assert_eq!(c["dry_run"], true);
    assert_eq!(c["jobs"], "auto");
    for lang in ["auto", "zh-Hans", "zh-Hant", "en"] {
        assert_eq!(config(&["input", "--game-lang", lang])["game_lang"], lang)
    }
    for t in ["self-death", "own-kill", "both"] {
        assert_eq!(config(&["input", "--target", t])["target"], t)
    }
}
