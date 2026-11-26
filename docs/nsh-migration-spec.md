## Problem Statement

当前桌面应用使用 Tauri + Vue，并通过 WebView2 承载界面。项目要切换到 longbridge/nsh 的纯 Rust Windows 原生 UI，移除 WebView2、Node/Vite 和前端壳层，同时保留独立 OCR CLI、任务协议、进程树清理和现有核心行为。

迁移必须保持危险操作确认、参数快照、启动/取消状态机、运行中关窗清理、Windows Job Object、日志与终态展示、数据目录权限等契约，而不是只替换窗口绘制。

## Solution

使用 NSH 的 Win32/Direct2D/DirectWrite Rust 原生后端重写桌面 UI，完全使用 Rust，不使用 NSH TSX、QuickJS、Node、Vite 或 WebView2。保留独立 OCR CLI、stdout/进度协议、现有进程树管理和 Job Object 语义。

迁移经过旁路 PoC、完整 UI、行为与 Windows 真机验收、安装发布替换。使用一个深的 Rust `UiSession` 模块作为 UI seam：NSH 只处理输入和展示，UiSession 隐藏参数快照、确认、run_id、状态观察、取消、清理、错误和终态逻辑。

正式发布使用单一 Windows x64 安装形态，采用 NSH 生成后定制的 Inno Setup，不保留旧 System/Fixed 和旧用户升级路径。数据继续放在安装目录的 `data` 下；应用按管理员权限模型运行。NSH 语义上跟随 latest，但每次人工更新并提交 Cargo.lock 后构建和验收。

## User Stories

1. As a Windows 10 1809 user, I want the native app to start without WebView2, so that installation has no browser-runtime prerequisite.
2. As a Windows 11 user, I want the native app to complete a real highlight-trimming task.
3. As a user, I want to choose video files and input/output directories, so that existing workflows remain usable.
4. As a user, I want settings to persist across restarts.
5. As a user, I want existing scan, dedupe, trim, merge and OCR options to retain their meanings.
6. As a user, I want dangerous options to require confirmation before starting.
7. As a user, I want the confirmed parameter snapshot to be the one actually executed.
8. As a user, I want duplicate starts blocked while confirmation or startup is pending.
9. As a user, I want startup failures to produce a clear terminal error.
10. As a user, I want scan, dedupe, trim and merge progress and logs displayed without freezing the window.
11. As a user, I want the final state and output to come from authoritative backend state.
12. As a user, I want cancellation to clean the entire CLI/ffmpeg process tree.
13. As a user, I want controls locked until cancellation cleanup reaches a terminal state.
14. As a user, I want cleanup failures shown distinctly rather than reported as a successful stop.
15. As a user, I want closing a running window to ask for confirmation.
16. As a user, I want confirmed close to wait for cleanup before exiting.
17. As a user, I want the window to remain responsive during startup, cancellation and close cleanup.
18. As a user, I want a crashed UI not to leave child processes behind.
19. As a user, I want Chinese, spaces and non-ASCII paths to work.
20. As a user, I want keyboard focus, Tab navigation, IME and cancellation to work.
21. As a screen-reader user, I want the primary controls, state and errors exposed through UI Automation/NVDA.
22. As a user, I want the UI to remain usable at supported DPI and multi-monitor configurations.
23. As a developer, I want OCR CLI, protocol and Job Object modules independently testable.
24. As a developer, I want a small UiSession interface rather than a second UI-owned state machine.
25. As a developer, I want slow startup, large logs and shared locks not to block the UI thread.
26. As a maintainer, I want the NSH version, Rust toolchain, Cargo.lock and license data traceable.
27. As a release owner, I want missing licenses, resources or build inputs to fail fast.
28. As an administrator, I want a single Windows x64 installer with explicit installation, shortcut, uninstall and data permissions.
29. As a release owner, I want no WebView2, System/Fixed or fixed-runtime release artifacts.
30. As a maintainer, I want CLI, UI and tag versions to agree.
31. As a project owner, I want no formal release until every hard gate passes.

## Implementation Decisions

- Use NSH Windows native Rust-only mode, explicitly excluding unneeded WebView/QuickJS features and validating the dependency tree on a machine without WebView2.
- Keep the independent OCR CLI, stdout/progress protocol, parameter generation, service, protocol, process, options, storage and Windows Job Object behavior. Do not merge OCR into the UI process.
- Add a deep `UiSession` module whose interface covers `bootstrap`, `validate`, `start(snapshot)`, `observe(run_id)`, `cancel(run_id)`, `request_close` and `confirm_close`. It hides threads, locks, process control, logs and terminal outcomes.
- Freeze a dangerous-parameter snapshot before confirmation; validation, confirmation and start use that same snapshot. Old run state cannot overwrite a new run.
- Run startup, cancellation, close and observation off the UI thread. The UI never waits on the service mutex, child process or cleanup.
- Disable NSH's default 250ms close behavior. A running app closes only after confirmation and authoritative terminal cleanup or an explicit cleanup-failed state.
- Preserve behavior, not pixel layout. File drag-and-drop is out of scope for the first migration.
- Build a side-by-side NSH PoC against the real CLI before replacing the product UI.
- Do not retain Tauri as a product fallback or publish a degraded NSH build. The pre-migration Tauri baseline may remain only as historical source until all gates pass.
- Use a single new Windows x64 installation form with custom Inno Setup. Remove WebView2 checks, fixed runtime, System/Fixed schema and old-user upgrade paths.
- Keep data under the installation directory's `data`; use the agreed administrator-owned and administrator-running model, with no silent permission fallback.
- Treat NSH as latest semantically but update it manually; commit Cargo.lock, toolchain, license and dependency changes with each update. Do not allow CI resolution drift.
- Rework license collection and version/manifest inputs for Rust-only NSH dependencies. Fail fast on missing declarations.

## Testing Decisions

- Test observable behavior through the UiSession interface and real windows, not NSH drawing internals.
- Reuse and extend shell, protocol, options, cancellation, process, abnormal-exit and distribution tests for CLI arguments, terminal states, cancellation and process-tree cleanup.
- Add behavior tests for snapshot races, duplicate starts, run_id fencing, locked cancellation, cleanup-failed, startup failure, close confirmation and error feedback.
- Run real Windows acceptance on Windows 10 1809 and Windows 11 x64, with Chinese/space paths, DPI, multi-monitor, Chinese IME, keyboard navigation, UI Automation/NVDA and no WebView2 Runtime.
- Cover success, cancellation, abnormal exit, startup failure, cleanup failure, slow startup and large logs while proving the window remains responsive and no CLI/ffmpeg processes remain.
- Compare cold/warm start, memory, installed footprint and full installer size under identical CLI, model, media, machine and build conditions. Record results without a predeclared numeric gain.
- Verify clean Inno Setup install, administrator behavior, shortcuts, uninstall, data permissions and version consistency. There is no old-user upgrade contract.
- Every NSH dependency update must include traceable version/commit, Cargo.lock, license data, build artifacts and all hard-gate results.
- Any failure in behavior, process cleanup, Windows interaction, no-WebView2, permissions, installation or build gates blocks release.

## Out of Scope

- OCR algorithms, models, DirectML/CPU selection, ffmpeg decoding strategy and CLI option semantics.
- Merging OCR into the UI process or creating a resident task service.
- NSH TSX/QuickJS/WebView, Node/Vite, DOM/Playwright as the formal UI implementation.
- Drag-and-drop, pixel-perfect reproduction and unrelated product features.
- macOS, Linux, Android, iOS, ARM64 and other platforms.
- Old users, old System/Fixed installs, old WebView2 runtime migration, downgrade and compatibility.
- Publishing Tauri and NSH together or publishing a degraded NSH build.
- Hiding failures through no-op checks or silently deleting unknown files/data.

## Further Notes

- NSH is 0.x and latest may break interfaces; each manual update needs a full regression and recorded evidence.
- No-WebView2 does not automatically mean faster or smaller; A/B evidence decides that.
- After implementation, update the ADR, UI verification, installer, release, license and README documentation so Tauri/WebView2/System/Fixed are no longer described as current product facts.
- The administrator-owned `data` and administrator-running UI decisions are intentional; implementation must not silently revert to a normal-user model without a new decision.
