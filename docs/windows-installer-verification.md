# Windows 安装生命周期验收（issue #104）

本文件描述**真实安装包**在 Windows disposable VM 中的验收流程。#105 用户明确本次没有 VM，允许先交付双运行时构建支持；本矩阵仍保留为未执行，不作为本次已通过结果，详见 [发布验证状态](tauri-release-verification.md)。它不是 mock 测试，也不能以构建成功或静态脚本测试代替。当前仓库没有宣称以下矩阵已通过；每个阶段必须由执行者填写证据目录和结果。

## 安全边界

- 严禁在开发宿主机执行提升安装、卸载、故意锁文件或破坏 WebView2/VC 运行时。
- `scripts/test_installers_windows.ps1` 默认是只读 audit：使用 CIM、进程实际 `ExecutablePath`、文件 SHA-256、注册表和 ACL 收集证据，不启动安装器、不修改系统。
- 只有 disposable VM 明确授权后，才可使用 `-RunInteractive -DisposableVM`。脚本仍要求人工确认每一阶段，不假装成功；取消、错误和未执行都要如实记录。
- VM 应有快照、管理员 UAC 可交互、Explorer 桌面用户与管理员凭据分离；准备 System 和 Fixed 两个真实 EXE，Fixed runtime 完全离线可用。

## 只读审计入口

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test_installers_windows.ps1 `
  -Variant System -InstallRoot 'C:\Program Files\PUBG Highlight Trim' `
  -EvidenceDirectory .\artifacts\windows-installer-verification
```

此命令保存脱敏 `registry.json`、`processes.json`、`loaded-modules.json`、`files.json`（presence/length/SHA-256）、角色化 `paths-and-acl.json`、网卡 up 状态、WebView 注册是否存在及 `summary.json`。实际 CIM 路径仅在内存校验，落盘为 `inInstallRoot` 与 `<install>`/`<system-runtime>`/`<redacted>`；未知文件名也脱敏。PID、父 PID、session、SID、计算机名、命令行不落盘，进程归属在内存关联。SID 映射 SYSTEM/Administrators/当前用户/AppContainer/other 角色。settings 不复制，仅保留文件清单 hash；run.log 只提取固定布尔字段，不复制原文。模块访问失败明确记录 error，不能视为通过。在每一阶段重新运行并将目录写入验收记录。审计应确认：进程路径确实位于安装目录；CLI/UI/DLL、`WebView2Runtime`（Fixed）和 `data` 路径实际存在；注册键 `HKLM\Software\Microsoft\Windows\CurrentVersion\Uninstall\pubg-highlight-trim` 的 `InstallLocation`、`Variant`、`UninstallString` 与现场一致；代码目录和 data 目录 ACL 与 `scripts/installer/README.md` 相符；没有共享运行时或意外用户 profile 路径。

## 每阶段真实交互入口

在 **VM 普通用户的 64 位 PowerShell** 中执行（脚本不会自行提升，EXE 通过 UAC 请求授权）：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test_installers_windows.ps1 `
  -Variant Fixed -Phase fresh -InstallRoot 'D:\验收 应用\PUBG Highlight Trim' `
  -InstallerPath 'C:\packages\pubg-highlight-trim-fixed-setup.exe' `
  -RunInteractive -DisposableVM
# 下一阶段使用同一入口，换 Phase / InstallerPath；卸载使用真实的 <InstallRoot>\Uninstall.exe。
# 启动 UI 后另做 audit，以捕获活跃进程；不要从提升安装器中启动应用。
```

入口在启动 EXE 前保留 `before/`，要求输入 `RUN` 后才启动，并在人工结束当前阶段后收集后状态。`Variant` 是操作者标注，不是自动判定。它不驱动 UAC、不传静默开关、不调用内部事务引擎，也不根据启动成功/退出码宣称验收通过；失败和取消均需留证。只读 audit 的 `-Phase` 可使用 `running`、`upgrade`、`locked`、`uninstall` 等。实际路径以安装器选择值为准，应显式传 `-InstallRoot`，不要依赖默认值。

## 真实交互矩阵（System 与 Fixed 各执行）

在每一项前后保存 audit 证据，并记录 VM 快照、Windows 版本、安装包 SHA-256、操作者、时间、结果和日志路径。升级须准备可区别的 A/B 两个真实构建，检查注册 DisplayVersion 与打包版本一致，同时比较程序 SHA-256/构建 commit，不能仅凭版本号证明文件已更新。业务样本至少包含安装目录下未被 manifest 拥有的 `业务样本.txt`、`data` 下未知文件和外部输出目录，各自预存 SHA-256。设置/日志正常运行可能变化，失败不变的比较应在 UI 关闭后进行。

| 场景 | VM/前置条件 | 前证据 | 后证据 | 实际观察/结果 |
|---|---|---|---|---|
| System 全新/普通用户/另一用户隔离 | 待填 | 待填 | 待填 | NOT RUN |
| Fixed 全新/普通用户/另一用户隔离 | 待填 | 待填 | 待填 | NOT RUN |
| System 离线，有/无 WebView2 | 两个独立快照 | 待填 | 待填 | NOT RUN |
| Fixed 离线，无共享 WebView2/VC CRT | 干净快照 | 待填 | 待填 | NOT RUN |
| 用户拥有/可写父目录，中文空格/非系统盘；父 ACL 不变 | 新的应用子目录；受支持 ACL 文件系统 | 待填 | 待填 | NOT RUN |
| 原路径假 root/复制同 manifest，身份不匹配 | VM 移走原树并放替代树 | 待填 | 待填 | NOT RUN |
| 两变体各自 A → B 升级 | 待填 | 待填 | 待填 | NOT RUN |
| System → Fixed / Fixed → System，取消与确认 | 待填 | 待填 | 待填 | NOT RUN |
| 占用失败/释放后重试/事务失败恢复 | 待填 | 待填 | 待填 | NOT RUN |
| 两变体卸载保留数据/拒绝旧 DeleteData 参数 | 独立恢复快照 | 待填 | 待填 | NOT RUN |

审计只读取文件；缺失文件不自动报失败，hash 为 null 表示无法读取。完整目录枚举和模块读取可能因权限/运行中锁定不全，应由现场操作者补证，不能从空数组推断“无残留”。

1. **全新安装与路径**：从普通桌面用户启动真实 EXE，接受 UAC，选择带空格和中文的全新本地目录。分别覆盖非系统盘、用户拥有/可写父目录（例如 `D:\Games\PUBG Highlight Trim`）和非用户目录。确认父目录 owner/ACL 安装前后完全一致；代码目录仅用户 RX、data 可写；安装器显示目录选择、没有自动启动应用；检查实际 process path、快捷方式、所有 payload、注册键和卸载入口。已存在不安全的非空目标目录应明确要求选择新的应用子目录，不得接管、删除业务文件或换盘。
   - **路径替换对抗**：在 VM 中安装后移走整树，在原路径放入另一目录并复制同一 manifest，调用原可信安装包升级。必须在移动文件/改 ACL 前因目录身份不匹配停止；外部硬链接目标和假树未知文件的 ACL、内容均不变。事务中并发 rename/设 junction 应被拒绝或安装安全中止。不能用摘要相同替代目录身份校验。
2. **权限与 data ACL**：以目标桌面用户正常启动 UI/CLI（非管理员），验证可读代码、可写 `data`，并现场核对 `Get-Acl`，只记录角色化 SYSTEM、Administrators、目标用户及 Fixed runtime AppContainer RX（不保存原始 SID/SDDL）。使用另一普通用户尝试读取/写入 data，记录拒绝；不要把管理员 UAC 账户误当 data owner。
3. **System 离线/共享运行时边界**：快照后确保机器上已有受支持 WebView2，断网运行 UI，记录真实加载/渲染日志和 process path。证实安装目录没有 `WebView2Runtime`，且没有安装器下载、共享 runtime 被修改或额外运行时副本。另从预先准备的无 WebView2 快照执行 System 安装，应清楚拒绝且旧安装不变；不得在宿主卸载/改名/破坏系统 WebView2 来制造条件。单独准备无共享 VC++ Redistributable 的快照，检查 CRT 模块路径来自应用目录，完成真实页面交互/CLI 业务；存在系统 DLL 本身不代表使用了共享 CRT。注册键不存在不是运行时缺失的充分证明，必须结合干净镜像来源、进程/模块路径和启动结果。
4. **Fixed 离线**：在没有网络、没有依赖机器 WebView2 的干净快照中安装 Fixed；确认 `WebView2Runtime\msedgewebview2.exe`、`msedge.dll`、`icudtl.dat`、`resources.pak` 实际加载，记录 UI 日志/进程路径和 runtime ACL。不得仅凭文件存在声称渲染成功。用 hypervisor 断开虚拟网卡（记录设置截图和网卡快照；网卡状态不能证明完全无联网），全程不补装运行时。浏览器子进程必须通过父 PID/命令行归属到本应用，不能把其他软件的 WebView2 误作证据。实际 `RUNTIME` 日志记录的数据/资源路径应与模块、CIM 内存校验结论和现场交互互相印证；做一次设置保存与重启确认 data 生效。
5. **升级同变体**：在已有安装和 data 中放置业务文件、未知文件及日志，运行同变体新包。确认旧 owned files 被替换、未知文件和业务文件保留，data 内容不丢失，注册键 ownership digest 更新且路径不变。占用任一 owned 文件时应安全拒绝并恢复。
6. **System ↔ Fixed 切换**：在快照中执行两方向切换，确认出现明确确认提示；选择 Cancel 时安装目录、注册键、data 完全不变；选择 Yes 后记录变体、runtime 文件、ACL 和实际离线启动结果。不同安装目录必须拒绝，不得接管旧目录。
7. **占用失败与恢复**：启动并保持真实 UI/CLI 或打开 owned 文件句柄，再运行升级/卸载。预期安装器拒绝，旧文件、ACL、注册键和 data 仍一致；释放句柄后重试。若自然进程未锁定可升级文件，可在 VM 中用真实只读句柄保持已拥有的 UI EXE：`$h=[IO.File]::Open('<InstallRoot>\pubg-highlight-trim-ui.exe','Open','Read','Read')`，结束后必须 `$h.Dispose()`。不要为测试赋予普通用户代码写权限。此项证明占用拒绝/旧版可重启，不自动证明事务中途回滚；如无可安全复现的中途失败，后者记 NOT RUN，勿用强杀/断电替代（当前实现不承诺该恢复）。若出现 recovery 目录，保留并记录路径，禁止删除后宣称恢复。
8. **卸载与业务文件保留**：从注册卸载入口运行，默认选择保留 data。确认仅 ownership manifest 列出的程序文件和控制文件处理，业务文件、未知文件、浏览器/cache 树保留，注册键和快捷方式状态符合预期。恢复已安装快照后，验证卸载明确告知保留全部 data，旧 `-DeleteData` 参数在修改前拒绝。特别放置同名 settings/settings.tmp/run.log 及浏览器/cache 子目录业务文件并核对内容不变。完整 UI 数据可选清理因缺少可信文件归属而尚未实现；不得把只删固定文件名误写成安全完整清理。

## 证据与判定

每阶段至少保存：安装包 SHA-256、脱敏 audit JSON、脱敏日志/截图（UAC/目录/切换/卸载提示）及人工观察。不要保存原始 `Get-Acl` SDDL 或 CIM 输出；现场核对后只记录角色/归属结论，不记录 SID/PID/session/私人路径。原始 settings 不得附入证据。脚本不打印私人证据路径，目录由操作者本机查阅。失败、取消、未执行、环境不满足都标记为 `FAIL`/`NOT RUN`，不得改写为通过。真实完整矩阵尚未在开发宿主执行；本入口的脚本语法检查只能证明脚本可解析，不能证明安装生命周期。

建议先做无副作用语法检查（解析错误为零仅表示语法通过）：

```powershell
$tokens = $null; $parseErrors = $null
$null = [System.Management.Automation.Language.Parser]::ParseFile(
  (Resolve-Path scripts/test_installers_windows.ps1).Path, [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count) { $parseErrors; throw 'PowerShell parse failed' }
```
