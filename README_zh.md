# pubg-highlight-trim

[English](README.md) | 简体中文

Windows 平台命令行工具，基于 OCR 事件对 PUBG 的 NVIDIA Highlight（精彩时刻）片段进行裁剪。

仓库同时提供原生 Windows 桌面 UI。UI 使用 WPF，不包含浏览器运行时；它把 CLI 当作独立进程运行，不引用 Python 源码。

默认行为同时检测“你击倒/淘汰敌人”和“敌人击倒/淘汰你”两类事件，保留事件前 5 秒与后 1 秒，跳过前 2 秒内的事件；对燃烧瓶 / 火瓶淘汰事件保留前 10 秒。

## 安装

无需安装 Python。

1. 从 [最新 release](https://github.com/SawXu/pubg-highlight-trim/releases/latest) 下载 `pubg-highlight-trim-windows-x64.zip`。
2. 将 zip 解压到任意目录。
3. 压缩包内已包含 `pubg-highlight-trim.exe`、打包的 `ffmpeg.exe` / `ffprobe.exe`，以及 OCR 检测所需的 PaddleOCR 模型。

需要图形界面时，下载 `pubg-highlight-trim-ui-windows-x64.zip`。解压后运行 `pubg-highlight-trim-ui.exe`；同包的 `cli` 目录包含完整 CLI 和 OCR 运行时，必须与 UI exe 保持并列（即 `cli\\pubg-highlight-trim.exe`）。请勿将 UI exe 移出该目录，或在缺少 `cli` 的情况下单独复制它。

GUI 发布包是 CLI 发布包之上的桌面外壳：WPF UI 通过参数、标准输出和最终的 `summary.json` 启动并监视 CLI，实际 OCR、FFmpeg 和视频处理仍由 `cli` 目录中的 CLI 完成。GUI 包已经自包含 .NET、Python、FFmpeg/FFprobe 和 OCR 模型，用户无需另行安装运行时。

在解压目录下直接运行：

```powershell
.\pubg-highlight-trim.exe "C:\Users\you\AppData\Local\Temp\Highlights\PLAYERUNKNOWN'S BATTLEGROUNDS\淘汰"
```

## 使用

### 图形界面

解压 `pubg-highlight-trim-ui-windows-x64.zip` 后，双击 `pubg-highlight-trim-ui.exe` 启动。窗口顶部会先检查并显示 CLI 版本；若提示“未找到 CLI”，请确认 `cli` 文件夹与 UI exe 位于同一目录，且其中存在 `cli\\pubg-highlight-trim.exe`。

主要操作如下：

1. 在“输入”中选择一个 PUBG Highlight 文件夹，或通过文件选择器多选 MP4。多选输入只能包含 MP4；勾选“递归扫描子文件夹”后才会搜索所选文件夹的子目录。
2. 可选填“输出目录”；留空时由 CLI 根据输入自动创建本次运行的目录。
3. 在“检测目标”中选择“全部”“自己倒地”或“我的击杀”，并选择游戏语言（自动识别、简体中文、繁体中文、English）。
4. 设置“扫描模式”（自动、快速、完整）和“剪辑范围”（事件前/后秒数）。
5. 常用复选项包括“合并输出”“仅扫描”和“覆盖输出”。“仅扫描”只检测并写入记录，不生成视频；“覆盖输出”允许复用同名输出目录或合并文件。
6. 展开“专家设置”可调整忽略开场事件秒数、燃烧瓶前置秒数、并行任务（`auto` 或正整数）以及亮色描边文字预筛选。命令预览会显示 GUI 将要执行的 CLI 命令，便于复核配置。

运行时右侧面板显示当前阶段（OCR 扫描或整理输出）、已完成文件数/总数、已保留和已跳过计数以及耗时。点击“显示日志”可查看 CLI 原始运行日志，必要时可清空记录。点击停止按钮会终止 CLI 及其子进程；窗口关闭时若任务仍在运行，也会先询问是否停止。

任务完成后，“打开输出”会在资源管理器中打开本次输出目录；当生成了合并视频时，“播放合并视频”会调用系统默认播放器打开该文件。没有匹配事件时仍会写入扫描记录和 `summary.json`，界面会显示“扫描完成，没有匹配事件”。

UI 仅通过 CLI 参数、标准输出和最终 `summary.json` 与后端通信。可通过 `PUBG_HIGHLIGHT_TRIM_CLI` 环境变量覆盖 CLI 路径，便于开发或测试其他 CLI 版本。

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

# 自动选择并行度，一般无需手动设置
pubg-highlight-trim "." --scan-only -o ".\parallel_scan" -y
pubg-highlight-trim "." --scan-only --jobs 1  # 关闭并行 worker
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
| `-y`, `--overwrite` | 关 | 覆盖输出目录 / 合并文件，而非生成唯一名称 |
| `--verbose` | 关 | 打印启动配置和第三方 OCR 诊断；默认输出会抑制依赖库噪声 |
| `--profile` | 关 | 打印每个片段的耗时明细（ffprobe、OCR、读帧、裁剪） |
| `--jobs` | `auto` | 根据视频数量、CPU 和可用内存自动选择 1 或 2 个 OCR worker；用 `1` 关闭并行，也可手动覆盖 |
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
| `--no-brightness-gate` | 关 | 禁用粗扫描前的 OpenCV 亮色描边文字门控 |

### 输出行为

- 单文件输入默认不生成合并 mp4；文件夹或多文件输入默认合并所有片段。
- 所有逐段 Trim 视频都写入 `<output>/clips/`；默认 Merge 视频写入 `<output>/` 根目录，因此根目录不会混放逐段视频。
- 不带值使用 `--merge` 强制合并，Merge 默认写入 output 根目录；`--merge ".\merged.mp4"` 等显式路径继续遵循用户指定的位置，`--no-merge` 仅保留 `clips/` 中的逐段视频。
- 使用 `--dry-run` / `--scan-only` 时只生成扫描记录和摘要，不生成 Trim 或 Merge 视频，也不要求创建空的 `clips/` 目录。
- 使用 `--profile` 打印每个片段的 ffprobe、OCR 预测、视频帧定位 / 读取、裁剪编码及总耗时。同样的耗时列也会写入 `检测与裁剪记录.csv`。

GUI 和 CLI 使用相同的输出结构。`<output>/clips/` 保存逐段 Trim 视频；根目录中的 `pubg_highlight_trim_merged.mp4`（如启用合并）是按时间顺序拼接的成片，旁边的 `.concat.txt` 是 FFmpeg 使用的拼接清单。`检测与裁剪记录.csv` 记录每个源视频的检测状态、事件时间、裁剪范围和耗时；`candidate_events.csv` 保存候选事件提示，`profile.json` 保存阶段耗时（启用 `--profile` 时还会额外打印 `PROFILE` 行）。`summary.json` 保存本次运行的源文件数、保留/跳过数量、输出路径、扫描模式、语言、合并状态及 OCR/缓存信息，GUI 用它更新最终结果。

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

## 检测

由于 own-kill 与混合事件裁剪依赖 OCR，检测仅使用 OCR。默认 OCR 扫描 PUBG 事件文本所在的固定下方居中区域。粗扫描先在相对位置 `x=0.26–0.74, y=0.635–0.725` 使用 OpenCV 检查亮色、深色描边和水平文字结构；未发现疑似事件文字时跳过 PaddleOCR。该门控只用于粗扫描，事件精修仍使用完整 OCR；对局结束源文件会自动禁用门控，以免胜利结算遮罩压暗文字。所有区域均使用相对坐标，不依赖视频分辨率。若布局不同，可用 `--no-brightness-gate` 禁用门控，或用 `--roi x1,y1,x2,y2` 覆盖 OCR 裁剪区域。

当前支持的游戏语言：`zh-Hans`、`zh-Hant`、`en`。默认的 `--game-lang auto` 模式会从 NVIDIA Highlight 文件名自动识别 PUBG 游戏语言，支持同一文件夹中混用不同语言标签的文件。仅当需要强制指定某一种语言时，才使用 `--game-lang zh-Hans`、`--game-lang zh-Hant` 或 `--game-lang en`。

检测优先级：

1. 双向 OCR 文本：`你用...击倒/淘汰了...`、`...击倒/淘汰了你`、`你在安全区外倒地了`、`YOU KNOCKED OUT/KILLED ...`、`... KNOCKED/KILLS YOU ...`。
2. 若同一源视频先出现“你被击倒”、后出现“你被淘汰”，只保留击倒片段。
3. 跳过助攻文本（`助攻` / `协助`）与延迟淘汰文本（`你终于淘汰了...`）。
4. 不支持 `淘汰画面` / `击倒画面` 等回放视角文件。

### 扫描模式

扫描模式默认为 `auto`：CLI 会在输入附近查找最新的 `fullscan_*/candidate_events.csv`，若存在则按候选 / 优先窗口快速扫描；否则执行全量扫描以避免漏掉非常规事件时机。用 `--scan-mode full` 强制全量扫描，`--scan-mode fast` 仅扫描候选 / 优先窗口。多杀源文件（`Double kill`、`Multi kill`、`雙殺`、`多殺`）与对局结束源文件（`End of match`）始终使用全量 OCR 扫描，以免漏掉后续淘汰。

## ffmpeg

Release 构建会在 app bundle 内的 `vendor/ffmpeg` 下打包 `ffmpeg.exe` 与 `ffprobe.exe`。

源码开发期间，解析器按以下顺序查找：

1. `--ffmpeg` / `--ffprobe`
2. bundle 内的 `vendor/ffmpeg`
3. `PUBG_HIGHLIGHT_TRIM_FFMPEG_DIR`
4. PATH
5. 常见 Shutter Encoder 路径

## 常见故障排查

- **GUI 提示缺少 CLI**：确认使用的是完整的 `pubg-highlight-trim-ui-windows-x64.zip`，并保持 `pubg-highlight-trim-ui.exe` 与 `cli` 文件夹并列；不要只复制 UI exe。开发或测试时也可设置 `PUBG_HIGHLIGHT_TRIM_CLI` 指向另一份 CLI 可执行文件。
- **CLI 启动失败或运行失败**：展开 GUI 的“运行记录”查看 `ERR` 行和退出码。命令行用户可加 `--verbose` 重试；确认输入路径存在、输出目录可写，并避免让合并文件覆盖输入视频。
- **没有匹配事件**：确认输入是 NVIDIA Highlight 原始源文件而非“淘汰画面/击倒画面”回放文件；检查游戏语言和检测目标是否正确。可先用“仅扫描”或 CLI 的 `--scan-mode full` 生成 `candidate_events.csv`，必要时使用 `--no-brightness-gate` 或调整 `--roi`。
- **OCR 模型或 FFmpeg 不可用**：优先使用官方 release 包（其中已捆绑模型和 `vendor/ffmpeg`）。源码运行时按“ffmpeg”章节配置 `--ffmpeg`/`--ffprobe` 或 `PUBG_HIGHLIGHT_TRIM_FFMPEG_DIR`，并确保首次运行可联网下载 OCR 模型。
- **任务很慢或内存不足**：首次 OCR 会加载模型；可在专家设置将并行任务设为 `1`，或使用 `--jobs 1`。快速扫描依赖候选提示，需确保候选 CSV 未被误删。

## 开发

### 运行测试

```powershell
$env:PYTHONPATH="src"
python -m unittest discover -s tests -v
```

### 本地构建 Windows release

#### 构建环境

本地构建需要 Windows x64、可联网的 PowerShell，以及以下工具：

- 推荐使用 64 位 Python 3.12.x。`pyproject.toml` 固定了 `paddleocr==3.7.0` 和 `paddlepaddle==3.2.2`；目前 `paddlepaddle==3.2.2` 没有适用于 Windows 的 Python 3.14 wheel，因此 Python 3.14 无法安装本项目的 OCR 构建依赖。
- 构建 UI 需要 .NET 10 SDK（项目目标框架为 `net10.0-windows`）。
- 构建脚本会用 `mt.exe` 检查最终 UI 可执行文件的 DPI manifest。请安装 Windows 10/11 SDK，并将包含 `mt.exe` 的目录加入 `PATH`；该目录通常类似 `C:\Program Files (x86)\Windows Kits\10\bin\<版本>\x64`。也可以从 Visual Studio Developer PowerShell（已配置 Windows SDK 的环境）运行构建。可先用 `Get-Command mt.exe` 验证。

不要使用系统 Python 3.14。先在仓库根目录创建并启用 Python 3.12 虚拟环境：

```powershell
py -3.12 -m venv .venv
.\.venv\Scripts\Activate.ps1
python --version  # 应显示 Python 3.12.x
python -m pip install --upgrade pip
```

如果 PowerShell 禁止执行激活脚本，也可以直接使用 `.\.venv\Scripts\python.exe` 执行下面所有 Python 命令。

#### 安装 Python 构建依赖

在已启用 `.venv` 的终端中安装 `pyproject.toml` 的 `[ocr,build]` 可选依赖：

```powershell
python -m pip install -e ".[ocr,build]"
```

该 extra 会安装以下构建所需的包（以及它们的传递依赖）：

- `opencv-contrib-python>=4.9`
- `paddleocr==3.7.0`
- `paddlepaddle==3.2.2`
- `pyinstaller>=6.10`

#### 构建 CLI bundle

推荐先单独构建 CLI。`build_windows.ps1` 在缺少 `vendor\ffmpeg\ffmpeg.exe` 或 `ffprobe.exe` 时自动下载 FFmpeg，并运行 `prefetch_ocr_models.py` 将 PaddleOCR 模型缓存到 `vendor\paddlex_cache`；模型和 FFmpeg 都会被复制进发布 bundle。

```powershell
# 可选：提前下载 FFmpeg；省略时 build_windows.ps1 会自动下载
powershell -ExecutionPolicy Bypass -File .\scripts\download_ffmpeg.ps1

# 可选：提前下载/校验 PaddleOCR 模型；构建脚本也会自动执行
python .\scripts\prefetch_ocr_models.py --cache-dir vendor\paddlex_cache

# 依赖已由上一步安装，因此跳过脚本内的重复安装
powershell -ExecutionPolicy Bypass -File .\scripts\build_windows.ps1 `
    -Python .\.venv\Scripts\python.exe -SkipInstall
```

CLI 构建会清理旧的 `build` / `dist` 目录，运行 PyInstaller 并生成：

- `dist\pubg-highlight-trim\pubg-highlight-trim.exe`：可直接运行的 CLI bundle。
- `dist\pubg-highlight-trim-windows-x64.zip`：包含 CLI、FFmpeg/FFprobe、PaddleOCR 模型和许可文件的发布包。该 zip 的用户无需安装 Python。

#### 构建 GUI EXE

CLI bundle 存在后，运行下面的命令构建自包含 WPF GUI。`-SkipCliBuild` 表示复用已有的 `dist\pubg-highlight-trim`；如果该目录中没有 CLI exe，脚本会报错。省略此参数则会先重新构建 CLI。

```powershell
# 复用刚刚生成的 CLI bundle（推荐）
powershell -ExecutionPolicy Bypass -File .\scripts\build_ui_windows.ps1 -SkipCliBuild

# 或者一条命令同时重新构建 CLI 和 GUI
powershell -ExecutionPolicy Bypass -File .\scripts\build_ui_windows.ps1
```

GUI 构建会执行 `dotnet publish`、检查 `mt.exe` 生成的 DPI manifest，并把 CLI bundle 复制到 UI 包的 `cli` 子目录。产物为：

- `dist\pubg-highlight-trim-ui\pubg-highlight-trim-ui.exe`：自包含 GUI EXE。
- `dist\pubg-highlight-trim-ui-windows-x64.zip`：GUI、并排的 `cli` bundle、FFmpeg/FFprobe、PaddleOCR 模型和文档的完整发布包。用户无需安装 .NET、Python 或其他运行时。

UI 测试单独运行：

```powershell
dotnet test .\ui\PubgHighlightTrim.Ui.Tests\PubgHighlightTrim.Ui.Tests.csproj --configuration Release
```

UI release 是自包含的原生 WPF 单文件应用，发布包在 `cli` 子目录并排携带完整 CLI bundle；用户无需安装 .NET 或 Python。

GitHub Actions 会在推送 `v0.1.0` 等标签或手动 `workflow_dispatch` 时构建 CLI 与 UI 两种 Windows release zip。

## 许可证

本项目以 `GPL-3.0-or-later` 许可。

发布包中捆绑 FFmpeg/FFprobe 与 PaddleOCR 运行时资源，第三方许可详情与源链接见 `THIRD_PARTY_NOTICES.md`。
