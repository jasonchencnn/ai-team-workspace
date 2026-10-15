// M3-04：活动与审计页（teams/:teamId/activity）的端到端用例。
// 真实界面 + 测试桥接 + 真实内核 + 临时工作空间；数据准备走桥接，被测的筛选与跳转用页面操作。
// 文件名 z- 开头，排在其他用例之后。
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { type Page, expect, test } from "@playwright/test";

import type { TaskRef } from "./z-qa-r2-helpers.js";
import { createTaskApi, openTask, setupTeam } from "./z-qa-r2-helpers.js";

const ROLE_FE = "前端开发";
const ROLE_BE = "后端开发";
const T_FE = "登录页改造";
const T_BE = "审计接口";

let workspace = "";
let teamId = "";
let roleFe = "";
let roleBe = "";
let taskFe: TaskRef;
let taskBe: TaskRef;

test.describe.configure({ mode: "serial", timeout: 180_000 });

test.beforeAll(() => {
  workspace = mkdtempSync(join(tmpdir(), "teamspace-e2e-m3-act-"));
  writeFileSync(join(workspace, "spec.md"), "# 需求说明\n");
});

test.afterAll(() => {
  rmSync(workspace, { recursive: true, force: true });
});

/** 打开活动与审计页；只改 hash 是同文档导航，重新加载才能保证拿到内核当前的返回。 */
async function gotoActivity(page: Page, query = ""): Promise<void> {
  await page.goto(`/#/teams/${teamId}/activity${query}`);
  await page.reload();
  await expect(page.getByRole("heading", { level: 1, name: "活动与审计" })).toBeVisible();
}

/** 按下拉框的显示文字选择一项。 */
async function choose(page: Page, label: string, optionLabel: string): Promise<void> {
  await page.getByLabel(label, { exact: true }).selectOption({ label: optionLabel });
}

/** 记录行的摘要文字。 */
function summary(page: Page, text: string | RegExp) {
  return page.locator(".act-event__summary", { hasText: text });
}

test("ACT-001 建团队、两个岗位各建一个任务并改一次状态", async ({ page }) => {
  const team = await setupTeam("活动审计组", workspace, [
    { name: ROLE_FE, prefix: "R1" },
    { name: ROLE_BE, prefix: "R2" },
  ]);
  teamId = team.teamId;
  roleFe = team.roleIds[ROLE_FE] ?? "";
  roleBe = team.roleIds[ROLE_BE] ?? "";
  expect(roleFe).not.toBe("");
  expect(roleBe).not.toBe("");

  // 两个任务都先是 TODO，只改前端那个的状态，这样两类记录都能被筛出来。
  taskFe = await createTaskApi({ teamId, roleId: roleFe, title: T_FE, status: "todo" });
  taskBe = await createTaskApi({ teamId, roleId: roleBe, title: T_BE, status: "todo" });

  await openTask(page, taskFe);
  await page.getByRole("button", { name: "READY", exact: true }).click();
  await expect(page.locator(".task-detail__status")).toContainText("READY");
  await expect(page.locator(".task-detail__events")).toContainText("状态 TODO → READY");
});

test("ACT-002 不带筛选时列出两个任务的记录，时间倒序", async ({ page }) => {
  await gotoActivity(page);
  await expect(summary(page, `创建任务 ${taskFe.key}`)).toHaveCount(1);
  await expect(summary(page, `创建任务 ${taskBe.key}`)).toHaveCount(1);
  await expect(summary(page, `${taskFe.key} 状态 TODO → READY`)).toHaveCount(1);
  // 时间倒序：最早的是建团队，最新的是刚才那次状态变化。
  await expect(page.locator(".act-event").first()).toContainText("状态 TODO → READY");
  // 侧边栏入口在，且高亮的是活动与审计。
  await expect(page.locator(".sidebar__nav .nav-item--active")).toHaveText("活动与审计");
});

test("ACT-003 按任务筛选，只剩该任务的记录，且筛选写进地址栏", async ({ page }) => {
  await gotoActivity(page);
  await choose(page, "任务", `${taskFe.key} ${T_FE}`);

  await expect(summary(page, `创建任务 ${taskFe.key}`)).toHaveCount(1);
  await expect(summary(page, `${taskFe.key} 状态 TODO → READY`)).toHaveCount(1);
  await expect(summary(page, `创建任务 ${taskBe.key}`)).toHaveCount(0);
  await expect(page).toHaveURL(new RegExp(`task=${taskFe.id}`));
  // 刷新后筛选仍在。
  await page.reload();
  await expect(page.getByLabel("任务", { exact: true })).toHaveValue(taskFe.id);
  await expect(summary(page, `创建任务 ${taskBe.key}`)).toHaveCount(0);
});

test("ACT-004 按岗位筛选，只剩归属该岗位的记录", async ({ page }) => {
  await gotoActivity(page);
  await choose(page, "岗位", ROLE_FE);

  await expect(summary(page, `创建任务 ${taskFe.key}`)).toHaveCount(1);
  await expect(summary(page, `创建任务 ${taskBe.key}`)).toHaveCount(0);
  await expect(page).toHaveURL(new RegExp(`role=${roleFe}`));

  // 换一个岗位，看到的是另一个任务。
  await choose(page, "岗位", ROLE_BE);
  await expect(summary(page, `创建任务 ${taskBe.key}`)).toHaveCount(1);
  await expect(summary(page, `创建任务 ${taskFe.key}`)).toHaveCount(0);
});

test("ACT-005 任务与岗位同时筛选时取交集", async ({ page }) => {
  await gotoActivity(page, `?task=${taskFe.id}&role=${roleFe}`);
  await expect(summary(page, `${taskFe.key} 状态 TODO → READY`)).toHaveCount(1);
  await expect(summary(page, `创建任务 ${taskBe.key}`)).toHaveCount(0);

  // 换成不匹配的岗位：交集为空。
  await gotoActivity(page, `?task=${taskFe.id}&role=${roleBe}`);
  await expect(page.locator(".act-event")).toHaveCount(0);
  await expect(page.getByText("当前筛选下没有活动记录")).toBeVisible();
});

test("ACT-006 按活动种类筛选", async ({ page }) => {
  await gotoActivity(page);
  await choose(page, "活动种类", "任务状态变化");
  await expect(page.locator(".act-event")).toHaveCount(1);
  await expect(summary(page, `${taskFe.key} 状态 TODO → READY`)).toHaveCount(1);

  await choose(page, "活动种类", "创建任务");
  await expect(page.locator(".act-event")).toHaveCount(2);
  await expect(summary(page, `${taskFe.key} 状态 TODO → READY`)).toHaveCount(0);

  // 清除筛选回到全部。
  await page.getByRole("button", { name: "清除筛选" }).click();
  await expect(page).not.toHaveURL(/kind=/);
  await expect(summary(page, `${taskFe.key} 状态 TODO → READY`)).toHaveCount(1);
});

test("ACT-007 记录里的关联任务可跳到任务详情，关联岗位可跳到岗位工作台", async ({ page }) => {
  await gotoActivity(page, `?task=${taskFe.id}`);
  await page
    .locator(".act-event__link", { hasText: `关联任务 ${taskFe.key}` })
    .first()
    .click();
  await expect(page.getByRole("heading", { level: 1, name: T_FE })).toBeVisible();

  await gotoActivity(page, `?role=${roleBe}`);
  await page
    .locator(".act-event__link", { hasText: `关联岗位 ${ROLE_BE}` })
    .first()
    .click();
  await expect(page.getByRole("heading", { level: 1, name: ROLE_BE })).toBeVisible();
});

test("ACT-008 时间范围为今天时包含刚写入的记录", async ({ page }) => {
  await gotoActivity(page);
  await choose(page, "时间范围", "今天");
  await expect(summary(page, `${taskFe.key} 状态 TODO → READY`)).toHaveCount(1);
  await expect(page).toHaveURL(/range=today/);

  await gotoActivity(page);
  await choose(page, "时间范围", "全部");
  await expect(page).not.toHaveURL(/range=/);
  await expect(summary(page, `${taskFe.key} 状态 TODO → READY`)).toHaveCount(1);
});

test("ACT-009 任务详情的「查看全部活动」带着任务筛选跳到本页", async ({ page }) => {
  await openTask(page, taskFe);
  await page.getByRole("link", { name: "查看全部活动" }).click();
  await expect(page.getByRole("heading", { level: 1, name: "活动与审计" })).toBeVisible();
  await expect(page).toHaveURL(new RegExp(`task=${taskFe.id}`));
  await expect(page.getByLabel("任务", { exact: true })).toHaveValue(taskFe.id);
  await expect(summary(page, `创建任务 ${taskBe.key}`)).toHaveCount(0);
});
