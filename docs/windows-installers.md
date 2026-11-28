# Windows 安装包构建

**当前为待真实安装生命周期验收的实现，不是已通过 #104 全部验收的正式发行版。** 旧 System/Fixed 未执行矩阵仅作为[历史记录](windows-installer-verification.md)保留，不是当前指南。Native 生命周期测试入口为 `scripts/tests/test_installer_lifecycle_windows.ps1`，只能在明确授权的 disposable VM 中执行；安全与已知限制见 [安装器实现说明](../scripts/installer/README.md)。

项目构建入口提供单一的 Windows native NSH 安装包。它不使用 Tauri、Wry 或 WebView2，安装包是唯一 GUI 发布入口，不生成或发布免安装 GUI ZIP。

## 构建

```powershell
# Native 单一 artifact
powershell -ExecutionPolicy Bypass -File scripts/build_installers_windows.ps1 -Variant Native
```

需要 NSIS **3.11+** 的 `makensis.exe` 位于 PATH。版本默认读取 `ui/native-ui/Cargo.toml` 的 package version，可以用 `-Version 0.4.0` 指定纯数字 SemVer。签名接入点为 `-SignInstallerScript <验证并签名的脚本>`，脚本收到 `-OutputDirectory` 和 `-Variant`，失败会中止构建。未签名版本会显示未知发布者或 SmartScreen 警告；不要关闭安全软件绕过验证。

Native artifact 使用 CLI 的完整 license inventory，并生成布局根部的 `distribution.json`（`pubg-highlight-trim/distribution/v2`、`artifact:native-ui`）及供安装器按文件所有权升级/卸载的 `installer-manifest.json`。不打包 WebView2 runtime，也不接受 runtime、license 或下载输入。安装器引擎位于 `scripts/installer/`，缺失时构建会 fail-fast。

`build_native_ui_windows.ps1` 是唯一 UI 开发布局入口：它只构建原生可运行目录，不生成安装包；支持 `-SkipCliBuild` 和 `-Configuration`。

## 安装与数据

安装器启动申请管理员权限（NSIS `RequestExecutionLevel admin`），始终展示目录选择页；升级必须选择现有注册目录。程序不从安装器启动：安装结束后关闭安装器，从开始菜单启动应用；应用同样按管理员权限运行，UAC 是预期行为。native UI 不需要共享或内置 WebView2。

Native 预检使用目标普通桌面用户的 Explorer token，而不是提升管理员的 HKCU 或环境；预检无法安全识别或启动该用户时拒绝安装/切换，保留旧文件。异帐号 UAC 的实现已加入，但缺少 VM，尚未实测，不能宣称覆盖所有环境。

支持本地固定磁盘上的自选路径，包括用户拥有的 `D:\Games`、非用户目录、中文及空格路径。安装器不会因为父目录属于普通用户而拒绝，也不会修改安装目录外的父目录权限或回退到 C 盘。事务期间持有目录句柄防止路径被重命名或重解析替换，升级/卸载还会核对注册信息绑定的真实目录身份。

请选择一个新的应用目录（例如 `D:\Games\PUBG Highlight Trim`）、空目录或现有受保护的安装目录。已有不安全空目录会仅在确认仍为空时删除该空目录对象，再原子创建受保护的新目录；不会原地改权限接管旧对象。不会将已有不安全的非空用户目录原地接管为代码目录：修改 ACL 不能撤销其他进程已持有的写句柄。UNC、网络映射盘、驱动器根目录、重解析路径，以及无法提供所需 Windows ACL 的文件系统不在支持范围内；错误会明确显示，不换盘或修改父目录权限。

安装版设置和日志固定在 `<安装目录>/data`，不采用开发环境覆盖。代码、依赖和 data 均为管理员拥有并受管理员 ACL 保护；应用以管理员权限运行。业务输出、归档与 OCR 缓存继续使用 CLI 策略，不承诺系统临时文件或注册信息零系统盘写入。

卸载保留全部 data，之后须人工检查再清理。此前只按设置/日志文件名删除的选项已移除：可写目录中的同名文件也可能是用户业务文件，不能仅凭名称证明归属。直接传旧 `-DeleteData` 参数会在任何修改前明确拒绝。不会递归删除安装根目录或系统文件。**完整 UI 数据删除选项仍需改变数据所有权约定后才能安全实现，尚未满足。**

## 发布边界

只支持 EXE 安装包；不支持 GUI ZIP。签名可通过顶层 `-SignInstallerScript` 接入，未签名候选明确提示未知发布者/SmartScreen，不默认声称已验收。Native CI 运行共享 core/native 测试及原生 release build；发布生成 SHA-256 和 `release-evidence.json`，未实测生命周期项目仍标记为未验收。测试命令见 [UI 文档](../ui/README.md)。安装目录升级/卸载必须依据 ownership manifest 处理，不得递归删除整个安装目录；用户数据、日志和业务输出应保留。
