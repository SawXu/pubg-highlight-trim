#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args
        .first()
        .is_some_and(|a| a == "--check-webview2-runtime")
    {
        let result = (|| {
            if args.len() != 2 {
                return Err("需要且只接受一个安装变体参数".to_string());
            }
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            highlight_shell::distribution::check_runtime(
                exe.parent().ok_or("无法定位应用")?,
                &args[1].to_string_lossy(),
            )
        })();
        if let Err(message) = result {
            eprintln!("{message}");
            std::process::exit(1);
        }
        return;
    }
    highlight_shell::run();
}
