import { describe, expect, it, vi } from "vitest";
import {
  Workflow,
  type Bootstrap,
  type Invoke,
  type RunOptions,
  type RunState,
} from "../src/workflow";

// Values here are fixtures, not production defaults. Production always bootstraps Rust.
const options: RunOptions = {
  inputPaths: ["C:\\视频\\a.mp4", "C:\\视频\\b.mp4"],
  outputDirectory: "D:\\output",
  selfDeath: true,
  ownKill: true,
  gameLanguage: "en",
  secondsBefore: 7.5,
  secondsAfter: 2,
  minimumEventSeconds: 0,
  molotovSecondsBefore: 12,
  scanMode: "fast",
  jobs: "3",
  trimJobs: "2",
  recursive: true,
  merge: true,
  scanOnly: false,
  overwrite: true,
  brightnessGate: false,
  moveOriginals: true,
};
function harness(
  handlers: Record<
    string,
    unknown | ((args?: Record<string, unknown>) => unknown)
  >,
) {
  const calls: [string, Record<string, unknown> | undefined][] = [];
  const invoke: Invoke = async <T>(
    command: string,
    args?: Record<string, unknown>,
  ) => {
    calls.push([command, args]);
    if (!(command in handlers))
      throw new Error(`Unexpected shell call: ${command}`);
    const value = handlers[command];
    return (typeof value === "function" ? await value(args) : value) as T;
  };
  return { api: new Workflow(invoke), calls };
}

describe("public UI → Rust workflow boundary (no DOM)", () => {
  it("uses shell defaults, schema and persisted settings without a second defaults table", async () => {
    const bootstrap = {
      defaults: { ...options, jobs: "auto" },
      options,
      schema: { gameLanguage: [], scanMode: [], trimJobs: [] },
      dataDir: "app/data",
      cliPath: "trusted/cli.exe",
      cliReady: true,
    } satisfies Bootstrap;
    const { api } = harness({ get_bootstrap: bootstrap });
    expect(await api.bootstrap()).toEqual(bootstrap);
  });
  it("validates all options, presents authoritative warnings then starts with the exact snapshot", async () => {
    const { api, calls } = harness({
      validate_options: {
        arguments: ["opaque"],
        preview: "Rust generated",
        warnings: ["移动原片", "覆盖输出"],
      },
      start_run: 19,
    });
    const confirm = vi.fn(async () => true);
    expect(await api.start(options, confirm)).toBe(19);
    expect(confirm).toHaveBeenCalledWith("移动原片\n\n覆盖输出");
    expect(calls).toEqual([
      ["validate_options", { options }],
      ["start_run", { options, confirmed: true }],
    ]);
  });
  it("does not start when a dangerous operation is declined", async () => {
    const { api, calls } = harness({
      validate_options: {
        arguments: [],
        preview: "",
        warnings: ["归档会移走源视频"],
      },
    });
    expect(await api.start(options, async () => false)).toBeNull();
    expect(calls.map((c) => c[0])).toEqual(["validate_options"]);
  });
  it("does not start on shell validation failure and permits correction/retry", async () => {
    let valid = false;
    const { api, calls } = harness({
      validate_options: () => {
        if (!valid) throw new Error("输入不存在");
        return { arguments: [], preview: "", warnings: [] };
      },
      start_run: 2,
    });
    await expect(api.start(options, async () => true)).rejects.toThrow(
      "输入不存在",
    );
    valid = true;
    expect(await api.start(options, async () => true)).toBe(2);
    expect(calls.filter((c) => c[0] === "start_run")).toHaveLength(1);
  });
  it("locks duplicate starts and freezes options while awaiting confirmation", async () => {
    let release!: (value: boolean) => void;
    const confirm = () =>
      new Promise<boolean>((resolve) => {
        release = resolve;
      });
    const { api, calls } = harness({
      validate_options: { arguments: [], preview: "", warnings: ["warning"] },
      start_run: 5,
    });
    const edited = structuredClone(options);
    const starting = api.start(edited, confirm);
    await Promise.resolve();
    await expect(api.start(edited, confirm)).rejects.toThrow("重复");
    edited.inputPaths.push("evil.mp4");
    edited.overwrite = false;
    release(true);
    await starting;
    expect(calls.at(-1)).toEqual(["start_run", { options, confirmed: true }]);
  });
  it("forwards authoritative status, unknown phases, summary and outcomes without reinterpreting stdout", async () => {
    const status: RunState = {
      runId: 42,
      running: false,
      cancelling: false,
      exitCode: 2,
      outcome: "partial-unreadable",
      summary: { future_field: true },
      phases: {
        future: {
          current: 1,
          total: 2,
          workers: 3,
          event: "skipped",
          reason: "future",
          planned: null,
        },
      },
      activePhase: "future",
      included: 3,
      skipped: 4,
      error: null,
    };
    const { api } = harness({
      get_run_state: status,
      get_logs: ["ERR SUMMARY fake"],
    });
    expect(await api.status()).toEqual(status);
    expect(await api.logs()).toEqual(["ERR SUMMARY fake"]);
  });
  it("keeps native dialogs, persistence, cleanup and cancellation behind the shell", async () => {
    const { api, calls } = harness({
      pick_inputs: ["a.mp4", "b.mp4"],
      pick_output: null,
      save_options: undefined,
      cancel_run: undefined,
      clear_logs: undefined,
      clear_cache: undefined,
      open_output: undefined,
      close_app: undefined,
    });
    expect(await api.pickInputs("files")).toEqual(["a.mp4", "b.mp4"]);
    expect(await api.pickOutput()).toBeNull();
    await api.save(options);
    await api.cancel();
    await api.clearLogs();
    await api.clearCache();
    await api.openOutput();
    await api.close(true);
    expect(calls).toEqual([
      ["pick_inputs", { kind: "files" }],
      ["pick_output", undefined],
      ["save_options", { options }],
      ["cancel_run", undefined],
      ["clear_logs", undefined],
      ["clear_cache", undefined],
      ["open_output", undefined],
      ["close_app", { confirmed: true }],
    ]);
  });
});
