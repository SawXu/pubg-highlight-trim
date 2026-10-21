# pubg-highlight-trim

[English](README.md) | 简体中文

Windows 平台命令行工具，基于 OCR 事件对 PUBG 的 NVIDIA Highlight（精彩时刻）片段进行裁剪。

仓库同时提供 Tauri 2 + Vue 3 Windows 10/11 x64 桌面 UI，将现有 Rust CLI 作为独立进程运行。安装包提供系统 WebView2 和内置 Fixed Version WebView2 两种变体，不维护旧 WPF UI。参见[安装说明](docs/windows-installers.md)及[发布验证状态](docs/tauri-release-verification.md)。

默认行为同时检测“你击倒/淘汰敌人”和“敌人击倒/淘汰你”两类事件，保留事件前 5 秒与后 1 秒，跳过前 2 秒内的事件；对燃烧瓶 / 火瓶淘汰事件保留前 10 秒。

## 安装

无需安装 Python。

1. 从 [最新 release](https://github.com/SawXu/pubg-highlight-trim/releases/latest) 下载 `pubg-highlight-trim-windows-x64.zip`。
2. 将 zip 解压到任意目录。
3. 默认压缩包包含 `pubg-highlight-trim.exe`、打包的 `ffmpeg.exe` / `ffprobe.exe`、Rust-only OCR 引擎及 PP-OCRv6 ONNX 模型；**不包含** PaddleOCR、PaddlePaddle 或 Paddle 模型。

图形界面提供时，请选择明确命名的 `pubg-highlight-trim-<版本>-windows-x64-system-setup.exe`（系统版）或 `pubg-highlight-trim-<版本>-windows-x64-fixed-setup.exe`（内置版），并与随附校验文件核对 SHA-256。**环境不确定或需要离线安装时推荐内置版。** 系统版要求已安装兼容共享 WebView2（120+），缺失时明确拒绝，不下载补装；内置版只使用随包运行时，资源缺失不回退系统版。两版共享安装位置和设置，不支持并装。安装器申请管理员权限并让你选择目录；完成后从开始菜单以普通权限运行。未签名包可能出现未知发布者/SmartScreen 提示，不要关闭安全软件。GUI 成品在收集完整证据前仍属草稿/未验收，旧 release 不能作为新 UI 的验收证据。独立 CLI ZIP 继续保留。

在解压目录下直接运行：

```powershell
.\pubg-highlight-trim.exe "C:\Users\you\AppData\Local\Temp\Highlights\PLAYERUNKNOWN'S BATTLEGROUNDS\淘汰"
```

## 使用

### 图形界面

完成开发构建后，双击 `dist\pubg-highlight-trim-ui\pubg-highlight-trim-ui.exe` 启动。窗口顶部会先检查并显示 CLI 版本；若提示“未找到 CLI”，请确认 `cli` 文件夹与 UI exe 位于同一目录，且其中存在 `cli\\pubg-highlight-trim.exe`。

主要操作如下：

1. 在“输入”中选择一个 PUBG Highlight 文件夹，或通过文件选择器多选 MP4。多选输入只能包含 MP4；勾选“递归扫描子文件夹”后才会搜索所选文件夹的子目录。
2. 可选填“输出目录”；留空时由 CLI 根据输入自动创建本次运行的目录。
3. 在“检测目标”中选择“全部”“自己倒地”或“我的击杀”；两个具体目标可以同时选择（此时分别生成两个集锦），并选择游戏语言（自动识别、简体中文、繁体中文、English）。
4. 设置“扫描模式”（自动、快速、完整）和“剪辑范围”（事件前/后秒数）。
5. 常用复选项包括“合并输出”“仅扫描”和“覆盖输出”。“仅扫描”只检测并写入记录，不生成视频；“覆盖输出”允许复用同名输出目录或合并文件。
6. 展开“专家设置”可调整忽略开场事件秒数、燃烧瓶前置秒数、OCR 并行度（`auto` 或正整数）、独立的 Trim 并行度（`auto`、`1`、`2`）以及亮色描边文字预筛选。命令预览会显示 GUI 将要执行的 CLI 命令，便于复核配置。

运行时右侧面板分别显示 Scan、Dedupe、Trim、Merge 阶段、已完成工作数/总数、worker 数和完成/跳过状态，以及已保留和已跳过计数与耗时。点击“显示日志”可查看 CLI 原始运行日志，必要时可清空记录。点击停止按钮会终止 CLI 及其子进程；窗口关闭时若任务仍在运行，也会先询问是否停止。

任务完成后，“打开输出”会在资源管理器中打开本次输出目录。没有匹配事件时仍会写入扫描记录和 `summary.json`，界面会显示“扫描完成，没有匹配事件”。

Rust 壳仅通过参数数组、标准输出和最终 `summary.json` 与独立 CLI 通信，使用受信任的并排 `cli\pubg-highlight-trim.exe`；前端不能指定任意可执行文件。

### 命令行

将 CLI 指向 PUBG NVIDIA Highlight 文件夹（或一个/多个 mp4）：

```powershell
pubg-highlight-trim "C:\Users\you\AppData\Local\Temp\Highlights\PLAYERUNKNOWN'S BATTLEGROUNDS\淘汰"
```

也可以直接传入单个 mp4：

```powershell
pubg-highlight-trim "F:\Highlights\PLAYERUNKNOWN'S BATTLEGROUNDS\淘汰\PLAYERUNKNOWN'S BATTLEGROUNDS 2026.06.28 - 22.30.13.65.淘汰.DVR.mp4"
```

指定多个 mp4 时，工具会按 PUBG 文件名中的录制时间从早到晚处理，并默认合并裁剪结果：

```powershell
pubg-highlight-trim --files "F:\Highlights\video1.mp4" "F:\Highlights\video2.mp4" "F:\Highlights\video3.mp4" --merge ".\selected_merged.mp4"
```

常用选项：

```powershell
pubg-highlight-trim "." -o ".\trimmed" --merge ".\merged.mp4" -y
pubg-highlight-trim "F:\NVIDIA\TEMP\Highlights\PLAYERUNKNOWN'S BATTLEGROUNDS" -o ".\trimmed_auto" --merge ".\merged_auto.mp4" -y
pubg-highlight-trim "F:\NVIDIA\TEMP\Highlights\PLAYERUNKNOWN'S BATTLEGROUNDS" --game-lang en -o ".\trimmed_en" --merge ".\merged_en.mp4" -y
pubg-highlight-trim "." --scan-only --scan-mode full --coarse-step 2 -o ".\fullscan_2s" -y
pubg-highlight-trim "." --scan-mode fast
pubg-highlight-trim "." --profile

# OCR 自动选择并行度；Trim 的 auto 保守使用 1 个 worker
pubg-highlight-trim "." --scan-only -o ".\parallel_scan" -y
pubg-highlight-trim "." --scan-only --jobs 1  # 关闭 OCR 并行 worker
pubg-highlight-trim "." --jobs 1 --trim-jobs 2 --profile  # 独立启用两个 Trim worker
```

运行 `pubg-highlight-trim --help` 查看全部选项。

### 命令行参数

| 参数 | 默认值 | 说明 |
| --- | --- | --- |
| `input`（位置参数） | `.` | PUBG highlight 文件夹或单个 mp4 文件 |
| `--files FILE [FILE ...]` | 无 | 按 PUBG 文件名中的录制时间从早到晚处理这些 mp4 文件 |
| `--target` | `both` | 检测事件：`self-death`（敌人击倒/淘汰你）、`own-kill`（你击倒/淘汰敌人）、`both`（两者） |
| `--game-lang` | `auto` | 游戏语言配置；`auto` 从 NVIDIA Highlight 文件名自动识别。可选：`auto`、`zh-Hans`、`zh-Hant`、`en` |
| `-o`, `--output-dir` | 自动 | 本次运行的 output 根目录；逐段 Trim 视频写入其下的 `clips/` 子目录 |
| `--before` | `5.0` | 事件前保留秒数 |
| `--after` | `1.0` | 事件后保留秒数 |
| `--min-event-sec` | `2.0` | 跳过早于该秒数的事件；`0` 保留开场事件 |
| `--molotov-elim-before` | `10.0` | 燃烧瓶 / 火瓶淘汰事件前保留秒数；`0` 禁用 |
| `--recursive` | 关 | 同时搜索子目录 |
| `--dry-run` / `--scan-only` | 关 | 仅检测并写 CSV / 摘要，不生成 Trim 或 Merge 视频 |
| `--merge [MERGED_MP4]` | 文件夹默认开 | 生成合并 mp4；可指定输出路径 |
| `--no-merge` | 单文件默认关 | 不生成合并 mp4 |
| `--split-merge-by-target` | 关 | `--target both` 时按被击杀/击杀分别生成合并视频；默认关闭以保持混合输出兼容 |
| `-y`, `--overwrite` | 关 | 覆盖输出目录 / 合并文件，而非生成唯一名称 |
| `--verbose` | 关 | 打印启动配置和第三方 OCR 诊断；默认输出会抑制依赖库噪声 |
| `--profile` | 关 | 打印逐源耗时和最终阶段耗时；不加此参数也会写入 `profile.json` |
| `--jobs` | `auto` | 根据视频数量、CPU 和可用内存自动选择 1 或 2 个 OCR worker；用 `1` 关闭并行，也可手动覆盖 |
| `--trim-jobs` | `auto` | 独立的 Trim 并行度，仅接受 `auto`、`1`、`2`；auto 保守选 1，实际不超过计划片段数；dry-run/无片段为 0 |
| `--ffmpeg` / `--ffprobe` | 自动 | 显式指定 ffmpeg.exe / ffprobe.exe 路径 |

OCR 选项（进阶）：

| 参数 | 默认值 | 说明 |
| --- | --- | --- |
| `--scan-mode` | `auto` | `auto` 有候选时快速扫描，否则全量；`fast` / `full` 强制模式。多杀与对局结束源文件始终全量扫描 |
| `--candidate-csv` | 自动 | 用作扫描提示的旧 CSV，使用其 `EventSec`；省略时自动检测 `fullscan_*/candidate_events.csv` |
| `--no-auto-candidate-csv` | 关 | 禁用 candidate_events.csv 自动发现 |
| `--priority-window` | `31:43`, `45:53` | 优先扫描该 OCR 窗口；可重复，格式 `start:end` |
| `--scan-start` / `--scan-end` | `0.0` / 结尾 | 限制扫描时间范围 |
| `--coarse-step` | `4.0` | 粗扫描帧间隔（秒） |
| `--candidate-lookback` / `--candidate-lookahead` | `8.0` / `0.5` | 候选提示前后的时间窗口 |
| `--candidate-step` | `4.0` | 候选扫描帧间隔（秒） |
| `--adaptive-step` | `0.5` | 粗扫描命中后自适应采样间隔（秒） |
| `--adaptive-window` | `2.0` | 粗扫描命中后的前向自适应采样窗口（秒） |
| `--refine-before` / `--refine-after` | `6.0` / `0.4` | 粗命中后的精修窗口 |
| `--refine-step` | `0.5` | 精修扫描帧间隔（秒） |
| `--roi` | `0.30,0.66,0.70,0.75` | OCR 裁剪比例 `x1,y1,x2,y2` |
| `--ocr-width` | `768` | OCR ROI 降采样宽度；`0` 禁用 |
| `--ocr-provider` | `auto` | 推理模式：`auto`、`cpu`、`directml`、`directml-only`；大小写不敏感，`dml` 为 `directml` 别名 |
| `--ocr-device-id` | `0` | 非负推理设备序号（0–2147483647），用于 DirectML |
| `--no-brightness-gate` | 关 | 禁用粗扫描前的 OpenCV 亮色描边文字门控 |
| `--brightness-gate-mode` | `light` | 粗扫描前的门控严格度：`light` 仅跳过近乎全黑的帧，`full` 额外检查亮色描边字形和横向文字结构，`none` 完全不过滤 |

### 性能、阶段进度与验证

`--jobs` 只控制 OCR 扫描，`--trim-jobs` 独立控制 Trim 编码。两个 Trim worker 会争用 CPU/GPU、内存和磁盘，并不保证更快。有界窗口中运行或等待发布的片段总数不超过有效 worker 数；即使编码乱序完成，仍按计划顺序发布，保持命名、CSV 和 Merge 顺序稳定。资源紧张时可使用 `--trim-jobs 1`。

Trim 进度只统计已严格验证并发布的片段，不统计刚启动的编码；慢的前序任务可能暂时阻塞进度。dry-run 跳过 Trim/Merge，不把计划片段计作已编码。结构化 `PROGRESS` 的阶段、worker 和完成/跳过语义见[进度协议](docs/progress-protocol.md)。

跨视频去重仅比较 OCR 事件的 `target + event_key`，不重新取帧或验证画面。相同身份优先保留事件前后上下文更完整的片段，条件相同则按输入顺序保留；同源事件和空事件 key 不去重。不同录屏只要 OCR 身份相同也会被视为重复，不按对局隔离。

每次成功运行均写入 `profile.json`。`stages` 是阶段墙钟耗时；`aggregates` 与逐任务耗时是嵌套工作量，不能再加到阶段总耗时上，并行 Trim 的任务耗时之和可能超过阶段耗时。比较运行时应同时检查 `runtime.trim_jobs_requested`/`trim_workers`、源编码格式、缓存命中/未命中、当前与历史缓存 OCR provider，以及编码器能力探测/回退元数据。`--profile` 额外打印耗时记录，不开启 verbose 诊断。字段见 [Profile schema](docs/profile-schema.md)，重复测量见[固定源集基准方法](docs/trim-performance-benchmark.md)。

验证始终严格：`runtime.trim_validation` 为 `ffprobe_duration_strict`，兼容字段 `runtime.validation` 仍为 `strict`。每个 Trim 输出在原子发布前均执行 FFprobe 时长验证，Merge 输出也会验证。当前没有快速/关闭验证模式或绕过验证的 CLI 开关；分析成本时应单独看 `trim_output_probe_seconds_sum`，而不是移除验证。

### 输出行为

- 单文件输入默认不生成合并 mp4；文件夹或多文件输入默认合并所有片段。
- 所有逐段 Trim 视频都写入 `<output>/clips/`；默认 Merge 视频写入 `<output>/` 根目录，因此根目录不会混放逐段视频。
- 不带值使用 `--merge` 强制合并，Merge 默认写入 output 根目录；`--merge ".\merged.mp4"` 等显式路径继续遵循用户指定的位置，`--no-merge` 仅保留 `clips/` 中的逐段视频。
- 使用 `--split-merge-by-target --target both` 时每个源文件只扫描一次，但按被击杀/击杀分别合并；没有某类事件时不创建该类空视频。显式路径会追加 `_self-death` 或 `_own-kill` 后缀；中文 Windows 使用中文目标文件名，其他系统回退为英文名。
- 使用 `--dry-run` / `--scan-only` 时只生成扫描记录和摘要，不生成 Trim 或 Merge 视频，也不要求创建空的 `clips/` 目录。
- 使用 `--profile` 打印每个片段的 ffprobe、OCR 预测、视频帧定位 / 读取、裁剪编码及总耗时。同样的耗时列也会写入 `检测与裁剪记录.csv`。

OCR 配置逐项遵循 **CLI > 环境变量 > 内置默认值**。继续支持 `PUBG_OCR_PROVIDER` 和旧 `PUBG_OCR_DIRECTML_DEVICE_ID`；历史 provider 别名 `default`（CPU）及 `dml-only` 也保持兼容。CLI 或环境变量显式提供空串或纯空白 provider 值均属非法；需要默认值时应取消设置环境变量。未配置时使用 `auto`，通过真实建立 DirectML session 尝试 DirectX 12 推理，不可用则回退 CPU，不做硬件枚举。使用 `--ocr-provider cpu` 可恢复 v0.3.6 的 CPU 行为；集成显卡可能比 CPU 更慢。例如：

```powershell
.\pubg-highlight-trim.exe "<输入目录>" --ocr-provider directml --ocr-device-id 0 --verbose
```

`directml` 初始化或推理失败时回退 CPU，`directml-only` 则明确失败，不静默降级。`--verbose` 显示所选模式、请求/实际后端、设备序号、回退原因及缓存命中状态，全部命中缓存或运行失败时也会显示（初始化失败时 `actual=unknown`）。缓存按解析后的后端、设备序号与回退策略隔离（严格模式不会复用宽松模式的回退缓存）；相同配置重复运行仍命中缓存。非法 provider/设备值以退出码 2 失败，且不生成 summary。`summary.json` 的 provider 诊断新增 `provider_mode`（`auto`/`cpu`/`directml`/`directml-only`）；`provider_requested` 表示实际尝试的后端（auto 时为 `directml`）。回退原因区分 `directml.dll` 缺失、无可用 DX12 设备及其它初始化失败。已有 summary 字段名、类型及 CSV 列序保持不变。

GUI 和 CLI 使用相同的输出结构。`<output>/clips/` 保存逐段 Trim 视频；根目录中的合并视频（如启用合并）及旁边的 `.concat.txt` 是 FFmpeg 使用的拼接清单。按目标合并时，每个实际有片段的目标各有一个视频和清单。`检测与裁剪记录.csv` 记录每个源视频的检测状态、事件时间、裁剪范围和耗时；`candidate_events.csv` 保存候选事件提示，`profile.json` 保存阶段耗时（启用 `--profile` 时还会额外打印 `PROFILE` 行）。`summary.json` 保存本次运行的源文件数、保留/跳过数量、输出路径、扫描模式、语言、合并状态及 OCR/缓存信息；`merge_outputs` 列出所有目标输出，旧字段仍指向第一个输出，GUI 用它更新最终结果。

默认合并输出的目录结构如下（CSV、摘要和 concat 清单等非视频文件也会保留在根目录）：

```text
pubg_highlight_trim_output/
├── clips/
│   ├── 001_<target>_<source>.mp4
│   └── 002_<target>_<source>.mp4
├── pubg_highlight_trim_merged.mp4
├── pubg_highlight_trim_merged.concat.txt
├── 检测与裁剪记录.csv
└── summary.json
```

## OCR 后端与发布包

本分支由 Rust 完成整个应用：事件解析、扫描、媒体处理、剪辑规划、缓存和报告均直接调用 Rust OCR library。`ocr_engine.exe` 保留为独立 JSONL 推理诊断，不再是 Python 桥接。应用和官方测试/构建/发布命令不需要 CPython、PyInstaller、Python OpenCV/NumPy 或 Paddle 运行时。

包名称仍为 `pubg-highlight-trim-windows-x64.zip`。不再支持 Paddle 后端；旧 `PUBG_OCR_BACKEND` / `PUBG_OCR_ENGINE` 桥接设置不选择后端，`PUBG_OCR_MODEL_DIR` 可指定已有模型目录。历史 `paddle-*` 检测标签与报告字段仅为消费者兼容保留，不代表依赖 Paddle。

模型 hash 在启动时校验。`SUPPLY_CHAIN_MANIFEST.json` 记录实际产物、组件来源、许可及 hash。仅显式构建工具下载模型，应用运行时不下载。

**迁移状态：**本集成分支尚未完成 #63 最终验收，也不改写旧 release 的事实。完整私有样本/性能、无 Python 干净 Windows、最终 CLI/GUI 压缩包验收仍是门槛。项目自有 Python 源码、测试及脚本已在建立 Rust 替代后移除，能力与检查映射见[退休审计](docs/python-retirement-audit.md)；这不代替仍待完成的最终验收证据。

## 检测

由于 own-kill 与混合事件裁剪依赖 OCR，检测仅使用 OCR。默认 OCR 扫描 PUBG 事件文本所在的固定下方居中区域。粗扫描前先在相对位置 `x=0.26–0.74, y=0.635–0.725` 用 Rust 图像预处理做预筛：默认 `light` 模式仅在画面近乎全黑时跳过 OCR；`--brightness-gate-mode full` 可恢复更严格的亮色描边与横向文字结构检查（更省 OCR 调用，但可能误拦细单行事件文本）。该门控只用于粗扫描，事件精修仍使用完整 OCR；对局结束源文件会自动禁用门控，以免胜利结算遮罩压暗文字。所有区域均使用相对坐标，不依赖视频分辨率。若布局不同，可用 `--no-brightness-gate` 禁用门控，或用 `--roi x1,y1,x2,y2` 覆盖 OCR 裁剪区域。

当前支持的游戏语言：`zh-Hans`、`zh-Hant`、`en`。默认的 `--game-lang auto` 模式会从 NVIDIA Highlight 文件名自动识别 PUBG 游戏语言，支持同一文件夹中混用不同语言标签的文件。仅当需要强制指定某一种语言时，才使用 `--game-lang zh-Hans`、`--game-lang zh-Hant` 或 `--game-lang en`。

检测优先级：

1. 双向 OCR 文本：`你用...击倒/淘汰了...`、`...击倒/淘汰了你`、`你在安全区外倒地了`、`YOU KNOCKED OUT/KILLED ...`、`... KNOCKED/KILLS YOU ...`。
2. 若同一源视频先出现“你被击倒”、后出现“你被淘汰”，只保留击倒片段。
3. 跳过助攻文本（`助攻` / `协助`）与延迟淘汰文本（`你终于淘汰了...`）。
4. 不支持 `淘汰画面` / `击倒画面` 等回放视角文件。

### 扫描模式

扫描模式默认为 `auto`：CLI 会在输入附近查找最新的 `fullscan_*/candidate_events.csv`，若存在则按候选 / 优先窗口快速扫描；否则执行全量扫描以避免漏掉非常规事件时机。用 `--scan-mode full` 强制全量扫描，`--scan-mode fast` 仅扫描候选 / 优先窗口。多杀源文件（`Double kill`、`Multi kill`、`雙殺`、`多殺`）与对局结束源文件（`End of match`）始终使用全量 OCR 扫描，以免漏掉后续淘汰。

## ffmpeg

Release 构建会在 app bundle 内的 `vendor/ffmpeg` 下打包 `ffmpeg.exe` 与 `ffprobe.exe`。包内还包含 Rust OCR 引擎和模型，不提供 Paddle 兼容运行时。

源码开发期间，解析器按以下顺序查找：

1. `--ffmpeg` / `--ffprobe`
2. bundle 内的 `vendor/ffmpeg`
3. `PUBG_HIGHLIGHT_TRIM_FFMPEG_DIR`
4. PATH
5. 常见 Shutter Encoder 路径

## 常见故障排查

- **GUI 提示缺少 CLI**：确认开发布局中 `pubg-highlight-trim-ui.exe` 与完整 `cli` 文件夹并列；不要只复制 UI exe。必要时重新构建开发布局。
- **CLI 启动失败或运行失败**：展开 GUI 的“运行记录”查看 `ERR` 行和退出码。命令行用户可加 `--verbose` 重试；确认输入路径存在、输出目录可写，并避免让合并文件覆盖输入视频。
- **没有匹配事件**：确认输入是 NVIDIA Highlight 原始源文件而非“淘汰画面/击倒画面”回放文件；检查游戏语言和检测目标是否正确。可先用“仅扫描”或 CLI 的 `--scan-mode full` 生成 `candidate_events.csv`，必要时使用 `--no-brightness-gate` 或调整 `--roi`。
- **OCR 模型或 FFmpeg 不可用**：优先使用官方 release 包（其中已捆绑模型和 `vendor/ffmpeg`）。源码运行时按“ffmpeg”章节配置 `--ffmpeg`/`--ffprobe` 或 `PUBG_HIGHLIGHT_TRIM_FFMPEG_DIR`，并在构建/准备阶段显式下载并校验模型；应用运行时不会联网下载模型。
- **任务很慢或内存不足**：首次 OCR 会加载模型；可在专家设置将并行任务设为 `1`，或使用 `--jobs 1`。快速扫描依赖候选提示，需确保候选 CSV 未被误删。

## 开发

### 环境

需要 Windows 10/11 x64、Rust stable MSVC、Visual Studio C++ Build Tools / Windows SDK、Node.js 22 LTS（npm）、PowerShell，以及已安装的 Microsoft Edge WebView2 Runtime。联网用于还原依赖、显式准备 FFmpeg 和校验过的 OCR 模型。不使用 .NET SDK、Python 虚拟环境或 pip。

### 测试与检查

仓库根目录运行：

```powershell
cargo test --locked --manifest-path rust/Cargo.toml --workspace --features highlight-cli
cargo test --locked --manifest-path rust/Cargo.toml --features cli,directml
npm --prefix ui ci
npm --prefix ui run typecheck
npm --prefix ui run build
cargo check --locked --manifest-path ui/src-tauri/Cargo.toml
powershell -ExecutionPolicy Bypass -File scripts/test_ui_behavior.ps1 -SkipInstall
# 显式真实模型门槛，需要校验过的模型目录
$env:PUBG_OCR_MODEL_DIR="<模型目录>"
cargo test --locked --manifest-path rust/Cargo.toml --features cli --test end_to_end -- --ignored --nocapture
```

公共行为测试入口运行前端和 Rust 壳契约，替代旧桌面框架专属自动化。它们不代表可见 UI → 壳 → 真实 CLI、固定真实视频 summary/CSV/媒体语义、交互关闭/取消或最终干净主机分发验收已通过；这些需要单独的 Windows 证据，不能用分层测试冒充桌面端到端测试。已完成检查和剩余缺口见 [Tauri UI 验证记录](docs/tauri-ui-verification.md)。既有 CLI 的模型、媒体、provider/回退、错误、取消、供应链和发布门槛保持不变。

### 构建并运行 Windows 开发应用

```powershell
powershell -ExecutionPolicy Bypass -File scripts/build_windows.ps1
powershell -ExecutionPolicy Bypass -File scripts/build_ui_windows.ps1 -SkipCliBuild
.\dist\pubg-highlight-trim-ui\pubg-highlight-trim-ui.exe
```

UI 脚本还原锁定的 npm 依赖，运行 Tauri `build --no-bundle`；`-Configuration Debug` 可构建调试版本。省略 `-SkipCliBuild` 会先构建 CLI。CLI 构建会清理 `build` / `dist`，因此须在 UI 布局构建之前执行。UI 脚本将完整 CLI bundle 复制到 `dist\pubg-highlight-trim-ui\cli`，请勿只移动 UI exe。

此产物是开发运行布局，**不是**自包含运行时包、安装器或发布 ZIP；主机须已有 WebView2 及所需原生运行时。安装包请使用[独立构建入口](docs/windows-installers.md)。运行时来源、ACL、普通权限、升级/卸载和干净主机结果必须另行留证，不能由开发构建成功推断。

先运行 `build_ui_windows.ps1 -SkipCliBuild` 准备受信任的并排布局后，也可使用热重载：

```powershell
cd ui
npm ci
npm run tauri -- dev
```

模型下载/hash/字典、推理 smoke、manifest、包审计和体积报告仍由 Rust `xtask` 提供；用 `cargo run --manifest-path rust/Cargo.toml --bin xtask -- --help` 查看。应用运行时不会下载模型。

### UI 数据目录

UI 默认使用运行中 exe 旁的 `data`（开发布局为 `dist\pubg-highlight-trim-ui\data`）。启动前可设置 `PUBG_HIGHLIGHT_TRIM_DATA_DIR` 为**绝对**可写路径，允许指定安装目录内的专用数据子目录。其下包含 `settings.json`、有容量上限的 `run.log`（1 MiB / 2,000 行）、浏览器用户数据 `webview` 和可清理 `cache`，不与视频输出混用。目录必须可写；失败应明确报错，不静默回退 AppData。不要将未配置的开发构建放在只读安装目录。清理浏览器/缓存文件前请退出应用，需要保留偏好时不要删除设置文件。视频输出和 OCR 扫描缓存继续遵循 CLI 靠近输入源/输出的原策略，不属于 UI 浏览器缓存。界面的清理缓存操作清理 `cache` 及全部 WebView 浏览数据（包括 cookies、localStorage）；不删除 `settings.json`、视频输出或 CLI 的 OCR 缓存。例如：`$env:PUBG_HIGHLIGHT_TRIM_DATA_DIR="D:\PUBG-Trim\data"`。

GitHub Actions 在 Windows 检查前端和壳，独立 CLI 发布继续执行完整 assurance。双 GUI 安装包构建要求显式锁定的可信 Fixed runtime 输入，生成可审查的草稿成品及校验和，不代表已通过干净主机验收，详见[发布验证状态](docs/tauri-release-verification.md)。安装版 UI 数据固定在 `<安装目录>/data`，不接受开发用 `PUBG_HIGHLIGHT_TRIM_DATA_DIR` 重定位；升级/切换、卸载及故障排查见[安装说明](docs/windows-installers.md)。

## 许可证

本项目以 `GPL-3.0-or-later` 许可。

发布包中捆绑 FFmpeg/FFprobe 与 Rust OCR 引擎/模型资源，不提供 Paddle 兼容运行时。第三方许可详情与源链接见 `THIRD_PARTY_NOTICES.md`。

每个 Windows 发布包还包含 `third_party/rust/INVENTORY.json`，由锁定的 Cargo metadata 通过 `cargo xtask licenses <bundle>` 生成，记录依赖的名称、版本、SPDX 许可、来源及 shipped/build-only 范围。对应的 `LICENSE`、`NOTICE`、`COPYING`、ONNX Runtime、PP-OCR/模型、CRT 等固定许可文本复制在 `third_party/` 下；`SUPPLY_CHAIN_MANIFEST.json` 会对 inventory 与全部许可文本逐一计算 hash。缺少许可文本或 hash 不匹配都会阻断发布。
