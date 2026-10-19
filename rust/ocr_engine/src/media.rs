//! FFmpeg boundary: one software-decoded process per seek (ADR-0002).
use crate::frame::BgrFrame;
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub enum MediaError {
    Io(io::Error),
    VideoUnreadable(String),
    InvalidInput(String),
    CommandFailed(String),
    RuntimeUnavailable(String),
    Timeout(Duration),
    Cancelled,
}
impl fmt::Display for MediaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::VideoUnreadable(s) => write!(f, "video unreadable: {s}"),
            Self::InvalidInput(s) | Self::CommandFailed(s) => f.write_str(s),
            Self::RuntimeUnavailable(s) => write!(f, "media runtime unavailable: {s}"),
            Self::Cancelled => f.write_str("Processing cancelled"),
            Self::Timeout(d) => write!(f, "ffmpeg timed out after {:.3}s", d.as_secs_f64()),
        }
    }
}
impl std::error::Error for MediaError {}
impl From<io::Error> for MediaError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// Drain both pipes concurrently: raw BGR frames are larger than pipe capacity.
fn run(command: &mut Command, timeout: Duration, verbose: bool) -> Result<Output, MediaError> {
    if crate::cancellation::is_cancelled() {
        return Err(MediaError::Cancelled);
    }
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let out = thread::spawn(move || {
        let mut b = Vec::new();
        stdout.read_to_end(&mut b).map(|_| b)
    });
    let err = thread::spawn(move || {
        let mut b = Vec::new();
        stderr.read_to_end(&mut b).map(|_| b)
    });
    let started = Instant::now();
    let result = loop {
        if crate::cancellation::is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            break Err(MediaError::Cancelled);
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if started.elapsed() < timeout => thread::sleep(Duration::from_millis(5)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(MediaError::Timeout(timeout));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(e.into());
            }
        }
    };
    let stdout = out
        .join()
        .map_err(|_| io::Error::other("stdout reader panicked"))??;
    let stderr = err
        .join()
        .map_err(|_| io::Error::other("stderr reader panicked"))??;
    if verbose {
        // Native diagnostics stay off stdout, including failed hardware attempts.
        let _ = io::stderr().lock().write_all(&stderr);
    }
    let status = result?;
    if let Some(code) = status.code().map(|code| code as u32) {
        let hint = match code {
            0xC0000135 => Some("a required DLL is missing"),
            0xC000007B => Some("bad executable/DLL image; installation may be corrupt"),
            0xC0000142 => Some("required DLL initialization failed"),
            _ => None,
        };
        if let Some(hint) = hint {
            return Err(MediaError::RuntimeUnavailable(format!(
                "{} could not start (0x{code:08X}): {hint}",
                command.get_program().to_string_lossy()
            )));
        }
    }
    // Windows NTSTATUS failures (including STATUS_DLL_NOT_FOUND) are not
    // FFmpeg reporting malformed input. Unix signal termination is likewise
    // a process/runtime failure, never a clean unreadable-video result.
    // FFmpeg also returns negative AVERROR values (e.g. -22 = EINVAL),
    // so sign alone cannot identify an NTSTATUS crash.
    let native_crash = cfg!(windows)
        && status
            .code()
            .is_some_and(|code| matches!((code as u32) & 0xffff_0000, 0xc000_0000 | 0x8000_0000));
    if status.code().is_none() || native_crash {
        return Err(MediaError::RuntimeUnavailable(format!(
            "{} exited with {status}: {}",
            command.get_program().to_string_lossy(),
            String::from_utf8_lossy(&stderr).trim()
        )));
    }
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}
fn probe_output(
    command: &mut Command,
    timeout: Duration,
    verbose: bool,
) -> Result<Output, MediaError> {
    let output = run(command, timeout, verbose)?;
    if output.status.code() == Some(1) {
        return Err(MediaError::VideoUnreadable(format!(
            "{} rejected input: {}",
            command.get_program().to_string_lossy(),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    check_output(output)
}
fn check_output(output: Output) -> Result<Output, MediaError> {
    if output.status.success() {
        Ok(output)
    } else {
        Err(MediaError::CommandFailed(format!(
            "media command exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}
fn checked(command: &mut Command, timeout: Duration, verbose: bool) -> Result<Output, MediaError> {
    check_output(run(command, timeout, verbose)?)
}

#[derive(Debug, Clone)]
pub struct FfmpegTools {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    /// Forward native media stderr only when explicitly enabled by the caller.
    pub verbose: bool,
}
#[derive(Debug, Clone, PartialEq)]
pub struct VideoInfo {
    pub width: u32,
    pub height: u32,
    pub duration: f64,
}

/// Search explicit path, release roots, environment directory, PATH, then Shutter Encoder.
pub fn find_binary(name: &str, explicit: Option<&Path>) -> Result<PathBuf, MediaError> {
    if let Some(path) = explicit {
        return if path.exists() {
            Ok(path.into())
        } else {
            Err(MediaError::InvalidInput(format!(
                "Explicit {name} path does not exist: {}",
                path.display()
            )))
        };
    }
    let mut roots = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            roots.push(parent.to_path_buf());
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd);
    }
    // Source-run crate root is also a runtime root, matching Python runtime_roots.
    roots.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."));
    let mut dirs = Vec::new();
    for root in roots {
        for suffix in ["vendor/ffmpeg", "ffmpeg", "_internal/vendor/ffmpeg"] {
            dirs.push(root.join(suffix));
        }
    }
    if let Some(dir) = std::env::var_os("PUBG_HIGHLIGHT_TRIM_FFMPEG_DIR") {
        dirs.push(dir.into());
    }
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    dirs.extend([
        PathBuf::from(r"C:\Program Files\Shutter Encoder\app\Library"),
        PathBuf::from(r"C:\Program Files\Shutter Encoder\Library"),
    ]);
    for dir in dirs {
        let path = dir.join(name);
        if path.exists() {
            return Ok(path);
        }
    }
    Err(MediaError::InvalidInput(format!("Could not find {name}. Release builds should bundle it; for source runs pass an explicit path or set PUBG_HIGHLIGHT_TRIM_FFMPEG_DIR.")))
}
impl FfmpegTools {
    pub fn discover(ffmpeg: Option<&Path>, ffprobe: Option<&Path>) -> Result<Self, MediaError> {
        Ok(Self {
            ffmpeg: find_binary("ffmpeg.exe", ffmpeg)?,
            ffprobe: find_binary("ffprobe.exe", ffprobe)?,
            verbose: false,
        })
    }
    fn dimensions(&self, path: &Path) -> Result<(u32, u32), MediaError> {
        self.dimensions_with_timeout(path, Duration::from_secs(60))
    }
    fn dimensions_with_timeout(
        &self,
        path: &Path,
        timeout: Duration,
    ) -> Result<(u32, u32), MediaError> {
        let output = probe_output(
            Command::new(&self.ffprobe)
                .args([
                    "-v",
                    "error",
                    "-select_streams",
                    "v:0",
                    "-show_entries",
                    "stream=width,height",
                    "-of",
                    "csv=p=0",
                ])
                .arg(path),
            timeout,
            self.verbose,
        )?;
        let text = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<_> = text.lines().next().unwrap_or("").split(',').collect();
        let dimensions = (
            parts.first().and_then(|s| s.parse::<u32>().ok()),
            parts.get(1).and_then(|s| s.parse::<u32>().ok()),
        );
        match dimensions {
            (Some(w), Some(h)) if w > 0 && h > 0 => Ok((w, h)),
            _ => Err(MediaError::VideoUnreadable(format!(
                "ffprobe reported invalid dimensions for {}",
                path.display()
            ))),
        }
    }
    pub fn duration_sec(&self, path: &Path) -> Result<f64, MediaError> {
        self.duration_with_timeout(path, Duration::from_secs(60))
    }
    fn duration_with_timeout(&self, path: &Path, timeout: Duration) -> Result<f64, MediaError> {
        let output = probe_output(
            Command::new(&self.ffprobe)
                .args([
                    "-v",
                    "error",
                    "-show_entries",
                    "format=duration",
                    "-of",
                    "default=nw=1:nk=1",
                ])
                .arg(path),
            timeout,
            self.verbose,
        )?;
        let duration = String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<f64>()
            .ok();
        duration
            .filter(|d| d.is_finite() && *d >= 0.0)
            .ok_or_else(|| {
                MediaError::VideoUnreadable(format!("invalid duration for {}", path.display()))
            })
    }
    pub fn probe(&self, path: &Path) -> Result<VideoInfo, MediaError> {
        self.probe_with_timeout(path, Duration::from_secs(60))
    }
    /// Bound each FFprobe child; timeouts and system failures remain fatal.
    pub fn probe_with_timeout(
        &self,
        path: &Path,
        timeout: Duration,
    ) -> Result<VideoInfo, MediaError> {
        let (width, height) = self.dimensions_with_timeout(path, timeout)?;
        Ok(VideoInfo {
            width,
            height,
            duration: self.duration_with_timeout(path, timeout)?,
        })
    }
    /// Only the caller's event window is encoded, never a full-length copy.
    pub fn trim_clip(
        &self,
        src: &Path,
        out: &Path,
        start: f64,
        length: f64,
    ) -> Result<&'static str, MediaError> {
        if !start.is_finite() || start < 0.0 || !length.is_finite() || length <= 0.0 {
            return Err(MediaError::InvalidInput("invalid trim window".into()));
        }
        ensure_distinct(src, out)?;
        create_parent(out)?;
        for (codec, options) in [
            (
                "h264_nvenc",
                vec!["-preset", "p4", "-rc", "vbr", "-cq", "22", "-b:v", "0"],
            ),
            ("libx264", vec!["-preset", "veryfast", "-crf", "20"]),
        ] {
            let result = checked(
                Command::new(&self.ffmpeg)
                    .args([
                        "-hide_banner",
                        "-loglevel",
                        "error",
                        "-y",
                        "-ss",
                        &format!("{start:.3}"),
                        "-i",
                    ])
                    .arg(src)
                    .args([
                        "-t",
                        &format!("{length:.3}"),
                        "-map",
                        "0:v:0",
                        "-map",
                        "0:a:0?",
                        "-c:v",
                        codec,
                    ])
                    .args(options)
                    .args(["-c:a", "aac", "-b:a", "192k", "-movflags", "+faststart"])
                    .arg(out),
                Duration::from_secs(3600),
                self.verbose,
            );
            match result {
                Ok(_) => return Ok(codec),
                Err(MediaError::Cancelled) => return Err(MediaError::Cancelled),
                // Python's encoder fallback also covered GPU/native startup failures.
                // A shared runtime failure is still returned by the software attempt.
                Err(_) if codec == "h264_nvenc" => continue,
                Err(error) => return Err(error),
            }
        }
        unreachable!()
    }
    pub fn concat_clips(
        &self,
        clips: &[PathBuf],
        final_path: &Path,
    ) -> Result<PathBuf, MediaError> {
        if clips.is_empty() {
            return Err(MediaError::InvalidInput("no clips to merge".into()));
        }
        for clip in clips {
            ensure_distinct(clip, final_path)?;
        }
        create_parent(final_path)?;
        let list_path = final_path.with_extension("concat.txt");
        let mut list = String::new();
        for clip in clips {
            ensure_distinct(clip, &list_path)?;
            let absolute = fs::canonicalize(clip)?;
            let text = absolute.to_string_lossy();
            // FFmpeg does not accept the Windows extended-length prefix in concat lists.
            let text = text
                .strip_prefix(r"\\?\")
                .unwrap_or(&text)
                .replace('\\', "/");
            if text.contains(['\n', '\r']) {
                return Err(MediaError::InvalidInput("newline in clip path".into()));
            }
            list.push_str(&format!("file '{}'\n", text.replace('\'', "'\\''")));
        }
        fs::write(&list_path, list)?;
        checked(
            Command::new(&self.ffmpeg)
                .args([
                    "-hide_banner",
                    "-loglevel",
                    "error",
                    "-y",
                    "-f",
                    "concat",
                    "-safe",
                    "0",
                    "-i",
                ])
                .arg(&list_path)
                .args(["-c", "copy", "-movflags", "+faststart"])
                .arg(final_path),
            Duration::from_secs(3600),
            self.verbose,
        )?;
        Ok(list_path)
    }
}
fn create_parent(path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}
fn ensure_distinct(src: &Path, out: &Path) -> Result<(), MediaError> {
    if out.exists() && same_file::is_same_file(src, out)? {
        return Err(MediaError::InvalidInput(
            "output must not overwrite source".into(),
        ));
    }
    let absolute = |p: &Path| -> io::Result<OsString> {
        let p = if p.exists() {
            fs::canonicalize(p)?
        } else {
            let p = std::path::absolute(p)?;
            if let (Some(parent), Some(name)) = (p.parent(), p.file_name()) {
                if parent.exists() {
                    fs::canonicalize(parent)?.join(name)
                } else {
                    p
                }
            } else {
                p
            }
        };
        Ok(p.into_os_string())
    };
    if absolute(src)?
        .to_string_lossy()
        .eq_ignore_ascii_case(&absolute(out)?.to_string_lossy())
    {
        return Err(MediaError::InvalidInput(
            "output must not overwrite source".into(),
        ));
    }
    Ok(())
}

pub struct FfmpegFrameSource {
    path: PathBuf,
    ffmpeg: PathBuf,
    width: u32,
    height: u32,
    hwaccel: Option<String>,
    decoded_any: bool,
    closed: bool,
    timeout: Duration,
    last_diagnostic: String,
    verbose: bool,
}
impl FfmpegFrameSource {
    pub fn open(
        path: &Path,
        tools: &FfmpegTools,
        hwaccel: Option<&str>,
    ) -> Result<Self, MediaError> {
        let (width, height) = tools.dimensions(path)?;
        Ok(Self {
            path: path.into(),
            ffmpeg: tools.ffmpeg.clone(),
            width,
            height,
            hwaccel: hwaccel.map(str::to_owned),
            decoded_any: false,
            closed: false,
            timeout: Duration::from_secs(60),
            last_diagnostic: String::new(),
            verbose: tools.verbose,
        })
    }
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn is_open(&self) -> bool {
        !self.closed
    }
    pub fn close(&mut self) {
        self.closed = true;
    }
    pub fn set_decode_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }
    pub fn last_diagnostic(&self) -> &str {
        &self.last_diagnostic
    }
    pub fn hwaccel(&self) -> Option<&str> {
        self.hwaccel.as_deref()
    }
    pub fn read_at(&mut self, sec: f64) -> Result<Option<BgrFrame>, MediaError> {
        if self.closed {
            return Ok(None);
        }
        if !sec.is_finite() {
            return Err(MediaError::InvalidInput("seek time must be finite".into()));
        }
        let frame = self.decode(sec)?;
        if frame.is_some() {
            self.decoded_any = true;
            return Ok(frame);
        }
        if self.decoded_any || self.decode(0.0)?.is_some() {
            self.decoded_any = true;
            return Ok(None);
        }
        Err(MediaError::VideoUnreadable(format!(
            "ffmpeg cannot decode any frame from {}: {}",
            self.path.display(),
            self.last_diagnostic
        )))
    }
    fn decode(&mut self, sec: f64) -> Result<Option<BgrFrame>, MediaError> {
        let hardware = self.hwaccel.clone();
        let frame = self.grab(sec, hardware.as_deref())?;
        if frame.is_none() && hardware.is_some() {
            self.hwaccel = None;
            return self.grab(sec, None);
        }
        Ok(frame)
    }
    fn grab(&mut self, sec: f64, hwaccel: Option<&str>) -> Result<Option<BgrFrame>, MediaError> {
        let mut command = Command::new(&self.ffmpeg);
        command.args(["-hide_banner", "-loglevel", "error"]);
        if let Some(hw) = hwaccel {
            command.args(["-hwaccel", hw]);
        }
        command
            .args(["-ss", &format!("{:.3}", sec.max(0.0)), "-i"])
            .arg(&self.path)
            .args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "bgr24", "-"]);
        let output = match run(&mut command, self.timeout, self.verbose) {
            Ok(output) => output,
            Err(e @ MediaError::Timeout(_)) => {
                self.last_diagnostic = e.to_string();
                return Ok(None);
            }
            Err(e) => return Err(e),
        };
        self.last_diagnostic =
            String::from_utf8_lossy(&output.stderr[output.stderr.len().saturating_sub(400)..])
                .trim()
                .into();
        if !output.status.success() {
            return Ok(None);
        }
        Ok(BgrFrame::new(self.width, self.height, output.stdout).ok())
    }
}
