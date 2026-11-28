# NSH 替换 Tauri + Vue 的迁移评估

> 历史评估：下文“当前”指评估时的迁移前基线，源码链接对应 Git 历史，不是现存入口。旧实现现已删除；共享代码及保留的契约测试位于 `ui/core`，唯一 UI 为 `ui/native-ui`。当前操作见 [UI 文档](../ui/README.md)。

## 结论与证据边界

- **不建议直接切换；若主要目标是移除 WebView2 与前端工具链，值得先做隔离的小型 PoC。**
- 这里的 NSH 指 [longbridge/nsh](https://github.com/longbridge/nsh)，是 Rust 原生 UI 框架，不是命令 shell。
- 本文依据上游文档、固定版本源码和本仓库现状；没有构建 NSH、运行 PoC 或执行真机验收。
- 上游能力不代表本项目已验证；性能、体积、无 WebView2 运行和安装兼容性均仍是待测项。

## 当前基线与可复用边界

- 当前产品是 Windows 10/11 x64 的 Tauri 2 + Vue 3 桌面壳，独立启动原有 Rust CLI；CLI 分发不应受 UI 迁移影响。[README](../README.md#L5-L7)
- `service.rs`、`protocol.rs`、`process.rs`、`options.rs`、`storage.rs` 不依赖 Tauri，可优先保留，而不是重写业务。[源码目录](../ui/src-tauri/src/)
- Tauri 适配层注册 13 个 commands，包含配置、校验、运行、取消、日志、清理、路径选择与退出；迁移需保持行为等价。[ipc.rs](../ui/src-tauri/src/ipc.rs#L174-L189)
- 当前输入是 textarea 与文件/文件夹按钮，底层用 rfd；没有现成文件拖放交互，拖放属于可选增益，不是迁移门槛。[App.vue](../ui/src/App.vue#L349-L372)、[ipc.rs](../ui/src-tauri/src/ipc.rs#L90-L118)
- 前端还承担启动/停止闩锁、运行 ID 隔离、取消待清理/失败态、权威状态刷新、异步校验与关闭确认，不能只移植按钮和布局。[状态与刷新](../ui/src/App.vue#L140-L163)、[启动与取消](../ui/src/App.vue#L182-L216)、[校验与关闭](../ui/src/App.vue#L254-L298)
- 危险参数必须在异步确认前冻结快照，校验、确认与启动使用同一份；不得直接调用 `Shell::start(..., true)` 绕过确认。[workflow.ts](../ui/src/workflow.ts#L103-L123)
- `Shell` 状态共用 Mutex，启动在锁内保存配置并创建子进程；启动/取消/退出及状态读取不可在 UI 线程等待同一把锁，应使用后台任务与异步状态回传。[service.rs](../ui/src-tauri/src/service.rs#L26-L65)
- 原生壳拦截窗口关闭与应用退出，运行中交由确认流程；新壳必须保留该边界。[ipc.rs](../ui/src-tauri/src/ipc.rs#L220-L241)
- CLI 挂起创建后先加入 kill-on-close Job Object，再恢复执行，并使用 `CREATE_NO_WINDOW`；取消和退出不能退化为仅杀直接子进程。[process.rs](../ui/src-tauri/src/process.rs#L31-L45)、[启动](../ui/src-tauri/src/process.rs#L111-L139)

## 上游成熟度与 Windows 路线

- 已核实 GitHub latest 返回 [v0.6.1](https://github.com/longbridge/nsh/releases/tag/v0.6.1)，是 0.x 非预发行版本；[nsh-platform-windows 0.6.1](https://crates.io/crates/nsh-platform-windows/0.6.1) 已发布，但不能因此推定接口长期稳定。
- Release 提供 Windows x64 ZIP，但那是工具发行包大小，不能作为本应用安装体积或节省量的证据。[发行页](https://github.com/longbridge/nsh/releases/tag/v0.6.1)
- v0.6.1 声明 MSRV 1.97；本仓库 CI 使用 `stable-x86_64-pc-windows-msvc`，需实测版本、依赖解析与链接，不能假设 stable 必然满足。[上游 Cargo.toml](https://github.com/longbridge/nsh/blob/v0.6.1/Cargo.toml)、[本仓库工具链](../.github/actions/rust-windows-setup/action.yml#L41-L43)
- Windows 文档要求 Windows 10 1809+，原生后端使用 Win32、Direct2D、DirectWrite；优先验证 `run_app(AppSource::Rust)` 或 AppView 路线。[Windows 文档](https://nsh.dev/docs/windows)
- 文档明确 WebView 才需要 WebView2，但平台 crate 默认包含 js/webview/media 等可选 feature，不能把默认构建等同于无 WebView2 依赖。[文档](https://nsh.dev/docs/windows)、[features](https://github.com/longbridge/nsh/blob/v0.6.1/crates/nsh-platform-windows/Cargo.toml)
- PoC 必须显式选择 Rust 原生路线及 feature，保存 `cargo tree -e features` 证据，并在无 WebView2 Runtime 的干净环境启动验证。
- 不把 WebView2 的窗口句柄支持当成纯原生路线门槛；应验证所选 Rust 窗口、事件循环与现有服务集成。

## UI 能力与行为差距

- Rust-only 文件对话框提供 `pick_paths`、目录选择，以及 `open_path`、`reveal_path`，更贴合 CLI 所需路径。[file_dialog.rs](https://github.com/longbridge/nsh/blob/v0.6.1/crates/nsh-core/src/file_dialog.rs)
- TSX Picker 面向文件字节，不是等价的路径 API；不能据此实现把本地路径交给 CLI 的流程。[Picker](https://nsh.dev/modules/nsh#picker)
- Windows 源码有 OLE `CF_HDROP`；若试做拖放，需实测资源管理器、多文件、目录、中文与空格路径，不能仅凭源码宣称可用。[file_drop.rs](https://github.com/longbridge/nsh/blob/v0.6.1/crates/nsh-platform-windows/src/file_drop.rs)
- 首要风险是运行中关闭：确认对话框与清理不能阻塞 UI，也不能提前释放停止闩锁或错误显示“已停止”。[现有闩锁](../ui/src/App.vue#L143-L163)、[关闭入口](../ui/src-tauri/src/ipc.rs#L119-L128)
- NSH 主窗口发出 `ClosePress`，默认 `closeOnPress=true` 自动关闭且最多等待 250ms；迁移必须设 `close_on_press=false`，事件中异步确认/清理，完成后显式关窗，不能依赖默认等待完成清理。[window_close.rs](https://github.com/longbridge/nsh/blob/v0.6.1/crates/nsh-platform-windows/src/window_close.rs)
- 进度、取消、错误、日志和打开输出目录需要对齐现有命令语义，而非只证明 NSH 能画出窗口。[commands](../ui/src-tauri/src/ipc.rs#L174-L189)
- DPI/缩放、多显示器、中文 IME、键盘焦点和屏幕阅读器都需真机覆盖；上游明确部分元素（如 toast）不暴露 UI Automation。[Windows 差异](https://nsh.dev/docs/windows#what-differs-on-windows)
- 上游提供 headless 与原生窗口测试，但现有 Playwright DOM 测试不能原封不动迁移，需改为同等行为测试及适用的 UIA 验证。[测试文档](https://nsh.dev/docs/testing-and-trace)
- Rust 测试可优先保留；逐项验证依赖边界后复用，而不是一律删除或承诺无需适配：[shell](../ui/src-tauri/tests/shell.rs)、[cancellation](../ui/src-tauri/tests/cancellation.rs)、[protocol](../ui/src-tauri/tests/protocol.rs)、[process](../ui/src-tauri/tests/process.rs)、[abnormal_exit](../ui/src-tauri/tests/abnormal_exit.rs)。

## 发布与 WebView2 不是简单删依赖

- 当前启动程序有 `--check-webview2-runtime` 预检入口，实际浏览器创建后还验证版本、进程路径及用户数据目录。[main.rs](../ui/src-tauri/src/main.rs#L4-L21)、[distribution.rs](../ui/src-tauri/src/distribution.rs#L197-L258)
- 安装事务也在移动旧文件前，以目标桌面用户身份验证运行时；删除 UI 依赖不等于可以直接跳过这条安装链。[install_transaction.ps1](../scripts/installer/install_transaction.ps1#L141-L174)
- 上游默认打包是 Inno Setup、每用户安装，不能替代本仓库 NSIS 的 per-machine 管理员安装和 PowerShell 5.1 事务。[上游打包](https://nsh.dev/docs/windows#packaging)、[安装入口](../scripts/installer/build_installer.ps1#L77-L102)
- 必须保留桌面用户识别、代码/数据 ACL、回滚、注册表与变体处理等安全语义。[用户探测](../scripts/installer/install_transaction.ps1#L113-L115)、[ACL](../scripts/installer/install_transaction.ps1#L224-L250)、[注册与回滚](../scripts/installer/install_transaction.ps1#L258-L269)
- System/Fixed 的去留、旧版升级兼容、旧浏览器数据何时清理，必须先形成产品决策，不能简单删脚本和目录。[安装契约](windows-installers.md)、[当前说明](../README.md#L19)
- PoC 阶段保留现有发布机制；通过后第二阶段才调整 WebView2 探测、发布门禁及 manifest，并重验安装/升级/卸载/回滚。新 Cargo 依赖还需通过现有[许可证与致谢收集](../scripts/collect_ui_licenses.mjs)。

## 最小 PoC 与通过条件

1. 新增隔离的 Rust Win32 原生实验入口，不修改现有 Tauri/Vue 入口；固定 NSH 版本与 feature，保留 CLI、协议和 Job Object 边界。
2. 首先实现文件/目录选择、配置校验、进度、取消、运行中关闭确认、错误反馈和打开输出目录；拖放可选，不阻塞基本迁移判断。
3. 复用后端用例并运行真实 CLI：正常完成、用户取消、真实关窗、异常退出、启动失败、清理失败均有明确状态与证据。
4. 关闭确认与清理期间保持窗口响应；只有权威终态或明确清理失败才能解除闩锁，验证没有遗留 CLI/ffmpeg 进程；以慢启动、启动失败、大量日志验证 UI 仍响应。
5. 在真实 Windows 10/11 x64 验证中文/空格路径、DPI、中文 IME、键盘与 UIA；无 WebView2 干净环境单列结果。
6. 相同机器、CLI、模型、媒体与构建模式下对比冷/热启动、内存、UI 产物及完整安装包体积；区分 UI 节省与模型/ffmpeg 的固定成本。
7. 将未覆盖项列为未验证；只有核心交互、退出清理、无 Runtime 与可接受的性能证据齐全，才进入剩余 UI 重写及安装脚本适配。

## 收益、成本与最终建议

- 潜在收益：原生 Rust UI 可能移除浏览器运行时与前端构建链；是否更小、更快必须以上述实测为准。
- 主要成本：Vue 状态/交互重写、窗口生命周期与异步桥接、测试替换，以及成熟安装安全链的重新验证。
- NSH 的跨平台能力不等于 OCR/CLI 已跨平台；本仓库仍以 Windows 为边界，不能把 UI 迁移包装成 OCR 平台迁移。[README](../README.md#L5-L7)
- 建议先把“无 WebView2、退出安全、原生交互质量、可量化收益”作为决策门；未通过就继续维护当前 Tauri + Vue。
- 当前仅完成文档评估，未运行 PoC，也没有任何迁移或真机测试通过的结论。
