<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  Workflow,
  type Bootstrap,
  type RunOptions,
  type RunState,
} from "./workflow";

const api = new Workflow(invoke);
const bootstrap = ref<Bootstrap>();
const options = ref<RunOptions>();
const state = ref<RunState>();
const error = ref("");
const preview = ref("");
const warnings = ref<string[]>([]);
const validationError = ref("");
const logs = ref<string[]>([]);
const showLogs = ref(false);
const appVersion = __APP_VERSION__;
const busy = ref(false);
const settings = ref(false);
const inputText = ref("");
const now = ref(Date.now());
let runStartedAt = Date.now();
let observedRunId: number | undefined;
const elapsed = computed(() => {
  const seconds = state.value?.summary?.profile_total_sec;
  const value =
    typeof seconds === "number"
      ? seconds
      : state.value?.running
        ? (now.value - runStartedAt) / 1000
        : 0;
  return `${Math.floor(value / 60)}:${Math.floor(value % 60)
    .toString()
    .padStart(2, "0")}`;
});
const locked = computed(() => busy.value || !!state.value?.running);
// Presentation of the existing four target combinations; Rust still owns CLI mapping.
const targetMode = computed({
  get: () => {
    if (options.value?.selfDeath && options.value?.ownKill) return "separate";
    if (options.value?.selfDeath) return "self-death";
    if (options.value?.ownKill) return "own-kill";
    return "combined";
  },
  set: (mode: string) => {
    if (!options.value) return;
    options.value.selfDeath = mode === "self-death" || mode === "separate";
    options.value.ownKill = mode === "own-kill" || mode === "separate";
  },
});
const outcomes: Record<string, string> = {
  success: "处理完成",
  "no-events": "没有检测到事件",
  "partial-unreadable": "已完成，部分视频不可读",
  "all-unreadable": "所有视频均不可读",
  failed: "处理失败",
  cancelled: "任务已取消",
  "cleanup-failed": "进程清理失败",
};
const phases: Record<string, string> = {
  scan: "OCR 扫描",
  dedupe: "事件去重",
  trim: "裁剪片段",
  merge: "合并输出",
};
const title = computed(() =>
  state.value?.outcome === "cleanup-failed"
    ? "进程清理失败，请重试停止任务"
    : state.value?.error
      ? "任务异常，请查看错误信息"
      : state.value?.cancelling
        ? "正在停止并清理任务…"
        : state.value?.running
          ? "正在处理精彩时刻"
          : (outcomes[state.value?.outcome ?? ""] ?? "准备处理"),
);
const active = computed(() =>
  state.value?.activePhase
    ? state.value.phases[state.value.activePhase]
    : undefined,
);
let timer: ReturnType<typeof setInterval> | undefined;
let validationTimer: ReturnType<typeof setTimeout> | undefined;
let generation = 0;
let unlisten: UnlistenFn | undefined;
let polling = false;
let disposed = false;

const confirmation = ref("");
const confirmationDialog = ref<HTMLDialogElement>();
watch(confirmation, async (message) => {
  await nextTick();
  if (message && !confirmationDialog.value?.open)
    confirmationDialog.value?.showModal();
  else if (!message) confirmationDialog.value?.close();
});
let resolveConfirmation: ((accepted: boolean) => void) | undefined;
function confirmAction(message: string): Promise<boolean> {
  if (resolveConfirmation) return Promise.resolve(false);
  confirmation.value = message;
  return new Promise((resolve) => {
    resolveConfirmation = resolve;
  });
}
function answerConfirmation(accepted: boolean) {
  const resolve = resolveConfirmation;
  resolveConfirmation = undefined;
  confirmation.value = "";
  resolve?.(accepted);
}

async function action(task: () => Promise<unknown>) {
  error.value = "";
  try {
    await task();
  } catch (e) {
    error.value = String(e);
  }
}
async function refresh() {
  if (polling || disposed) return;
  polling = true;
  try {
    const [next, lines] = await Promise.all([api.status(), api.logs()]);
    if (!disposed) {
      if (observedRunId !== next.runId) {
        observedRunId = next.runId;
        runStartedAt = Date.now();
      }
      now.value = Date.now();
      state.value = next;
      logs.value = lines;
    }
  } finally {
    polling = false;
  }
}
async function chooseInput(kind: "folder" | "files") {
  await action(async () => {
    const paths = await api.pickInputs(kind);
    if (paths.length && options.value) {
      options.value.inputPaths = paths;
      inputText.value = paths.join("\n");
    }
  });
}
async function chooseOutput() {
  await action(async () => {
    const path = await api.pickOutput();
    if (path && options.value) options.value.outputDirectory = path;
  });
}
function editInput() {
  if (options.value)
    options.value.inputPaths = inputText.value
      .split(/\r?\n/)
      .map((p) => p.trim())
      .filter(Boolean);
}
async function start() {
  if (!options.value || locked.value) return;
  busy.value = true;
  await action(async () => {
    await api.start(options.value!, async (message) =>
      confirmAction(`${message}\n\n确认开始处理？`),
    );
    await refresh();
  });
  busy.value = false;
}
async function reset() {
  if (
    !bootstrap.value ||
    locked.value ||
    !(await confirmAction("恢复所有处理选项为默认值？"))
  )
    return;
  options.value = {
    ...bootstrap.value.defaults,
    inputPaths: [...bootstrap.value.defaults.inputPaths],
  };
  inputText.value = options.value.inputPaths.join("\n");
}
async function clearCache() {
  await action(async () => {
    if (
      await confirmAction(
        "清理应用缓存及全部浏览器用户数据（含 Cookie、本地存储和 IndexedDB）？设置、日志、源视频、视频输出及 OCR 缓存不受影响。",
      )
    )
      await api.clearCache();
  });
}
async function save() {
  busy.value = true;
  await action(async () => {
    await api.save(options.value!);
    settings.value = false;
  });
  busy.value = false;
}
watch(
  options,
  () => {
    const current = ++generation;
    clearTimeout(validationTimer);
    preview.value = "";
    validationError.value = "";
    warnings.value = [];
    validationTimer = setTimeout(async () => {
      try {
        const result = await api.validate(options.value!);
        if (generation === current) {
          preview.value = result.preview;
          warnings.value = result.warnings;
        }
      } catch (e) {
        if (generation === current) validationError.value = String(e);
      }
    }, 200);
  },
  { deep: true },
);
onMounted(async () => {
  await action(async () => {
    bootstrap.value = await api.bootstrap();
    options.value = bootstrap.value.options;
    inputText.value = options.value.inputPaths.join("\n");
    // Polling remains available even if the first refresh or event permission fails.
    timer = setInterval(() => {
      void refresh().catch((e) => {
        error.value = String(e);
      });
    }, 500);
    await action(refresh);
    await action(async () => {
      const stop = await listen("close-requested", () => {
        void action(async () => {
          if (await confirmAction("任务仍在运行。停止任务、清理子进程并退出？"))
            await api.close(true);
        });
      });
      if (disposed) stop();
      else unlisten = stop;
    });
  });
});
onUnmounted(() => {
  disposed = true;
  answerConfirmation(false);
  clearInterval(timer);
  clearTimeout(validationTimer);
  unlisten?.();
});
</script>

<template>
  <header class="topbar">
    <div class="brand-mark">P</div>
    <div>
      <h1>PUBG Highlight Trim</h1>
      <p>OCR 视频剪辑工作台</p>
    </div>
    <span class="connection" :class="{ ready: bootstrap?.cliReady }"
      >● {{ bootstrap?.cliReady ? "CLI 已连接" : "CLI 未就绪" }}</span
    >
    <button class="header-button" @click="settings = !settings">设置</button>
  </header>
  <div v-if="error || bootstrap?.error" class="error" role="alert">
    {{ error || bootstrap?.error }}
    <button @click="error = ''">关闭提示</button>
  </div>
  <main v-if="options && bootstrap" class="workspace">
    <aside>
      <fieldset :disabled="locked">
        <h2>输入与输出</h2>
        <label for="inputs"
          >视频或文件夹 <small>多 MP4 路径可每行一个</small></label
        >
        <textarea
          id="inputs"
          v-model="inputText"
          rows="3"
          placeholder="选择文件夹、单个或多个 MP4"
          @input="editInput"
        ></textarea>
        <div class="button-row">
          <button @click="chooseInput('folder')">选择文件夹</button
          ><button @click="chooseInput('files')">选择 MP4（可多选）</button>
        </div>
        <label for="output">输出目录 <small>留空由 CLI 自动创建</small></label>
        <div class="inline">
          <input
            id="output"
            v-model="options.outputDirectory"
            placeholder="自动创建"
          /><button @click="chooseOutput">浏览</button>
        </div>
        <h2>检测与剪辑</h2>
        <div class="target-heading" id="target-heading">检测目标与输出方式</div>
        <div class="targets" role="radiogroup" aria-labelledby="target-heading">
          <label :class="{ selected: targetMode === 'combined' }">
            <input v-model="targetMode" type="radio" name="target-mode" value="combined" />
            <span><strong>全部合在一起</strong><small>两类事件 → 一个视频</small></span>
          </label>
          <label :class="{ selected: targetMode === 'separate' }">
            <input v-model="targetMode" type="radio" name="target-mode" value="separate" />
            <span><strong>按目标分别合并</strong><small>两类事件 → 各一个视频</small></span>
          </label>
          <label :class="{ selected: targetMode === 'self-death' }">
            <input v-model="targetMode" type="radio" name="target-mode" value="self-death" />
            <span><strong>仅自己倒地</strong></span>
          </label>
          <label :class="{ selected: targetMode === 'own-kill' }">
            <input v-model="targetMode" type="radio" name="target-mode" value="own-kill" />
            <span><strong>仅我的击杀</strong></span>
          </label>
        </div>
        <p class="hint" v-if="!options.merge">已关闭合并输出：只生成独立片段，不生成合并视频。</p>
        <p class="hint" v-else>仅包含检测到的片段；某类没有匹配事件时，不生成该类视频。</p>
        <div class="fields two">
          <label
            >游戏语言<select v-model="options.gameLanguage">
              <option
                v-for="choice in bootstrap.schema.gameLanguage"
                :key="choice.value"
                :value="choice.value"
              >
                {{ choice.label }}
              </option>
            </select></label
          >
          <label
            >扫描模式<select v-model="options.scanMode">
              <option
                v-for="choice in bootstrap.schema.scanMode"
                :key="choice.value"
                :value="choice.value"
              >
                {{ choice.label }}
              </option>
            </select></label
          >
          <label
            >事件前（秒）<input
              v-model.number="options.secondsBefore"
              type="number"
              step="any"
          /></label>
          <label
            >事件后（秒）<input
              v-model.number="options.secondsAfter"
              type="number"
              step="any"
          /></label>
        </div>
        <div class="checks">
          <label
            ><input v-model="options.merge" type="checkbox" />合并输出</label
          >
          <label
            ><input
              v-model="options.recursive"
              type="checkbox"
            />递归扫描子文件夹</label
          >
          <label
            ><input v-model="options.scanOnly" type="checkbox" />仅扫描</label
          >
          <label
            ><input v-model="options.overwrite" type="checkbox" />覆盖输出
            <span class="danger-text">会替换已有输出</span></label
          >
          <label
            ><input v-model="options.moveOriginals" type="checkbox" />归档原片
            <span class="danger-text">会移走源文件</span></label
          >
        </div>
        <p v-if="options.moveOriginals" class="warning">
          产出过片段的源 MP4 将从原目录移到输出目录 originals 子目录，并写入
          moved_sources.csv 清单。
        </p>
        <details>
          <summary>专家设置</summary>
          <div class="fields two">
            <label
              >忽略开场事件（秒）<input
                v-model.number="options.minimumEventSeconds"
                type="number"
                step="any"
              /><small>0 表示保留开场事件</small></label
            >
            <label
              >燃烧瓶前置（秒）<input
                v-model.number="options.molotovSecondsBefore"
                type="number"
                step="any"
              /><small>0 表示禁用特殊前置</small></label
            >
            <label
              >OCR 并行度<input
                v-model="options.jobs"
                placeholder="auto 或正整数"
            /></label>
            <label
              >Trim 并行度<select v-model="options.trimJobs">
                <option
                  v-for="choice in bootstrap.schema.trimJobs"
                  :key="choice.value"
                  :value="choice.value"
                >
                  {{ choice.label }}
                </option>
              </select></label
            >
          </div>
          <label class="check"
            ><input
              v-model="options.brightnessGate"
              type="checkbox"
            />启用亮色描边文字预筛选</label
          >
        </details>
      </fieldset>
      <p v-if="validationError" class="validation" role="status">
        {{ validationError }}
      </p>
      <div class="run-actions">
        <button
          class="primary"
          :disabled="locked || !bootstrap.cliReady || !preview"
          @click="start"
        >
          {{ busy ? "请稍候…" : "开始处理" }}</button
        ><button
          :disabled="!state?.running || state.cancelling"
          @click="
            action(async () => {
              await api.cancel();
              await refresh();
            })
          "
        >
          停止任务
        </button>
      </div>
    </aside>
    <section class="results">
      <div class="result-heading">
        <div>
          <h2>{{ title }}</h2>
          <p>
            {{
              state?.running
                ? "配置已锁定；停止任务后会清理所有任务子进程。"
                : "选择输入，保留属于你的精彩时刻。"
            }}
          </p>
        </div>
        <button
          :disabled="!state?.summary || state.running"
          @click="action(() => api.openOutput())"
        >
          打开输出
        </button>
      </div>
      <div class="metrics">
        <div>
          <span>当前进度</span
          ><strong>{{
            active ? `${active.current} / ${active.total}` : "—"
          }}</strong>
        </div>
        <div>
          <span>已保留</span
          ><strong class="accent">{{ state?.included ?? 0 }}</strong>
        </div>
        <div>
          <span>已跳过</span><strong>{{ state?.skipped ?? 0 }}</strong>
        </div>
        <div>
          <span>耗时</span><strong>{{ elapsed }}</strong>
        </div>
        <progress
          v-if="active && active.total > 0"
          :value="active.current"
          :max="active.total"
        ></progress>
        <progress v-else-if="state?.running"></progress>
      </div>
      <div class="activity-header">
        <h3>运行记录</h3>
        <button @click="showLogs = !showLogs">
          {{ showLogs ? "收起日志" : "显示日志" }}</button
        ><button
          v-if="showLogs"
          @click="
            action(async () => {
              await api.clearLogs();
              await refresh();
            })
          "
        >
          清空日志
        </button>
      </div>
      <pre
        v-if="showLogs"
        class="logs"
        tabindex="0"
        aria-label="CLI 运行日志"
        >{{ logs.join("\n") || "暂无日志" }}</pre
      >
      <div v-else class="activity">
        <div class="activity-icon">{{ state?.running ? "◌" : "◈" }}</div>
        <h3>{{ title }}</h3>
        <p v-if="state?.error" class="danger-text">{{ state.error }}</p>
        <p v-else-if="!state?.outcome && !state?.running">
          运行阶段与结果将在这里显示
        </p>
        <ol class="phase-list">
          <li
            v-for="(phase, name) in state?.phases"
            :key="name"
            :class="{ active: state?.activePhase === name }"
          >
            <strong>{{ phases[name] ?? name }}</strong
            ><span>{{ phase.current }} / {{ phase.total }}</span
            ><span v-if="phase.workers !== null"
              >{{ phase.workers }} workers</span
            ><span>{{
              phase.event === "done"
                ? "完成"
                : phase.event === "skipped"
                  ? "已跳过"
                  : phase.event
            }}</span
            ><small v-if="phase.reason">{{ phase.reason }}</small>
          </li>
        </ol>
        <p v-if="state?.exitCode !== null && state?.outcome">
          退出码 {{ state?.exitCode }}
        </p>
        <details v-if="state?.summary" class="summary">
          <summary>查看结果摘要</summary>
          <pre>{{ JSON.stringify(state.summary, null, 2) }}</pre>
        </details>
      </div>
      <div class="command">
        <h3>命令预览</h3>
        <code>{{ preview || "输入有效选项后显示由 Rust 生成的命令" }}</code>
        <p v-for="warning in warnings" :key="warning" class="danger-text">
          {{ warning }}
        </p>
      </div>
    </section>
  </main>
  <section v-else class="loading">
    <h2>正在连接 Rust 工作台…</h2>
    <p>此界面需通过 Tauri 桌面应用启动。</p>
  </section>
  <section
    v-if="settings && bootstrap && options"
    class="settings"
    aria-label="应用设置"
  >
    <div class="result-heading">
      <h2>应用设置</h2>
      <button @click="settings = false">关闭</button>
    </div>
    <p>数据目录（设置、日志与 WebView 用户数据及缓存）</p>
    <code>{{ bootstrap.dataDir }}</code>
    <p>
      通过启动环境变量 PUBG_HIGHLIGHT_TRIM_DATA_DIR
      配置专用数据目录；应用不会静默回退到 AppData。
    </p>
    <p>可信 CLI 路径</p>
    <code>{{ bootstrap.cliPath }}</code>
    <p>
      清理操作将删除应用缓存及全部浏览器用户数据（含 Cookie、本地存储和
      IndexedDB）。设置、日志、源视频、视频输出和 OCR 缓存不受影响。
    </p>
    <div class="button-row">
      <button :disabled="locked" @click="save">保存当前处理选项</button
      ><button :disabled="locked" @click="reset">恢复默认选项</button
      ><button :disabled="locked" @click="clearCache">
        清理缓存及浏览器用户数据
      </button>
    </div>
  </section>
  <dialog
    ref="confirmationDialog"
    class="confirmation-dialog"
    role="alertdialog"
    aria-labelledby="confirmation-title"
    aria-describedby="confirmation-message"
    @cancel.prevent="answerConfirmation(false)"
  >
    <h2 id="confirmation-title">请确认操作</h2>
    <p id="confirmation-message">{{ confirmation }}</p>
    <div class="button-row">
      <button autofocus @click="answerConfirmation(false)">取消</button>
      <button class="primary" @click="answerConfirmation(true)">确认</button>
    </div>
  </dialog>
  <footer>
    v{{ appVersion }} · Vue 3 · Tauri 2
    <span>CLI black-box mode · OCR provider auto</span>
  </footer>
</template>
