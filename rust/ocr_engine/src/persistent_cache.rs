//! Versioned detection cache. Corrupt/foreign entries are misses, never detections.
//!
//! Identity uses the entire input content, serialized effective configuration and
//! caller-supplied model/provider identity. No compatibility with Python caches is
//! required. A single checksummed envelope is published via atomic replacement.
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

const SCHEMA: &str = "pubg-highlight-trim/rust-detections/v1";
const MAX_ENTRY_BYTES: u64 = 64 * 1024 * 1024;
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Serialize, Deserialize)]
struct Envelope {
    schema: String,
    key: String,
    checksum: String,
    payload: serde_json::Value,
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn valid_key(key: &str) -> bool {
    key.len() == 64
        && key
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn json_error(error: serde_json::Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

/// Supply *effective* config (including language, candidates and scan budgets),
/// and validated model hashes/provider/algorithm identity, not just model names.
pub fn cache_key(
    video: &Path,
    config: &impl Serialize,
    model_identity: &str,
) -> io::Result<String> {
    let mut source = File::open(video)?;
    let mut contents = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = source.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        contents.update(&buffer[..count]);
    }
    // Value's sorted object keys make equivalent serialized maps deterministic.
    let config = serde_json::to_value(config).map_err(json_error)?;
    let identity = serde_json::json!({
        "schema": SCHEMA,
        "content_sha256": format!("{:x}", contents.finalize()),
        "configuration": config,
        "model_provider_identity": model_identity,
    });
    Ok(digest(&serde_json::to_vec(&identity).map_err(json_error)?))
}

pub fn read<T: DeserializeOwned>(directory: &Path, key: &str) -> Option<T> {
    if !valid_key(key) {
        return None;
    }
    let file = File::open(directory.join(format!("{key}.json"))).ok()?;
    if file.metadata().ok()?.len() > MAX_ENTRY_BYTES {
        return None;
    }
    let envelope: Envelope = serde_json::from_reader(file.take(MAX_ENTRY_BYTES + 1)).ok()?;
    if envelope.schema != SCHEMA || envelope.key != key {
        return None;
    }
    let payload = serde_json::to_vec(&envelope.payload).ok()?;
    if digest(&payload) != envelope.checksum {
        return None;
    }
    serde_json::from_value(envelope.payload).ok()
}

pub fn write<T: Serialize>(directory: &Path, key: &str, payload: &T) -> io::Result<()> {
    if !valid_key(key) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid detection cache key",
        ));
    }
    let payload = serde_json::to_value(payload).map_err(json_error)?;
    let envelope = Envelope {
        schema: SCHEMA.into(),
        key: key.into(),
        checksum: digest(&serde_json::to_vec(&payload).map_err(json_error)?),
        payload,
    };
    let bytes = serde_json::to_vec(&envelope).map_err(json_error)?;
    if bytes.len() as u64 > MAX_ENTRY_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "detection cache entry too large",
        ));
    }
    fs::create_dir_all(directory)?;
    let name = format!(
        "{key}.{}.{}.tmp",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let temporary = directory.join(name);
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        atomic_replace(&temporary, &directory.join(format!("{key}.json")))
    })();
    let _ = fs::remove_file(&temporary);
    result
}

#[cfg(not(windows))]
pub(crate) fn atomic_replace(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
pub(crate) fn atomic_replace(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(source: *const u16, destination: *const u16, flags: u32) -> i32;
    }
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH, same-directory volume.
    if unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), 0x1 | 0x8) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
