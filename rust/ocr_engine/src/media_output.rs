//! Owned staging for media outputs: failed encoding never replaces an existing result.
use crate::output_files::resolved;
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};
static SEQUENCE: AtomicUsize = AtomicUsize::new(0);
pub struct MediaOutput {
    temporary: PathBuf,
    destination: PathBuf,
    overwrite: bool,
    staging_directory: PathBuf,
}
impl MediaOutput {
    pub fn reserve(destination: &Path, overwrite: bool, inputs: &[PathBuf]) -> io::Result<Self> {
        let destination = resolved(destination)?;
        Self::validate(&destination, inputs)?;
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?
        }
        if !overwrite && destination.exists() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "output already exists",
            ));
        }
        let staging_directory = destination.with_file_name(format!(
            ".pubg-trim-{}-{}.partial",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&staging_directory)?;
        let temporary = staging_directory.join("output.mp4");
        Ok(Self {
            temporary,
            destination,
            overwrite,
            staging_directory,
        })
    }
    fn validate(destination: &Path, inputs: &[PathBuf]) -> io::Result<()> {
        let resolved = resolved(destination)?;
        for input in inputs {
            let source = crate::output_files::resolved(input)?;
            if resolved
                .to_string_lossy()
                .eq_ignore_ascii_case(&source.to_string_lossy())
                || (destination.exists() && same_file::is_same_file(destination, input)?)
            {
                return Err(io::Error::other(format!(
                    "Media output must not overwrite an input file: {}",
                    destination.display()
                )));
            }
        }
        Ok(())
    }
    pub fn temporary(&self) -> &Path {
        &self.temporary
    }
    /// Merge's concat list is published under the original public naming scheme.
    pub fn publish(self, inputs: &[PathBuf], concat: bool) -> io::Result<Option<PathBuf>> {
        crate::cancellation::check().map_err(io::Error::other)?;
        Self::validate(&self.destination, inputs)?;
        fs::OpenOptions::new()
            .write(true)
            .open(&self.temporary)?
            .sync_all()?;
        let list = if concat {
            let source = self.temporary.with_extension("concat.txt");
            let dest = self.destination.with_extension("concat.txt");
            Self::validate(&dest, inputs)?;
            publish_file(&source, &dest, self.overwrite)?;
            Some(dest)
        } else {
            None
        };
        publish_file(&self.temporary, &self.destination, self.overwrite)?;
        Ok(list)
    }
}
/// Exclusive publication without an empty final placeholder; hard kill leaves only .partial staging.
pub(crate) fn publish_file(source: &Path, destination: &Path, overwrite: bool) -> io::Result<()> {
    if overwrite {
        return crate::persistent_cache::atomic_replace(source, destination);
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "kernel32")]
        extern "system" {
            fn MoveFileExW(source: *const u16, destination: *const u16, flags: u32) -> i32;
        }
        let source = source
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let destination = destination
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        if unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), 0x8) } == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
    #[cfg(not(windows))]
    {
        fs::hard_link(source, destination)?;
        fs::remove_file(source)
    }
}
impl Drop for MediaOutput {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.temporary);
        let _ = fs::remove_file(self.temporary.with_extension("concat.txt"));
        let _ = fs::remove_dir(&self.staging_directory);
    }
}
