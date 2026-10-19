use pubg_ocr_engine::events::{extract_text_events, language_profile, Target};
use std::time::Instant;
fn main() {
    let profile = language_profile("en").unwrap();
    let input = "OpponentDelta KILLS YOU with MP5K";
    let start = Instant::now();
    let mut count = 0usize;
    for _ in 0..1000 {
        count += extract_text_events(input, Target::Both, Some(&profile)).len();
    }
    println!("1000 parses: {:?}, events={count}", start.elapsed());
}
