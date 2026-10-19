# CLI acceptance 为什么需要 7–10 分钟

调查范围：#76 / PR #77，源代码基线 `e80c0db`。只调查，未改应用、workflow 或测试。以下区分托管 runner 测量、本地对照和源码推断。

## 1. 实际时间花在哪里

一手证据：

- [冷缓存 run 39196575725 attempt 1](https://github.com/SawXu/pubg-highlight-trim/actions/runs/39196575725/attempts/1)
- [同 commit 热缓存 attempt 2](https://github.com/SawXu/pubg-highlight-trim/actions/runs/39196575725/attempts/2)
- [最终全绿 run 39199708303](https://github.com/SawXu/pubg-highlight-trim/actions/runs/39199708303)

从 GitHub 原始 step 日志中的 libtest `finished in` 提取测试执行时间；Cargo 时间另列，不混进测试时间。

| 测试程序 / 阶段 | 冷缓存 | 热缓存 attempt 2 | 最终热缓存 |
| --- | ---: | ---: | ---: |
| application_e2e（14 tests） | 388.47s | 287.84s | 345.78s |
| dedupe_cli（11 场景的一个 test） | 111.25s | 83.13s | 102.38s |
| nonfinite_summary | 24.67s | 17.32s | 21.45s |
| preprocess（2 tests） | 3.76s | 4.24s | 3.32s |
| CPU smoke | 4.41s | 2.94s | 3.68s |
| HTTPS failure safety | 0.08s | 0.18s | 0.15s |
| step 总时间（含 Cargo） | 约614s | 约426s | 约502s |

最终运行约 502s（8m22s），其中 application + dedupe 约 **448s，89%**。六条 Cargo 命令的 `Finished ... in` 合计约 **24.27s**。因此热缓存下主要是测试执行，不是重新编译，也不是模型下载。两次热缓存都有波动，不能把 7m06s 当固定时长。

工作流入口：`.github/workflows/ocr-rust.yml:152–169`，六条测试命令顺序执行；application/dedupe 保留 `--test-threads=1`。

## 2. 关键原因：测的是 debug CLI，不是前面构建的 release CLI

工作流使用 `cargo test --features cli`，没有 `--release`。`application_e2e.rs:86–90`、`dedupe_cli.rs:174`、`nonfinite_summary.rs:40` 使用 `env!("CARGO_BIN_EXE_pubg_highlight_trim")` 启动 Cargo 对应 profile 的应用。因此这些验收使用 debug/unoptimized 应用，前面正式构建好的 release 应用并未被这些测试复用。

特别昂贵的是模型完整性校验：

1. `scan_adapter.rs:88–111` 的 `EngineSettings::discover()` 在启动阶段调用 `validate_manifest`，**缓存命中也必须验证**。
2. `lib.rs:729–746` 的 `validate_manifest` 对每个模型 `fs::read` 后执行 SHA-256。
3. 创建真实识别器时 `lib.rs:406–414` 再验证；`OcrEngine::new` 在 `lib.rs:599–602` 又验证。`application.rs:443` 在 worker 中创建 engine，所以真实识别路径还有额外的模型读取/哈希轮次，不只一次启动验证。

固定模型总计 **138,662,763 bytes（约138.7 MB）**：detector 62,032,837，recognizer 76,554,979，dictionary 74,947。debug 下对这批数据反复哈希，会放大每次 CLI 启动成本；不能把所有耗时都称为 ONNX 推理。

### 本地公共 CLI 对照实验

相同 Windows 主机、固定模型、FFmpeg，单进程顺序执行，无应用逻辑修改。debug/release CLI 对照不替代 hosted runner 测量。

| 相同操作 | debug | release |
| --- | ---: | ---: |
| 无效 `--scan-start=nan` CLI，3次 | 1.693–1.762s/次 | 0.122–0.130s/次 |
| `xtask verify`，不加 `--check-onnx`，3次 | 1.673–1.745s/次 | 0.096–0.099s/次 |
| 完整 float-validation 测试（13参数） | 29.06s | 2.47s |
| 完整 dedupe_cli（全部11场景） | 52.89s | 15.53s |
| 完整 application_e2e（全部14项） | 204.83s | 83.21s |

以上测试均通过。无效参数用例和不执行推理的 verify 用例也存在明显差距，支持“debug 模型验证是重要成本”的判断；不是单纯推理/GPU慢。

复现方式（先设置模型和FFmpeg绝对路径）：

```powershell
$env:PUBG_OCR_MODEL_DIR = (Resolve-Path .scratch/ocr_models).Path
$env:PUBG_HIGHLIGHT_TRIM_FFMPEG_DIR = (Resolve-Path vendor/ffmpeg).Path
cargo test --locked --manifest-path rust/Cargo.toml --features cli --test application_e2e -- --ignored --test-threads=1
cargo test --locked --manifest-path rust/Cargo.toml --release --features cli --test application_e2e -- --ignored --test-threads=1
# 分别替换 --test dedupe_cli，或在 application_e2e 后加完整 float 测试名及 --exact。
```

表中是 libtest 的执行时间，**不包含 release 测试首次编译时间**；尚未证明整条 CI 切换 release 后一定节省相同秒数。实验用 `cli` feature、默认本地 flags；正式打包为 `cli,directml` + delay-load flags，若实施需要对齐构建配置、确认 feature 覆盖与冷编译成本。

## 3. 固定的 60 秒超时下限

`application_e2e.rs:869–909` 的 `cli_probe_system_failure_and_timeout_are_fatal_not_source_skips` 测试真实系统故障和真实超时。

- wrapper 在 `application_e2e.rs:775` 故意 sleep 90s。
- 媒体默认 timeout 是60s（`media.rs:229,267`）；`media.rs:42–75` 超时终止进程。
- 该项在 hosted 最终日志中约 **82.05s**，上次热跑约 **77.14s**，含两次CLI、wrapper构建、初始化及60s等待。
- 不应把90s sleep误计为实际等待90s。取消测试中的30s是等待上限，也不是每次固定花30s。

单项时长由同一个 `--test-threads=1` 测试程序相邻 completion 时间戳估算；libtest 汇总是直接执行计时。这个真实超时是当前测试契约，不应为了速度直接缩短生产超时。

## 4. 测试矩阵反复启动应用、生成媒体和编译工具

### application_e2e

- `python_float_validation...`（`:304–350`）遍历13组参数，每组是独立CLI进程；最终hosted约58.53s，上次热跑44.29s。很多失败路径也先付模型校验成本。
- `native_media_diagnostics...`（`:800–866`）对default/profile/verbose各运行一次真实CLI，最终约38.74s。
- `parallel_completion...`（`:702`起）比较jobs=1、2、auto，反复运行CLI。
- `own_kill...`（`:411`起）显式重复执行以验证缓存命中；其他缓存/顺序测试也需要完整生命周期。
- `blank` / `event`（`:36–84`）通过FFmpeg分别生成视频；`media_wrapper`（`:762–796`）每次调用rustc编译包装程序，没有跨测试复用。包装器按可执行文件名选择行为。

### dedupe_cli

`dedupe_cli.rs:88–185` 在一个test内串行遍历11个mode，每个mode生成两份视频、预写检测缓存、再启动CLI，总计22次视频生成和11次应用启动。预写OCR缓存并不免除模型身份校验或真实媒体裁剪/合并；不是11次完整OCR扫描，不能错误归因到模型推理。

### nonfinite / 其他

`nonfinite_summary.rs:35–81` 循环两个字段，每个字段启动一次CLI，共两次；最终21.45s。preprocess+CPU smoke最终合计仅7s左右，HTTPS测试实际0.15s，都不是优先瓶颈。冷跑最后一条Cargo命令耗时较大也不代表HTTPS网络测试本身慢。

## 5. 后续优化优先级与边界

1. **优先试验release-profile acceptance**：本地同一套14项测试从204.83s到83.21s，11场景dedupe从52.89s到15.53s，实测比盲目并行更有依据。先用hosted runner测完整冷/热耗时，并统一features/flags，不能只比较测试执行、不计算编译。
2. **诊断重复校验**：设计可信的“已验证模型”生命周期以消除同次初始化重复读取/哈希，同时保留缓存命中和加载前的完整验证。涉及应用保障语义，不在当前调查中直接修改或跳过。
3. **包装器/只读媒体夹具复用**：编译一次包装器、复制为不同名称；相同视频可按测试契约复用，只允许共享只读输入，输出/缓存仍隔离。
4. **最后评估独立验收并行**：减少串行关键路径，但模型哈希、推理、FFmpeg会争CPU/内存，需限制并发并证明资源安全。不能直接删除现有单线程隔离。
5. **60s超时验收单独看待**：属于明确的正确性等待。若需要短超时注入测试，应明确保留生产默认超时的保障，不能把测试加速等同改变生产默认。

没有修改代码、跳过验证、降低超时或改变测试调度。尚未给出hosted整条CI的release-profile加速保证。
