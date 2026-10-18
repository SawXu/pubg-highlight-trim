# Rust OCR 生产切换记录

## 发布契约

- 默认 artifact：`pubg-highlight-trim-windows-x64.zip`，OCR 后端为 Rust `ocr_engine.exe`，不携带 PaddleOCR/PaddlePaddle/PaddleX 或 Paddle 模型。
- Paddle 仅作为显式兼容回滚：源码安装 `pip install .[paddle]`，运行前设置 `PUBG_OCR_BACKEND=paddle`。兼容 artifact 为 `pubg-highlight-trim-paddle-windows-x64.zip`，不得成为默认下载包；使用该后端必须同时下载/选择该包并设置环境变量。
- “Rust-only OCR”仅表示 OCR 推理后端；CLI/app 仍由 Python 编排并通过 PyInstaller 携带 CPython 及实际使用的 Python 依赖（例如 NumPy/OpenCV）。
- 发布包内的 `SUPPLY_CHAIN_MANIFEST.json` 按实际文件 presence 列出组件，并记录所有其他文件的 SHA-256；manifest 自身不纳入 hash，重复生成结果应稳定。
- UI 包继续通过并列的 `cli` 目录携带 CLI；本切换阶段不新增独立整体 UI manifest。

## 已验证证据

以下为本次切换记录中已获得的验证结果；没有将这些结果表述为“断网证明”：

- Python 测试：224 passed, 3 skipped（由发布协调者在本地执行）。
- Rust 测试：`cargo test --features cli,directml`，10 passed, 1 ignored（由发布协调者在本地执行）。
- .NET UI 测试：26 passed（由发布协调者在本地执行）。
- CLI 构建：Rust DirectML engine、模型和 bundle 构建成功（由发布协调者执行）。
- UI 构建及 DPI manifest 校验成功（由发布协调者执行）。
- 供应链清单单元测试覆盖：Rust-only 组件过滤、CPython 当前解释器版本记录、manifest 连续两次生成稳定且不 hash 自身。
- 默认 CLI 最终构建与 smoke 通过：ACL 拒写目录、模型 SHA-256 校验、CPU JSONL 请求、两个合成视频/两个 worker 默认后端扫描，以及缺失引擎明确失败。默认包 Paddle 残留审计通过。
- UI 最终 zip 解包后的内嵌 CLI 同样通过上述 smoke；修复 Windows 静默输出修改底层句柄导致嵌套 PowerShell/multiprocessing 出现 WinError 6 的问题后，连续两次包验证通过。UI 程序启动并保持运行检查通过；未进行 GUI 操作全流程验收。
- 本地 CLI zip 约 320.7 MB，UI zip 约 377.2 MB，位于 `dist/`。本地构建使用 Python 3.12.15；CI 配置为 Python 3.13，尚未在远端执行。
- 验证日志（本机临时目录）：`issue45-release-final2.log`、`issue45-ui-final2.log`、`issue45-ui-confirm.log`。
- 本次没有管理员权限，未设置防火墙/网络隔离。因此已验证的是本地离线资源完整性和只读执行，不声称完成物理断网验收。Paddle 独立兼容包的构建路径与测试已更新，但本次未实际构建该兼容包；发布前仍需 CI 构建及显式 Paddle smoke。

## 发布前检查

1. 确认默认 zip 名称和 UI 内嵌 CLI 指向 Rust 默认 bundle。
2. 解压后检查 `SUPPLY_CHAIN_MANIFEST.json`：默认包无 Paddle 组件，包含 Rust engine、模型、FFmpeg、CPython/实际 Python 依赖及 hash。
3. 连续执行 `scripts/build_supply_manifest.py <bundle>` 两次，确认 manifest 内容一致且不含自身路径。
4. 如发布 Paddle 兼容包，必须单独命名并在 release notes 中标注显式选择方式。
5. 保留 CLI smoke、UI smoke、runtime/model hash 校验结果；不要以“无网络”措辞替代这些具体证据。
