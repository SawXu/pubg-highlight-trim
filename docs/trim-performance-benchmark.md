# Trim 性能基准：固定源集、固定配置、重复测量（#91）

## 范围与证据状态

本方法区分 **OCR 波动、Trim 吞吐、严格验证成本**，不把一次端到端加速归因于并行编码。先读 [Profile schema](profile-schema.md)、[进度协议](progress-protocol.md) 和 [CLI acceptance 性能调查](cli-acceptance-performance.md)。后者测的是验收测试/debug 与 release 的差异，不是当前 Trim 私有语料吞吐基线；其中数值不能移植到本表。

**实测状态：待补充。** 本文不声称已在固定私有 AV1/H.264 源集上完成重复基准，也不提供虚构秒数或加速比。需要有权访问语料、固定 release CLI、模型、FFmpeg/FFprobe 的 Windows 主机运行下述流程。缺少其中任何一项，应记录缺失项并标记“环境限制/未测”，不能用合成测试或 CI 时间代替私有语料结论。

## 1. 固定实验条件

- 使用同一个 release CLI（二进制 hash、commit、feature/build flags），不要把编译、下载、打包时间放入运行耗时。跨版本比较保留各自 hash，严格对齐配置。
- 固定只读输入清单及每个文件 SHA-256、大小、时长、codec、分辨率、帧率；保留原始 Highlight 文件名和事件标签，不在不同重复间重新转码。公开报告使用匿名源 ID，原始路径/文件名、hash 对照和产物仅保存在获授权环境。
- 主矩阵使用固定 AV1/H.264 混合源集，记录两类数量与媒体时长。若要分析 codec 差异，另建固定 AV1-only、H.264-only 子集并分别完整重复；不能只用混合集总耗时证明某一 codec 更快。不同编码的源文件也可能有不同 GOP、时长、分辨率，不能把这些差异全归因于 codec。
- 固定 `--jobs`、OCR provider/device/threads、语言、target、扫描模式、采样/ROI/gate、裁剪范围、dedupe、Merge 和候选配置。下面显式使用 CPU、OCR jobs=1、full scan、禁用候选自动发现，避免 auto provider 和候选文件漂移。测试 DirectML 时另开矩阵，不混合统计。
- 主矩阵开启 Merge 以验证输出顺序，单独报告 `stages.merge`；若测 no-merge，作为另一组固定配置。禁止 `--move-originals`，不要用覆盖已有输出充当新样本。
- 电源模式、CPU/GPU/驱动、内存、源/输出盘及空闲空间保持一致；关闭竞争任务，记录温度/降频和 CPU/GPU/磁盘资源峰值。每次 CLI 串行运行，不能同时启动多个基准进程。

## 2. 实验矩阵

每个源集、每个缓存条件，对 `--trim-jobs auto`、`1`、`2` 各至少 **3 次有效重复**（建议 5 次）。auto 保守解析为 1，是兼容性对照，不是动态 GPU 调优。有效 worker 为 `min(解析后请求, 计划片段数)`；dry-run/无片段为 0。必须至少有两个可裁剪片段，才能评价双 worker 吞吐。

| 条件 | 缓存准备 | 主要用途 |
| --- | --- | --- |
| cold-scan | 每次使用新的空 OCR 缓存目录 | 观察真实当前 OCR 波动与端到端阶段分布 |
| cache-hit | 用相同固定配置预热一次，再复用该缓存 | 隔离大部分 OCR 工作，比较 Trim、Merge 与验证成本 |

“cold”仅指应用 OCR 持久缓存未命中，**不表示**操作系统文件页缓存、模型内存、GPU/驱动或磁盘缓存为冷。记录实际 `cache.hits/misses`；不要仅凭目录名判定条件。若只部分命中，标记 partial，不与全命中样本合并。预热不进入重复统计。轮换/反转配置顺序，避免把所有 `1` 放在冷机、所有 `2` 放在热机。

## 3. 可复制 PowerShell 采集

在仓库根目录的 Windows PowerShell 5.1 或 PowerShell 7 执行。先修改前四个路径；源集只放本次固定的 MP4（此例不递归）。`$Root` 每次创建唯一目录，不删除现有缓存或源文件。此处的 CPU 是固定基线选择，不代表推荐所有硬件使用 CPU。

```powershell
$ErrorActionPreference = 'Stop'
$Cli = (Resolve-Path '.\dist\pubg-highlight-trim\pubg-highlight-trim.exe').Path
$Source = (Resolve-Path 'F:\benchmark\fixed-mixed-sources').Path
$Ffmpeg = (Resolve-Path '.\vendor\ffmpeg\ffmpeg.exe').Path
$Ffprobe = (Resolve-Path '.\vendor\ffmpeg\ffprobe.exe').Path
$Root = Join-Path (Get-Location) ('benchmark-results-' + (Get-Date -Format 'yyyyMMdd-HHmmss-fff'))
New-Item -ItemType Directory -Path $Root | Out-Null
$Files = @(Get-ChildItem -LiteralPath $Source -File -Filter '*.mp4' | Sort-Object Name)
if ($Files.Count -eq 0) { throw 'Fixed source set is empty' }

# 此清单含私有路径，只留本地；hash 与探测不计入 CLI 样本耗时。
$Manifest = foreach ($File in $Files) {
    $Probe = & $Ffprobe -v error -select_streams v:0 `
        -show_entries 'stream=codec_name,width,height,avg_frame_rate:format=duration' `
        -of json $File.FullName
    if ($LASTEXITCODE -ne 0) { throw "Source probe failed: $($File.Name)" }
    [pscustomobject]@{
        Path = $File.FullName; Bytes = $File.Length
        Sha256 = (Get-FileHash -LiteralPath $File.FullName -Algorithm SHA256).Hash
        Media = (($Probe -join "`n") | ConvertFrom-Json)
    }
}
$Manifest | ConvertTo-Json -Depth 10 | Set-Content -Encoding UTF8 (Join-Path $Root 'sources.json')
[pscustomobject]@{
    Commit = (git rev-parse HEAD); CliSha256 = (Get-FileHash $Cli -Algorithm SHA256).Hash
    FfmpegSha256 = (Get-FileHash $Ffmpeg -Algorithm SHA256).Hash
    FfprobeSha256 = (Get-FileHash $Ffprobe -Algorithm SHA256).Hash
    CliVersion = (& $Cli --version); FfmpegVersion = (& $Ffmpeg -version | Select-Object -First 1)
    OS = (Get-CimInstance Win32_OperatingSystem).Caption
    CPU = (Get-CimInstance Win32_Processor | Select-Object Name, NumberOfLogicalProcessors)
    GPU = (Get-CimInstance Win32_VideoController | Select-Object Name, DriverVersion)
    MemoryBytes = (Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory
    OcrEnvironment = @(Get-ChildItem Env: | Where-Object Name -Match '^PUBG_')
} | ConvertTo-Json -Depth 8 | Set-Content -Encoding UTF8 (Join-Path $Root 'environment.json')

# 指定 --files 冻结清单；明确扫描/裁剪配置。不要加入 --move-originals。
$Common = @('--files') + @($Files.FullName) + @(
    '--ffmpeg', $Ffmpeg, '--ffprobe', $Ffprobe,
    '--jobs', '1', '--ocr-provider', 'cpu', '--ocr-device-id', '0',
    '--scan-mode', 'full', '--no-auto-candidate-csv', '--game-lang', 'auto',
    '--target', 'both', '--before', '5', '--after', '1',
    '--min-event-sec', '2', '--molotov-elim-before', '10',
    '--brightness-gate-mode', 'light', '--merge', '--profile'
)
$Common | ConvertTo-Json | Set-Content -Encoding UTF8 (Join-Path $Root 'common-args.json')
function Invoke-Sample([string]$Name, [string]$TrimJobs, [string]$Cache) {
    $Run = Join-Path $Root $Name
    New-Item -ItemType Directory -Path $Run | Out-Null
    $Out = Join-Path $Run 'output'
    $Arguments = $Common + @('--trim-jobs', $TrimJobs, '--cache-dir', $Cache, '-o', $Out)
    $Arguments | ConvertTo-Json | Set-Content -Encoding UTF8 (Join-Path $Run 'args.json')
    $Clock = [Diagnostics.Stopwatch]::StartNew()
    & $Cli @Arguments 1> (Join-Path $Run 'stdout.log') 2> (Join-Path $Run 'stderr.log')
    $Code = $LASTEXITCODE
    $Clock.Stop()
    [pscustomobject]@{ Name = $Name; ExitCode = $Code; ProcessWallSec = $Clock.Elapsed.TotalSeconds } |
        ConvertTo-Json | Set-Content -Encoding UTF8 (Join-Path $Run 'process.json')
    if ($Code -ne 0) { throw "Sample $Name exited $Code; retain logs, do not count as throughput sample" }
    $Profile = Get-Content -Raw -LiteralPath (Join-Path $Out 'profile.json') | ConvertFrom-Json
    if ($Profile.counts.outputs -lt 2) { throw 'Need at least two outputs to compare two Trim workers' }
}
$WarmCache = Join-Path $Root 'warm-cache'
Invoke-Sample 'warmup-excluded' '1' $WarmCache
foreach ($Repeat in 1..3) {
    $Order = if ($Repeat % 2 -eq 1) { @('auto', '1', '2') } else { @('2', '1', 'auto') }
    foreach ($Jobs in $Order) {
        $Name = "cold-scan-r$Repeat-j$Jobs"
        Invoke-Sample $Name $Jobs (Join-Path $Root "$Name-cache")
        Invoke-Sample "cache-hit-r$Repeat-j$Jobs" $Jobs $WarmCache
    }
}
Write-Output "Artifacts: $Root"
```

每次保留完整 `profile.json`、`summary.json`、CSV、stdout/stderr、参数和外部进程墙钟；`run_id` 关联记录。还需另存模型 manifest/hash、构建 flags、磁盘/电源信息和人工资源观察。不要直接公开上述含路径/环境变量的原始产物。

## 4. 元数据对照与计算

| 问题 | 必须对照的证据 | 解释边界 |
| --- | --- | --- |
| OCR 是否真的执行、为何波动 | `stages.scan`，`cache.enabled/hits/misses`，`runtime.ocr_worker_capacity/ocr_workers_active/ocr_threads_per_worker`，`telemetry.current`，`ocr_providers_current` | 当前完整扫描统计才描述本轮 OCR；cache-hit 的 `cached_historical` 是截至最后事件的历史快照，不能充作当前推理耗时 |
| codec/配置是否一致 | `runtime.source_codec_counts`、`runtime.source_scan_configs`、`runtime.scan_mode`、`runtime.full_scan`、`runtime.candidate_input_present`、`runtime.brightness_gate_mode`，固定源 manifest | codec `unknown` 要披露；混合比例与配置漂移使样本不可直接比较 |
| Trim 并行是否生效 | `runtime.trim_jobs_requested/trim_workers`，`counts.outputs`，`trim_tasks.sequence`，`stages.trim` | worker 是容量，不是同时活跃编码器的采样数；有界窗口最多保留 workers 个运行或待发布任务，慢前序片段可能阻塞发布 |
| 编码器是否相同 | `runtime.ffmpeg_version`、FFmpeg hash、`runtime.trim_encoder`、summary `encoders` | 能力探测、NVENC 不可用或生产回退必须单列；两组不同回退不能只归因于 worker 数 |
| 验证成本 | `runtime.trim_validation`，`aggregates.trim_output_probe_seconds_sum`，逐任务 `output_probe_seconds` | 仅严格 Trim 输出探测工作；不是所有源探测/能力探测/Merge 验证，也不是可从墙钟直接减去的串行阶段 |
| 输出契约是否保持 | CSV 事件/范围/顺序、Trim 数量、concat 清单顺序、summary `merge_outputs`、独立 FFprobe 时长检查 | 不要求不同并行编码输出字节 hash 相同；失败/取消不算成功吞吐，不得隐藏后重跑替代 |

报告每组每次原始值以及中位数、最小/最大值（建议另报 IQR）。不要用不同缓存状态、provider、codec 集或不同输出片段数量的样本求一个加速比。

- **Trim 吞吐** = 成功发布片段数 / `stages.trim`（clips/s）；也可报告固定输出媒体秒数 / `stages.trim`（media-s/s）。`summary.merge_duration_sec` 是媒体时长，不是 Merge 耗时，不能当作阶段性能。
- **Trim 加速比** = 同条件 `trim-jobs=1` 的 Trim 墙钟中位数 / `trim-jobs=2` 中位数。报告两组资源峰值，不能承诺 2×。auto 与 1 应单列用于核对策略。
- **OCR 波动**：按缓存条件单列 `stages.scan`、当前 predict/frame/probe 工作量与 provider；检查端到端差异是否主要来自 scan。不要只比较 `stages.total`。
- **验证工作量占比** = `trim_output_probe_seconds_sum / trim_task_total_seconds_sum`（分母非零时），同时报告绝对秒数和逐任务分布。分子与分母为嵌套 task work；**不能**解释为验证占 Trim 墙钟比例。编码、验证在不同任务间可重叠，任务和可能大于墙钟。
- 编码能力探测每次真实 Trim 前最多一次，记录 `runtime.trim_encoder.probe_seconds`。它在 `stages.trim` 内，不在逐任务 encode/total 或 CSV `TrimSeconds` 内；短任务集对此更敏感。
- `stages.total` 的冻结点排除最终 JSON 序列化/写入/stdout；外部 `ProcessWallSec` 还包含这些尾部动作、进程启动与重定向开销，两者应并列而非强行相等。各 `stages` 可相加，不能把 task/aggregate 工作再次加到 total。

## 5. 验证策略与待填结果

目前仅有严格验证：`runtime.trim_validation = ffprobe_duration_strict`，兼容字段 `runtime.validation = strict`。没有关闭或降级验证模式。基准只观察探测成本，不删除 FFprobe、不跳过时长检查。dry-run 和零片段可用于进度/元数据契约检查，但不能用于 Trim 吞吐计算。

以下是**待测模板，不是实测结果**；按混合/AV1-only/H.264-only 源集分别填表，环境限制保留为明确未测。

| 源集 / 缓存 | trim-jobs 请求 / 有效 | 有效重复数 | scan 中位数 [min,max] | Trim 中位数 [min,max] | clips/s | probe/task work | encoder / fallback | 状态 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| mixed / cold-scan | auto / 待测 | — | — | — | — | — | — | 环境限制，待实测 |
| mixed / cold-scan | 1 / 待测 | — | — | — | — | — | — | 环境限制，待实测 |
| mixed / cold-scan | 2 / 待测 | — | — | — | — | — | — | 环境限制，待实测 |
| mixed / cache-hit | auto / 待测 | — | — | — | — | — | — | 环境限制，待实测 |
| mixed / cache-hit | 1 / 待测 | — | — | — | — | — | — | 环境限制，待实测 |
| mixed / cache-hit | 2 / 待测 | — | — | — | — | — | — | 环境限制，待实测 |

填入结果时附上 commit/二进制与源集匿名指纹、环境、完整固定配置、重复次序、缓存实际命中率、Merge/total/外部墙钟和资源峰值。记录失败/取消/无片段次数及原因，不能只展示最快成功样本。结论应分别回答：OCR 是否稳定、双 Trim worker 是否提高吞吐、严格验证工作有多大；未测项目继续标为未测。

## 6. 本次验证记录与基准状态（2026-10-20，环境时钟）

执行环境为 Windows x64、Windows PowerShell 5.1；CPU 为 Intel Core i7-13700KF（24 个逻辑处理器），GPU 为 NVIDIA GeForce RTX 5080，驱动版本 `32.0.16.1742`。仓库锁定的 FFmpeg 下载脚本已成功下载 `N-127021-ge0c94b2d1c-20260930`。

本次已执行的检查：

| 检查 | 结果 | 证据边界 |
| --- | --- | --- |
| `cargo test --locked --manifest-path rust/Cargo.toml --workspace --all-features`，并将 `PUBG_HIGHLIGHT_TRIM_FFMPEG_DIR` 设为本仓库 `vendor/ffmpeg` | 201 passed、32 ignored、0 failed | 自动化正确性检查通过；ignored 项未执行，不代表真实语料性能验收通过 |
| `dotnet test ui/PubgHighlightTrim.Ui.Tests --configuration Release --no-restore --verbosity minimal` | 52 passed | UI 测试通过，不是 OCR/Trim 吞吐测量 |
| 生成真实 1 秒 H.264 黑屏视频后运行 CLI preflight | exit 1；明确报告 `Rust OCR ocr_models directory not found; set PUBG_OCR_MODEL_DIR or use offline release bundle` | 验证了模型缺失的启动阻断；未进入真实 OCR/Trim 基准 |

上述模型缺失是首次 preflight 的历史结果，**已通过以下本地准备流程解除，不是当前阻塞**。参照 Wiki `wiki/sources/回归测试方法.md`，在当前 checkout 构建生产 Release，并下载固定 manifest 的模型：

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/build_rust_release.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/build_rust_release.ps1 -VerifyOnly
& rust/target/release/xtask.exe download --output-dir .scratch/ocr_models
& rust/target/release/xtask.exe verify --model-dir .scratch/ocr_models --check-onnx --engine rust/target/release/ocr_engine.exe
$env:PUBG_OCR_MODEL_DIR = (Resolve-Path .scratch/ocr_models).Path
$env:PUBG_HIGHLIGHT_TRIM_FFMPEG_DIR = (Resolve-Path vendor/ffmpeg).Path
$env:RUSTFLAGS = '-C link-args=/DELAYLOAD:directml.dll -C link-args=delayimp.lib'
cargo test --locked --manifest-path rust/Cargo.toml --release --features cli,directml --test application_e2e --test ocr_provider_e2e --test dedupe_cli --test phase_progress -- --include-ignored --test-threads=1
cargo test --locked --manifest-path rust/Cargo.toml --release --features cli,directml --test trim_encoder --test trim_planning_contract --test media --test media_output --test dedupe_cancellation --test phase_timing
powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/build_rust_release.ps1 -VerifyOnly
```

模型下载与 SHA-256 校验成功，`verify --check-onnx` 实际加载并推理 detector/recognizer，状态为 `ok`。两组 Release 定向回归最终为 **29 + 33 = 62 passed、0 failed、0 ignored**；其中 `application_e2e` 16、`ocr_provider_e2e` 1，已显式执行真实模型测试，不能再描述为“仅受控媒体测试”。其他覆盖包括真实 FFmpeg AV1/H.264 媒体、去重、取消和输出安全；受控 NVENC 分支仍不等于目标 GPU 的实机认证。

第二组首次运行发现模拟 worker 的共享日志使用格式化分段写入，两个进程行内容交错，导致并发计数误报。改为先格式化、再单次追加完整记录后，相关 Release 用例连续 **12/12** 次通过，整组 33 项通过；未改生产调度逻辑，也未隐藏首次失败。

构建与测试前后 `VerifyOnly` 均通过。被验证的生产源码提交为 `ebc85070b090e7b294868f857d7a0e0560590b6f`（后续变更仅测试夹具日志与本记录）；`pubg_highlight_trim.exe` SHA-256 为 `b656c4a5c464447d39b177cdfb98eee204e99f3f6ac593fe9cbd5bc272930a30`。完整构建、资产校验、首次失败、重复用例和最终测试日志保存在本地 `.scratch/release-regression/`，不提交原始私有路径日志。

**固定 AV1/H.264 私有源集的真实重复性能基准仍未执行**，性能表保持未测；当前是尚待采集而非缺少模型。F 盘录屏未运行基准、未移动或修改。上述正确性回归不能替代固定配置下的吞吐、OCR 波动或验证成本结论。
