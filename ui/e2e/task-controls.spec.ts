import { expect, test, type Page } from "@playwright/test";

async function setup(page: Page, warnings: string[] = []) {
  await page.addInitScript(({ warnings }) => {
    const options = {
      inputPaths: ["video.mp4"], outputDirectory: null, selfDeath: false,
      ownKill: false, gameLanguage: "en", secondsBefore: 7, secondsAfter: 2,
      minimumEventSeconds: 0, molotovSecondsBefore: 12, scanMode: "fast",
      jobs: "auto", trimJobs: "auto", recursive: false, merge: true,
      scanOnly: false, overwrite: false, brightnessGate: false, moveOriginals: false,
    };
    const shell = {
      state: {
        runId: 0, running: false, cancelling: false, exitCode: null,
        outcome: null, summary: null, phases: {}, activePhase: null,
        included: 0, skipped: 0, error: null,
      },
      calls: [] as string[], warnings, cancelFails: false, startDelayed: false,
      validationError: "", cliReady: true,
    };
    Object.assign(window, {
      testShell: shell,
      __TAURI_INTERNALS__: {
        transformCallback: () => 1,
        invoke: async (command: string) => {
          shell.calls.push(command);
          switch (command) {
            case "get_bootstrap": return {
              defaults: options, options, cliReady: shell.cliReady, cliPath: "cli.exe",
              dataDir: "data", schema: { gameLanguage: [], scanMode: [], trimJobs: [] },
            };
            case "validate_options":
              if (shell.validationError) throw new Error(shell.validationError);
              return { preview: "cli video.mp4", arguments: [], warnings: shell.warnings };
            case "get_run_state": return structuredClone(shell.state);
            case "get_logs": return ["test log\n".repeat(500)];
            case "start_run":
              while (shell.startDelayed) await new Promise((resolve) => setTimeout(resolve, 20));
              shell.state.runId++;
              shell.state.running = true;
              return shell.state.runId;
            case "cancel_run":
              if (shell.cancelFails) {
                shell.state.outcome = "cleanup-failed";
                shell.state.error = "无法清理子进程";
                shell.state.cancelling = false;
                throw new Error("无法清理子进程");
              }
              shell.state.cancelling = true;
              return;
            case "plugin:event|listen": return 1;
            case "plugin:event|unlisten": return;
            case "open_output": return;
            default: throw new Error(`Unexpected command: ${command}`);
          }
        },
      },
    });
  }, { warnings });
  await page.goto("/");
  await expect(startButton(page)).toBeEnabled();
}

const startButton = (page: Page) => page.getByRole("button", { name: "开始处理", exact: true });
const controls = (page: Page) => page.getByRole("region", { name: "任务控制" });
async function updateShell(page: Page, patch: Record<string, unknown>, state = true) {
  await page.evaluate(({ patch, state }) => {
    Object.assign(state ? (window as any).testShell.state : (window as any).testShell, patch);
  }, { patch, state });
}
async function calls(page: Page, command: string) {
  return page.evaluate((command) => (window as any).testShell.calls.filter((c: string) => c === command).length, command);
}

// The mock represents the existing public Tauri IPC, not component internals.
test("control area precedes configuration and keeps start and stop independent", async ({ page }) => {
  await setup(page);
  await expect(controls(page)).toBeVisible();
  expect(await controls(page).evaluate((el) => !!(el.compareDocumentPosition(document.querySelector("aside")!) & Node.DOCUMENT_POSITION_FOLLOWING))).toBe(true);
  await updateShell(page, { startDelayed: true }, false);
  await startButton(page).dblclick({ delay: 20 });
  await expect(controls(page)).toContainText("正在校验并准备启动");
  await expect(startButton(page)).toBeDisabled();
  await updateShell(page, { startDelayed: false }, false);
  await expect(controls(page).getByRole("button", { name: "停止任务", exact: true })).toBeEnabled();
  expect(await calls(page, "cancel_run")).toBe(0);
  expect(await calls(page, "start_run")).toBe(1);
  await expect(page.locator("#inputs")).toBeDisabled();
});

test("declining dangerous-operation confirmation restores start without launching", async ({ page }) => {
  await setup(page, ["归档会移走源视频"]);
  await startButton(page).click();
  const dialog = page.getByRole("alertdialog");
  await expect(dialog).toBeVisible();
  await expect(controls(page)).toContainText("等待启动确认");
  await expect(page.locator("#inputs")).toBeDisabled();
  await expect(dialog.getByRole("button", { name: "取消", exact: true })).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(dialog).not.toBeVisible();
  await expect(startButton(page)).toBeEnabled();
  expect(await calls(page, "start_run")).toBe(0);
  await expect(startButton(page)).toBeFocused();
});

test("accepting confirmation restores focus to the stable start control while preparing", async ({ page }) => {
  await setup(page, ["覆盖已有输出"]);
  await updateShell(page, { startDelayed: true }, false);
  await startButton(page).click();
  const dialog = page.getByRole("alertdialog");
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "确认", exact: true }).click();
  await expect(dialog).not.toBeVisible();
  await expect(startButton(page)).toBeDisabled();
  await expect(startButton(page)).toBeFocused();
  // A keyboard user can inspect the disabled reason without falling back to
  // document/body focus, which makes screen readers reread the whole page.
  await page.keyboard.press("Enter");
  expect(await calls(page, "start_run")).toBe(1);
  await updateShell(page, { startDelayed: false }, false);
});

test("stop waits for backend completion; cleanup failure locks start and allows retry", async ({ page }) => {
  await setup(page);
  await startButton(page).click();
  const stop = controls(page).getByRole("button", { name: "停止任务", exact: true });
  await expect(stop).toBeEnabled();
  await stop.click();
  await expect(stop).toBeDisabled();
  await expect(stop).toBeFocused();
  await expect(controls(page)).toContainText("等待后端确认");
  await expect(startButton(page)).toBeDisabled();
  // aria-disabled keeps focus without permitting another cancellation request.
  await page.keyboard.press("Enter");
  expect(await calls(page, "cancel_run")).toBe(1);
  // Polling while cancellation is pending must not unlock or move focus.
  await page.waitForTimeout(1100);
  await expect(stop).toBeDisabled();
  await expect(stop).toBeFocused();
  await expect(startButton(page)).toBeDisabled();
  await updateShell(page, { outcome: "cleanup-failed", cancelling: false, error: "无法清理子进程" });
  const retry = controls(page).getByRole("button", { name: "重试停止" });
  await expect(retry).toBeEnabled();
  await expect(startButton(page)).toBeDisabled();
  await expect(page.locator("#inputs")).toBeDisabled();
  await updateShell(page, { cancelFails: true }, false);
  await retry.click();
  await expect(retry).toBeEnabled();
  await expect(page.getByRole("alert")).toContainText("无法清理子进程");
  await updateShell(page, { cancelFails: false }, false);
  await retry.click();
  await expect(startButton(page)).toBeDisabled();
  await updateShell(page, { running: false, cancelling: false, outcome: "cancelled", error: null });
  await expect(startButton(page)).toBeEnabled();
  await expect(controls(page)).toContainText("任务已取消");
  await expect(page.locator("#inputs")).toBeEnabled();
  expect(await calls(page, "cancel_run")).toBe(3);
});

test("phase progress is truthful, final outcomes are specific and polling does not steal focus or repeat live announcements", async ({ page }) => {
  await setup(page);
  await startButton(page).click();
  await updateShell(page, { activePhase: "scan", phases: { scan: { current: 2, total: 0, event: "start", workers: null, reason: null, planned: null } } });
  await expect(controls(page)).toContainText("OCR 扫描");
  await expect(controls(page)).toContainText("进度未知");
  await expect(controls(page).locator("progress")).not.toHaveAttribute("value");
  const stop = controls(page).getByRole("button", { name: "停止任务", exact: true });
  await stop.focus();
  await page.evaluate(() => {
    (window as any).announcements = 0;
    new MutationObserver(() => (window as any).announcements++).observe(document.querySelector('[role="status"]')!, { childList: true, characterData: true, subtree: true });
  });
  await updateShell(page, { phases: { scan: { current: 3, total: 10, event: "progress", workers: null, reason: null, planned: null } } });
  await expect(controls(page)).toContainText("3 / 10");
  await page.waitForTimeout(1100);
  await expect(stop).toBeFocused();
  expect(await page.evaluate(() => (window as any).announcements)).toBe(0);
  await updateShell(page, { running: false, outcome: "partial-unreadable", summary: { source_count: 3 }, exitCode: 2 });
  await expect(controls(page)).toContainText("已完成，部分视频不可读");
  const output = page.getByRole("button", { name: "打开输出" });
  await expect(output).toBeEnabled();
  expect(await output.evaluate((el) => !el.closest('.task-controls'))).toBe(true);
});

for (const viewport of [
  { name: "desktop", width: 1280, height: 800 },
  { name: "short", width: 1024, height: 480 },
  { name: "narrow", width: 480, height: 700 },
  // 1280×800 at 200% browser zoom has a 640×400 CSS layout viewport.
  { name: "200-percent-reflow", width: 640, height: 400 },
  { name: "small", width: 320, height: 568 },
]) {
  test(`${viewport.name}: controls remain reachable during config/log scrolling and keyboard focus is unobscured`, async ({ page }) => {
    await page.setViewportSize(viewport);
    await setup(page, ["覆盖已有输出"]);
    const box = await controls(page).boundingBox();
    expect(box!.y).toBeGreaterThanOrEqual(0);
    expect(box!.y + box!.height).toBeLessThan(viewport.height);
    await page.getByRole("button", { name: "显示日志" }).click();
    await page.locator("aside, .results, .workspace-content, .logs").evaluateAll((els) => els.forEach((el) => el.scrollTop = el.scrollHeight));
    expect(await controls(page).boundingBox()).toEqual(box);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    await startButton(page).focus();
    await expect(startButton(page)).toBeInViewport({ ratio: 1 });
    await page.keyboard.press("Enter");
    const dialog = page.getByRole("alertdialog");
    await expect(dialog).toBeVisible();
    await expect(dialog.getByRole("button", { name: "取消", exact: true })).toBeFocused();
    await page.keyboard.press("Tab");
    await expect(dialog.getByRole("button", { name: "确认", exact: true })).toBeFocused();
    await page.keyboard.press("Enter");
    const stop = controls(page).getByRole("button", { name: "停止任务", exact: true });
    await expect(stop).toBeEnabled();
    await stop.focus();
    await expect(stop).toBeInViewport({ ratio: 1 });
    await page.keyboard.press("Tab");
    await expect(page.getByRole("button", { name: "打开输出" })).not.toBeFocused();
    // The next enabled control is in the result area; scrollIntoView must not
    // move the fixed control row over it.
    const focusedBox = await page.locator(":focus").boundingBox();
    expect(focusedBox!.y).toBeGreaterThanOrEqual(box!.y + box!.height);
  });
}
