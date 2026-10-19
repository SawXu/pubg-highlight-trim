//! Filesystem changes are preceded by validation of every output, including split merge destinations.
use crate::{config::Config, source_files::Inputs};
use std::{
    fs,
    io::{self, Write},
    path::{Component, Path, PathBuf},
};
#[derive(Clone, Debug)]
pub struct OutputPaths {
    pub directory: PathBuf,
    pub merged: PathBuf,
    pub split_merges: Vec<(String, PathBuf)>,
}
/// Resolve existing ancestors so junctions and symbolic links cannot hide an input alias.
pub fn resolved(path: &Path) -> io::Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut clean = PathBuf::new();
    for c in absolute.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                clean.pop();
            }
            _ => clean.push(c.as_os_str()),
        }
    }
    let mut ancestor = clean.as_path();
    let mut tail = Vec::new();
    while !ancestor.exists() {
        tail.push(
            ancestor
                .file_name()
                .ok_or_else(|| io::Error::other("cannot resolve output path"))?
                .to_os_string(),
        );
        ancestor = ancestor
            .parent()
            .ok_or_else(|| io::Error::other("cannot resolve output parent"))?;
    }
    let mut p = crate::source_files::canonical(ancestor)?;
    for part in tail.into_iter().rev() {
        p.push(part)
    }
    Ok(p)
}
fn key(path: &Path) -> String {
    let s = path.to_string_lossy().into_owned();
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s
    }
}
fn same(a: &Path, b: &Path) -> bool {
    key(a) == key(b)
}
fn ancestor_of(a: &Path, b: &Path) -> bool {
    if cfg!(windows) {
        let a = key(a);
        let b = key(b);
        b == a || b.starts_with(&(a.trim_end_matches(['\\', '/']).to_owned() + "\\"))
    } else {
        b.starts_with(a)
    }
}
pub fn unique_path(path: &Path) -> io::Result<PathBuf> {
    if !path.exists() {
        return Ok(path.to_path_buf());
    }
    for i in 1..1000 {
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        let ext = path
            .extension()
            .map(|s| format!(".{}", s.to_string_lossy()))
            .unwrap_or_default();
        let p = path.with_file_name(format!("{stem}_{i}{ext}"));
        if !p.exists() {
            return Ok(p);
        }
    }
    Err(io::Error::other(format!(
        "Could not create unique path for {}",
        path.display()
    )))
}
fn unique_dir(path: &Path) -> io::Result<PathBuf> {
    if !path.exists() {
        return Ok(path.to_path_buf());
    }
    for i in 1..1000 {
        let p = path.with_file_name(format!(
            "{}_{i}",
            path.file_name().unwrap_or_default().to_string_lossy()
        ));
        if !p.exists() {
            return Ok(p);
        }
    }
    Err(io::Error::other("Could not create unique directory"))
}
fn chinese_ui() -> bool {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetUserDefaultUILanguage() -> u16;
        }
        unsafe { GetUserDefaultUILanguage() & 0x3ff == 4 }
    }
    #[cfg(not(windows))]
    {
        false
    }
}
impl OutputPaths {
    /// Pure preflight: this method never creates, deletes, or truncates anything.
    pub fn plan(c: &Config, inputs: &Inputs) -> io::Result<Self> {
        let base = &inputs.base_folder;
        let stem = inputs
            .input_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy();
        let requested = c.output_dir.clone().unwrap_or_else(|| {
            base.join(if inputs.single_file {
                format!("{stem}_pubg_trim_clips")
            } else {
                "pubg_highlight_trim_output".into()
            })
        });
        let directory = resolved(&if c.overwrite {
            requested.clone()
        } else {
            unique_dir(&requested)?
        })?;
        for source in &inputs.sources {
            let src = resolved(&source.path)?;
            if ancestor_of(&directory, &src) {
                return Err(io::Error::other(format!(
                    "Output directory must not contain an input file or its ancestor: {}",
                    directory.display()
                )));
            }
        }
        if directory.exists() && !directory.is_dir() {
            return Err(io::Error::other("Output directory is not a directory"));
        }
        if c.overwrite && directory.join("originals").exists() {
            return Err(io::Error::other(
                "Refusing to overwrite an output directory containing archived originals",
            ));
        }
        let merged = c.merge.as_ref().and_then(|m| m.clone()).unwrap_or_else(|| {
            directory.join(if inputs.single_file {
                format!("{stem}_pubg_trim.mp4")
            } else {
                "pubg_highlight_trim_merged.mp4".into()
            })
        });
        let mut split_merges = Vec::new();
        if c.split_targets() {
            for target in ["self-death", "own-kill"] {
                let dest = if let Some(Some(p)) = &c.merge {
                    let stem = p.file_stem().unwrap_or_default().to_string_lossy();
                    let ext = p
                        .extension()
                        .map(|e| format!(".{}", e.to_string_lossy()))
                        .unwrap_or_default();
                    p.with_file_name(format!("{stem}_{target}{ext}"))
                } else {
                    let label = if chinese_ui() {
                        if target == "self-death" {
                            "被击杀集锦"
                        } else {
                            "击杀集锦"
                        }
                    } else {
                        target
                    };
                    directory.join(format!("{label}.mp4"))
                };
                split_merges.push((target.into(), dest));
            }
        }
        let destinations = if c.split_targets() {
            split_merges.iter().map(|(_, p)| p).collect::<Vec<_>>()
        } else {
            vec![&merged]
        };
        // Even a naming-only merge path is validated before removing an existing output tree.
        for dest in destinations {
            let dest = resolved(dest)?;
            for src in &inputs.sources {
                if same(&dest, &resolved(&src.path)?)
                    || (dest.exists() && same_file::is_same_file(&dest, &src.path)?)
                {
                    return Err(io::Error::other(format!(
                        "Merge output must not overwrite an input file: {}",
                        dest.display()
                    )));
                }
            }
            if dest.is_dir() {
                return Err(io::Error::other("Merge output is a directory"));
            }
        }
        let merged = if !c.overwrite && !c.split_targets() {
            unique_path(&merged)?
        } else {
            merged
        };
        for (_, p) in &mut split_merges {
            if !c.overwrite {
                *p = unique_path(p)?
            }
        }
        Ok(Self {
            directory,
            merged: resolved(&merged)?,
            split_merges: split_merges
                .into_iter()
                .map(|(t, p)| Ok((t, resolved(&p)?)))
                .collect::<io::Result<_>>()?,
        })
    }
    pub fn prepare(&self, c: &Config, inputs: &Inputs) -> io::Result<()> {
        let checked = Self::plan(c, inputs)?;
        if checked.directory != self.directory
            || checked.merged != self.merged
            || checked.split_merges != self.split_merges
        {
            return Err(io::Error::other(
                "Output paths changed after preflight; refusing destructive cleanup",
            ));
        }
        if c.overwrite && self.directory.exists() {
            fs::remove_dir_all(&self.directory)?
        }
        if let Some(parent) = self.directory.parent() {
            fs::create_dir_all(parent)?
        }
        fs::create_dir(&self.directory)
    }
}
/// Archive only the exact sources whose completed outputs were verified, never basename matches.
/// A destination is reserved with create_new; cross-volume copies are durable before source removal.
pub fn archive_sources(sources: &[PathBuf], outdir: &Path) -> io::Result<(PathBuf, usize, usize)> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    fn hash(path: &Path) -> io::Result<Vec<u8>> {
        let mut f = fs::File::open(path)?;
        let mut d = Sha256::new();
        let mut b = [0u8; 65536];
        loop {
            let n = f.read(&mut b)?;
            if n == 0 {
                break;
            }
            d.update(&b[..n]);
        }
        Ok(d.finalize().to_vec())
    }
    fn journal(path: &Path, rows: &[Vec<String>]) -> io::Result<()> {
        // Publish a durable journal before any source unlink; a crash may leave an
        // extra copy, but never an untracked sole copy of the user's original.
        let temporary = unique_path(&path.with_extension("csv.tmp"))?;
        let mut owned = false;
        let result = (|| {
            let file = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary)?;
            owned = true;
            let mut writer = io::BufWriter::new(file);
            writer.write_all(b"\xef\xbb\xbf")?;
            let mut csv = csv::WriterBuilder::new()
                .terminator(csv::Terminator::CRLF)
                .from_writer(writer);
            csv.write_record(["Name", "SourcePath", "ArchivePath"])?;
            for row in rows {
                csv.write_record(row)?
            }
            csv.flush()?;
            csv.get_ref().get_ref().sync_all()?;
            drop(csv);
            crate::persistent_cache::atomic_replace(&temporary, path)
        })();
        if owned {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
    let originals = outdir.join("originals");
    fs::create_dir_all(&originals)?;
    let journal_path = outdir.join("moved_sources.csv");
    let mut rows = Vec::new();
    journal(&journal_path, &rows)?;
    let (mut moved, mut failed) = (0, 0);
    for src in sources {
        crate::cancellation::check().map_err(io::Error::other)?;
        let dest = unique_path(
            &originals.join(
                src.file_name()
                    .ok_or_else(|| io::Error::other("source has no filename"))?,
            ),
        )?;
        let temporary = unique_path(&dest.with_extension("mp4.partial"))?;
        let mut owned = false;
        let result = (|| -> io::Result<()> {
            let mut input = fs::File::open(src)?;
            let mut output = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary)?;
            owned = true;
            let expected = input.metadata()?.len();
            let copied = io::copy(&mut input, &mut output)?;
            output.flush()?;
            output.sync_all()?;
            drop(output);
            if copied != expected
                || input.metadata()?.len() != expected
                || hash(src)? != hash(&temporary)?
            {
                return Err(io::Error::other("source changed while archiving"));
            }
            drop(input);
            crate::cancellation::check().map_err(io::Error::other)?;
            crate::media_output::publish_file(&temporary, &dest, false)?;
            rows.push(vec![
                src.file_name().unwrap().to_string_lossy().into_owned(),
                src.display().to_string(),
                dest.display().to_string(),
            ]);
            if let Err(e) = journal(&journal_path, &rows) {
                rows.pop();
                return Err(e);
            }
            crate::cancellation::check().map_err(io::Error::other)?;
            fs::remove_file(src)?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                moved += 1;
                println!(
                    "ARCHIVE {} -> {}",
                    src.file_name().unwrap().to_string_lossy(),
                    dest.display()
                );
            }
            Err(e) => {
                failed += 1;
                if owned {
                    let _ = fs::remove_file(&temporary);
                }
                // If published, keep the verified complete copy even if journal
                // or source removal failed. The original is still present.
                eprintln!("WARNING could not archive {}: {e}", src.display());
            }
        }
    }
    Ok((originals, moved, failed))
}
