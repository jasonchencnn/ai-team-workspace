import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { expect, test } from "@playwright/test";

import type { TaskDetail } from "../src/bindings/TaskDetail.js";
import {
  bind,
  createTaskApi,
  invoke,
  openTask,
  setupMockHarness,
  setupTeam,
} from "./z-qa-r2-helpers.js";

let workspace = "";

test.beforeEach(() => {
  workspace = mkdtempSync(join(tmpdir(), "teamspace-m3-dependencies-"));
});

test.afterEach(() => {
  rmSync(workspace, { recursive: true, force: true });
});

let scenarioSeq = 0;

async function scenario() {
  scenarioSeq += 1;
  const team = await setupTeam(`依赖与冲突 ${String(scenarioSeq)}`, workspace, [
    { name: "前端", prefix: "FE" },
  ]);
  const roleId = team.roleIds["前端"] ?? "";
  const harness = await setupMockHarness(team.teamId, "依赖 Mock");
  await bind(roleId, harness);
  const first = await createTaskApi({
    teamId: team.teamId,
    roleId,
    title: "A 前置接口",
  });
  const second = await createTaskApi({
    teamId: team.teamId,
    roleId,
    title: "B 联调任务",
    status: "todo",
    dependsOn: [first.id],
  });
  return { teamId: team.teamId, roleId, first, second };
}

test("前置待完成 → Mock 验收通过 → 下游自动 READY → 编辑前置时拒绝环路", async ({ page }) => {
  const { teamId, roleId, first, second } = await scenario();
  const third = await createTaskApi({
    teamId,
    roleId,
    title: "C 下游验收",
    status: "todo",
    dependsOn: [second.id],
  });
  await openTask(page, second);
  const chain = page.getByRole("region", { name: "依赖链", exact: true });
  await expect(chain).toContainText(`${first.key} ${first.title}`);
  await expect(chain).toContainText(`${second.key} ${second.title}`);
  await expect(chain).toContainText(`${third.key} ${third.title}`);
  await expect(chain).toContainText("前置 1 个 · 下游 1 个");
  await expect(page.getByRole("button", { name: "开始执行", exact: true })).toBeDisabled();
  await expect(
    page.getByText(`依赖任务 ${first.key} 尚未完成（READY）`, { exact: true }),
  ).toBeVisible();
  await expect(page.getByText(/前置完成后会自动变为 READY/)).toBeVisible();
  await page.screenshot({ path: "/tmp/m3-07-dependency-detail.png", fullPage: true });

  await page.getByRole("button", { name: "编辑", exact: true }).click();
  const preview = page.getByRole("region", { name: "依赖链预览" });
  await expect(preview).toContainText(first.key);
  await expect(preview).toContainText(third.key);
  await page.screenshot({ path: "/tmp/m3-07-dependency-preview.png", fullPage: true });

  await openTask(page, first);
  await page.getByRole("button", { name: "开始执行", exact: true }).click();
  await expect(page.locator(".task-detail__status")).toContainText("待验收", { timeout: 25_000 });
  await page
    .getByRole("region", { name: "验收入口" })
    .getByRole("button", { name: "去验收" })
    .click();
  await page.getByRole("button", { name: "通过", exact: true }).click();
  await expect(page.locator(".review-banner--success")).toBeVisible();
  await openTask(page, second);
  await page.reload();
  await expect(page.locator(".task-detail__status")).toContainText("READY");
  await expect(chain.getByLabel("前置任务")).toContainText("已完成");
  await expect(page.locator(".task-detail__events")).toContainText("自动就绪");
  await expect(page.getByRole("button", { name: "开始执行", exact: true })).toBeEnabled();

  await page.goto(`/#/teams/${teamId}/tasks/${first.id}/edit`);
  await page.getByRole("checkbox", { name: new RegExp(second.title) }).check();
  const error = page.getByRole("alert");
  await expect(error).toContainText(`依赖形成环路：${first.key} → ${second.key} → ${first.key}`);
  await expect(page.getByRole("button", { name: "保存", exact: true })).toBeDisabled();
  await page.screenshot({ path: "/tmp/m3-07-dependency-cycle.png", fullPage: true });
  const unchanged = await invoke<TaskDetail>("get_task", { taskId: first.id });
  expect(unchanged.task.dependsOn).toEqual([]);
  await page.getByRole("button", { name: new RegExp(`移除依赖 ${second.key}`) }).click();
  await expect(error).toHaveCount(0);
  await expect(page.getByRole("button", { name: "保存", exact: true })).toBeEnabled();

  await openTask(page, first);
  await page.getByRole("button", { name: "READY", exact: true }).click();
  await expect(page.locator(".task-detail__events")).toContainText("未回退");
  await openTask(page, second);
  await page.reload();
  await expect(page.locator(".task-detail__status")).toContainText("READY");
  await expect(page.getByRole("button", { name: "开始执行", exact: true })).toBeDisabled();
});

test("创建表单选择依赖显示预览，保存后详情和直接下游来自真实数据", async ({ page }) => {
  const { teamId, roleId, first } = await scenario();
  await page.goto(`/#/teams/${teamId}/tasks/new`);
  await page.getByLabel("任务名称").fill("表单新建的下游");
  await page.getByLabel("负责人岗位").selectOption(roleId);
  await page.getByRole("checkbox", { name: new RegExp(first.title) }).check();
  const preview = page.getByRole("region", { name: "依赖链预览" });
  await expect(preview).toContainText(first.key);
  await expect(preview).toContainText("表单新建的下游");
  await expect(page.getByText(/前置完成后会自动变为 READY/)).toBeVisible();
  await page.getByRole("button", { name: "创建并分配", exact: true }).click();
  await expect(page.getByRole("heading", { name: "表单新建的下游", exact: true })).toBeVisible();
  const detail = await invoke<TaskDetail>("get_task", {
    taskId: page.url().split("/").pop() ?? "",
  });
  expect(detail.task.dependsOn).toEqual([first.id]);
  await page
    .getByRole("region", { name: "依赖链", exact: true })
    .getByRole("link", { name: new RegExp(first.title) })
    .click();
  await expect(page.getByRole("heading", { name: first.title, exact: true })).toBeVisible();
  await expect(
    page.getByRole("region", { name: "依赖链", exact: true }).getByLabel("直接下游任务"),
  ).toContainText("表单新建的下游");
});

test("预检失败时禁止保存，重试恢复且不丢失表单", async ({ page }) => {
  const { teamId, second } = await scenario();
  let fail = true;
  await page.route("**/invoke", async (route) => {
    const request = route.request().postDataJSON() as { cmd: string };
    if (request.cmd === "check_task_dependencies" && fail) {
      await route.fulfill({ json: { err: { kind: "io", message: "模拟预检不可用" } } });
    } else {
      await route.continue();
    }
  });
  await page.goto(`/#/teams/${teamId}/tasks/${second.id}/edit`);
  await expect(page.getByRole("alert")).toContainText("模拟预检不可用");
  await page.getByLabel("任务名称").fill("仍保留的编辑");
  await expect(page.getByRole("button", { name: "保存", exact: true })).toBeDisabled();
  fail = false;
  await page.getByRole("button", { name: "重试依赖检查" }).click();
  await expect(page.getByRole("alert")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "保存", exact: true })).toBeEnabled();
  await expect(page.getByLabel("任务名称")).toHaveValue("仍保留的编辑");
});

test("检查期间禁用保存，迟到的环路响应不会覆盖新的无环选择", async ({ page }) => {
  const { teamId, first, second } = await scenario();
  let release = () => {};
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.route("**/invoke", async (route) => {
    const request = route.request().postDataJSON() as {
      cmd: string;
      args?: { input?: { dependsOn?: string[] } };
    };
    if (
      request.cmd === "check_task_dependencies" &&
      request.args?.input?.dependsOn?.includes(second.id)
    ) {
      const response = await route.fetch();
      await gate;
      await route.fulfill({ response });
    } else {
      await route.continue();
    }
  });
  await page.goto(`/#/teams/${teamId}/tasks/${first.id}/edit`);
  await expect(page.getByRole("button", { name: "保存", exact: true })).toBeEnabled();
  const cycleRequest = page.waitForRequest((request) => {
    if (!request.url().endsWith("/invoke") || request.method() !== "POST") {
      return false;
    }
    const body = request.postDataJSON() as {
      cmd: string;
      args?: { input?: { dependsOn?: string[] } };
    };
    return (
      body.cmd === "check_task_dependencies" &&
      (body.args?.input?.dependsOn?.includes(second.id) ?? false)
    );
  });
  await page.getByRole("checkbox", { name: new RegExp(second.title) }).check();
  await expect(page.getByRole("status")).toContainText("正在检查依赖");
  await expect(page.getByRole("button", { name: "保存", exact: true })).toBeDisabled();
  await cycleRequest;
  await page.getByRole("button", { name: new RegExp(`移除依赖 ${second.key}`) }).click();
  await expect(page.getByRole("button", { name: "保存", exact: true })).toBeEnabled();
  const lateResponse = page.waitForResponse((response) => {
    if (!response.url().endsWith("/invoke") || response.request().method() !== "POST") {
      return false;
    }
    const body = response.request().postDataJSON() as {
      cmd: string;
      args?: { input?: { dependsOn?: string[] } };
    };
    return (
      body.cmd === "check_task_dependencies" &&
      (body.args?.input?.dependsOn?.includes(second.id) ?? false)
    );
  });
  release();
  await lateResponse;
  await expect(page.getByRole("alert")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "保存", exact: true })).toBeEnabled();
});

test("依赖预检防抖 200ms，快速选择只请求一次且选择顺序不影响缓存", async ({ page }) => {
  const { teamId, roleId, first, second } = await scenario();
  const third = await createTaskApi({ teamId, roleId, title: "C 独立前置", status: "todo" });
  const checked: string[][] = [];
  await page.route("**/invoke", async (route) => {
    const request = route.request().postDataJSON() as {
      cmd: string;
      args?: { input?: { dependsOn?: string[] } };
    };
    if (request.cmd === "check_task_dependencies") {
      checked.push(request.args?.input?.dependsOn ?? []);
    }
    await route.continue();
  });
  await page.goto(`/#/teams/${teamId}/tasks/${second.id}/edit`);
  const save = page.getByRole("button", { name: "保存", exact: true });
  await expect(save).toBeEnabled();
  await expect(
    page.getByRole("button", { name: new RegExp(`移除依赖 ${first.key}`) }),
  ).toBeVisible();
  await page.clock.install();
  await page.clock.pauseAt(new Date(Date.now() + 1_000));
  checked.length = 0;
  await page.getByRole("checkbox", { name: new RegExp(third.title) }).check();
  await expect(save).toBeDisabled();
  await expect(page.getByRole("status")).toContainText("正在检查依赖");
  await page.clock.runFor(100);
  expect(checked).toHaveLength(0);
  await page.getByRole("button", { name: new RegExp(`移除依赖 ${first.key}`) }).click();
  await page.getByRole("checkbox", { name: new RegExp(first.title) }).check();
  await page.clock.runFor(199);
  expect(checked).toHaveLength(0);
  await expect(save).toBeDisabled();
  await page.clock.runFor(1);
  await expect(save).toBeEnabled();
  expect(checked).toEqual([[first.id, third.id].sort()]);
  await page.getByRole("button", { name: new RegExp(`移除依赖 ${first.key}`) }).click();
  await page.getByRole("checkbox", { name: new RegExp(first.title) }).check();
  await page.clock.runFor(200);
  await expect(save).toBeEnabled();
  expect(checked).toHaveLength(1);
});

test("多跳环路展示全部任务编号，移除后可保存", async ({ page }) => {
  const { teamId, roleId, first, second } = await scenario();
  const third = await createTaskApi({
    teamId,
    roleId,
    title: "C 第三级",
    status: "todo",
    dependsOn: [second.id],
  });
  await page.goto(`/#/teams/${teamId}/tasks/${first.id}/edit`);
  await page.getByRole("checkbox", { name: new RegExp(third.title) }).check();
  await expect(page.getByRole("alert")).toContainText(
    `依赖形成环路：${first.key} → ${third.key} → ${second.key} → ${first.key}`,
  );
  await expect(page.getByRole("button", { name: "保存", exact: true })).toBeDisabled();
  await page.getByRole("button", { name: new RegExp(`移除依赖 ${third.key}`) }).click();
  await page.getByRole("button", { name: "保存", exact: true }).click();
  await expect(page.getByRole("heading", { name: first.title, exact: true })).toBeVisible();
});
