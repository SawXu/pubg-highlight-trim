# NSH 原生 Windows 工作台

唯一桌面实现为 `native-ui`：纯 Rust NSH Win32/Direct2D/DirectWrite UI，无浏览器运行时或 Node 前端工具链。共享的 `core` 负责参数、协议、CLI 子进程树、存储及 `UiSession` 状态机；原生 UI 只负责交互与展示。Vue/Tauri 实现及其构建入口已移除。

## 检查与构建

Windows 10/11 x64、Rust stable MSVC、Visual Studio C++ Build Tools / Windows SDK 和 PowerShell 环境下，从仓库根目录运行：

```powershell
cargo check --locked --manifest-path ui/native-ui/Cargo.toml
cargo build --locked --manifest-path ui/native-ui/Cargo.toml --release
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test_ui_behavior.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build_native_ui_windows.ps1
.\dist\pubg-highlight-trim-native-ui\pubg-highlight-trim-native-ui.exe
```

行为脚本实际运行 `cargo test --locked --manifest-path ui/core/Cargo.toml` 和 `cargo test --locked --manifest-path ui/native-ui/Cargo.toml`。core 保留参数（含旧设置默认 SDR 与显式 HDR）、协议、启动、取消、异常退出、Job Object 进程树和 UiSession 契约；native 测试覆盖原生展示/交互模型。Windows CI 同时构建原生 release 并拒绝浏览器壳依赖。

`-SkipCliBuild` 可复用已构建的完整 CLI bundle；`-Configuration Debug` 可构建调试布局。不要单独移动 exe，须保留旁边的 `cli/`。安装版与开发版均使用管理员运行模型。版本来自 `native-ui/Cargo.toml` 的 package version，图标资源在 `native-ui/icons/`。

## 真实视频回归（显式运行）

core 的 `real_video` 测试默认 ignored，需要固定视频及从当前源码构建的完整 Windows CLI bundle：

```powershell
$env:PUBG_UI_REAL_INPUTS='["D:\\videos\\fixed.mp4"]'
$env:PUBG_UI_REAL_BUNDLE='D:\bundle\pubg-highlight-trim'
cargo test --locked --manifest-path ui/core/Cargo.toml --test real_video -- --ignored --nocapture
```

测试比较直接 CLI 与共享 Shell 的 summary、CSV、媒体解码结果并校验源文件未变化，不等价于真实窗口验收。自动契约测试不证明交互关闭/取消、UIA/NVDA、IME、DPI 或干净 Windows 主机安装生命周期已验收。安装与发布边界见 [安装说明](../docs/windows-installers.md)；旧框架验证记录仅为历史证据。
