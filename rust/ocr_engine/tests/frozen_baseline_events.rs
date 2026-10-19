use pubg_ocr_engine::events::{extract_text_events, language_profile, Target, TextEvent};
use serde::Deserialize;
#[derive(Deserialize)]
struct Corpus {
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    language: String,
    target: String,
    text: String,
    expected: Vec<TextEvent>,
}
#[test]
fn baseline_text_cases_replay_exact_detached_python_outputs() {
    let corpus: Corpus =
        serde_json::from_str(include_str!("../../../docs/baselines/issue66_events.json")).unwrap();
    let mut differences = Vec::new();
    for case in corpus.cases {
        let target = match case.target.as_str() {
            "both" => Target::Both,
            "own-kill" => Target::OwnKill,
            "self-death" => Target::SelfDeath,
            _ => panic!("unsupported baseline target"),
        };
        let language = language_profile(&case.language).unwrap();
        // This oracle records the old flat-text parser contract. Visual OCR row
        // boundaries intentionally have new semantics since #73; their behavior
        // is covered by events_contract/scanner and the real frame-008 test.
        let flat = pubg_ocr_engine::events::normalize_text(&case.text);
        let actual = extract_text_events(&flat, target, Some(&language));
        if actual != case.expected {
            differences.push(format!(
                "{}: actual={actual:?}; expected={:?}",
                case.name, case.expected
            ));
        }
    }
    assert!(differences.is_empty(), "{}", differences.join("\n"));
}
