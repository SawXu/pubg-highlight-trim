use pubg_ocr_engine::events::{
    same_followup_subject, same_noisy_close_event, same_text_event, TextEvent,
};
use serde::Deserialize;
#[derive(Deserialize)]
struct Pair {
    left: TextEvent,
    right: TextEvent,
    same: bool,
    noisy: bool,
    followup: bool,
}
#[test]
fn identity_matches_python_sequence_matcher_and_unicode() {
    let source = std::env::var("EVENT_IDENTITY_CORPUS")
        .map(|p| std::fs::read_to_string(p).unwrap())
        .unwrap_or_else(|_| include_str!("../fixtures/events_identity_python.json").into());
    let pairs: Vec<Pair> = serde_json::from_str(&source).unwrap();
    let mut failures = Vec::new();
    for p in pairs {
        let actual = (
            same_text_event(&p.left, &p.right),
            same_noisy_close_event(&p.left, &p.right),
            same_followup_subject(&p.left, &p.right, None),
        );
        if actual != (p.same, p.noisy, p.followup) {
            failures.push(format!(
                "{:?} {:?}: {actual:?} != {:?}",
                p.left,
                p.right,
                (p.same, p.noisy, p.followup)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
