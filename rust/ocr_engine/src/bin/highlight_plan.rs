//! Offline inspection of the same structured clip planner used by the application.
//! This diagnostic has no OCR/model dependency and never performs media/file writes.
#![cfg(feature = "highlight-cli")]
use std::io::{self, BufReader};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    pubg_ocr_engine::planning::run_jsonl(BufReader::new(io::stdin().lock()), io::stdout().lock())?;
    Ok(())
}
