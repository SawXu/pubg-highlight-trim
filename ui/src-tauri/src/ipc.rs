use crate::{
    options::{preview, GAME_LANGUAGES, SCAN_MODES, TRIM_JOBS},
    storage::{RuntimeLayout, Storage},
    RunOptions, RunState, Shell,
};
use serde_json::{json, Value};
use tauri::{Emitter, Manager, State};
#[tauri::command]
fn get_bootstrap(s: State<Shell>) -> Value {
    let options = s.storage.load();
    let error = options
        .as_ref()
        .err()
        .cloned()
        .or_else(|| s.layout.as_ref().err().cloned());
    json!({"defaults":RunOptions::default(),"options":options.unwrap_or_default(),"dataDir":s.storage.root,"cliPath":s.layout.as_ref().ok().map(|l|&l.cli),"cliReady":s.layout.is_ok(),"error":error,"schema":{"gameLanguage":GAME_LANGUAGES,"scanMode":SCAN_MODES,"trimJobs":TRIM_JOBS}})
}
#[tauri::command]
fn validate_options(s: State<Shell>, options: RunOptions) -> Result<Value, String> {
    let args = options.arguments()?;
    let exe = s
        .layout
        .as_ref()
        .map(|l| l.cli.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "cli/pubg-highlight-trim.exe".into());
    Ok(json!({"arguments":args,"preview":preview(&exe,&args),"warnings":options.warnings()}))
}
#[tauri::command]
fn save_options(s: State<Shell>, options: RunOptions) -> Result<(), String> {
    s.save(&options)
}
#[tauri::command]
async fn start_run(
    s: State<'_, Shell>,
    options: RunOptions,
    confirmed: bool,
) -> Result<u64, String> {
    let shell = s.inner().clone();
    tauri::async_runtime::spawn_blocking(move || shell.start(options, confirmed))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
fn get_run_state(s: State<Shell>) -> RunState {
    s.state()
}
#[tauri::command]
async fn cancel_run(s: State<'_, Shell>) -> Result<(), String> {
    let shell = s.inner().clone();
    tauri::async_runtime::spawn_blocking(move || shell.cancel())
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
fn get_logs(s: State<Shell>) -> Result<Vec<String>, String> {
    s.logs()
}
#[tauri::command]
fn clear_logs(s: State<Shell>) -> Result<(), String> {
    s.clear_logs()
}
#[tauri::command]
fn clear_cache(s: State<Shell>, window: tauri::WebviewWindow) -> Result<(), String> {
    s.clear_cache()?;
    window.clear_all_browsing_data().map_err(|e| e.to_string())
}
#[tauri::command]
fn open_output(s: State<Shell>) -> Result<(), String> {
    let state = s.state();
    let p = state
        .summary
        .as_ref()
        .and_then(|v| v["output_dir"].as_str())
        .ok_or("没有输出目录")?;
    let path = std::path::Path::new(p);
    if !path.is_dir() {
        return Err("输出目录不存在".into());
    }
    let explorer = std::path::PathBuf::from(
        std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()),
    )
    .join("explorer.exe");
    std::process::Command::new(explorer)
        .arg(path)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}
#[tauri::command]
async fn pick_inputs(kind: String) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || match kind.as_str() {
        "folder" => Ok(rfd::FileDialog::new()
            .pick_folder()
            .into_iter()
            .map(|p| p.to_string_lossy().into())
            .collect()),
        "files" => Ok(rfd::FileDialog::new()
            .add_filter("MP4", &["mp4"])
            .pick_files()
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.to_string_lossy().into())
            .collect()),
        _ => Err("未知输入类型".into()),
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn pick_output() -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        rfd::FileDialog::new()
            .pick_folder()
            .map(|p| p.to_string_lossy().into())
    })
    .await
    .map_err(|e| e.to_string())
}
#[tauri::command]
async fn close_app(
    app: tauri::AppHandle,
    s: State<'_, Shell>,
    confirmed: bool,
) -> Result<(), String> {
    let shell = s.inner().clone();
    tauri::async_runtime::spawn_blocking(move || shell.close(confirmed))
        .await
        .map_err(|e| e.to_string())??;
    app.exit(0);
    Ok(())
}
pub fn run() {
    let exe = std::env::current_exe().expect("无法定位应用");
    let root = exe.parent().expect("应用目录").to_path_buf();
    // The development root is fixed at build time, never selected through renderer input.
    let resource_root = if cfg!(debug_assertions) {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../dist/pubg-highlight-trim-ui")
    } else {
        root.clone()
    };
    let data = std::env::var_os("PUBG_HIGHLIGHT_TRIM_DATA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| root.join("data"));
    let storage = Storage::open(data)
        .expect("专用数据目录不可写；请设置 PUBG_HIGHLIGHT_TRIM_DATA_DIR 为可写绝对目录");
    let shell = Shell::new(storage.clone(), RuntimeLayout::from_root(&resource_root));
    tauri::Builder::default()
        .manage(shell)
        .invoke_handler(tauri::generate_handler![
            get_bootstrap,
            validate_options,
            save_options,
            start_run,
            get_run_state,
            cancel_run,
            get_logs,
            clear_logs,
            clear_cache,
            open_output,
            pick_inputs,
            pick_output,
            close_app
        ])
        .setup(move |app| {
            let w = tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::App("index.html".into()),
            )
            .title("PUBG Highlight Trim")
            .inner_size(1180., 840.)
            .data_directory(storage.root.join("webview"))
            .additional_browser_args("--disk-cache-size=67108864 --media-cache-size=33554432")
            .build()?;
            let h = app.handle().clone();
            w.on_window_event(move |e| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = e {
                    api.prevent_close();
                    if h.state::<Shell>().state().running {
                        let _ = h.emit("close-requested", ());
                    } else {
                        h.exit(0)
                    }
                }
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("Tauri 启动失败")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                if app.state::<Shell>().state().running {
                    api.prevent_exit();
                    let _ = app.emit("close-requested", ());
                }
            }
        });
}
