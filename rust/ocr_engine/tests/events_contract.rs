use pubg_ocr_engine::events::{
    classify_self_text, extract_text_events, is_assist_own_kill_text, is_delayed_own_elim_text,
    is_molotov_weapon, language_profile, Target,
};

#[test]
fn multiline_hud_isolated_from_previous_row() {
    use pubg_ocr_engine::events::{same_followup_subject, same_noisy_close_event, same_text_event};
    for (language, knock, eliminate, other) in [
        (
            "zh-Hans",
            "[TAG]EnemyAlpha用M416击倒了你",
            "[TAG]EnemyAlpha用AKM淘汰了你",
            "OpponentBravo用M416击倒了你",
        ),
        (
            "zh-Hant",
            "[TAG]EnemyAlpha以M416擊倒您",
            "[TAG]EnemyAlpha以MP5K擎殺您",
            "OpponentBravo以M416擊倒您",
        ),
        (
            "en",
            "EnemyAlpha KNOCKED YOU OUT with M416",
            "EnemyAlpha KILLED YOU with AKM",
            "OpponentBravo KNOCKED YOU OUT with M416",
        ),
    ] {
        let profile = language_profile(language).unwrap();
        let clean = extract_text_events(knock, Target::SelfDeath, Some(&profile));
        let noisy = extract_text_events(
            &format!("HUDNOISE EnemyB\r\n{knock}"),
            Target::SelfDeath,
            Some(&profile),
        );
        assert_eq!(
            noisy, clean,
            "{language}: HUD must not enter any event field"
        );
        let followup = extract_text_events(
            &format!("Different HUD\n{eliminate}"),
            Target::SelfDeath,
            Some(&profile),
        );
        assert!(same_followup_subject(
            &noisy[0],
            &followup[0],
            Some(&profile)
        ));
        let different = extract_text_events(
            &format!("HUDNOISE EnemyB\n{other}"),
            Target::SelfDeath,
            Some(&profile),
        );
        assert!(!same_text_event(&noisy[0], &different[0]));
        assert!(!same_noisy_close_event(&noisy[0], &different[0]));
        assert!(!same_followup_subject(
            &noisy[0],
            &different[0],
            Some(&profile)
        ));
    }
}

#[test]
fn multilingual_events_keep_historical_keys_weapons_and_perspectives() {
    let cases = [
        (
            "en",
            "YOU KNOCKED OUT OpponentAlpha with Beryl M762",
            "own-kill:knock:opponentalpha",
            "knock",
            "BerylM762",
        ),
        (
            "en",
            "YOU KILLED OpponentBravo with Beryl M762 2 KILLS",
            "own-kill:eliminate:opponentbravo",
            "eliminate",
            "BerylM762",
        ),
        (
            "en",
            "OpponentCharlie KNOCKED YOU out with ACE32",
            "self-death:knock:opponentcharlie",
            "knock",
            "ACE32",
        ),
        (
            "en",
            "OpponentDelta KILLS YOU with MP5K",
            "self-death:eliminate:opponentdelta",
            "eliminate",
            "MP5K",
        ),
        (
            "zh-Hant",
            "您以SLR爆頭擊倒OpponentAlpha",
            "own-kill:knock:opponentalpha",
            "knock",
            "SLR爆頭",
        ),
        (
            "zh-Hant",
            "您以BerylM762擊殺OpponentBravo2擊殺數",
            "own-kill:eliminate:opponentbravo",
            "eliminate",
            "BerylM762",
        ),
        (
            "zh-Hant",
            "OpponentCharlie以M416擊倒您",
            "self-death:knock:opponentcharlie",
            "knock",
            "M416",
        ),
        (
            "zh-Hant",
            "OpponentDelta以M16A4擊殺您",
            "self-death:eliminate:opponentdelta",
            "eliminate",
            "M16A4",
        ),
        (
            "zh-Hant",
            "您以BerylM762擊殺OpponentAlpha(148m)擊殺",
            "own-kill:eliminate:opponentalpha",
            "eliminate",
            "BerylM762",
        ),
        (
            "zh-Hans",
            "你用燃烧弹淘汰了OpponentAlpha淘汰数",
            "own-kill:eliminate:opponentalpha",
            "eliminate",
            "燃烧弹",
        ),
        (
            "zh-Hans",
            "OpponentAlpha用燃烧瓶淘汰了你",
            "self-death:eliminate:opponentalpha",
            "eliminate",
            "燃烧瓶",
        ),
    ];
    for (language, text, key, action, weapon) in cases {
        let profile = language_profile(language).unwrap();
        let events = extract_text_events(text, Target::Both, Some(&profile));
        assert_eq!(events.len(), 1, "{language}: {text}");
        assert_eq!(
            (&*events[0].key, &*events[0].action, &*events[0].weapon),
            (key, action, weapon),
            "{text}"
        );
    }
    assert!(language_profile("ja").is_err());
}

#[test]
fn falling_and_molotov_aliases_preserve_language_semantics() {
    for (language, texts) in [
        (
            "en",
            vec![
                "YOU WERE KNOCKED OUT BY FALLING",
                "YOU WERE KNOCKED OUT FROM FALLING",
                "YOU DIED FROM FALLING",
            ],
        ),
        ("zh-Hans", vec!["你因坠落遭击倒", "你因摔落倒地"]),
        ("zh-Hant", vec!["您因墜落遭擊倒"]),
    ] {
        let profile = language_profile(language).unwrap();
        for text in texts {
            let event = extract_text_events(text, Target::Both, Some(&profile)).remove(0);
            assert_eq!(event.key, "self-death:zone", "{text}");
            assert_eq!(event.action, "zone-downed", "{text}");
        }
    }
    let profile = language_profile("zh-Hant").unwrap();
    assert_eq!(
        classify_self_text("您因墜落遭擊倒", Some(&profile)),
        Some("paddle-zone-self-downed-text")
    );
    for weapon in ["燃燒彈", "汽油彈"] {
        assert!(is_molotov_weapon(weapon, Some(&profile)));
    }
}

#[test]
fn assist_and_delayed_annotations_do_not_turn_into_primary_events() {
    for (language, text) in [
        ("zh-Hans", "你用M416淘汰了OpponentAlpha助攻"),
        ("zh-Hant", "您以M416擊殺OpponentAlpha助攻"),
        ("zh-Hant", "您以AKM擊殺OpponentBravo助殺"),
        ("en", "YOU KILLED OpponentAlpha with M416 1 ASSIST"),
    ] {
        let profile = language_profile(language).unwrap();
        assert!(is_assist_own_kill_text(text, Some(&profile)), "{text}");
        assert!(
            extract_text_events(text, Target::Both, Some(&profile)).is_empty(),
            "{text}"
        );
    }
    for (language, delayed, mixed) in [
        (
            "en",
            "YOU finally KILLED OpponentAlpha",
            "YOU KNOCKED OUT OpponentAlpha with M416 YOU finally KILLED OpponentAlpha 1 KILL",
        ),
        (
            "zh-Hans",
            "你终于淘汰了OpponentAlpha",
            "你用M416击倒了OpponentAlpha你终于淘汰了OpponentAlpha淘汰数1",
        ),
        (
            "zh-Hant",
            "您終於擊殺OpponentAlpha擊殺",
            "您以M416擊倒OpponentAlpha您終於擊殺OpponentAlpha擊殺",
        ),
    ] {
        let profile = language_profile(language).unwrap();
        assert!(
            is_delayed_own_elim_text(delayed, Some(&profile)),
            "{delayed}"
        );
        let events = extract_text_events(mixed, Target::Both, Some(&profile));
        assert_eq!(events.len(), 1, "{mixed}");
        assert_eq!(events[0].subject, "OpponentAlpha");
        assert_eq!(events[0].action, "knock");
    }
    let profile = language_profile("zh-Hant").unwrap();
    assert!(!is_assist_own_kill_text(
        "您以P90擊殺OpponentCharlie擊殺數",
        Some(&profile)
    ));
}
