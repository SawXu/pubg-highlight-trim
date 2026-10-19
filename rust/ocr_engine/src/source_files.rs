//! NVIDIA source selection and language voting. Explicit paths bypass target filtering.
use crate::config::{Config, GameLanguage, Target};
use regex::Regex;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};
#[derive(Clone, Debug)]
pub struct Source {
    pub path: PathBuf,
    pub language: GameLanguage,
    pub force_full_scan: bool,
    pub match_end: bool,
}
#[derive(Debug)]
pub struct Inputs {
    pub sources: Vec<Source>,
    pub input_path: PathBuf,
    pub base_folder: PathBuf,
    pub single_file: bool,
    pub directory_input: bool,
}
struct Profile {
    language: GameLanguage,
    own: &'static [&'static str],
    kill: &'static [&'static str],
    multi: &'static [&'static str],
    end: &'static [&'static str],
    replay: &'static str,
}
fn profiles() -> [Profile; 3] {
    [
        Profile {
            language: GameLanguage::SimplifiedChinese,
            own: &["被击倒", "淘汰"],
            kill: &["单次淘汰", "双次淘汰", "多杀"],
            multi: &["双次淘汰", "多杀"],
            end: &["比赛结束"],
            replay: "淘汰画面|击倒画面",
        },
        Profile {
            language: GameLanguage::TraditionalChinese,
            own: &["被擊倒", "死亡", "淘汰"],
            kill: &["單次擊殺", "雙殺", "多殺", "多重擊殺", "擊倒"],
            multi: &["雙殺", "多殺", "多重擊殺"],
            end: &["對戰結束"],
            replay: "死亡畫面|淘汰畫面|擊倒畫面",
        },
        Profile {
            language: GameLanguage::English,
            own: &["Knockouted", "Death"],
            kill: &[
                "Knockout",
                "Single kill",
                "Double kill",
                "Triple kill",
                "Multi kill",
                "Multiple kill",
            ],
            multi: &["Double kill", "Triple kill", "Multi kill", "Multiple kill"],
            end: &[
                "Match end",
                "End of match",
                "Match ended",
                "Match complete",
                "Game over",
            ],
            replay: r"Death\s*cam|Kill\s*cam|Replay",
        },
    ]
}
pub(crate) fn canonical(path: &Path) -> std::io::Result<PathBuf> {
    let p = fs::canonicalize(path)?;
    #[cfg(windows)]
    {
        let s = p.to_string_lossy();
        if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
            return Ok(PathBuf::from(format!(r"\\{rest}")));
        }
        if let Some(rest) = s.strip_prefix(r"\\?\") {
            return Ok(PathBuf::from(rest));
        }
    }
    Ok(p)
}
fn name(p: &Path) -> String {
    p.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}
fn pattern(terms: &[&str], suffix: bool) -> Regex {
    let alt = terms.join("|");
    Regex::new(&if suffix {
        format!(r"(?i)\.(?:{alt})\.DVR(?:_\d+)?\.mp4$")
    } else {
        format!("(?i)(?:{alt})")
    })
    .unwrap()
}
fn terms(p: &Profile, target: Target) -> Vec<&'static str> {
    match target {
        Target::SelfDeath => p.own.to_vec(),
        Target::OwnKill => p.kill.iter().chain(p.end).copied().collect(),
        Target::Both => p.own.iter().chain(p.kill).chain(p.end).copied().collect(),
    }
}
fn matches(name: &str, p: &Profile, target: Target) -> bool {
    pattern(&terms(p, target), true).is_match(name)
        && !Regex::new(&format!("(?i){}", p.replay))
            .unwrap()
            .is_match(name)
}
fn scores(path: &Path, target: Target, explicit: bool, ps: &[Profile]) -> Vec<(usize, usize)> {
    let n = name(path);
    if explicit
        && ps.iter().any(|p| {
            Regex::new(&format!("(?i){}", p.replay))
                .unwrap()
                .is_match(&n)
        })
    {
        return vec![];
    }
    let mut v = Vec::new();
    for (i, p) in ps.iter().enumerate() {
        let score = terms(p, target)
            .iter()
            .filter_map(|term| {
                pattern(&[term], false)
                    .find(&n)
                    .map(|m| m.as_str().chars().count())
            })
            .max()
            .unwrap_or(0);
        if (explicit && score > 0) || (!explicit && matches(&n, p, target)) {
            v.push((i, score.max(1)))
        }
    }
    v.sort_by_key(|(i, s)| (std::cmp::Reverse(*s), *i));
    v
}
fn infer(paths: &[PathBuf], target: Target, ps: &[Profile]) -> HashMap<PathBuf, usize> {
    let mut selected = HashMap::new();
    let mut ambiguous = Vec::new();
    let mut counts = [0usize; 3];
    let mut totals = [0usize; 3];
    for path in paths {
        let s = scores(path, target, false, ps);
        if s.is_empty() {
            if let Some((i, _)) = scores(path, Target::Both, true, ps).first() {
                selected.insert(path.clone(), *i);
            }
            continue;
        }
        let top = s[0].1;
        let candidates = s
            .iter()
            .filter(|(_, v)| *v == top)
            .map(|(i, _)| *i)
            .collect::<Vec<_>>();
        if candidates.len() == 1 {
            let i = candidates[0];
            selected.insert(path.clone(), i);
            counts[i] += 1;
            totals[i] += top;
        } else {
            ambiguous.push((path, candidates));
        }
    }
    let mut dominant = (0..ps.len()).filter(|i| counts[*i] > 0).collect::<Vec<_>>();
    dominant.sort_by_key(|i| {
        (
            std::cmp::Reverse(counts[*i]),
            std::cmp::Reverse(totals[*i]),
            *i,
        )
    });
    for (path, candidates) in ambiguous {
        let i = dominant
            .iter()
            .find(|i| candidates.contains(i))
            .copied()
            .unwrap_or(candidates[0]);
        selected.insert(path.clone(), i);
    }
    selected
}
fn sort_sources(paths: &mut [PathBuf]) -> Result<(), String> {
    let timestamp =
        Regex::new(r"(\d{4})\.(\d{2})\.(\d{2})\s*-\s*(\d{2})\.(\d{2})\.(\d{2})").unwrap();
    let mut keys = HashMap::new();
    for path in paths.iter() {
        let n = name(path).to_lowercase();
        let key = if let Some(c) = timestamp.captures(&n) {
            let v = (1..=6)
                .map(|i| c[i].parse::<u32>().unwrap())
                .collect::<Vec<_>>();
            let leap = v[0] % 4 == 0 && (v[0] % 100 != 0 || v[0] % 400 == 0);
            let days = [
                31,
                if leap { 29 } else { 28 },
                31,
                30,
                31,
                30,
                31,
                31,
                30,
                31,
                30,
                31,
            ];
            if v[0] == 0
                || v[1] == 0
                || v[1] > 12
                || v[2] == 0
                || v[2] > days[(v[1] - 1) as usize]
                || v[3] > 23
                || v[4] > 59
                || v[5] > 59
            {
                return Err(format!("Invalid recording timestamp: {}", path.display()));
            };
            (false, v, n)
        } else {
            (true, vec![], n)
        };
        keys.insert(path.clone(), key);
    }
    paths.sort_by(|a, b| keys[a].cmp(&keys[b]));
    Ok(())
}
fn collect(folder: &Path, recursive: bool, out: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(folder).map_err(|e| e.to_string())? {
        let e = entry.map_err(|e| e.to_string())?;
        let p = e.path();
        let ty = e.file_type().map_err(|e| e.to_string())?;
        if ty.is_dir() && recursive {
            collect(&p, true, out)?
        } else if p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("mp4")) {
            out.push(p)
        }
    }
    Ok(())
}
pub fn select(config: &Config) -> Result<Inputs, String> {
    if config.input.is_some() && !config.files.is_empty() {
        return Err("Use either the positional input or --files, not both.".into());
    }
    let raw = if !config.files.is_empty() {
        config.files.clone()
    } else {
        vec![config.input.clone().unwrap_or_else(|| PathBuf::from("."))]
    };
    let mut paths = Vec::new();
    for p in raw {
        if !p.exists() {
            return Err(format!("Input path does not exist: {}", p.display()));
        }
        paths.push(canonical(&p).map_err(|e| e.to_string())?);
    }
    let input_path = paths[0].clone();
    let directory_input = paths.len() == 1 && input_path.is_dir();
    let single_file = paths.len() == 1 && !directory_input;
    let base_folder = if directory_input {
        input_path.clone()
    } else {
        input_path.parent().unwrap().to_path_buf()
    };
    if directory_input {
        paths.clear();
        collect(&input_path, config.recursive, &mut paths)?;
    } else {
        for p in &paths {
            if !p.is_file() {
                return Err(format!(
                    "Multiple inputs must all be mp4 files: {}",
                    p.display()
                ));
            }
            if !p.extension().is_some_and(|e| e.eq_ignore_ascii_case("mp4")) {
                return Err(format!(
                    "{} must be .mp4 files: {}",
                    if single_file {
                        "Single-file input"
                    } else {
                        "Multiple inputs"
                    },
                    p.display()
                ));
            }
        }
    }
    let ps = profiles();
    if directory_input {
        paths.retain(|p| {
            ps.iter().any(|profile| {
                (config.game_lang == GameLanguage::Auto || config.game_lang == profile.language)
                    && matches(&name(p), profile, config.target)
            })
        });
    }
    sort_sources(&mut paths)?;
    let inferred = infer(&paths, config.target, &ps);
    let sources = paths
        .iter()
        .map(|p| {
            let i = if config.game_lang == GameLanguage::Auto {
                *inferred.get(p).unwrap_or(&0)
            } else {
                ps.iter()
                    .position(|profile| profile.language == config.game_lang)
                    .unwrap()
            };
            let profile = &ps[i];
            let n = name(p);
            let match_end = pattern(profile.end, true).is_match(&n);
            Source {
                path: p.clone(),
                language: profile.language,
                force_full_scan: match_end || pattern(profile.multi, true).is_match(&n),
                match_end,
            }
        })
        .collect::<Vec<_>>();
    if sources.is_empty() {
        return Err(format!(
            "No matching PUBG highlight source files found for game_lang={}",
            config.game_lang.code()
        ));
    }
    Ok(Inputs {
        sources,
        input_path,
        base_folder,
        single_file,
        directory_input,
    })
}
pub fn language_counts(inputs: &Inputs) -> BTreeMap<&'static str, usize> {
    let mut c = BTreeMap::new();
    for s in &inputs.sources {
        *c.entry(s.language.code()).or_default() += 1;
    }
    c
}
/// Search only direct children, as the old application's candidate discovery did.
pub fn discover_candidates(config: &Config, inputs: &Inputs) -> Option<PathBuf> {
    if config.candidate_csv.is_some() {
        return config.candidate_csv.clone();
    }
    if config.no_auto_candidate_csv {
        return None;
    }
    let mut roots = vec![inputs.base_folder.clone()];
    if inputs.directory_input {
        roots.push(inputs.input_path.clone())
    }
    if let Some(p) = inputs.base_folder.parent() {
        roots.push(p.to_path_buf())
    }
    let mut seen = HashSet::new();
    let mut found = Vec::new();
    for root in roots {
        if !seen.insert(root.clone()) {
            continue;
        }
        let direct = root.join("candidate_events.csv");
        if direct.is_file() {
            found.push(direct)
        }
        if let Ok(entries) = fs::read_dir(root) {
            for e in entries.flatten() {
                let candidate = e.path().join("candidate_events.csv");
                if candidate.is_file() {
                    found.push(candidate)
                }
            }
        }
    }
    found
        .into_iter()
        .max_by_key(|p| fs::metadata(p).and_then(|s| s.modified()).ok())
}
pub fn read_candidates(path: Option<&Path>) -> Result<BTreeMap<String, Vec<f64>>, String> {
    let mut out = BTreeMap::new();
    let Some(path) = path.filter(|p| p.exists()) else {
        return Ok(out);
    };
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&bytes);
    let mut reader = csv::Reader::from_reader(bytes);
    let headers = reader.headers().map_err(|e| e.to_string())?.clone();
    let n = headers.iter().position(|h| h == "Name");
    let o = headers.iter().position(|h| h == "OcrEventSec");
    let e = headers.iter().position(|h| h == "EventSec");
    for row in reader.records() {
        let row = row.map_err(|e| e.to_string())?;
        let name = n.and_then(|i| row.get(i)).unwrap_or("");
        let value = o
            .and_then(|i| row.get(i))
            .filter(|v| !v.is_empty())
            .or_else(|| e.and_then(|i| row.get(i)))
            .unwrap_or("");
        if !name.is_empty() {
            if let Ok(v) = value.parse::<f64>() {
                if v.is_finite() {
                    out.entry(name.to_owned()).or_insert_with(Vec::new).push(v)
                }
            }
        }
    }
    Ok(out)
}
