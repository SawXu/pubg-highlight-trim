// The Rust shell owns defaults, validation, command construction and run outcomes.
// This public seam is deliberately independent of Vue and the DOM.
export interface RunOptions {
  inputPaths: string[];
  outputDirectory: string | null;
  selfDeath: boolean;
  ownKill: boolean;
  gameLanguage: string;
  secondsBefore: number;
  secondsAfter: number;
  minimumEventSeconds: number;
  molotovSecondsBefore: number;
  scanMode: string;
  jobs: string;
  trimJobs: string;
  recursive: boolean;
  merge: boolean;
  scanOnly: boolean;
  overwrite: boolean;
  brightnessGate: boolean;
  moveOriginals: boolean;
}
export interface Choice {
  value: string;
  label: string;
}
export interface Bootstrap {
  defaults: RunOptions;
  options: RunOptions;
  schema: { gameLanguage: Choice[]; scanMode: Choice[]; trimJobs: Choice[] };
  dataDir: string;
  cliPath: string | null;
  cliReady: boolean;
  error?: string | null;
}
export interface Validation {
  arguments: string[];
  preview: string;
  warnings: string[];
}
export interface Phase {
  current: number;
  total: number;
  workers: number | null;
  event: string | null;
  reason: string | null;
  planned: number | null;
}
export interface RunState {
  runId: number;
  running: boolean;
  cancelling: boolean;
  exitCode: number | null;
  outcome: string | null;
  summary: Record<string, unknown> | null;
  phases: Record<string, Phase>;
  activePhase: string | null;
  included: number;
  skipped: number;
  error: string | null;
}
export type Invoke = <T>(
  command: string,
  args?: Record<string, unknown>,
) => Promise<T>;
export type Confirm = (message: string) => Promise<boolean>;

export class Workflow {
  constructor(private readonly invoke: Invoke) {}

  bootstrap() {
    return this.invoke<Bootstrap>("get_bootstrap");
  }
  validate(options: RunOptions) {
    return this.invoke<Validation>("validate_options", { options });
  }
  status() {
    return this.invoke<RunState>("get_run_state");
  }
  logs() {
    return this.invoke<string[]>("get_logs");
  }
  clearLogs() {
    return this.invoke<void>("clear_logs");
  }
  clearCache() {
    return this.invoke<void>("clear_cache");
  }
  cancel() {
    return this.invoke<void>("cancel_run");
  }
  openOutput() {
    return this.invoke<void>("open_output");
  }
  pickInputs(kind: "folder" | "files") {
    return this.invoke<string[]>("pick_inputs", { kind });
  }
  pickOutput() {
    return this.invoke<string | null>("pick_output");
  }
  save(options: RunOptions) {
    return this.invoke<void>("save_options", { options });
  }
  close(confirmed: boolean) {
    return this.invoke<void>("close_app", { confirmed });
  }

  private starting = false;
  async start(options: RunOptions, confirm: Confirm): Promise<number | null> {
    if (this.starting) throw new Error("正在启动任务，请勿重复操作。");
    this.starting = true;
    try {
      // Snapshot before async confirmation so edits cannot bypass validated warnings.
      const snapshot = { ...options, inputPaths: [...options.inputPaths] };
      const validation = await this.validate(snapshot);
      if (
        validation.warnings.length &&
        !(await confirm(validation.warnings.join("\n\n")))
      )
        return null;
      return await this.invoke<number>("start_run", {
        options: snapshot,
        confirmed: true,
      });
    } finally {
      this.starting = false;
    }
  }
}
