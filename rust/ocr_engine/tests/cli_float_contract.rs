//! Frozen against the retained Python cli.py/build_parser, not the old Rust probe scaffold.
use clap::Parser;
use pubg_ocr_engine::config::Config;
fn parse(option: &str, value: &str) -> Result<Config, clap::Error> {
    Config::try_parse_from(["app", "video.mp4", &format!("{option}={value}")])
}
#[test]
fn python_float_syntax_and_parser_stage_nonfinite_acceptance_are_preserved() {
    for option in [
        "--before",
        "--after",
        "--min-event-sec",
        "--molotov-elim-before",
        "--scan-start",
        "--scan-end",
        "--coarse-step",
        "--candidate-lookback",
        "--candidate-lookahead",
        "--candidate-step",
        "--refine-before",
        "--refine-after",
        "--refine-step",
        "--adaptive-step",
        "--adaptive-window",
        "--ocr-min-interval",
        "--refine-max-seconds",
        "--assist-max-seconds",
    ] {
        for value in ["nan", "inf", " 1.25 ", "1_000.5", "+Infinity", "  NaN  "] {
            assert!(parse(option, value).is_ok(), "{option}={value}");
        }
        assert!(parse(option, "1__0").is_err(), "{option}");
        let nonnegative = [
            "--adaptive-step",
            "--adaptive-window",
            "--ocr-min-interval",
            "--refine-max-seconds",
            "--assist-max-seconds",
        ]
        .contains(&option);
        assert_eq!(parse(option, "-1").is_err(), nonnegative, "{option}");
        assert_eq!(parse(option, "-inf").is_err(), nonnegative, "{option}");
    }
    for value in ["nan:2", "1:nan", "1:inf", "1:2_0"] {
        assert!(parse("--priority-window", value).is_ok(), "{value}")
    }
    assert!(parse("--priority-window", "-inf:2").is_err());
    assert!(parse("--priority-window", "inf:2").is_err());
    for value in ["nan,0,1,1", "0,0,inf,1"] {
        assert!(parse("--roi", value).is_err())
    }
}
