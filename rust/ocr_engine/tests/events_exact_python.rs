// Literal full-field outputs from detached Python a070076 ocr.py and language
// tables; no OCR runtime or private media. The external deterministic corpus
// additionally replays 17,304 generated texts through this same public seam.
use pubg_ocr_engine::events::*;
use serde::Deserialize;

#[derive(Deserialize)]
struct Case {
    language: String,
    target: String,
    text: String,
    events: Vec<TextEvent>,
    allow_assist_events: Vec<TextEvent>,
    self_method: Option<String>,
    candidate: bool,
    assist: bool,
    delayed: bool,
}

#[test]
fn exact_legacy_molotov_case_and_unicode_aliases() {
    for (language, weapon, expected) in [
        ("en", "FİRE-BOMB", true),
        ("en", "FIRE BOMB", true),
        ("zh-Hans", "MOLOTOV", false),
        ("zh-Hans", "molotov", true),
        ("zh-Hant", "MOLOTOV", false),
    ] {
        let profile = language_profile(language).unwrap();
        assert_eq!(
            is_molotov_weapon(weapon, Some(&profile)),
            expected,
            "{language} {weapon}"
        );
    }
}

#[test]
fn exact_detached_python_event_contract() {
    let source = std::env::var("EVENT_DIFFERENTIAL_CORPUS")
        .map(|path| std::fs::read_to_string(path).unwrap())
        .unwrap_or_else(|_| include_str!("../fixtures/events_exact_python.json").to_owned());
    let cases: Vec<Case> = serde_json::from_str(&source).unwrap();
    let mut failures = Vec::new();
    for case in &cases {
        let p = language_profile(&case.language).unwrap();
        let target = match case.target.as_str() {
            "both" => Target::Both,
            "own-kill" => Target::OwnKill,
            "self-death" => Target::SelfDeath,
            _ => panic!("unknown target"),
        };
        let actual = extract_text_events(&case.text, target, Some(&p));
        let allowed = extract_own_kill_events(&case.text, true, Some(&p));
        let flags = (
            classify_self_text(&case.text, Some(&p)),
            has_own_kill_candidate(&case.text, Some(&p)),
            is_assist_own_kill_text(&case.text, Some(&p)),
            is_delayed_own_elim_text(&case.text, Some(&p)),
        );
        let expected_flags = (
            case.self_method.as_deref(),
            case.candidate,
            case.assist,
            case.delayed,
        );
        if actual != case.events || allowed != case.allow_assist_events || flags != expected_flags {
            failures.push(format!("{} {:?}: actual={actual:?}, allowed={allowed:?}, flags={flags:?}; expected={:?}, allowed={:?}, flags={expected_flags:?}", case.language, case.text, case.events, case.allow_assist_events));
        }
    }
    assert!(
        failures.is_empty(),
        "{} / {} differences:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}
