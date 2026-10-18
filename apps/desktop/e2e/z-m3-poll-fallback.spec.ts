// M3-12（D23）：真实应用里任务详情不随执行状态实时更新——根因是 capability 没放行事件权限，
// 前端的 listen 被 Tauri 拒绝。用例在这里复现同一条件：页面加载前把模拟事件通道关掉，
// 让 plugin:event|listen 像被权限拒绝一样失败，只剩轮询兜底。
// 断言：不离开页面，状态从「准备中 / 执行中」走到「待验收」，心跳指示出现并在执行结束后消失。
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { type Page, expect, test } from "@playwright/test";

import { createMockHarness } from "./harnessHelpers.js";

const artifacts = fileURLToPath(new URL("../e2e-artifacts/", import.meta.url));
mkdirSync(artifacts, { recursive: true });

const TEAM = "轮询兜底团队";
const ROLE = "兜底执行员";
const HARNESS = "m3-fallback-mock";
// [mock:perm]：Agent 会请求一次执行命令的权限并等确认，执行在等待期间保持活动状态，
// 足够让心跳指示稳定出现；确认后正常完成，任务进入待验收。
const TITLE = "没有实时通道也要跟上状态 [mock:perm]";
const TIMEOUT = 30_000;

let workspace = "";
let team = "";
let taskUrl = "";

test.describe.configure({ mode: "serial", timeout: 120_000 });

/** 关掉模拟事件通道：订阅会被拒绝，界面只能靠定时刷新。 */
async function blockRealtimeChannel(page: Page): Promise<void> {
  await page.addInitScript(() => {
    (window as unknown as Record<string, unknown>).__teamspace_no_realtime = true;
  });
}

async function addRole(page: Page, name: string, description: string): Promise<void> {
  await page.getByRole("button", { name: "+ 添加岗位" }).click();
  const card = page.locator(".tc-role").last();
  await card.getByLabel("岗位名称").fill(name);
  await card.getByLabel("岗位名称").blur();
  await expect(card.getByLabel("任务编号前缀")).toHaveValue(/^[A-Z][A-Z0-9]{1,3}$/);
  await card.getByLabel("职责").fill(description);
}

test.beforeAll(() => {
  workspace = mkdtempSync(join(tmpdir(), "teamspace-e2e-m3-fallback-"));
  writeFileSync(join(workspace, "brief.txt"), "需求说明\n");
});

test.afterAll(() => {
  rmSync(workspace, { recursive: true, force: true });
});

test("准备：关掉模拟事件通道后，建团队、接 Mock Harness、派一个 READY 任务", async ({ page }) => {
  await blockRealtimeChannel(page);

  await page.goto("/#/teams/new");
  await page.getByLabel("团队名称").fill(TEAM);
  await page.getByLabel("手动输入工作空间路径").fill(workspace);
  await page.getByRole("button", { name: "检测", exact: true }).click();
  await expect(page.getByText(/普通文件夹|Git 仓库/).first()).toBeVisible();
  await addRole(page, ROLE, "负责验证轮询兜底");
  await page.getByRole("button", { name: "创建团队", exact: true }).click();
  await expect(page).toHaveURL(/\/teams\/(?!new$)[^/]+$/);
  team = /\/teams\/([^/?]+)/.exec(page.url())?.[1] ?? "";
  expect(team).not.toBe("");

  await page.goto(`/#/teams/${team}/roles?tab=harness`);
  await createMockHarness(page, HARNESS);
  await page.goto(`/#/teams/${team}/roles`);
  await page.locator(".role-chip", { hasText: ROLE }).click();
  await page.getByRole("button", { name: "绑定 Harness", exact: true }).click();
  const select = page.getByLabel("选择本团队的 Harness 实例");
  await select.selectOption(
    (await select.locator("option", { hasText: HARNESS }).getAttribute("value")) ?? "",
  );
  await page.getByRole("button", { name: "确认绑定" }).click();
  await expect(page.getByText(`当前绑定 ${HARNESS}`)).toBeVisible();

  await page.goto(`/#/teams/${team}/tasks/new`);
  await page.getByLabel("任务名称").fill(TITLE);
  const owner = page.getByLabel("负责人岗位");
  await owner.selectOption(
    (await owner.locator("option", { hasText: ROLE }).getAttribute("value")) ?? "",
  );
  await page.getByRole("radio", { name: "READY", exact: true }).check();
  await page.getByRole("button", { name: "创建并分配" }).click();
  await expect(page.locator(".task-detail__status")).toContainText("READY");
  taskUrl = page.url();
});

test("事件通道被拒：开始执行后不离开页面，状态走到待验收，心跳指示出现并消失", async ({ page }) => {
  await blockRealtimeChannel(page);
  await page.goto(taskUrl);
  await expect(page.getByRole("heading", { level: 1, name: TITLE })).toBeVisible();

  const heartbeat = page.getByTestId("run-heartbeat");
  const statusLine = page.locator(".task-detail__status");

  await page.getByRole("button", { name: /^(开始执行|重新执行)$/ }).click();

  // 权限弹窗由 run.pendingPermission 驱动——事件通道被拒时它也只能靠轮询出现
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible({ timeout: TIMEOUT });
  await expect(dialog).toContainText(`${ROLE} 请求权限`);

  // 先关掉弹窗：等确认期间执行区自己就写着「有一个权限请求在等你确认」
  await dialog.getByRole("button", { name: "稍后处理" }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.locator(".run-pending")).toContainText("有一个权限请求在等你确认");

  // 心跳指示：监控中 + 已运行时长 + 最近更新
  await expect(heartbeat).toBeVisible({ timeout: TIMEOUT });
  await expect(page.getByTestId("run-heartbeat-live")).toContainText("监控中");
  await expect(page.getByTestId("run-heartbeat-clock")).toContainText(/已运行 \d+:\d\d/);
  await expect(page.getByTestId("run-heartbeat-since")).toContainText("最近更新");
  // 订阅失败不再静默：界面说明已改为定时刷新
  await expect(page.getByTestId("run-heartbeat-fallback")).toHaveText(
    "实时通道不可用，已改为定时刷新",
  );
  // 等待确认时任务状态就在详情页上，不靠回到任务面板再进来
  await expect(statusLine).toContainText("执行中");
  await page.screenshot({ path: `${artifacts}m3-fallback-heartbeat.png`, fullPage: false });

  // 不做任何导航，重新打开权限请求并确认，等轮询把状态带到「待验收」
  await page.getByRole("button", { name: "重新打开权限请求" }).click();
  const reopened = page.getByRole("dialog");
  await expect(reopened).toBeVisible();
  await reopened.getByRole("button", { name: "允许一次" }).click();
  await expect(statusLine).toContainText("待验收", { timeout: TIMEOUT });
  const success = page.locator(".run-outcome--success");
  await expect(success).toBeVisible({ timeout: TIMEOUT });
  await expect(success).toContainText("已完成 · 待验收");
  // 进度也是轮询补齐的：过程文件出现在改动文件里
  await expect(page.locator(".run-side__panel", { hasText: "本次改动文件" })).toContainText(
    "mock-",
  );

  // 执行结束后心跳指示随之消失
  await expect(heartbeat).toHaveCount(0, { timeout: TIMEOUT });
  await page.screenshot({ path: `${artifacts}m3-fallback-review.png`, fullPage: false });
});
