//! Installed runtime policy. A missing manifest is a development layout only.
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeVariant {
    System,
    Fixed,
}
#[derive(Debug, Deserialize)]
pub struct Distribution {
    pub variant: RuntimeVariant,
    pub minimum_webview2_major: u32,
}

/// Called on the main thread before Tauri or any process is started.
pub fn prepare(root: &Path) -> Result<Option<Distribution>, String> {
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY},
        System::{
            LibraryLoader::SetDllDirectoryW,
            Threading::{GetCurrentProcess, OpenProcessToken},
        },
    };
    // Never search the caller's working directory for DLLs. Packaged DLLs live by their executable.
    unsafe {
        if SetDllDirectoryW([0u16].as_ptr()) == 0 {
            return Err(format!(
                "无法保护 DLL 搜索路径: {}",
                std::io::Error::last_os_error()
            ));
        }
    }
    std::env::set_current_dir(root).map_err(|e| e.to_string())?;
    let policy = Distribution::from_root(root)?;
    if let Some(policy) = &policy {
        unsafe {
            let mut token = std::ptr::null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return Err("无法检查应用权限".into());
            }
            let mut elevation: TOKEN_ELEVATION = std::mem::zeroed();
            let mut length = 0;
            let ok = GetTokenInformation(
                token,
                TokenElevation,
                &mut elevation as *mut _ as _,
                std::mem::size_of_val(&elevation) as u32,
                &mut length,
            );
            CloseHandle(token);
            if ok == 0 {
                return Err("无法检查应用权限".into());
            }
            if elevation.TokenIsElevated != 0 {
                return Err("请勿以管理员身份运行应用。安装完成后关闭安装器，从开始菜单或桌面以普通权限启动。".into());
            }
        }
        // Loader policy can override API parameters (including the data directory).
        // Fail explicitly rather than moving data or executing a different browser.
        for hive in [
            winreg::enums::HKEY_LOCAL_MACHINE,
            winreg::enums::HKEY_CURRENT_USER,
        ] {
            for view in [
                winreg::enums::KEY_WOW64_32KEY,
                winreg::enums::KEY_WOW64_64KEY,
            ] {
                for setting in [
                    "BrowserExecutableFolder",
                    "UserDataFolder",
                    "AdditionalBrowserArguments",
                    "ReleaseChannels",
                    "ChannelSearchKind",
                ] {
                    let key = format!("Software\\Policies\\Microsoft\\Edge\\WebView2\\{setting}");
                    match winreg::RegKey::predef(hive)
                        .open_subkey_with_flags(&key, winreg::enums::KEY_READ | view)
                    {
                        Ok(key) => {
                            for app_id in [
                                "com.pubg-highlight-trim.desktop",
                                "pubg-highlight-trim-ui.exe",
                                "*",
                            ] {
                                match key.get_raw_value(app_id) {
                                    Ok(_) => return Err(format!("WebView2 策略 {setting} 覆盖了本应用运行位置；请移除该应用的覆盖策略后重试。")),
                                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
                                    Err(e) => return Err(format!("无法检查 WebView2 策略: {e}")),
                                }
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                        Err(e) => return Err(format!("无法检查 WebView2 策略: {e}")),
                    }
                }
            }
        }
        for directory in [
            root.to_path_buf(),
            policy.data_directory(root),
            policy.data_directory(root).join("webview"),
            root.join("WebView2Runtime"),
        ] {
            if let Ok(metadata) = std::fs::symlink_metadata(&directory) {
                use std::os::windows::fs::MetadataExt;
                if metadata.file_attributes() & 0x400 != 0 {
                    return Err(format!(
                        "拒绝通过重解析点使用安装资源或数据目录: {}",
                        directory.display()
                    ));
                }
            }
        }
        let folder = policy.browser_directory(root)?;
        // Environment values take precedence over loader registry overrides. Restrict shared
        // discovery to the stable runtime; never use an Edge Beta/Dev installation as fallback.
        for key in [
            "WEBVIEW2_BROWSER_EXECUTABLE_FOLDER",
            "WEBVIEW2_USER_DATA_FOLDER",
            "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
            "WEBVIEW2_CHANNEL_SEARCH_KIND",
            "WEBVIEW2_RELEASE_CHANNELS",
            "WEBVIEW2_WAIT_FOR_SCRIPT_DEBUGGER",
            "WEBVIEW2_PIPE_FOR_SCRIPT_DEBUGGER",
        ] {
            std::env::remove_var(key);
        }
        if let Some(folder) = &folder {
            std::env::set_var("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER", folder);
        }
        std::env::set_var(
            "WEBVIEW2_USER_DATA_FOLDER",
            policy.data_directory(root).join("webview"),
        );
        std::env::set_var("WEBVIEW2_RELEASE_CHANNELS", "0");
        std::env::set_var("WEBVIEW2_CHANNEL_SEARCH_KIND", "0");
        policy.check_version(&native_version(folder.as_deref())?)?;
    }
    Ok(policy)
}

fn native_version(folder: Option<&Path>) -> Result<String, String> {
    use std::os::windows::ffi::OsStrExt;
    use webview2_com::{
        take_pwstr, Microsoft::Web::WebView2::Win32::GetAvailableCoreWebView2BrowserVersionString,
    };
    use windows::core::{PCWSTR, PWSTR};
    let wide = folder.map(|p| {
        p.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>()
    });
    let mut version = PWSTR::null();
    unsafe {
        GetAvailableCoreWebView2BrowserVersionString(
            wide.as_ref().map_or(PCWSTR::null(), |s| PCWSTR(s.as_ptr())),
            &mut version,
        )
        .map_err(|_| "找不到兼容 WebView2。请安装内置版；不会下载安装共享运行时。".to_string())?;
        Ok(take_pwstr(version))
    }
}

/// Installer preflight: no UI, data writes or elevation restriction. Run from the staged payload
/// before touching the existing installation. The native x64 loader checks the selected variant.
pub fn check_runtime(root: &Path, variant: &str) -> Result<(), String> {
    let policy = Distribution::from_root(root)?.ok_or("缺少安装配置")?;
    let expected = match variant {
        "system" => RuntimeVariant::System,
        "fixed" => RuntimeVariant::Fixed,
        _ => return Err("未知安装变体".into()),
    };
    if policy.variant != expected {
        return Err("安装变体不匹配".into());
    }
    for key in [
        "WEBVIEW2_BROWSER_EXECUTABLE_FOLDER",
        "WEBVIEW2_RELEASE_CHANNELS",
        "WEBVIEW2_CHANNEL_SEARCH_KIND",
    ] {
        std::env::remove_var(key);
    }
    std::env::set_var("WEBVIEW2_RELEASE_CHANNELS", "0");
    let folder = policy.browser_directory(root)?;
    policy.check_version(&native_version(folder.as_deref())?)
}

/// Verify the created browser, not just loader discovery. Record native evidence for lifecycle tests.
pub fn verify_webview(
    view: tauri::webview::PlatformWebview,
    root: &Path,
    storage: &crate::storage::Storage,
) -> Result<(), String> {
    use webview2_com::{take_pwstr, Microsoft::Web::WebView2::Win32::ICoreWebView2Environment7};
    use windows::core::{Interface, PWSTR};
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
        },
    };
    unsafe {
        let environment = view.environment();
        let mut version = PWSTR::null();
        environment
            .BrowserVersionString(&mut version)
            .map_err(|e| e.to_string())?;
        let version = take_pwstr(version);
        let mut udf = PWSTR::null();
        environment
            .cast::<ICoreWebView2Environment7>()
            .map_err(|e| e.to_string())?
            .UserDataFolder(&mut udf)
            .map_err(|e| e.to_string())?;
        let udf = PathBuf::from(take_pwstr(udf));
        let mut pid = 0;
        view.controller()
            .CoreWebView2()
            .map_err(|e| e.to_string())?
            .BrowserProcessId(&mut pid)
            .map_err(|e| e.to_string())?;
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return Err("无法验证 WebView2 进程路径".into());
        }
        let mut path = vec![0u16; 32768];
        let mut len = path.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, 0, path.as_mut_ptr(), &mut len);
        CloseHandle(handle);
        if ok == 0 {
            return Err("无法读取实际 WebView2 路径".into());
        }
        let browser = PathBuf::from(String::from_utf16_lossy(&path[..len as usize]));
        if let Some(policy) = Distribution::from_root(root)? {
            policy.check_version(&version)?;
            policy.verify_loaded_paths(root, &browser, &udf)?;
        }
        let executable = if browser.starts_with(root.join("WebView2Runtime")) {
            "<install>/WebView2Runtime/msedgewebview2.exe"
        } else {
            "<shared-runtime>/msedgewebview2.exe"
        };
        // Verify native absolute paths in memory, but do not persist private user paths.
        storage.append(&format!(
            "RUNTIME webview2={version}; executable={executable}; user_data=<install>/data/webview; verified=true"
        ))?;
    }
    Ok(())
}

impl Distribution {
    pub fn from_root(root: &Path) -> Result<Option<Self>, String> {
        let path = root.join("distribution.json");
        match std::fs::read(path) {
            Ok(bytes) => {
                let policy: Self = serde_json::from_slice(&bytes)
                    .map_err(|e| format!("安装配置损坏，请重新安装: {e}"))?;
                if policy.minimum_webview2_major < 120 {
                    return Err("安装配置中的 WebView2 最低版本无效".into());
                }
                Ok(Some(policy))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if root.join(".installer").exists() {
                    Err("安装配置缺失，请修复安装；拒绝作为开发布局启动".into())
                } else {
                    Ok(None)
                }
            }
            Err(e) => Err(format!("无法读取安装配置: {e}")),
        }
    }
    pub fn data_directory(&self, root: &Path) -> PathBuf {
        root.join("data")
    }
    pub fn check_version(&self, version: &str) -> Result<(), String> {
        let parts: Vec<_> = version.split('.').collect();
        let stable = parts.len() == 4
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
        let major = parts.first().and_then(|v| v.parse::<u32>().ok());
        if stable && major.is_some_and(|v| v >= self.minimum_webview2_major) {
            Ok(())
        } else {
            Err(format!(
                "WebView2 版本不兼容（需要 {}+，当前 {version}）。请安装内置版；不会下载或切换运行时。",
                self.minimum_webview2_major
            ))
        }
    }
    pub fn verify_loaded_paths(
        &self,
        root: &Path,
        browser: &Path,
        data: &Path,
    ) -> Result<(), String> {
        let expected_data = self.data_directory(root).join("webview");
        let actual_data = data.canonicalize().map_err(|e| e.to_string())?;
        if actual_data != expected_data.canonicalize().map_err(|e| e.to_string())? {
            return Err("WebView2 数据路径被外部配置改写，拒绝在安装目录外写入".into());
        }
        if let Some(folder) = self.browser_directory(root)? {
            let expected = folder
                .join("msedgewebview2.exe")
                .canonicalize()
                .map_err(|e| e.to_string())?;
            if browser.canonicalize().map_err(|e| e.to_string())? != expected {
                return Err("实际 WebView2 并非随包运行时，拒绝回退系统运行时".into());
            }
        }
        Ok(())
    }
    pub fn browser_directory(&self, root: &Path) -> Result<Option<PathBuf>, String> {
        match self.variant {
            RuntimeVariant::System => Ok(None),
            RuntimeVariant::Fixed => {
                let folder = root.join("WebView2Runtime");
                for name in [
                    "msedgewebview2.exe",
                    "msedge.dll",
                    "icudtl.dat",
                    "resources.pak",
                ] {
                    if !folder.join(name).is_file() {
                        return Err(format!(
                            "内置 WebView2 不完整，拒绝使用系统运行时，请重新安装内置版: {name}"
                        ));
                    }
                }
                Ok(Some(folder))
            }
        }
    }
}
