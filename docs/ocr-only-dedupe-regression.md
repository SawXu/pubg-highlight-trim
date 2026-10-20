# OCR-only 跨视频去重：小批量 CLI 回归

## 行为变化

用户明确选择：跨视频去重只用已有 OCR `target + event_key`，不再 seek / 解码画面验证。保留原有上下文优先与稳定顺序；同源事件、空 key 不去重。不同对局/录屏出现相同 OCR 身份也会去重，这是规则变化，不声称与旧画面验证结果等价。原始输入文件不修改；检测算法、采样、provider 和编码质量参数不变。

## 构建与范围

- 基准旧 CLI：commit `b4bdcfe`，SHA-256 `6f10f2860afa11025d9039ee927ad643c8a7731252df6f0a54d55b7275ba46ad`。
- 新 CLI：本 PR 源码编译的 Windows release，`--locked --features cli,directml --bin pubg_highlight_trim`；DirectML delay-load linker flags 与发布脚本一致。
- 新 CLI SHA-256：`74258e00d372b0aad4c66a24b405dcfcb8aeeba0c1d67fd694ab2d1fc505e459`。
- 仅复用 #95 固定输入中的 3 个真实源（匿名 S05/S06/S07），不合成、不复制视频模拟重复。
- 原 PP-OCRv6 模型与 FFmpeg/FFprobe；默认 provider auto，OCR jobs auto，threads=1，trim-jobs=1，before=5、after=1、min-event=2，merge/profile 开启。
- 前后独立空 OCR 缓存各跑一次，再各跑一次全命中，共 4 次真实 CLI。单次小回归，不是重复统计性能基准。

## 实测

| 运行 | 外部墙钟 | scan | dedupe | trim | 输出片段 | OCR cache |
|---|---:|---:|---:|---:|---:|---|
| 旧 miss | 34.798 s | 15.186 s | 11.203 s | 7.510 s | 5 | 0 hit / 3 miss |
| 新 miss | 22.070 s | 15.162 s | 0.000 s | 6.084 s | 4 | 0 hit / 3 miss |
| 旧 hit | 20.229 s | 0.338 s | 11.378 s | 7.661 s | 5 | 3 hit / 0 miss |
| 新 hit | 7.339 s | 0.344 s | 0.000 s | 6.147 s | 4 | 3 hit / 0 miss |

0.000 是 profile 毫秒精度舍入，不是计算完全无成本。整体 miss 约快 36.6%、hit 约快 63.7%，**既包括移除画面验证，也包括新规则多去重一段、少编码一段的收益**，不能全部归因于相同工作量下的执行加速。

## 检查结果

- 4 次均 exit 0。
- 前后真实 OCR 缓存 detections 的 duration/event/target/key/count/weapon/text/method/detector、sampled_count/ocr_requests 一致；不比较性能计时字段。
- 新 CLI 的 miss/hit 都是 5 candidates、1 comparison、1 duplicate、0 verification windows、0 verification failures；旧 CLI 同批为 0 duplicates。
- 新规则按现有上下文排序保留：上下文同分时优先前面的源。不能将新增删除称为“已通过画面确认的真实重复”。
- 保留片段的 KeepStartSec / KeepEndSec 与旧输出一致。
- 新 miss/hit 的 8 clips + 2 Merge 独立 FFprobe 时长检查通过（计划时长额外容差 0.1 s）；原有严格输出验证未修改。
- 3 个输入源前后 SHA-256 不变；产物与缓存全部位于独立临时目录，无移动原片。
- 不是人工内容正确性或召回率验收。

## 自动化覆盖

- `cross_source_duplicates`：8 项身份、空 key、同源、稳定顺序、上下文和零验证 telemetry 测试。
- `dedupe_cancellation`：1 项取消测试。
- `phase_progress`：3 项普通测试（另外 8 项环境依赖测试本轮不运行）。
- `trim_planning_contract`：19 项通过；重复场景的媒体替身禁止 rawvideo 解码，覆盖串行和并行 Trim。
- `dedupe_cli`：显式运行 ignored 实际 CLI 集成测试，11 个小型合成场景通过，包括不同画面但相同 OCR key 的新规则预期、空 key、不同 key、同源和后源上下文更完整。该测试使用预写 OCR 缓存，与上述真实 OCR 回归分开陈述。

## 用户指定的真实重复对补测

用户另行指定同一目录里的两个自然重叠原片，本轮只测这两个；下表使用文件名时间部分标识，不公开绝对路径。使用同一个已编译 CLI（上述 SHA-256），独立空应用缓存与输出目录，未使用预写 OCR 缓存，参数与上表一致。

| 原片标识 | OCR 事件时间 | 结果 | 原因 |
|---|---:|---|---|
| 2026.10.13 / 23.19.54.08 / 死亡 | 30.500 s | 保留 | 可提供完整前 5 秒、后 1 秒 |
| 2026.10.13 / 23.20.15.09 / 对战结束 | 2.000 s | 跳过：`skipped-cross-source-duplicate` | 相同 OCR 身份，仅有事件前 2 秒，上下文较短 |

两源均识别出同一条“指定玩家以 AUG 爆头击倒您”的 OCR 事件，`target=self-death`，归一化 `event_key` 完全一致；没有使用画面二次验证。用户已确认输入是重复片段，这次补测验证其实际 OCR 去重路径。

- exit 0；外部墙钟 **9.169 s**，scan **6.896 s**，dedupe **0.000 s**（毫秒舍入），trim **1.657 s**，merge **0.210 s**。
- 应用缓存 0 hit / 2 miss；2 candidates / 1 comparison / 1 duplicate / 0 verification windows / 0 verification failures。
- 最终 1 included / 1 skipped；保留裁剪区间 **25.500–31.500 s**。
- 外部 FFprobe：clip **6.000000 s**、Merge **6.021354 s**，均在计划 6 秒额外 0.1 秒容差内。
- 两个原片前后 SHA-256 一致，未移动或覆盖。
- 原始日志、独立缓存、CSV/profile/summary、脚本与输出保留在临时目录 `pubg96-regression/real-pair`（脚本为其父目录下 `real-pair.py`）。

本机私有日志与脚本保留在用户临时目录 `pubg96-regression` 下，不公开私有绝对路径。
