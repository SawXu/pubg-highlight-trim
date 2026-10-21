# Windows 安装包构建

**当前为待真实安装生命周期验收的实现，不是已通过 #104 全部验收的正式发行版。** 验收矩阵见 [windows-installer-verification.md](windows-installer-verification.md)，安全与已知限制见 [安装器实现说明](../scripts/installer/README.md)。

项目构建入口提供两个同标识的 Windows 安装包变体：`System` 使用机器上已安装的 WebView2，`Fixed` 将可信的 WebView2 Fixed Version runtime 随包放在 `WebView2Runtime/`。安装包是唯一发布入口，不生成或发布免安装 ZIP。

## 构建

```powershell
# System
powershell -ExecutionPolicy Bypass -File scripts/build_installers_windows.ps1 -Variant System

# Fixed（必须提前解包、离线提供并校验 runtime）
powershell -ExecutionPolicy Bypass -File scripts/build_installers_windows.ps1 `
  -Variant Fixed -FixedRuntimePath C:\trusted\WebView2Runtime `
  -FixedRuntimeSha256 <msedgewebview2.exe-SHA256>
```

需要 NSIS **3.11+** 的 `makensis.exe` 位于 PATH。版本默认读取 `ui/package.json.version`，可以用 `-Version 0.4.0` 指定纯数字 SemVer。签名接入点为 `-SignInstallerScript <验证并签名的脚本>`，脚本收到 `-OutputDirectory` 和 `-Variant`，失败会中止构建。未签名版本会显示未知发布者或 SmartScreen 警告；不要关闭安全软件绕过验证。

Fixed runtime 至少必须包含 `msedgewebview2.exe`、`msedge.dll`、`icudtl.dat` 和 `resources.pak`。脚本只校验本地输入，不联网下载或静默补装 WebView2。生成布局根部的 `distribution.json`（以及供安装器按文件所有权升级/卸载的 `installer-manifest.json`）。安装器引擎位于 `scripts/installer/`，缺失时构建会 fail-fast。

`build_ui_windows.ps1` 仍是开发布局入口：它只构建可运行目录，不声称自包含 WebView2，也不生成安装包。其 `-SkipCliBuild` 和 `-Configuration` 参数保持兼容。

## 安装与数据

安装器启动申请管理员权限，始终展示目录选择页；升级必须选择现有注册目录，两版不能并装。程序不从安装器启动：安装结束后关闭安装器，从开始菜单以普通权限运行。共享运行时缺失或不兼容时选择内置版，本安装器不会下载共享运行时。

系统版运行时预检暂仅支持同帐号 UAC 提权；使用其他管理员凭据时明确拒绝系统版安装或切换，保留旧安装，需选择内置版。以目标普通用户身份预检系统运行时的支持尚未完成。

支持本地固定磁盘上的自选路径，包括用户拥有的 `D:\Games`、非用户目录、中文及空格路径。安装器不会因为父目录属于普通用户而拒绝，也不会修改安装目录外的父目录权限或回退到 C 盘。事务期间持有目录句柄防止路径被重命名或重解析替换，升级/卸载还会核对注册信息绑定的真实目录身份。

请选择一个新的应用目录（例如 `D:\Games\PUBG Highlight Trim`）、空目录或现有受保护的安装目录。已有不安全空目录会仅在确认仍为空时删除该空目录对象，再原子创建受保护的新目录；不会原地改权限接管旧对象。不会将已有不安全的非空用户目录原地接管为代码目录：修改 ACL 不能撤销其他进程已持有的写句柄。UNC、网络映射盘、驱动器根目录、重解析路径，以及无法提供所需 Windows ACL 的文件系统不在支持范围内；错误会明确显示，不换盘或修改父目录权限。

安装版设置、日志、WebView2 数据和缓存固定在 `<安装目录>/data`，不采用开发环境的 `PUBG_HIGHLIGHT_TRIM_DATA_DIR` 覆盖。代码/依赖仅可读执行，data 仅目标桌面用户和必要系统主体可写。业务输出、归档与 OCR 缓存继续使用 CLI 策略，不承诺系统临时文件或注册信息零系统盘写入。

卸载默认保留全部 data；显式选项只删除设置与日志三个固定文件，浏览器数据和缓存暂时保守保留，须人工检查后清理。不会递归删除安装根目录或系统共享 WebView2。**完整 UI 数据删除选项尚未满足。**

内置 WebView2 的安全更新由项目随新版手动安装包交付，需维护者定期更新受信任 Fixed runtime 输入；本版没有应用内自动更新或运行时自动补装。

## 发布边界

只支持 EXE 安装包；不支持 GUI ZIP。签名可以由安装器引擎通过可选参数接入，但未验证签名的 GUI 不应默认发布。安装目录升级/卸载必须依据 ownership manifest 处理，不得递归删除整个安装目录；用户数据、日志和业务输出应保留。
