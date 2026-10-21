use crate::RunOptions;
use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};
pub const MAX_LOG_BYTES: usize = 1024 * 1024;
#[derive(Clone)]
pub struct Storage {
    pub root: PathBuf,
}
impl Storage {
    pub fn open(root: PathBuf) -> Result<Self, String> {
        if !root.is_absolute() {
            return Err("数据目录必须是绝对路径，不回退 AppData".into());
        }
        fs::create_dir_all(root.join("cache")).map_err(|e| format!("数据目录不可写: {e}"))?;
        fs::create_dir_all(root.join("webview")).map_err(|e| e.to_string())?;
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(root.join("run.log"))
            .map_err(|e| format!("日志不可写: {e}"))?;
        let storage = Self { root };
        storage.bound_cache()?;
        Ok(storage)
    }
    pub fn load(&self) -> Result<RunOptions, String> {
        let p = self.root.join("settings.json");
        if !p.exists() {
            return Ok(RunOptions::default());
        }
        serde_json::from_slice(&fs::read(p).map_err(|e| e.to_string())?)
            .map_err(|e| format!("设置损坏: {e}"))
    }
    pub fn save(&self, o: &RunOptions) -> Result<(), String> {
        let tmp = self.root.join("settings.tmp");
        fs::write(
            &tmp,
            serde_json::to_vec_pretty(o).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        fs::rename(tmp, self.root.join("settings.json")).map_err(|e| e.to_string())
    }
    pub fn logs(&self) -> Result<Vec<String>, String> {
        let mut f = fs::File::open(self.root.join("run.log")).map_err(|e| e.to_string())?;
        let length = f.metadata().map_err(|e| e.to_string())?.len();
        f.seek(SeekFrom::Start(length.saturating_sub(MAX_LOG_BYTES as u64)))
            .map_err(|e| e.to_string())?;
        let mut b = Vec::new();
        f.take(MAX_LOG_BYTES as u64)
            .read_to_end(&mut b)
            .map_err(|e| e.to_string())?;
        Ok(String::from_utf8_lossy(&b)
            .lines()
            .map(str::to_owned)
            .collect())
    }
    pub fn append(&self, line: &str) -> Result<(), String> {
        let mut lines = self.logs()?;
        lines.push(line.chars().take(16384).collect());
        while lines.len() > 2000 || lines.iter().map(|s| s.len() + 1).sum::<usize>() > MAX_LOG_BYTES
        {
            lines.remove(0);
        }
        fs::write(self.root.join("run.log"), lines.join("\n") + "\n").map_err(|e| e.to_string())
    }
    pub fn clear_logs(&self) -> Result<(), String> {
        fs::write(self.root.join("run.log"), "").map_err(|e| e.to_string())
    }
    pub fn bound_cache(&self) -> Result<(), String> {
        fn files(
            p: &Path,
            out: &mut Vec<(std::time::SystemTime, u64, PathBuf)>,
        ) -> Result<(), String> {
            for e in fs::read_dir(p).map_err(|e| e.to_string())? {
                let e = e.map_err(|e| e.to_string())?;
                let m = e.metadata().map_err(|e| e.to_string())?;
                if e.path().is_symlink() {
                    continue;
                }
                if m.is_dir() {
                    files(&e.path(), out)?
                } else if m.is_file() {
                    out.push((
                        m.modified().unwrap_or(std::time::UNIX_EPOCH),
                        m.len(),
                        e.path(),
                    ));
                }
            }
            Ok(())
        }
        let mut entries = vec![];
        files(&self.root.join("cache"), &mut entries)?;
        entries.sort_by_key(|e| e.0);
        let mut total: u64 = entries.iter().map(|e| e.1).sum();
        for (_, size, path) in entries {
            if total <= 64 * 1024 * 1024 {
                break;
            }
            fs::remove_file(path).map_err(|e| e.to_string())?;
            total -= size;
        }
        Ok(())
    }
    pub fn clear_cache(&self) -> Result<(), String> {
        let p = self.root.join("cache");
        if p.is_symlink() {
            return Err("拒绝清理链接缓存目录".into());
        }
        if p.exists() {
            fs::remove_dir_all(&p).map_err(|e| e.to_string())?
        }
        fs::create_dir_all(p).map_err(|e| e.to_string())
    }
}
#[derive(Clone)]
pub struct RuntimeLayout {
    pub cli: PathBuf,
    pub models: PathBuf,
}
impl RuntimeLayout {
    /// Only the application-owned cli directory is trusted. No environment executable override.
    pub fn from_root(root: &Path) -> Result<Self, String> {
        let cli = root.join("cli/pubg-highlight-trim.exe");
        let models = root.join("cli/ocr_models");
        for p in [
            cli.clone(),
            models.join("pp_ocrv6_medium.json"),
            models.join("det.onnx"),
            models.join("rec.onnx"),
            models.join("dict.txt"),
            root.join("cli/_internal/vendor/ffmpeg/ffmpeg.exe"),
            root.join("cli/_internal/vendor/ffmpeg/ffprobe.exe"),
        ] {
            if !p.is_file() {
                return Err(format!("缺少运行资源: {}", p.display()));
            }
        }
        Ok(Self {
            cli: cli.canonicalize().map_err(|e| e.to_string())?,
            models: models.canonicalize().map_err(|e| e.to_string())?,
        })
    }
}
