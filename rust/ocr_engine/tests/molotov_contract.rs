//! Frozen Python language-table + re.IGNORECASE contract at the public helper.
use pubg_ocr_engine::events::{is_molotov_weapon, language_profile};
#[test]
fn molotov_terms_follow_python_unicode_ignorecase_without_full_casefold() {
    for (lang, positive, negative) in [
        (
            "en",
            vec![
                "MOLOTOV",
                "FİREBOMB",
                "FıRE-BOMB",
                "GAſCAN",
                "C4",
                "F\u{1c}IREBOMB",
            ],
            vec!["GAßCAN", "燃烧弹"],
        ),
        (
            "zh-Hans",
            vec!["燃烧弹", "燃燒彈", "燃烧瓶", "燃燒瓶", "molotov"],
            vec!["汽油彈", "FIREBOMB"],
        ),
        (
            "zh-Hant",
            vec!["燃燒彈", "汽油彈", "molotov"],
            vec!["燃烧瓶", "FIREBOMB"],
        ),
    ] {
        let profile = language_profile(lang).unwrap();
        for text in positive {
            assert!(is_molotov_weapon(text, Some(&profile)), "{lang}: {text}");
        }
        for text in negative {
            assert!(!is_molotov_weapon(text, Some(&profile)), "{lang}: {text}");
        }
    }
}
