use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn language(text: &str) -> &'static str {
    if text.chars().any(|c| c.is_ascii_alphabetic())
        && !text.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
    {
        "en"
    } else if text.chars().any(|c| "擊您淘汰".contains(c)) {
        "zh-Hant"
    } else {
        "zh-Hans"
    }
}
fn contains(text: &str, terms: &[&str]) -> bool {
    let text = text.to_uppercase();
    terms.iter().any(|term| text.contains(term))
}
fn action(text: &str, keys: &str) -> &'static str {
    if contains(text, &["擊殺", "击杀", "淘汰", "KILLED", "ELIMINATED"]) {
        "eliminate"
    } else if contains(text, &["擊倒", "击倒", "KNOCKED"]) {
        "knock"
    } else if contains(keys, &["ELIMINATE", "淘汰", "擊殺", "击杀"]) {
        "eliminate"
    } else {
        "unknown"
    }
}
fn kind(text: &str, keys: &str) -> &'static str {
    if contains(text, &["助攻", "助殺", "ASSIST"]) {
        "assist"
    } else if contains(text, &["終於", "终于", "FINALLY"]) {
        "delayed-elimination"
    } else {
        action(text, keys)
    }
}
fn find_source(root: &Path, name: &str) -> Option<PathBuf> {
    // CSV contains basenames, never a path that can escape the local media root.
    if Path::new(name).file_name()?.to_str()? != name {
        return None;
    }
    fn walk(root: &Path, name: &str, matches: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(root) else {
            return;
        };
        for entry in entries.flatten() {
            let Ok(t) = entry.file_type() else { continue };
            if t.is_dir() {
                walk(&entry.path(), name, matches);
            } else if t.is_file() && entry.file_name() == name {
                matches.push(entry.path());
            }
        }
    }
    let mut matches = Vec::new();
    walk(root, name, &mut matches);
    matches.sort_by_key(|p| {
        (
            !p.components().any(|c| {
                c.as_os_str()
                    .to_string_lossy()
                    .eq_ignore_ascii_case("original")
            }),
            p.clone(),
        )
    });
    matches.into_iter().next()
}
/// Local-only extraction. Inputs are read-only; no networking or upload path exists.
pub fn run(
    records: &[PathBuf],
    media_root: &Path,
    output: &Path,
    limit: usize,
    ffmpeg: &Path,
) -> Result<(), String> {
    if limit == 0 {
        return Err("limit-per-language must be positive".into());
    }
    if output.exists()
        && fs::read_dir(output)
            .map_err(|e| e.to_string())?
            .next()
            .is_some()
    {
        return Err("output must be empty to avoid overwriting samples".into());
    }
    let mut candidates = Vec::new();
    for (record_index, record) in records.iter().enumerate() {
        let mut reader = csv::Reader::from_path(record)
            .map_err(|_| format!("record {}: cannot read CSV", record_index + 1))?;
        for row in reader.deserialize::<BTreeMap<String, String>>() {
            let row = row.map_err(|_| format!("record {}: invalid CSV row", record_index + 1))?;
            if row.get("Status").map(String::as_str) == Some("included")
                && row.get("PaddleText").is_some_and(|s| !s.is_empty())
            {
                candidates.push(row);
            }
        }
    }
    let mut selected = Vec::new();
    for lang in ["zh-Hans", "zh-Hant", "en"] {
        for target in ["self-death", "own-kill"] {
            let mut rows: Vec<_> = candidates
                .iter()
                .filter(|r| {
                    language(&r["PaddleText"]) == lang
                        && r.get("Target").map(String::as_str) == Some(target)
                })
                .collect();
            rows.sort_by_key(|r| {
                !matches!(
                    kind(
                        &r["PaddleText"],
                        r.get("EventKeys").map(String::as_str).unwrap_or("")
                    ),
                    "assist" | "delayed-elimination"
                )
            });
            selected.extend(rows.into_iter().take((limit / 2).max(1)));
        }
    }
    fs::create_dir_all(output).map_err(|e| e.to_string())?;
    let mut rows: Vec<Value> = Vec::new();
    let mut counts = BTreeMap::from([("zh-Hans", 0), ("zh-Hant", 0), ("en", 0)]);
    for (sample_index, item) in selected.into_iter().enumerate() {
        let Some(source) = item
            .get("Name")
            .and_then(|name| find_source(media_root, name))
        else {
            continue;
        };
        let Some(sec) = item
            .get("EventSec")
            .and_then(|s| s.parse::<f64>().ok())
            .filter(|s| s.is_finite() && *s >= 0.0)
        else {
            continue;
        };
        let decoded = Command::new(ffmpeg)
            .args(["-hide_banner", "-loglevel", "error", "-ss"])
            .arg(format!("{sec:.6}"))
            .arg("-i")
            .arg(&source)
            .args(["-frames:v", "1", "-f", "image2pipe", "-vcodec", "ppm", "-"])
            .output()
            .map_err(|_| format!("sample {}: cannot launch decoder", sample_index + 1))?;
        if !decoded.status.success() {
            eprintln!("skip sample {}: decoder failed", sample_index + 1);
            continue;
        }
        let Ok(frame) =
            image::load_from_memory_with_format(&decoded.stdout, image::ImageFormat::Pnm)
        else {
            continue;
        };
        let (w, h) = (frame.width(), frame.height());
        let (x1, y1, x2, y2) = (
            (w as f64 * 0.30) as u32,
            (h as f64 * 0.66) as u32,
            (w as f64 * 0.70) as u32,
            (h as f64 * 0.75) as u32,
        );
        if x1 >= x2 || y1 >= y2 {
            continue;
        }
        let roi = frame.crop_imm(x1, y1, x2 - x1, y2 - y1).to_rgb8();
        let id = rows.len() + 1;
        let name = format!("frame_{id:03}.jpg");
        let file = fs::File::create(output.join(&name)).map_err(|e| e.to_string())?;
        image::codecs::jpeg::JpegEncoder::new_with_quality(file, 95)
            .encode_image(&roi)
            .map_err(|e| e.to_string())?;
        let text = &item["PaddleText"];
        let lang = language(text);
        let keys = item.get("EventKeys").map(String::as_str).unwrap_or("");
        let profile = pubg_ocr_engine::events::language_profile(lang)?;
        let events = pubg_ocr_engine::events::extract_text_events(
            text,
            pubg_ocr_engine::events::Target::Both,
            Some(&profile),
        );
        let source_sha256 = super::digest(&source)
            .map_err(|_| format!("sample {}: cannot fingerprint source", sample_index + 1))?;
        rows.push(json!({"schema":"reference-v2","frame_id":id,"image":name,"source":format!("source_{id:03}"),"source_sha256":source_sha256,"event_sec":sec,"language":lang,"normalized_ocr_sha256":super::text_digest(&super::normalize(text)),"event_signatures":super::event_signatures(&events),"action":action(text,keys),"event_kind":kind(text,keys),"target":if item.get("Target").map(String::as_str)==Some("self-death"){"self-death"}else{"own-kill"},"boxes":[]}));
        *counts.get_mut(lang).unwrap() += 1;
    }
    fs::write(
        output.join("baseline.jsonl"),
        rows.iter().map(|r| format!("{r}\n")).collect::<String>(),
    )
    .map_err(|e| e.to_string())?;
    counts.retain(|_, count| *count > 0);
    let manifest = json!({"count":rows.len(),"languages":counts,"roi":[0.30,0.66,0.70,0.75]});
    fs::write(
        output.join("manifest.json"),
        serde_json::to_string_pretty(&manifest).unwrap() + "\n",
    )
    .map_err(|e| e.to_string())?;
    println!("{manifest}");
    if rows.is_empty() {
        Err("no readable local ROI samples selected".into())
    } else {
        Ok(())
    }
}
