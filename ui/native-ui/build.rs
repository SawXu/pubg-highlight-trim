use std::{env, fs, path::PathBuf, process::Command};

fn find_rc() -> Option<PathBuf> {
    if let Some(paths) = env::var_os("PATH") {
        for directory in env::split_paths(&paths) {
            let rc = directory.join("rc.exe");
            if rc.is_file() {
                return Some(rc);
            }
        }
    }
    let kits = PathBuf::from(r"C:\Program Files (x86)\Windows Kits\10\bin");
    let mut versions: Vec<_> = fs::read_dir(kits)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    versions.sort_by_key(|path| {
        path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .split('.')
            .map(|part| part.parse::<u32>().unwrap_or(0))
            .collect::<Vec<_>>()
    });
    versions
        .into_iter()
        .rev()
        .map(|v| v.join("x64/rc.exe"))
        .find(|p| p.is_file())
}

fn main() {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    println!("cargo:rerun-if-changed=icons/icon.ico");
    let Some(rc) = find_rc() else {
        println!("cargo:warning=rc.exe not found; Windows icon resource was not embedded");
        return;
    };
    let icon = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap())
        .join("icons/icon.ico")
        .canonicalize()
        .expect("resolve application icon");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let source = out.join("app_icon.rc");
    let resource = out.join("app_icon.res");
    // RC strings require escaped backslashes and quotes. Avoid Win32's extended
    // path prefix, which the resource compiler does not accept consistently.
    let path = icon.to_string_lossy();
    let path = path.strip_prefix(r"\\?\").unwrap_or(&path);
    let escaped = path.replace('\\', "\\\\").replace('"', "\\\"");
    fs::write(&source, format!("1 ICON \"{escaped}\"\n")).expect("write icon resource script");
    let status = Command::new(rc)
        .arg("/nologo")
        .arg("/fo")
        .arg(&resource)
        .arg(&source)
        .status()
        .expect("launch Windows resource compiler");
    assert!(
        status.success(),
        "Windows resource compiler failed: {status}"
    );
    println!("cargo:rustc-link-arg-bins={}", resource.display());
}
