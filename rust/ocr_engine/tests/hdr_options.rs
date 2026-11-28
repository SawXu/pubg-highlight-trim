use clap::Parser;
use pubg_ocr_engine::config::Config;

#[test]
fn hdr_preservation_is_explicit_and_serialized() {
    let defaults = Config::try_parse_from(["pubg-highlight-trim"]).unwrap();
    assert!(!defaults.preserve_hdr);
    let opt_in = Config::try_parse_from(["pubg-highlight-trim", "--preserve-hdr"]).unwrap();
    assert!(opt_in.preserve_hdr);
    assert_eq!(serde_json::to_value(opt_in).unwrap()["preserve_hdr"], true);
}
