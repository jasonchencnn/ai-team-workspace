// M7-03：1 万条量级下的常用页面性能（端到端）。
// 真实界面 + 测试桥接 + 真实内核；数据由桥接的测试专用命令 `__seed_tasks` 直接写库造，
// 被测的首屏时间与「加载更多」用页面操作断言。
// 文件名 z- 开头，排在其他用例之后。
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { type Page, expect, test } from "@playwright/test";

import { invoke, setupTeam } from "./z-qa-r2-helpers.js";

/** 造的团队里有多少条任务。 */
const TASK_COUNT = 2000;

/** 首屏预算（毫秒）。 */
const FIRST_SCREEN_BUDGET = 3000;

/** 每列一次渲染多少条任务。 */
const COLUMN_PAGE = 200;

let workspace = "";
let teamId = "";

test.describe.configure({ mode: "serial", timeout: 180_000 });

test.beforeAll(() => {
  workspace = mkdtempSync(join(tmpdir(), "teamspace-e2e-m7-perf-"));
  writeFileSync(join(workspace, "spec.md"), "# 需求说明\n");
});

test.afterAll(() => {
  rmSync(workspace, { recursive: true, force: true });
});

/** 打开任务面板并等看板画出来。 */
async function gotoBoard(page: Page): Promise<void> {
  await page.goto(`/#/teams/${teamId}/tasks`);
  await page.reload();
  await expect(page.getByRole("heading", { level: 1, name: "任务面板" })).toBeVisible();
}

/** 一列里的任务卡数量。 */
async function cardCount(column: ReturnType<Page["locator"]>): Promise<number> {
  return await column.locator(".task-card-link").count();
}

test("PERF-001 造 2000 条任务，看板首屏在预算内出现，每列只渲染前 200 条", async ({ page }) => {
  const team = await setupTeam("性能组", workspace, [
    { name: "前端开发", prefix: "P1" },
    { name: "后端开发", prefix: "P2" },
  ]);
  teamId = team.teamId;

  const seeded = await invoke<{ taskCount: number; eventCount: number }>("__seed_tasks", {
    teamId,
    taskCount: TASK_COUNT,
  });
  expect(seeded.taskCount).toBe(TASK_COUNT);
  expect(seeded.eventCount).toBeGreaterThan(TASK_COUNT);

  // 预热一次：前端模块与内核缓存都热了，后面量的是数据量带来的耗时。
  await gotoBoard(page);
  await expect(page.locator(".task-board__column").first()).toBeVisible();

  const started = Date.now();
  await page.reload();
  await page.locator(".task-board__column").first().waitFor({ state: "visible" });
  await page.locator(".task-card-link").first().waitFor({ state: "visible" });
  const elapsed = Date.now() - started;
  expect(elapsed).toBeLessThan(FIRST_SCREEN_BUDGET);

  // 每列最多渲染一页，不是一口气渲染两千张卡片。
  const columns = page.locator(".task-board__column");
  const counts = await columns.evaluateAll((elements) =>
    elements.map((element) => element.querySelectorAll(".task-card-link").length),
  );
  expect(counts.length).toBeGreaterThan(0);
  for (const count of counts) {
    expect(count).toBeLessThanOrEqual(COLUMN_PAGE);
  }
  const rendered = counts.reduce((sum, count) => sum + count, 0);
  expect(rendered).toBeLessThan(TASK_COUNT);
});

test("PERF-002 看板每列能「加载更多」，加载后卡片变多且总数不变", async ({ page }) => {
  await gotoBoard(page);
  const column = page.locator(".task-board__column").first();
  await column.locator(".task-card-link").first().waitFor({ state: "visible" });
  const before = await cardCount(column);
  expect(before).toBe(COLUMN_PAGE);

  // TODO 列合并 todo 与 ready，任务更多，一定有「加载更多」。
  const more = column.getByRole("button", { name: "加载更多" });
  await expect(more).toBeVisible();
  await expect(column.locator(".task-board__more-note")).toContainText("已显示 200 /");

  await more.click();
  // 加载一页后多出一页的卡片。
  await expect(column.locator(".task-card-link")).toHaveCount(before + COLUMN_PAGE);
  await expect(column.locator(".task-board__more-note")).toContainText(
    `已显示 ${String(before + COLUMN_PAGE)} /`,
  );

  // 加载更多不会重复卡片：编号仍然唯一。
  const keys = await column.locator(".task-card__name").allInnerTexts();
  expect(new Set(keys).size).toBe(keys.length);
});

test("PERF-004 TODO 合并列：后端翻完后本地还有没显示的，「加载更多」仍在，能一路展开到全部", async ({
  page,
}) => {
  await gotoBoard(page);
  const column = page.locator(".task-board__column").first();
  await column.locator(".task-card-link").first().waitFor({ state: "visible" });
  const more = column.getByRole("button", { name: "加载更多" });
  await expect(more).toBeVisible();

  // TODO 列合并 todo(400) 与 ready(300)：第一次「加载更多」把两边都取完，
  // 后端已经没有更多，但本地还有 300 条没展开显示。
  await more.click();
  await expect(column.locator(".task-card-link")).toHaveCount(400);
  await expect(column.locator(".task-board__more-note")).toContainText("已显示 400 / 700");

  // 关键：后端没有更多了，按钮不能消失——剩下的任务还在本地，只是还没展开。
  await expect(more).toBeVisible();
  await more.click();
  await expect(column.locator(".task-card-link")).toHaveCount(600);
  await expect(more).toBeVisible();
  await more.click();
  await expect(column.locator(".task-card-link")).toHaveCount(700);

  // 全部展开后按钮才消失。
  await expect(more).toHaveCount(0);
});

test("PERF-003 活动页在 2000 条任务、几千条活动下首屏在预算内出现", async ({ page }) => {
  await page.goto(`/#/teams/${teamId}/activity`);
  await page.reload();
  await expect(page.getByRole("heading", { level: 1, name: "活动与审计" })).toBeVisible();

  const started = Date.now();
  await page.reload();
  await page.locator(".act-table tbody tr").first().waitFor({ state: "visible" });
  const elapsed = Date.now() - started;
  expect(elapsed).toBeLessThan(FIRST_SCREEN_BUDGET);

  // 每页 100 条。
  await expect(page.locator(".act-table tbody tr")).toHaveCount(100);
  await expect(page.getByText("已显示 100 条，按时间从新到旧")).toBeVisible();
});
