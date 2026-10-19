use pubg_ocr_engine::media_output::MediaOutput;
use std::fs;
#[test]
fn staging_failure_preserves_existing_output_and_cleans_only_owned_partials() {
    let root = std::env::temp_dir().join(format!("pubg-stage-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let src = root.join("original.mp4");
    let dest = root.join("merged.mp4");
    fs::write(&src, b"precious original").unwrap();
    fs::write(&dest, b"previous complete output").unwrap();
    let temp;
    {
        let staging = MediaOutput::reserve(&dest, true, &[src.clone()]).unwrap();
        temp = staging.temporary().to_path_buf();
        fs::write(&temp, b"half encoded").unwrap();
    }
    assert!(!temp.exists());
    assert_eq!(fs::read(&dest).unwrap(), b"previous complete output");
    assert_eq!(fs::read(&src).unwrap(), b"precious original");
    let uncreated = root.join("new.mp4");
    {
        let staging = MediaOutput::reserve(&uncreated, false, &[src.clone()]).unwrap();
        assert!(
            !uncreated.exists(),
            "hard kill must not leave an empty final filename"
        );
        assert!(staging
            .temporary()
            .parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .ends_with(".partial"));
    }
    assert!(!uncreated.exists());
    let staging = MediaOutput::reserve(&uncreated, false, &[src.clone()]).unwrap();
    fs::write(staging.temporary(), b"owned complete").unwrap();
    fs::write(&uncreated, b"other process output").unwrap();
    assert!(staging.publish(&[src.clone()], false).is_err());
    assert_eq!(fs::read(&uncreated).unwrap(), b"other process output");
    let staging = MediaOutput::reserve(&dest, true, &[src.clone()]).unwrap();
    fs::write(staging.temporary(), b"new complete output").unwrap();
    staging.publish(&[src.clone()], false).unwrap();
    assert_eq!(fs::read(&dest).unwrap(), b"new complete output");
    assert!(MediaOutput::reserve(&src, true, &[src.clone()]).is_err());
    fs::remove_dir_all(root).unwrap();
}
