use clap::Parser;
use pubg_ocr_engine::{
    config::Config,
    output_files::{archive_sources, OutputPaths},
    source_files::{discover_candidates, language_counts, read_candidates, select},
};
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
            "pubg-files-{}-{}",
            std::process::id(),
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
    fn file(&self, n: &str) -> PathBuf {
        let p = self.0.join(n);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, b"original fixture").unwrap();
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
fn directory_discovery_preserves_target_language_timestamp_and_recursion_rules() {
    let t = Temp::new();
    for n in [
        "z 2025.01.01 - 10.00.00.淘汰.DVR.mp4",
        "a 2025.01.02 - 10.00.00.單次擊殺.DVR.mp4",
        "b 2025.01.03 - 10.00.00.Match end.DVR_2.mp4",
        "arbitrary.mp4",
        "x.死亡畫面.DVR.mp4",
        "nested/deep.单次淘汰.DVR.mp4",
    ] {
        t.file(n);
    }
    let c = config(&[t.0.to_str().unwrap()]);
    let inputs = select(&c).unwrap();
    assert_eq!(inputs.sources.len(), 3);
    assert_eq!(inputs.sources[0].language.code(), "zh-Hant"); // ambiguous 淘汰 follows the dominant unambiguous Traditional input
    assert_eq!(inputs.sources[1].language.code(), "zh-Hant");
    assert!(inputs.sources[2].match_end && inputs.sources[2].force_full_scan);
    assert_eq!(language_counts(&inputs)["en"], 1);
    let c = config(&[t.0.to_str().unwrap(), "--recursive", "--target", "own-kill"]);
    assert_eq!(select(&c).unwrap().sources.len(), 3);
    let c = config(&[
        t.0.to_str().unwrap(),
        "--game-lang",
        "zh-Hans",
        "--target",
        "self-death",
    ]);
    assert_eq!(select(&c).unwrap().sources.len(), 1);
}
#[test]
fn explicit_files_are_chronological_and_never_target_filtered() {
    let t = Temp::new();
    let a = t.file("b.Death.mp4");
    let b = t.file("a.单次淘汰.mp4");
    let c = config(&[
        "--files",
        a.to_str().unwrap(),
        b.to_str().unwrap(),
        "--target",
        "self-death",
    ]);
    let inputs = select(&c).unwrap();
    assert_eq!(inputs.sources.len(), 2);
    assert_eq!(inputs.sources[0].language.code(), "zh-Hans");
    assert_eq!(inputs.sources[1].language.code(), "en");
    assert!(inputs.sources[0].path.ends_with("a.单次淘汰.mp4"));
}
#[test]
fn multilingual_directory_selection_preserves_original_source_fixture_sets() {
    for (language, names, own, death) in [
        (
            "zh-Hans",
            vec![
                "a.淘汰.DVR.mp4",
                "b.被击倒.DVR_1.mp4",
                "c.单次淘汰.DVR.mp4",
                "d.双次淘汰.DVR.mp4",
                "e.比赛结束.DVR.mp4",
                "f.淘汰画面.DVR.mp4",
            ],
            vec![2, 3, 4],
            vec![0, 1],
        ),
        (
            "zh-Hant",
            vec![
                "a.死亡.DVR.mp4",
                "b.擊倒.DVR.mp4",
                "c.單次擊殺.DVR.mp4",
                "d.多重擊殺.DVR.mp4",
                "e.對戰結束.DVR.mp4",
                "f.死亡畫面.DVR.mp4",
            ],
            vec![1, 2, 3, 4],
            vec![0],
        ),
        (
            "en",
            vec![
                "a.Knockouted.DVR.mp4",
                "b.Death.DVR.mp4",
                "c.Knockout.DVR.mp4",
                "d.Single kill.DVR.mp4",
                "e.Double kill.DVR.mp4",
                "f.Match end.DVR.mp4",
                "g.End of match.DVR.mp4",
                "h.Death cam.DVR.mp4",
            ],
            vec![2, 3, 4, 5, 6],
            vec![0, 1],
        ),
    ] {
        let t = Temp::new();
        for name in &names {
            t.file(name);
        }
        for (target, expected) in [
            ("self-death", death.clone()),
            ("own-kill", own.clone()),
            ("both", {
                let mut all = death.clone();
                all.extend(own);
                all.sort();
                all
            }),
        ] {
            let c = config(&[
                t.0.to_str().unwrap(),
                "--game-lang",
                language,
                "--target",
                target,
            ]);
            let actual: Vec<_> = select(&c)
                .unwrap()
                .sources
                .into_iter()
                .map(|source| {
                    source
                        .path
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned()
                })
                .collect();
            let expected: Vec<_> = expected.into_iter().map(|i| names[i].to_string()).collect();
            assert_eq!(actual, expected, "{language} / {target}");
        }
    }
}

#[test]
fn automatic_language_voting_excludes_out_of_target_context_but_keeps_explicit_language() {
    for (names, expected) in [
        (vec!["a.淘汰.DVR.mp4"], vec!["zh-Hans"]),
        (
            vec!["a.淘汰.DVR.mp4", "b.對戰結束.mp4", "c.單次擊殺.DVR.mp4"],
            vec!["zh-Hans", "zh-Hant", "zh-Hant"],
        ),
        (
            vec!["a.死亡.DVR.mp4", "b.淘汰.DVR.mp4", "c.比赛结束.mp4"],
            vec!["zh-Hant", "zh-Hant", "zh-Hans"],
        ),
    ] {
        let t = Temp::new();
        let paths: Vec<_> = names.iter().map(|name| t.file(name)).collect();
        let mut args = vec!["--target", "self-death", "--files"];
        args.extend(paths.iter().map(|path| path.to_str().unwrap()));
        let actual: Vec<_> = select(&config(&args))
            .unwrap()
            .sources
            .into_iter()
            .map(|source| source.language.code())
            .collect();
        assert_eq!(actual, expected, "{names:?}");
    }
    let t = Temp::new();
    for name in [
        "a.淘汰.DVR.mp4",
        "b.對戰結束.DVR.mp4",
        "c.Single kill.DVR.mp4",
    ] {
        t.file(name);
    }
    let actual = select(&config(&[t.0.to_str().unwrap(), "--target", "self-death"])).unwrap();
    assert_eq!(actual.sources.len(), 1);
    assert_eq!(actual.sources[0].language.code(), "zh-Hans");
    for (name, language) in [
        ("a.對戰結束.mp4", "zh-Hant"),
        ("b.End of match.DVR.mp4", "en"),
        ("c.比赛结束.DVR.mp4", "zh-Hans"),
    ] {
        let path = t.file(name);
        let actual = select(&config(&[path.to_str().unwrap(), "--target", "self-death"])).unwrap();
        assert_eq!(actual.sources[0].language.code(), language);
    }
}

#[test]
fn candidate_reader_supports_bom_quoted_names_and_ocr_event_precedence() {
    let t = Temp::new();
    let p = t.file("hints.csv");
    fs::write(&p,"\u{feff}Name,OcrEventSec,EventSec\r\n\"录像,一.mp4\",4.25,9\r\n\"录像,一.mp4\",,6\r\nignored,garbage,3\r\n").unwrap();
    let c = read_candidates(Some(&p)).unwrap();
    assert_eq!(c["录像,一.mp4"], vec![4.25, 6.0]);
    assert!(!c.contains_key("ignored"));
}
#[test]
fn automatic_candidate_discovery_checks_siblings_and_can_be_disabled() {
    let t = Temp::new();
    let p = t.file("input/source.mp4");
    let hint = t.file("fullscan_2s/candidate_events.csv");
    let c = config(&[p.to_str().unwrap()]);
    let inputs = select(&c).unwrap();
    let discovered = discover_candidates(&c, &inputs).expect("discover the sibling candidate CSV");
    // Hosted Windows TEMP may use an 8.3 spelling while selection resolves its
    // long spelling. Preserve the exact intended file, not a path presentation.
    assert!(same_file::is_same_file(&discovered, &hint).unwrap());
    let c = config(&[p.to_str().unwrap(), "--no-auto-candidate-csv"]);
    assert!(discover_candidates(&c, &inputs).is_none());
}
#[test]
fn split_merge_conflicts_are_rejected_before_output_cleanup() {
    let t = Temp::new();
    let src = t.file("merge_own-kill.mp4");
    let keep = t.file("output/keep.txt");
    let c = config(&[
        src.to_str().unwrap(),
        "-o",
        keep.parent().unwrap().to_str().unwrap(),
        "--merge",
        t.0.join("merge.mp4").to_str().unwrap(),
        "--split-merge-by-target",
        "-y",
    ]);
    let inputs = select(&c).unwrap();
    assert!(OutputPaths::plan(&c, &inputs).is_err());
    assert!(keep.exists());
    assert!(src.exists());
}
#[test]
fn overwrite_never_erases_an_archive_from_a_previous_run() {
    let t = Temp::new();
    let p = t.file("source.mp4");
    let archive = t.file("output/originals/other.mp4");
    let c = config(&[
        p.to_str().unwrap(),
        "-o",
        archive
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_str()
            .unwrap(),
        "-y",
    ]);
    assert!(OutputPaths::plan(&c, &select(&c).unwrap()).is_err());
    assert!(archive.exists());
}
#[test]
fn archiving_exact_paths_does_not_move_an_unprocessed_same_basename() {
    let t = Temp::new();
    let included = t.file("a/same.mp4");
    let skipped = t.file("b/same.mp4");
    let output = t.0.join("output");
    let (_, moved, failed) = archive_sources(&[included.clone()], &output).unwrap();
    assert_eq!((moved, failed), (1, 0));
    assert!(!included.exists());
    assert!(skipped.exists());
    assert_eq!(
        fs::read(output.join("originals/same.mp4")).unwrap(),
        b"original fixture"
    );
    assert!(fs::read(output.join("moved_sources.csv"))
        .unwrap()
        .starts_with(b"\xef\xbb\xbf"));
}
