# #64 上游修复的 Rust 移植

## 精确上游范围

旧迁移基线 `a070076` 到 `origin/main` 的完整差异是提交
`50c11773ae6f103e4b5736524e75441b5e62a5fd`（PR #65，Fixes #64）：

- `src/pubg_highlight_trim/pipeline.py` 新增 132 行。
- `tests/test_pipeline_merge.py` 新增 143 行。
- 没有其他上游文件变化。已读取 `gh issue view 64` 的问题来源与验收条件。

merge 的 modify/delete 冲突继续保持这两个 Python 文件删除；行为由
`rust/ocr_engine/src/dedupe.rs` 和 application 两阶段准备流程承接，没有恢复 Python。

## 相对旧基线的预期行为变化

旧基线只在单文件扫描中去重，重叠的不同源录像会重复进入集锦。现在必须同时满足：

1. 非空、完整相等的 `event_key` 和相同 `target`，且源路径不同。
2. 先按实际可用的请求上下文排序：`min(event,before) + min(duration-event,after)`；
   稳定排序保留相同分数的先前输入。不是按全视频长度或玩家 ID 全局去重。
3. 对齐窗口 `left=max(-event,-other_event,-2)`、
   `right=min(duration-event,other_duration-other_event,0.5)` 跨度至少 1 秒。
4. 优先比较事件后 `0,0.1,0.2,0.3,0.4,0.5`，再尝试
   `left,(left+right)/2,right`。前一窗口不匹配不否决后一窗口。
5. 必须全分辨率尺寸和 BGR 像素逐字节相等；仅另一个源允许按顺序
   `-1/60,+1/60,-1/30,+1/30` 秒重试，且时间不得越界。
   尺寸不一致或缺帧立即失败；像素不匹配但重试无果则跳过该采样点。
6. 每个成功匹配帧的中心 80% 分为 4×4 格；采样步长为
   `max(1,height/80)`、`max(1,width/80)`。相邻成功匹配帧必须至少 8 格
   有严格大于 5% 的字节绝对差严格大于 8；至少成功匹配 3 帧。
   小于 16×16 的帧不能作为证据。
7. 解码器打开/读取错误、短窗口、缺帧、黑屏、静态帧、仅 HUD 动画、
   不同对局、不同/缺失事件键、同源事件均不凭不足证据抑制。
   精确匹配故意接受重新编码、时间漂移造成的保守漏去重。
8. 全部源先完成原有 min-event 分区和语言/武器上下文，再做跨源验证，
   最后才按原源顺序合并/裁剪。因此上下文更完整的后文件也能胜出。
9. 重复检测保留全部 OCR 遥测、身份和上下文，只有方法改为
   `skipped-cross-source-duplicate`，输出路径为空。记录仍按源 Index 有序，
   summary/profile 的原有计数、命名、退出码、拼接行为不另起一套规则。

OCR 扫描策略和 min-event 阈值没有修改。

## 测试边界与证据

用户允许的边界：公共帧源验证接口、完整 CLI/批处理入口；全部使用临时合成源，
不读取用户录像，不上传源文件。

- `tests/cross_source_duplicates.rs`：成功去重与后源胜出、不同目标、请求上下文排序、
  回退三点窗口；移植 Python 有用的不同对局/黑屏/静态/HUD/缺帧/异常/
  不同键/缺键/同源/额外事件/早 GOP 不匹配场景，增加邻帧、短证据、尺寸、
  最小尺寸、7/8 格、差值恰为 8、低于 5% 字节变化边界。
- `tests/dedupe_cli.rs`：完整生产 CLI，公共持久扫描缓存提供确定的检测输入，
  FFmpeg 生成无损动态合成视频并实际进行全分辨率读取、裁剪和拼接。
  11 场景：重复、更完整后文件、早期 seek 不同、不同对局、黑屏、静态、
  HUD、不同键、缺键、额外事件、同源多事件。
  验证 clip 文件数、concat 行数、CSV included/skipped、skip 方法和空 Output、
  25.500–31.500 六秒窗口、更完整后文件胜出、唯一事件保留、OCR 遥测、
  summary 方法计数、Index 排序及源文件字节完全不变。
  该测试明确是缓存边界的完整批处理测试，不宣称重测 OCR 识别准确率。

本机运行 CLI 测试（模型和 FFmpeg 只读复用另一专用 worktree 的已存在资产）：

```bash
export PUBG_OCR_MODEL_DIR=G:/work/orca/workspaces/pubg-highlight-trim/issue63-release/dist/pubg-highlight-trim/ocr_models
export PUBG_HIGHLIGHT_TRIM_FFMPEG_DIR=G:/work/orca/workspaces/pubg-highlight-trim/issue63-release/vendor/ffmpeg
cd rust
cargo test --features onnx-test-runtime --test dedupe_cli -- --ignored --nocapture
cargo test -p pubg-ocr-engine --tests
```

注意：单独 `--features onnx` 在未设置 ORT 链接配置的机器会链接失败；本机
`onnx-test-runtime` 使用已有测试 runtime 成功，不需要更改构建或业务配置。
