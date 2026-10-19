//! Language-aware PUBG event text extraction.
//!
//! This module intentionally keeps the event vocabulary explicit per game language.  OCR
//! whitespace is removed before matching (the same Unicode `\s+` operation used by Python),
//! while boundaries between adjacent events are handled explicitly rather than by a regex
//! look-around (the Rust regex crate deliberately does not implement look-around).

use regex::Regex;
use std::cmp::{max, min};
use std::sync::OnceLock;

macro_rules! cached_regex {
    ($pattern:literal) => {{
        static REGEX: OnceLock<Regex> = OnceLock::new();
        REGEX.get_or_init(|| {
            // Python IGNORECASE includes dotted/dotless I in ASCII literals.
            let pattern = if $pattern.starts_with("(?i)") {
                format!("(?i){}", $pattern[4..].replace('I', "[Iiİı]"))
            } else {
                $pattern.to_owned()
            };
            Regex::new(&pattern).expect("event regex is valid")
        })
    }};
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TextEvent {
    pub target: String,
    pub method: String,
    pub action: String,
    pub subject: String,
    pub key: String,
    pub text: String,
    pub weapon: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    SelfDeath,
    OwnKill,
    Both,
}

impl Target {
    fn includes_self(self) -> bool {
        matches!(self, Self::SelfDeath | Self::Both)
    }
    fn includes_own(self) -> bool {
        matches!(self, Self::OwnKill | Self::Both)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameLanguage {
    ZhHans,
    ZhHant,
    En,
}

impl GameLanguage {
    pub fn code(self) -> &'static str {
        match self {
            Self::ZhHans => "zh-Hans",
            Self::ZhHant => "zh-Hant",
            Self::En => "en",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LanguageProfile {
    pub language: GameLanguage,
    pub code: &'static str,
}

pub fn language_profile(code: &str) -> Result<LanguageProfile, String> {
    match code {
        "zh-Hans" => Ok(LanguageProfile {
            language: GameLanguage::ZhHans,
            code: "zh-Hans",
        }),
        "zh-Hant" => Ok(LanguageProfile {
            language: GameLanguage::ZhHant,
            code: "zh-Hant",
        }),
        "en" => Ok(LanguageProfile {
            language: GameLanguage::En,
            code: "en",
        }),
        _ => Err(format!("Unsupported PUBG game language: {code}")),
    }
}

pub fn default_language_profile() -> LanguageProfile {
    LanguageProfile {
        language: GameLanguage::ZhHans,
        code: "zh-Hans",
    }
}

pub fn normalize_text(text: &str) -> String {
    // `char::is_whitespace` follows Unicode White_Space and is equivalent to Python's
    // Unicode regular-expression `\\s` for the OCR strings we accept.
    text.chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '\u{1c}'..='\u{1f}'))
        .collect()
}

fn resolve_profile(profile: Option<&LanguageProfile>) -> LanguageProfile {
    profile.cloned().unwrap_or_else(default_language_profile)
}
fn trim_punct(s: &str) -> &str {
    s.trim_matches(|c: char| "，。,.、:：;；|/\\()（）[]【】 ".contains(c))
}

fn clean_weapon(text: &str, p: GameLanguage) -> String {
    let mut s = normalize_text(text);
    let re = match p {
        GameLanguage::ZhHans => cached_regex!(r"(?:淘汰数?\d*|淘汰\d+|协助次数|协助|助.{0,2}攻)+$"),
        GameLanguage::ZhHant => cached_regex!(
            r"(?:(?:\d+\s*)?擊殺[數数]\d*|(?:\d+\s*)?撃殺[數数]\d*|協助次數|協助|助.{0,2}[攻殺])+$"
        ),
        GameLanguage::En => cached_regex!(r"(?i)(?:\d(?:KILLS?|ASSISTS?)|ASSISTS?)+$"),
    };
    s = re.replace(&s, "").into_owned();
    trim_punct(&s).chars().take(32).collect()
}

fn subject_stop_re(p: GameLanguage) -> &'static Regex {
    match p {
        GameLanguage::ZhHans => cached_regex!(
            r"(?:\(\d+\s*[mM]\)|（\d+\s*[mM]）|(?:\d+\s*)?(?:淘汰数?\d*|淘汰\d+)|淘汰数|淘汰数?\d*|淘汰\d+|协助次数|协助|助.{0,2}攻|使用|用|你(?:使用|用)|你.{0,24}?(?:击倒|淘汰)了|击倒了你|淘汰了你)"
        ),
        GameLanguage::ZhHant => cached_regex!(
            r"(?:\(\d+\s*[mM]\)|（\d+\s*[mM]）|(?:\d+\s*)?(?:擊殺[數数]\d*|撃殺[數数]\d*|擊殺\d+|撃殺\d+)|擊殺[數数]|撃殺[數数]|击杀数|擊殺[數数]\d*|撃殺[數数]\d*|擊殺\d+|撃殺\d+|協助次數|協助|助.{0,2}攻|助.{0,2}殺|以|使用|用|您(?:以|使用|用)|您.{0,24}?(?:擊倒|击倒|擊殺|撃殺|擎殺|淘汰)|擊倒您|击倒了你|擊殺您|撃殺您|擎殺您|淘汰您|淘汰了你)"
        ),
        GameLanguage::En => cached_regex!(
            r"(?i)(?:\(\d+\s*[mM]\)|（\d+\s*[mM]）|(?:\d+\s*)?(?:\d+KILLS?)|\d+KILLS?|(?:\d+)?ASSISTS?|WITH|USING|YOU(?:FINALLY)?(?:WITH|USING)|YOU(?:FINALLY)?(?:KNOCKEDOUT|KILLED|ELIMINATED)|KNOCKEDYOU(?:OUT)?|KILLEDYOU|KILLSYOU|ELIMINATEDYOU)"
        ),
    }
}

fn clean_subject(text: &str, p: GameLanguage) -> String {
    let text = normalize_text(text);
    let stop = subject_stop_re(p)
        .find(&text)
        .map(|m| m.start())
        .unwrap_or(text.len());
    let stop = if p == GameLanguage::En {
        count_match(&text, p)
            .map(|(start, _)| start.min(stop))
            .unwrap_or(stop)
    } else {
        stop
    };
    let text = &text[..stop];
    let text = trim_punct(text);
    let text = if p == GameLanguage::En {
        cached_regex!(r"(?i)(?:BYHEADSHOT)+$").replace(text, "")
    } else {
        std::borrow::Cow::Borrowed(text)
    };
    trim_punct(&text).chars().take(48).collect()
}

fn python_alphanumeric(c: char) -> bool {
    // str.isalnum uses Unicode Letter/Number categories, not Alphabetic
    // (which also includes combining marks such as U+0345).
    let mut bytes = [0; 4];
    cached_regex!(r"^[\p{L}\p{N}]$").is_match(c.encode_utf8(&mut bytes))
}
fn subject_key(text: &str, p: GameLanguage) -> String {
    let s = clean_subject(text, p);
    let key: String = s
        .chars()
        .filter(|c| python_alphanumeric(*c) || matches!(c, '_' | '-' | '[' | ']'))
        .flat_map(char::to_lowercase)
        .collect();
    if key.is_empty() {
        s.to_lowercase()
    } else {
        key
    }
}

fn canonical_action(raw: &str, p: GameLanguage) -> &'static str {
    match p {
        GameLanguage::ZhHans => {
            if raw.contains("击倒") || raw.contains("倒了你") {
                "knock"
            } else {
                "eliminate"
            }
        }
        GameLanguage::ZhHant => {
            if raw.contains("擊倒") || raw.contains("击倒") || raw.contains("倒了你") {
                "knock"
            } else {
                "eliminate"
            }
        }
        GameLanguage::En => {
            if raw.to_ascii_uppercase().contains("KNOCK") {
                "knock"
            } else {
                "eliminate"
            }
        }
    }
}

pub fn is_molotov_weapon(weapon: &str, profile: Option<&LanguageProfile>) -> bool {
    let p = resolve_profile(profile).language;
    let s = normalize_text(weapon);
    match p {
        GameLanguage::ZhHans => cached_regex!(r"燃烧弹|燃燒彈|燃烧瓶|燃燒瓶|molotov").is_match(&s),
        GameLanguage::ZhHant => cached_regex!(r"燃燒彈|汽油彈|molotov").is_match(&s),
        GameLanguage::En => cached_regex!(r"(?i)MOLOTOV|FIRE-?BOMB|GASCAN|C4").is_match(&s),
    }
}

pub fn has_assist_text(text: &str, profile: Option<&LanguageProfile>) -> bool {
    assist_match(&normalize_text(text), resolve_profile(profile).language).is_some()
}
pub fn is_delayed_own_elim_text(text: &str, profile: Option<&LanguageProfile>) -> bool {
    delayed_match(&normalize_text(text), resolve_profile(profile).language)
}

fn assist_match(s: &str, p: GameLanguage) -> Option<usize> {
    let r = match p {
        GameLanguage::ZhHans => cached_regex!(r"协助次数|协助|助.{0,2}攻"),
        GameLanguage::ZhHant => cached_regex!(r"協助次數|協助|助.{0,2}[攻殺]"),
        GameLanguage::En => cached_regex!(r"(?i)(?:\d+)?ASSISTS?"),
    };
    r.find(s).map(|m| m.start())
}
fn delayed_match(s: &str, p: GameLanguage) -> bool {
    match p {
        GameLanguage::ZhHans => s.contains("你终于淘汰了"),
        GameLanguage::ZhHant => cached_regex!(r"您終於.{0,4}(?:擊殺|撃殺|擎殺|淘汰)").is_match(s),
        GameLanguage::En => cached_regex!(r"(?i)YOUFINALLY(?:KILLED|ELIMINATED)").is_match(s),
    }
}

pub fn is_assist_own_kill_text(text: &str, profile: Option<&LanguageProfile>) -> bool {
    let p = resolve_profile(profile).language;
    let s = normalize_text(text);
    let Some(pos) = assist_match(&s, p) else {
        return false;
    };
    let scoreboard_assist = match p {
        GameLanguage::ZhHans => {
            cached_regex!(r"(?:淘汰数?\d*|淘汰\d+)协助|(?:淘汰数?\d*|淘汰\d+)助.{0,2}攻")
        }
        GameLanguage::ZhHant => {
            cached_regex!(r"(?:擊殺[數数]\d*|撃殺[數数]\d*|擊殺\d+|撃殺\d+)(?:協助|助.{0,2}[攻殺])")
        }
        GameLanguage::En => cached_regex!(r"(?i)\d+KILLS?(?:\d+)?ASSISTS?"),
    };
    if scoreboard_assist.is_match(&s) {
        return false;
    }
    count_match(&s, p)
        .map(|(start, _)| pos < start)
        .unwrap_or(true)
}

pub fn classify_self_text(text: &str, profile: Option<&LanguageProfile>) -> Option<&'static str> {
    let p = resolve_profile(profile).language;
    let s = normalize_text(text);
    if self_strict(&s, p) {
        Some("paddle-strict-self-text")
    } else if self_zone(&s, p) {
        Some("paddle-zone-self-downed-text")
    } else if self_fuzzy(&s, p) {
        Some("paddle-fuzzy-self-text")
    } else {
        None
    }
}
// The old patterns use positive lookahead. Keep their consuming grammar exact,
// and test lookahead at Unicode boundaries explicitly (regex has no lookaround).
fn own_starts(_s: &str, p: GameLanguage) -> &'static Regex {
    match p {
        GameLanguage::ZhHans => cached_regex!(r"你(?P<prefix>.{0,24}?)(?P<action>击倒|淘汰)了"),
        GameLanguage::ZhHant => {
            cached_regex!(r"您(?P<prefix>.{0,24}?)(?P<action>擊倒|击倒|擊殺|撃殺|擎殺|淘汰)")
        }
        GameLanguage::En => {
            cached_regex!(r"(?i)YOU(?:FINALLY)?(?P<action>KNOCKEDOUT|KILLED|ELIMINATED)")
        }
    }
}
fn count_re(p: GameLanguage) -> &'static Regex {
    match p {
        GameLanguage::ZhHans => cached_regex!(r"淘汰数"),
        GameLanguage::ZhHant => cached_regex!(r"擊殺[數数]|撃殺[數数]|击杀数"),
        GameLanguage::En => cached_regex!(r"(?i)(?:\d+)?KILLS?"),
    }
}
fn count_match(s: &str, p: GameLanguage) -> Option<(usize, usize)> {
    count_re(p)
        .find_iter(s)
        .find(|m| {
            p != GameLanguage::En
                || s[m.end()..]
                    .chars()
                    .next()
                    .is_none_or(|c| !python_alphanumeric(c) && c != '_')
        })
        .map(|m| (m.start(), m.end()))
}
fn boundary(s: &str, at: usize, p: GameLanguage, assists: bool, counts: bool) -> bool {
    at == s.len()
        || own_starts(s, p)
            .find_at(s, at)
            .is_some_and(|m| m.start() == at)
        || (assists && assist_match(&s[at..], p) == Some(0))
        || (counts && count_match(&s[at..], p).is_some_and(|(start, _)| start == 0))
}
fn ends(s: &str, from: usize) -> impl Iterator<Item = usize> + '_ {
    s[from..]
        .char_indices()
        .map(move |(i, _)| from + i)
        .chain(std::iter::once(s.len()))
}
fn weapon_re(p: GameLanguage) -> &'static Regex {
    match p {
        GameLanguage::ZhHans => cached_regex!(r"(?:使用|用)(?P<weapon>.+)$"),
        GameLanguage::ZhHant => cached_regex!(r"(?:以|使用|用)(?P<weapon>.+)$"),
        GameLanguage::En => cached_regex!(r"(?i)(?:WITH|USING)(?P<weapon>.+)$"),
    }
}
fn weapon_from_prefix(prefix: &str, p: GameLanguage) -> String {
    weapon_re(p)
        .captures(prefix)
        .map(|m| clean_weapon(m.name("weapon").unwrap().as_str(), p))
        .unwrap_or_default()
}
fn self_strict_re(p: GameLanguage) -> &'static Regex {
    match p {
        GameLanguage::ZhHans => cached_regex!(r"(?P<action>击倒了你|淘汰了你)"),
        GameLanguage::ZhHant => {
            cached_regex!(r"(?P<action>擊倒您|击倒了你|擊殺您|撃殺您|擎殺您|淘汰您|淘汰了你)")
        }
        GameLanguage::En => cached_regex!(
            r"(?i)(?P<action>KNOCKEDYOU(?:OUT)?|KILLEDYOU|KILLSYOU|ELIMINATEDYOU)(?:(?:WITH|USING)(?P<weapon>.+?))?$"
        ),
    }
}
fn fuzzy_re(p: GameLanguage) -> &'static Regex {
    match p {
        GameLanguage::ZhHans => cached_regex!(r"(?P<action>击倒.{0,2}你|倒了你|淘.{0,2}了?你)"),
        GameLanguage::ZhHant => cached_regex!(
            r"(?P<action>擊倒.{0,2}您|击倒.{0,2}你|倒了你|[擊撃擎]?殺.{0,2}您|淘.{0,2}您)"
        ),
        GameLanguage::En => cached_regex!(
            r"(?i)(?P<action>KNOCKED.{0,8}YOU|KILL(?:ED|S).{0,4}YOU|ELIMINAT(?:ED)?.{0,4}YOU)"
        ),
    }
}
fn strict_capture(s: &str, p: GameLanguage) -> Option<(usize, String, String)> {
    if p != GameLanguage::En {
        return self_strict_re(p).captures(s).map(|m| {
            let a = m.name("action").unwrap();
            (a.start(), a.as_str().into(), String::new())
        });
    }
    // Search start first, then the earliest valid lookahead. Optional weapon is lazy.
    for start in ends(s, 0) {
        for end in ends(s, start).filter(|&end| boundary(s, end, p, true, true)) {
            if let Some(m) = self_strict_re(p).captures(&s[start..end]) {
                let a = m.name("action").unwrap();
                if a.start() == 0 {
                    return Some((
                        start,
                        a.as_str().into(),
                        m.name("weapon")
                            .map(|m| clean_weapon(m.as_str(), p))
                            .unwrap_or_default(),
                    ));
                }
            }
        }
    }
    None
}
fn self_strict(s: &str, p: GameLanguage) -> bool {
    strict_capture(s, p).is_some()
}
fn self_fuzzy(s: &str, p: GameLanguage) -> bool {
    fuzzy_re(p).is_match(s)
}
fn self_zone(s: &str, p: GameLanguage) -> bool {
    match p {
        GameLanguage::ZhHans => cached_regex!(r"你在安全区外倒地了|安全区外倒地了|安全区外倒地|你因.{0,8}(?:坠落|跌落|摔落).{0,4}(?:击倒|倒地)").is_match(s),
        GameLanguage::ZhHant => cached_regex!(r"您.{0,4}(?:安全區|遊戲區域)外(?:倒地|死亡)|您在安全区外倒地了|您因.{0,8}(?:墜落|跌落|摔落).{0,4}擊倒").is_match(s),
        GameLanguage::En => cached_regex!(r"(?i)OUTSIDETHEPLAYZONE|OUTSIDETHEBLUEZONE|YOU(?:WERE)?(?:KNOCKEDOUT|KILLED|DIED).{0,18}(?:PLAYZONE|BLUEZONE|ZONE)|YOUWEREKNOCKEDOUTBY(?:FALLING|FALLDAMAGE)|YOUWEREKNOCKEDOUTFROMFALLING|YOUDIEDFROMFALLING").is_match(s),
    }
}

pub fn extract_own_kill_events(
    text: &str,
    allow_assist: bool,
    profile: Option<&LanguageProfile>,
) -> Vec<TextEvent> {
    let profile = resolve_profile(profile);
    let p = profile.language;
    let s = normalize_text(text);
    let mut out = Vec::new();
    let mut cursor = 0;
    while cursor < s.len() {
        let Some(m) = own_starts(&s, p).captures_at(&s, cursor) else {
            break;
        };
        let whole = m.get(0).unwrap();
        let victim_start = whole.end();
        let mut matched = None;
        for victim_end in ends(&s, victim_start).filter(|&at| at > victim_start) {
            if p == GameLanguage::En {
                if let Some(marker) = cached_regex!(r"(?i)^(?:WITH|USING)").find(&s[victim_end..]) {
                    let weapon_start = victim_end + marker.end();
                    if let Some(end) = ends(&s, weapon_start)
                        .find(|&at| at > weapon_start && boundary(&s, at, p, true, false))
                    {
                        matched = Some((end, victim_end, &s[weapon_start..end]));
                        break;
                    }
                }
            }
            if boundary(&s, victim_end, p, p == GameLanguage::En, false) {
                matched = Some((victim_end, victim_end, ""));
                break;
            }
        }
        let Some((end, victim_end, captured_weapon)) = matched else {
            cursor = whole.start() + s[whole.start()..].chars().next().unwrap().len_utf8();
            continue;
        };
        cursor = end;
        let raw = &s[whole.start()..end];
        if delayed_match(raw, p) {
            continue;
        }
        let probe_end = ends(&s, end).nth(24).unwrap_or(s.len());
        if !allow_assist
            && (is_assist_own_kill_text(raw, Some(&profile))
                || is_assist_own_kill_text(&s[whole.start()..probe_end], Some(&profile)))
        {
            continue;
        }
        let victim = clean_subject(&s[victim_start..victim_end], p);
        let key = subject_key(&victim, p);
        let is_self = match p {
            GameLanguage::ZhHans => ["你", "ni"].contains(&key.as_str()),
            GameLanguage::ZhHant => ["您", "你", "ni"].contains(&key.as_str()),
            GameLanguage::En => key == "you",
        };
        if key.is_empty() || is_self {
            continue;
        }
        let action_match = m.name("action").unwrap();
        let action = canonical_action(action_match.as_str(), p);
        let mut weapon = weapon_from_prefix(&s[whole.start()..action_match.start()], p);
        if weapon.is_empty() {
            weapon = clean_weapon(captured_weapon, p)
        }
        out.push(TextEvent {
            target: "own-kill".into(),
            method: "paddle-own-kill-text".into(),
            action: action.into(),
            subject: victim,
            key: format!("own-kill:{action}:{key}"),
            text: raw.into(),
            weapon,
        });
    }
    out
}

pub fn extract_self_events(text: &str, profile: Option<&LanguageProfile>) -> Vec<TextEvent> {
    let profile = resolve_profile(profile);
    let p = profile.language;
    let s = normalize_text(text);
    let Some(method) = classify_self_text(&s, Some(&profile)) else {
        return vec![];
    };
    if self_zone(&s, p) {
        return vec![TextEvent {
            target: "self-death".into(),
            method: method.into(),
            action: "zone-downed".into(),
            subject: "zone".into(),
            key: "self-death:zone".into(),
            text: s,
            weapon: String::new(),
        }];
    }
    let (start, raw, captured_weapon) = strict_capture(&s, p)
        .or_else(|| {
            fuzzy_re(p).captures(&s).map(|m| {
                let a = m.name("action").unwrap();
                (a.start(), a.as_str().into(), String::new())
            })
        })
        .unwrap();
    let action = canonical_action(&raw, p);
    let actor = &s[..start];
    let actor_key_source: String = actor
        .chars()
        .rev()
        .take(32)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let key = subject_key(&actor_key_source, p);
    let mut weapon = weapon_from_prefix(actor, p);
    if weapon.is_empty() {
        weapon = captured_weapon
    }
    vec![TextEvent {
        target: "self-death".into(),
        method: method.into(),
        action: action.into(),
        subject: actor.into(),
        key: format!(
            "self-death:{action}:{}",
            if key.is_empty() { "unknown" } else { &key }
        ),
        text: s,
        weapon,
    }]
}

pub fn extract_text_events(
    text: &str,
    target: Target,
    profile: Option<&LanguageProfile>,
) -> Vec<TextEvent> {
    let mut out = Vec::new();
    if target.includes_self() {
        out.extend(extract_self_events(text, profile));
    }
    if target.includes_own() {
        out.extend(extract_own_kill_events(text, false, profile));
    }
    out
}
pub fn classify_own_kill_text(text: &str, profile: Option<&LanguageProfile>) -> Option<String> {
    extract_own_kill_events(text, false, profile)
        .first()
        .map(|e| e.method.clone())
}
pub fn classify_target_kind(
    text: &str,
    target: Target,
    profile: Option<&LanguageProfile>,
) -> Option<(String, String)> {
    extract_text_events(text, target, profile)
        .first()
        .map(|e| (e.method.clone(), e.target.clone()))
}
pub fn has_own_kill_candidate(text: &str, profile: Option<&LanguageProfile>) -> bool {
    let p = resolve_profile(profile).language;
    let s = normalize_text(text);
    own_starts(&s, p).captures_iter(&s).any(|m| {
        let tail = &s[m.get(0).unwrap().end()..];
        match p {
            GameLanguage::ZhHans => !tail.starts_with("你"),
            GameLanguage::ZhHant => !tail.starts_with("您"),
            GameLanguage::En => !cached_regex!(r"(?i)^YOU").is_match(tail),
        }
    })
}

fn split_key(e: &TextEvent) -> (&str, &str, &str) {
    let mut i = e.key.splitn(3, ':');
    (
        i.next().unwrap_or(&e.target),
        i.next().unwrap_or(&e.action),
        i.next().unwrap_or(&e.subject),
    )
}
fn stable_subject(a: &str, b: &str) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    };
    let ratio = min(a.chars().count(), b.chars().count()) as f64
        / max(a.chars().count(), b.chars().count()) as f64;
    if ratio >= 0.75
        && a.chars().count() >= 4
        && b.chars().count() >= 4
        && (a.contains(b) || b.contains(a))
    {
        return true;
    }
    ratio >= 0.75 && sequence_ratio(a, b) >= 0.86
}
/// Ratcliff/Obershelp matching blocks, matching difflib.SequenceMatcher's ratio semantics.
fn sequence_ratio(a: &str, b: &str) -> f64 {
    use std::collections::{HashMap, HashSet};
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    // SequenceMatcher's autojunk index is built once from the entire second
    // sequence, not independently for recursive subranges.
    let mut frequencies = HashMap::new();
    for &c in &b {
        *frequencies.entry(c).or_insert(0usize) += 1;
    }
    let popular: HashSet<char> = frequencies
        .into_iter()
        .filter(|(_, n)| b.len() >= 200 && *n > b.len() / 100 + 1)
        .map(|(c, _)| c)
        .collect();
    let mut pending = vec![(0, a.len(), 0, b.len())];
    let mut total = 0;
    while let Some((alo, ahi, blo, bhi)) = pending.pop() {
        let mut best = (alo, blo, 0);
        let mut previous = HashMap::new();
        for i in alo..ahi {
            let mut current = HashMap::new();
            for j in blo..bhi {
                if a[i] != b[j] || popular.contains(&b[j]) {
                    continue;
                }
                let n = if j > 0 {
                    previous.get(&(j - 1)).copied().unwrap_or(0)
                } else {
                    0
                } + 1;
                current.insert(j, n);
                if n > best.2 {
                    best = (i + 1 - n, j + 1 - n, n);
                }
            }
            previous = current;
        }
        while best.0 > alo && best.1 > blo && a[best.0 - 1] == b[best.1 - 1] {
            best.0 -= 1;
            best.1 -= 1;
            best.2 += 1;
        }
        while best.0 + best.2 < ahi
            && best.1 + best.2 < bhi
            && a[best.0 + best.2] == b[best.1 + best.2]
        {
            best.2 += 1;
        }
        if best.2 == 0 {
            continue;
        }
        total += best.2;
        if alo < best.0 && blo < best.1 {
            pending.push((alo, best.0, blo, best.1));
        }
        if best.0 + best.2 < ahi && best.1 + best.2 < bhi {
            pending.push((best.0 + best.2, ahi, best.1 + best.2, bhi));
        }
    }
    if a.is_empty() && b.is_empty() {
        1.0
    } else {
        2.0 * total as f64 / (a.len() + b.len()) as f64
    }
}
fn noisy_subject(a: &str, b: &str, target: &str) -> bool {
    if stable_subject(a, b) {
        return true;
    };
    if target == "self-death" {
        let anchor = |s: &str| {
            cached_regex!(r"\[[a-z0-9]+\][a-z0-9_-]{3,}")
                .find(
                    &s.chars()
                        .filter(|c| c.is_ascii() && (c.is_alphanumeric() || "_-[]".contains(*c)))
                        .flat_map(char::to_lowercase)
                        .collect::<String>(),
                )
                .map(|m| m.as_str().to_string())
        };
        if anchor(a).is_some() && anchor(a) == anchor(b) {
            return true;
        }
    }
    let compact = |s: &str| {
        s.chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    let (x, y) = (compact(a), compact(b));
    if x.is_empty() || y.is_empty() {
        return false;
    };
    let (short, long) = if x.len() <= y.len() {
        (&x, &y)
    } else {
        (&y, &x)
    };
    if short.len() >= 5 && long.contains(short) {
        return true;
    };
    let common = short
        .chars()
        .zip(long.chars())
        .take_while(|(x, y)| x == y)
        .count();
    (x.chars().any(|c| c.is_ascii_digit()) || y.chars().any(|c| c.is_ascii_digit()))
        && common >= 6
        && common as f64 / short.len() as f64 >= 0.75
}
pub fn same_text_event(a: &TextEvent, b: &TextEvent) -> bool {
    if a.target != b.target || a.action != b.action {
        return false;
    };
    if a.key == b.key {
        return true;
    };
    let (_, _, x) = split_key(a);
    let (_, _, y) = split_key(b);
    stable_subject(x, y)
}
pub fn same_noisy_close_event(a: &TextEvent, b: &TextEvent) -> bool {
    if same_text_event(a, b) {
        true
    } else if a.target != b.target || a.action != b.action {
        false
    } else {
        let (_, _, x) = split_key(a);
        let (_, _, y) = split_key(b);
        noisy_subject(x, y, &a.target)
    }
}

/// Compare subjects for a knock/elimination follow-up. Self-death uses the actor
/// portion before its action; own-kill uses the normalized key subject.
pub fn same_followup_subject(
    a: &TextEvent,
    b: &TextEvent,
    profile: Option<&LanguageProfile>,
) -> bool {
    if a.target != b.target {
        return false;
    }
    let p = resolve_profile(profile).language;
    let left = if a.target == "self-death" {
        subject_key(&self_actor_subject(&a.subject, p), p)
    } else {
        split_key(a).2.to_owned()
    };
    let right = if b.target == "self-death" {
        subject_key(&self_actor_subject(&b.subject, p), p)
    } else {
        split_key(b).2.to_owned()
    };
    noisy_subject(&left, &right, &a.target)
}

fn self_actor_subject(actor_text: &str, p: GameLanguage) -> String {
    let text = normalize_text(actor_text);
    let without_weapon = weapon_re(p)
        .find(&text)
        .map(|m| &text[..m.start()])
        .unwrap_or(&text);
    trim_punct(without_weapon).chars().take(48).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    #[derive(Debug, Deserialize)]
    struct GoldenCase {
        language: String,
        target: String,
        text: String,
        events: Vec<GoldenEvent>,
    }
    #[derive(Debug, Deserialize)]
    struct GoldenEvent {
        target: String,
        method: String,
        action: String,
        subject: String,
        key: String,
        text: String,
        weapon: String,
    }
    #[test]
    fn normalize_unicode_space() {
        assert_eq!(normalize_text(" YOU\u{2003} KILLED\nX "), "YOUKILLEDX");
    }
    #[test]
    fn chinese_events_and_weapon() {
        let p = language_profile("zh-Hans").unwrap();
        let e = extract_text_events("你用M416淘汰了Player123", Target::Both, Some(&p));
        assert_eq!(e[0].key, "own-kill:eliminate:player123");
        assert_eq!(e[0].weapon, "M416");
    }
    #[test]
    fn english_events() {
        let p = language_profile("en").unwrap();
        let e = extract_text_events(
            "YOU KNOCKED OUT OpponentAlpha with Beryl M762",
            Target::Both,
            Some(&p),
        );
        assert_eq!(e[0].key, "own-kill:knock:opponentalpha");
        assert_eq!(e[0].weapon, "BerylM762");
    }
    #[test]
    fn multiple_events() {
        let p = default_language_profile();
        let e = extract_text_events(
            "你用M416击倒了EnemyA你用M416击倒了EnemyB",
            Target::OwnKill,
            Some(&p),
        );
        assert_eq!(e.len(), 2);
    }
    #[test]
    fn assist_allowed_keeps_primary_event() {
        let p = default_language_profile();
        let e = extract_own_kill_events("你用M416淘汰了EnemyA助攻", true, Some(&p));
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].subject, "EnemyA");
        assert_eq!(e[0].key, "own-kill:eliminate:enemya");
    }
    #[test]
    fn self_empty_actor_and_followup_weapon() {
        let p = default_language_profile();
        let empty = extract_text_events("击倒了你", Target::SelfDeath, Some(&p));
        assert_eq!(empty[0].key, "self-death:knock:unknown");
        let a = extract_text_events("EnemyA用M416击倒了你", Target::SelfDeath, Some(&p));
        let b = extract_text_events("EnemyA用AKM淘汰了你", Target::SelfDeath, Some(&p));
        assert!(same_followup_subject(&a[0], &b[0], Some(&p)));
    }
    #[test]
    fn assist_scoreboard_is_not_primary() {
        let p = default_language_profile();
        assert!(is_assist_own_kill_text(
            "你用M416淘汰了EnemyA助攻",
            Some(&p)
        ));
        assert!(!is_assist_own_kill_text(
            "你终于淘汰了EnemyA淘汰数2协助次数",
            Some(&p)
        ));
    }
    #[test]
    fn sequence_similarity() {
        assert!(
            sequence_ratio("abc123", "abc12Q") == 1.0 / 1.0
                || sequence_ratio("abc123", "abc12Q") > 0.8
        );
    }
    #[test]
    fn frozen_multilingual_event_fixture() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/events_golden.json");
        let cases: Vec<GoldenCase> =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        for case in cases {
            let profile = language_profile(&case.language).unwrap();
            let target = match case.target.as_str() {
                "self-death" => Target::SelfDeath,
                "own-kill" => Target::OwnKill,
                "both" => Target::Both,
                other => panic!("unknown target {other}"),
            };
            let actual = extract_text_events(&case.text, target, Some(&profile));
            assert_eq!(actual.len(), case.events.len(), "{}", case.text);
            for (got, want) in actual.iter().zip(case.events.iter()) {
                assert_eq!(
                    (
                        &got.target,
                        &got.method,
                        &got.action,
                        &got.subject,
                        &got.key,
                        &got.text,
                        &got.weapon
                    ),
                    (
                        &want.target,
                        &want.method,
                        &want.action,
                        &want.subject,
                        &want.key,
                        &want.text,
                        &want.weapon
                    ),
                    "{}",
                    case.text
                );
            }
        }
    }
}
