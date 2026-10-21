# Tauri 双安装包发布与验证（#105）

## 当前约定和状态

用户已确认发布入口→成品、真实安装器→Windows、UI/Rust 壳→CLI 三个测试边界；并明确没有 Windows 10/11 disposable VM、可信 Fixed runtime 或固定视频样本，本次不要求完成原票的完整 VM 验收矩阵，只要求支持系统与随包 WebView2。不能因此声称干净主机、离线渲染或完整安装生命周期已经通过。

独立 Rust CLI 的算法、参数和发布保障保留，不恢复 WPF UI，也不新增 GUI ZIP。GUI 双安装器必须使用相同的一次 UI/CLI 构建，并显式提供锁定的可信 Fixed runtime。没有可信输入时只构建既有 CLI，不自动下载最新 runtime 或拿宿主共享 runtime 冒充 Fixed。

## 维护者构建入口

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/release_windows.ps1 `
  -InputDirectory 'D:\trusted\fixed-input' -LockSha256 '<runtime-lock.json SHA-256>'
```

需要 Windows x64、Rust MSVC、Node/npm、NSIS 3.11+、7-Zip。npm 使用 `ci`，Cargo 使用 `--locked`；依赖版本锁定不代表 PE/安装器逐字节可重现（工具链、签名和构建时间也会影响字节）。禁止把未经校验的网络输入直接塞入 runtime。可选签名脚本必须验证签名并在失败时退出非零；没有证书不要求采购，不要求用户关闭安全软件。

输入目录包含 `runtime.zip` 与 `runtime-lock.json`。ZIP 内必须有 `runtime/`（纯 Fixed runtime 文件）、`fixed-runtime-license.json` 和许可证清单引用的原始条款/声明文件（例如 `notices/`）。许可证清单不能放在 `runtime/` 内，避免逐文件 hash 自引用。锁文件格式：

```json
{
  "schema": "pubg-highlight-trim/fixed-runtime/v1",
  "archive_sha256": "<runtime.zip SHA-256>",
  "files": [
    { "path": "runtime/msedgewebview2.exe", "sha256": "<SHA-256>" }
  ]
}
```

`files` 必须完整列出所有 ZIP 文件及其 SHA-256，上例仅示意单条。维护者从 Microsoft 官方 Fixed Version 分发取得完整 x64 runtime，审核适用再分发条款、第三方声明及安全维护版本后准备输入；记录来源和版本，不从其他机器随意复制。构建入口验证锁文件摘要、压缩包摘要、文件集合和每文件摘要。许可证收集还要求安装包附带运行时相关条款；缺失即拒绝构建。上例中的完整文件列表必须同时覆盖 runtime 文件、`fixed-runtime-license.json` 和所有条款文件；编排从已验证树计算许可证清单摘要传给构建器，不需要另设未受锁保护的输入。许可证清单的完整格式和 CLI 参数见 [collect_ui_licenses.md](../scripts/collect_ui_licenses.md)。

CI 的手动 release 支持可信输入下载及锁摘要，具体参数见 `.github/workflows/release.yml` 的 `workflow_dispatch.inputs`。GUI 产物仍为未验收候选，仅上传 workflow artifacts，不由成功构建自动晋升正式发行；既有标签触发的独立 CLI release 不变。

产物包括带版本与 `windows-x64-system` / `windows-x64-fixed` 的 EXE、CLI ZIP、SHA-256 校验文件和机器可读证据（GUI 候选 NOT_ACCEPTED）。安装器内部文件需从真实 EXE 提取后与 staging 的 hash 对照；这是**成品完整性校验，不是安装运行验收**。UI/前端/Tauri 许可证和完整 payload manifest 必须随包。

## 本地已执行验证

本次 Windows 开发宿主执行：

- `npm --prefix ui ci`、`run typecheck`、`test`：通过，7 个工作流测试。
- `cargo check --locked --manifest-path ui/src-tauri/Cargo.toml`：通过。新增 runtime probe 环境清理测试先红后绿，distribution 专项 3 项通过。
- 离线实际 UI 许可证收集：274 个组件，原始声明和锁摘要完整生成。
- `cargo test --locked --manifest-path ui/src-tauri/Cargo.toml`：14 项通过；固定真实视频测试因缺少样本按设计 ignored。
- `npm --prefix ui run tauri -- build --no-bundle -- --locked`：生产前端和 release 原生 EXE 构建通过，不依赖 Vite 开发服务器。
- 将该 release EXE 复制到独立临时目录，写入 distribution 配置，调用真实原生 `--check-webview2-runtime`：System 在当前宿主已安装共享 runtime 时退出 0；Fixed 没有随包文件时退出 1，即便宿主有共享 runtime 也不回退；预检没有创建 `data`。
- `cargo test --locked --manifest-path rust/Cargo.toml`：默认工作区全套 176 项通过，1 项 ignored。首次因当前目录没有 FFmpeg 失败，随后显式使用已有 FFmpeg 工具目录重跑通过。最终重跑发现既有 xtask 测试按 PID 命名临时目录撞上残留目录，隔离到全新 TEMP/TMP 后完整重跑通过，未为此修改无关 CLI 代码。未执行依赖模型/网络的 ignored 扩展验收；它们继续由原 Windows assurance 保证。

- 发布入口专项使用 NSIS 3.11 真正编译 System/Fixed 两个受控 fixture EXE，并以 7-Zip 实际提取、逐文件验证 hash，最终 SHA-256 生成通过。首次提取暴露 NSIS 会生成额外 `Uninstall.exe`，已仅为该根级 PE 文件增加明确允许，其余额外文件仍拒绝。这些 fixture 不是完整生产 runtime。
- 许可证 6 项 Node 测试、公开输入拒绝、双变体 packaging、许可证→产物、目录保护专项通过；所有 PowerShell 脚本语法检查通过。

以上不是 Fixed 成品渲染证据，也不是完整 UI→安装器→真实媒体的端到端验收。

## 双轴 review

Standards 与 Spec 分别独立审查。已修复预检临时目录创建/固定之间的替换风险（校验固定对象的 owner/ACL 和空目录，未可信目录不清理），真实 NSIS 提取契约、runtime 子树接线、预检环境/策略一致性及测试路径泄露。Spec 另指出 Fixed 的提升管理员 HKCU 与桌面用户上下文不一致，已修正为与 System 一样使用目标桌面用户 token；Fixed 预检仅复制根依赖与随包 runtime 到新受保护目录，不修改既有文件权限。完整安全可选清理继续列为未完成，不以人工清理冒充满足。

## 开发宿主真实安装补验

后续按用户要求，从 Microsoft 官方下载 x64 Fixed Runtime 155.0.4283.62；主 EXE Authenticode 为 Valid、签发者 Microsoft Corporation。原始条款及 runtime 完整文件 hash 用于本地评估构建，不代表完成公开再分发法律审查。

完整 CLI bundle smoke、生产 UI 构建及 Fixed 安装器真实构建/提取通过。安装器约 482 MiB；不是实际安装占用或性能指标。用户在开发宿主交互执行后确认安装成功，只读注册核验为 Fixed、0.4.0。尚未取得安装后应用的实际 runtime 验证日志，不能据此宣称页面渲染、离线或 Windows 10/11 全矩阵通过。

真实安装暴露并修复三项静态提取无法发现的问题：

1. NSIS 原生 `System.dll` 插件被 CodeDOM 误作 .NET 引用：两处 `Add-Type` 改用框架程序集绝对路径；同名原生 DLL 工作目录回归先失败后通过。
2. 父目录先关闭 ACL 继承导致子项失去访问权限：改为叶子及深层目录先于祖先应用 ACL。
3. 严格目录 pin 阻止 Windows 移动子文件：程序目录采用允许文件更新、仍拒绝目录 delete sharing 的 pin；用户可写祖先仍用严格 pin。真实文件移动及目录不可替换回归通过。

安装页面改用中文 MUI2 欢迎/目录/安装/完成流程、专用侧栏和字体，不再启动即弹英文说明；错误才展开详细日志。不自动以安装器管理员身份启动应用。

## 未执行、指标和已知限制

| 项目 | 状态 / 原因 |
|---|---|
| Fixed 完整真实 runtime 打包 / 安装 / 页面渲染 | 打包及宿主安装已补验；页面渲染与实际加载路径仍未取证 |
| Windows 10 / 11 独立干净主机、断网及无共享 runtime | NOT RUN：没有 disposable VM |
| UAC 异帐号、实际 ACL、升级、双向切换、占用恢复和卸载 | NOT RUN：未在开发宿主执行提升或破坏性操作 |
| 固定真实视频 summary、记录和解码媒体语义 | NOT RUN：本次未提供样本；既有 opt-in 测试保留 |
| 两版 EXE 大小、真实安装目录占用 | Fixed EXE 约 482 MiB；System EXE 和真实安装占用未测量 |
| 首次启动、内存和业务处理指标 | NOT MEASURED：不承诺缩包或处理提速 |

完整原始矩阵与证据收集方法保留于 [windows-installer-verification.md](windows-installer-verification.md)。发生安装故障时保留 recovery 目录、成品 SHA-256、构建提交和脱敏日志；不要删除恢复文件后声称已恢复。数据清理仍须遵守“即使业务文件位于安装目录也不能误删”的安全约定；无法识别所有权时必须保留并报告限制。
