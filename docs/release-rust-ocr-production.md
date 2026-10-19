# Rust OCR 生产切换记录

## 当前 #63 源码与构建状态（尚非最终发布验收）

当前集成分支的正式入口是 Cargo 构建的完整 Rust `pubg-highlight-trim.exe`，直接调用 OCR library；`ocr_engine.exe` 是独立推理诊断。官方 Cargo/.NET/PowerShell 测试、构建、CI 和发布流程不安装或调用 Python，产物不应包含 CPython/PyInstaller/Python OpenCV/NumPy/Paddle。当前开发、测试与构建命令见根目录双语 README。

模型由 `cargo run --manifest-path rust/Cargo.toml --bin xtask -- download --output-dir <models>` 在构建阶段下载；`verify --check-onnx`、`onnx-smoke`、`real-roi`、`compare`、`manifest`、`audit`、`size` 提供校验与报告。`verify --check-onnx` 的完整图校验、直接双模型推理以及其他尚未替代开发工具保障仍见[退休审计](python-retirement-audit.md)，不能把现有轻量 protobuf 校验表述成完整 ONNX checker。最终 CLI/GUI zip、离线与干净无 Python Windows、完整原片差异与性能门槛尚未由本次收尾验收；不要据此关闭 #63 或发布。

下面保留早期 OCR-only 切换时的事实、命令和测量，不将历史 Python 发布结构改写成当时已是全 Rust。

## 历史发布契约

- 默认 artifact：`pubg-highlight-trim-windows-x64.zip`，OCR 后端为 Rust `ocr_engine.exe`，不携带 PaddleOCR/PaddlePaddle/PaddleX 或 Paddle 模型。
- #63 收尾切片后，Rust 为唯一 OCR 后端：删除 Paddle 运行时选择和 `[paddle]` extra。旧 `PUBG_OCR_BACKEND` 设置会被忽略，不再提供运行时兼容回退；历史 `paddle-*` 检测方法标签为报告兼容保留。
- “Rust-only OCR”仅表示 OCR 推理后端；CLI/app 仍由 Python 编排并通过 PyInstaller 携带 CPython 及实际使用的 Python 依赖（例如 NumPy/OpenCV）。
- 发布包内的 `SUPPLY_CHAIN_MANIFEST.json` 按实际文件 presence 列出组件，并记录所有其他文件的 SHA-256；manifest 自身不纳入 hash，重复生成结果应稳定。
- UI 包继续通过并列的 `cli` 目录携带 CLI；本切换阶段不新增独立整体 UI manifest。

## 已验证证据（历史切换记录）

以下为早期生产切换时已获得的验证结果，不代表 #63 全量迁移验收，也没有将这些结果表述为“断网证明”：

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
- 本次没有管理员权限，未设置防火墙/网络隔离。因此已验证的是本地离线资源完整性和只读执行，不声称完成物理断网验收。当时未实际构建 Paddle 独立兼容包；#63 已停止支持该后端，不再要求 Paddle smoke。

## 历史发布前检查（不再作为当前命令）

1. 确认默认 zip 名称和 UI 内嵌 CLI 指向 Rust 默认 bundle。
2. 解压后检查 `SUPPLY_CHAIN_MANIFEST.json`：默认包无 Paddle 组件，包含 Rust engine、模型、FFmpeg、CPython/实际 Python 依赖及 hash。
3. 连续执行 `scripts/build_supply_manifest.py <bundle>` 两次，确认 manifest 内容一致且不含自身路径。
4. 确认不再发布 Paddle 兼容包；旧环境变量不得启用其他 OCR 后端。
5. 保留 CLI smoke、UI smoke、runtime/model hash 校验结果；不要以“无网络”措辞替代这些具体证据。
