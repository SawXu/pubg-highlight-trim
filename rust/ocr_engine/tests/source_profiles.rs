//! User-visible source selection matrix frozen from source_files.py/game_language_tables.py.
use clap::Parser;
use pubg_ocr_engine::{config::Config, source_files};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
static ID: AtomicUsize = AtomicUsize::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "pubg-source-matrix-{}-{}",
            std::process::id(),
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
    fn file(&self, n: &str) -> PathBuf {
        let p = self.0.join(n);
        fs::write(&p, b"fixture").unwrap();
        p
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn config(args: &[&str]) -> Config {
    Config::try_parse_from(std::iter::once("app").chain(args.iter().copied())).unwrap()
}
#[test]
fn every_language_filename_term_obeys_target_replay_and_dvr_boundaries() {
    let cases = [
        (
            "zh-Hans",
            vec!["被击倒", "淘汰"],
            vec!["单次淘汰", "双次淘汰", "多杀"],
            vec!["比赛结束"],
            vec!["淘汰画面", "击倒画面"],
        ),
        (
            "zh-Hant",
            vec!["被擊倒", "死亡", "淘汰"],
            vec!["單次擊殺", "雙殺", "多殺", "多重擊殺", "擊倒"],
            vec!["對戰結束"],
            vec!["死亡畫面", "淘汰畫面", "擊倒畫面"],
        ),
        (
            "en",
            vec!["Knockouted", "Death"],
            vec![
                "Knockout",
                "Single kill",
                "Double kill",
                "Triple kill",
                "Multi kill",
                "Multiple kill",
            ],
            vec![
                "Match end",
                "End of match",
                "Match ended",
                "Match complete",
                "Game over",
            ],
            vec!["Death cam", "DeathCam", "Kill cam", "Replay"],
        ),
    ];
    for (lang, own, kills, ends, replays) in cases {
        let t = Temp::new();
        for term in own.iter().chain(&kills).chain(&ends).chain(&replays) {
            t.file(&format!("record.{term}.DVR_2.mp4"));
        }
        t.file("arbitrary.mp4");
        t.file(&format!("record.{}.mp4", kills[0]));
        for (target, expected) in [
            ("self-death", own.clone()),
            (
                "own-kill",
                kills.iter().chain(&ends).copied().collect::<Vec<_>>(),
            ),
            (
                "both",
                own.iter()
                    .chain(&kills)
                    .chain(&ends)
                    .copied()
                    .collect::<Vec<_>>(),
            ),
        ] {
            let c = config(&[
                t.0.to_str().unwrap(),
                "--game-lang",
                lang,
                "--target",
                target,
            ]);
            let selected = source_files::select(&c).unwrap();
            let mut actual = selected
                .sources
                .iter()
                .map(|s| s.path.file_name().unwrap().to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            actual.sort();
            let mut expected = expected
                .iter()
                .map(|term| format!("record.{term}.DVR_2.mp4"))
                .collect::<Vec<_>>();
            expected.sort();
            assert_eq!(actual, expected, "{lang}/{target}");
            assert!(selected.sources.iter().all(|s| s.language.code() == lang));
        }
    }
}
#[test]
fn target_excluded_explicit_inputs_do_not_vote_on_ambiguous_self_death() {
    let t = Temp::new();
    let ambiguous = t.file("a.淘汰.DVR.mp4");
    let end = t.file("b.對戰結束.mp4");
    let kill = t.file("c.單次擊殺.DVR.mp4");
    let c = config(&[
        "--files",
        end.to_str().unwrap(),
        kill.to_str().unwrap(),
        ambiguous.to_str().unwrap(),
        "--target",
        "self-death",
    ]);
    let s = source_files::select(&c).unwrap();
    assert_eq!(
        s.sources
            .iter()
            .map(|s| s.language.code())
            .collect::<Vec<_>>(),
        ["zh-Hans", "zh-Hant", "zh-Hant"]
    );
    let death = t.file("d.死亡.DVR.mp4");
    let c = config(&[
        "--files",
        end.to_str().unwrap(),
        ambiguous.to_str().unwrap(),
        death.to_str().unwrap(),
        "--target",
        "self-death",
    ]);
    let s = source_files::select(&c).unwrap();
    assert_eq!(
        s.sources
            .iter()
            .map(|s| s.language.code())
            .collect::<Vec<_>>(),
        ["zh-Hant", "zh-Hant", "zh-Hant"]
    );
}
#[test]
fn explicit_match_end_language_inference_does_not_need_dvr_suffix() {
    for (n, lang) in [
        ("a.對戰結束.mp4", "zh-Hant"),
        ("a.對戰結束.DVR.mp4", "zh-Hant"),
        ("a.End of match.DVR.mp4", "en"),
        ("a.比赛结束.DVR.mp4", "zh-Hans"),
    ] {
        let t = Temp::new();
        let p = t.file(n);
        let c = config(&[p.to_str().unwrap(), "--target", "self-death"]);
        assert_eq!(
            source_files::select(&c).unwrap().sources[0].language.code(),
            lang
        );
    }
}
