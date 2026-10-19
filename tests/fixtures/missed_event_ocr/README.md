# 真实漏事件 ROI 回归

这些 PNG 来自本地 NVIDIA Highlights 录像目录
`F:/NVIDIA/TEMP/Highlights/PLAYERUNKNOWN'S BATTLEGROUNDS/`，2026-10-13 批次。
只保存事件提示的小块 ROI；测试不读取、不需要分发私有原视频，也不调用 ffmpeg。

| fixture | 原视频文件名 | 视频内时间 | 人工可见事件 |
| --- | --- | --- | --- |
| `23.04.41.02_32.png` | `PLAYERUNKNOWN'S BATTLEGROUNDS 2026.10.13 - 23.04.41.02.單次擊殺.DVR.mp4` | 32.0 秒 | 您以 Mini14 擊倒 heartbreak_kidz |
| `23.09.16.04_64.png` | `PLAYERUNKNOWN'S BATTLEGROUNDS 2026.10.13 - 23.09.16.04.單次擊殺.DVR.mp4` | 64.0 秒 | 您以 P92 擊殺 [CSYL] Lc100p |
| `23.20.15.09_2.png` | `PLAYERUNKNOWN'S BATTLEGROUNDS 2026.10.13 - 23.20.15.09.對戰結束.DVR.mp4` | 2.0 秒 | [JXT] AG_GuHan 以 AUG 爆頭擊倒 您 |

来源为诊断目录 `F:/NVIDIA/TEMP/miss-event-diagnosis/`：前两张同名 PNG 原样复制；
第三张来自 `death/roi/04_2.0.png`（该诊断编号不是原视频编号），原宽 1024，
以 OpenCV 默认双线性插值 resize 为 768 × 98 后无损保存。
均使用默认归一化 ROI `(left=.30, top=.66, right=.70, bottom=.75)` 和默认 OCR 宽度 768。

运行（PowerShell）：

```powershell
$env:PUBG_OCR_MODEL_DIR='<已校验模型目录>'
cargo test --locked --manifest-path rust/Cargo.toml --features cli --test preprocess real_roi_preprocessing_keeps_recognized_subjects -- --ignored --nocapture
```

该测试属于显式 ignored 真实模型门槛；启用后模型缺失、启动或推理错误应失败。
三帧在 Rust 内直接使用真实 OCR library，CPU、单线程，随后调用 canonical
`extract_text_events`，固定 `zh-Hant`；不再经过已退休的 Python 适配器。
断言 target/action/subject 主身份和事件 key，不锁定武器、完整 OCR 文本或 HUD 噪声。
图片不能验证文件名语言推断，因此这里不测试 auto 语言推断或视频扫描时间边界。

旧发布引擎（SHA256 `27e97e28728bd459c287e4132542b557e90b4c13bb89fe6c976ffaaf48e05b4f`）
实测三帧均失败：`.02@32`、`.04@64` 因同一行文本框错序无法分类；
`.09@2` 虽可分类为 self-death，但提取到的 subject 是 `AUG爆頭` 而非攻击者。
修复排序后应通过；不使用 xfail。
现有 real benchmark 图片均有目标事件，不能作为“旁人击杀/助攻”的确切负例；
这里不编造负例，原 `test_ocr_refine.py` / `test_game_languages.py` 的固定规则保障已迁入 Rust scanner fixtures 和 `events_contract.rs`，不保留旧 Python 测试执行器。
