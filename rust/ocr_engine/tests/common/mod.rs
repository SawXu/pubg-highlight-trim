use serde_json::Value;
use sha2::{Digest, Sha256};
/// Same identity-substring contract as the original real-ROI tests, without
/// committing another copy of a real player identifier into test source.
pub fn contains_identity(text: &str, reference: &Value) -> bool {
    let Some(length) = reference["identity_chars"].as_u64().filter(|n| *n > 0) else {
        return false;
    };
    let Some(expected) = reference["identity_sha256"].as_str() else {
        return false;
    };
    let text: Vec<char> = text.to_lowercase().chars().collect();
    text.windows(length as usize).any(|window| {
        let candidate: String = window.iter().collect();
        format!("{:x}", Sha256::digest(candidate.as_bytes())) == expected
    })
}
