use clap::Parser;
use pubg_ocr_engine::config::Config;
#[test]
fn repeated_files_option_keeps_the_last_list_like_argparse_store() {
    // Captured from a070076 build_parser, whose --files uses store, not append.
    let config = Config::try_parse_from([
        "pubg-highlight-trim",
        "--files",
        "first.mp4",
        "another.mp4",
        "--files",
        "last.mp4",
    ])
    .unwrap();
    assert_eq!(config.files, vec![std::path::PathBuf::from("last.mp4")]);
}
