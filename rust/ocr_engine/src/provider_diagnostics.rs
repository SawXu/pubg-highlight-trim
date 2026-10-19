//! Shareable provider failure reasons retain actionable diagnostics, not local paths.
use std::path::Path;

pub fn redact(reason: &str, model_dir: &Path) -> String {
    let mut text = reason.to_owned();
    let mut roots = vec![model_dir.to_path_buf()];
    if let Ok(canonical) = model_dir.canonicalize() {
        roots.push(canonical);
    }
    for root in roots {
        let value = root.to_string_lossy();
        for variant in [
            value.to_string(),
            value.replace('\\', "/"),
            value.replace('/', "\\"),
        ] {
            if !variant.is_empty() {
                text = text.replace(&variant, "<model-directory>");
            }
        }
    }
    // Unknown operator paths can also occur in native loader/inference errors.
    let paths = regex::Regex::new(
        r#"(?i)[a-z]:[\\/][^\r\n\"',;]*|(?:\\\\|//)[^\s\"',;]+|/(?:[^/\s:]+/)+[^\s\"',;]*"#,
    )
    .unwrap();
    paths.replace_all(&text, "<local-path>").into_owned()
}
