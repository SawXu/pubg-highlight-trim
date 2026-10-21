# Tauri 工作台前端

Vue 3 + TypeScript + Vite；不引入重量级组件库。Windows 桌面运行方式和 CLI 资源准备见仓库根 README。

```powershell
cd ui
npm ci
npm run typecheck
npm test
npm run build
npm run tauri dev
```

`npm run dev` 仅启动 Vite；网页浏览器没有 Tauri IPC，不会模拟业务结果或启动 CLI。

## 边界

- `src/workflow.ts` 是可独立测试的公开 UI→Rust seam；Vue 仅负责展示及用户交互。
- `get_bootstrap` 提供持久化选项、默认值、枚举 schema、可信 CLI 与数据目录。
- `validate_options` 提供参数数组、命令预览、危险操作警告；前端不生成 CLI 参数、不重复参数合法性规则。
- `start_run` 使用已校验选项快照，确认警告后调用；壳仍必须自行检查确认、并发和配置合法性。
- `get_run_state` 提供权威阶段、计数及终态；前端不解析 stdout/stderr 或重新判定退出码。未知阶段可显示，完整 summary 可展开查看。
- 原生选择器、配置持久化、取消/进程树清理、输出目录打开、日志与缓存操作均通过 Rust 命令。
- 运行期间锁定配置。窗口关闭通过壳的 `close-requested` 事件确认，再调用 `close_app`，等待壳清理进程树。

## 测试

`tests/workflow.test.ts` 不挂载 DOM，不依赖私有 Vue 函数。覆盖权威 bootstrap、选项透传、危险操作确认/拒绝、校验失败重试、重复启动锁、选项快照、未知阶段与终态透传，以及原生操作委派。这些是公开 seam 合同测试，并不替代 `src-tauri/tests` 与仓库外部行为脚本的 Rust→CLI 集成测试。

数据目录通过 `PUBG_HIGHLIGHT_TRIM_DATA_DIR` 配置。“清理缓存及浏览器用户数据”会删除应用缓存与全部 WebView 浏览器用户数据（含 Cookie、本地存储和 IndexedDB），并在操作前明确确认。设置、日志、源视频、视频输出和 OCR 缓存不受影响；视频输出和 OCR 缓存仍由 CLI 保持原策略。
