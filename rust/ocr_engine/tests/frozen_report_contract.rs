use pubg_ocr_engine::reports;
use serde_json::Value;
use std::{collections::BTreeMap, fs};
#[test]
fn serialized_csv_column_order_matches_actual_historical_reports() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../docs/baselines/issue66_cli_contract.json"
    ))
    .unwrap();
    let directory = std::env::temp_dir().join(format!(
        "pubg-report-schema-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&directory).unwrap();
    reports::write_records(&directory, "schema-test", &[], &BTreeMap::new()).unwrap();
    for (name, key) in [
        ("检测与裁剪记录.csv", "csv_required"),
        ("candidate_events.csv", "candidate_columns"),
    ] {
        let bytes = fs::read(directory.join(name)).unwrap();
        assert!(bytes.starts_with(b"\xef\xbb\xbf"));
        let mut reader = csv::Reader::from_reader(&bytes[3..]);
        let actual: Vec<_> = reader
            .headers()
            .unwrap()
            .iter()
            .map(str::to_owned)
            .collect();
        let expected: Vec<_> = fixture["report_schema"][key]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect();
        assert_eq!(actual, expected, "{name}");
    }
    fs::remove_dir_all(directory).unwrap();
}
