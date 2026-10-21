// M4-04 使用记录的端到端：成功但没有任何改动的提示与二次确认、人工接管的「已被接管」文案。
// 真实界面 + 测试桥接 + 真实内核；Harness 是内置 Mock ACP Agent，工作空间是临时普通文件夹。
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { type Page, expect, test } from "@playwright/test";

import { createMockHarness } from "./harnessHelpers.js";

const TIMEOUT = 25_000;
const HARNESS = "m404-mock";
const ROLE = "M404员";

let plainWs = "";
let team = "";

interface TaskRef {
  id: string;
  key: string;
  title: string;
  url: string;
}

test.describe.configure({ mode: "serial", timeout: 90_000 });

test.beforeAll(() => {
  plainWs = mkdtempSync(join(tmpdir(), "teamspace-e2e-m404-plain-"));
  writeFileSync(join(plainWs, "brief.txt"), "需求说明\n");
});

test.afterAll(() => {
  rmSync(plainWs, { recursive: true, force: true });
});

async function createTeam(page: Page, name: string, workspace: string, role: string) {
  await page.goto("/#/teams/new");
  await page.getByLabel("团队名称").fill(name);
  await page.getByLabel("手动输入工作空间路径").fill(workspace);
  await page.getByRole("button", { name: "检测", exact: true }).click();
  await expect(page.getByText(/普通文件夹|Git 仓库/).first()).toBeVisible();
  await page.getByRole("button", { name: "+ 添加岗位" }).click();
  const card = page.locator(".tc-role").last();
  await card.getByLabel("岗位名称").fill(role);
  await card.getByLabel("岗位名称").blur();
  await expect(card.getByLabel("任务编号前缀")).toHaveValue(/^[A-Z][A-Z0-9]{1,3}$/);
  await card.getByLabel("职责").fill(`${role}的职责`);
  await page.getByRole("button", { name: "创建团队", exact: true }).click();
  await expect(page).toHaveURL(/\/teams\/(?!new$)[^/]+$/);
  return /\/teams\/([^/?]+)/.exec(page.url())?.[1] ?? "";
}

async function bindRole(page: Page, teamId: string, role: string, harness: string) {
  await page.goto(`/#/teams/${teamId}/roles`);
  await page.locator(".role-chip", { hasText: role }).click();
  await page.getByRole("button", { name: "绑定 Harness", exact: true }).click();
  const select = page.getByLabel("选择本团队的 Harness 实例");
  await select.selectOption(
    (await select.locator("option", { hasText: harness }).getAttribute("value")) ?? "",
  );
  await page.getByRole("button", { name: "确认绑定" }).click();
  await expect(page.getByText(`当前绑定 ${harness}`)).toBeVisible();
}

async function createTask(
  page: Page,
  teamId: string,
  title: string,
  owner: string,
): Promise<TaskRef> {
  await page.goto(`/#/teams/${teamId}/tasks/new`);
  await page.getByLabel("任务名称").fill(title);
  const select = page.getByLabel("负责人岗位");
  await select.selectOption(
    (await select.locator("option", { hasText: owner }).getAttribute("value")) ?? "",
  );
  await page.locator("#task-file-search").fill("brief");
  await page.locator(".task-form__suggestion", { hasText: "brief.txt" }).click();
  await page.getByRole("radio", { name: "READY", exact: true }).check();
  await page.getByRole("button", { name: "创建并分配" }).click();
  await expect(page.locator(".task-detail__status")).toContainText("READY");
  const key = /\/ (\S+)$/.exec(
    (await page.locator(".task-detail__status").innerText()).trim(),
  )?.[1];
  const url = page.url();
  return { url, id: url.split("/").pop() ?? "", key: key ?? "", title };
}

/** 开始执行并等到「待验收」。 */
async function runToReview(page: Page): Promise<void> {
  await page
    .getByRole("button", { name: /^(开始执行|重新执行)$/ })
    .first()
    .click();
  await expect(page.locator(".task-detail__status")).toContainText("待验收", {
    timeout: TIMEOUT,
  });
}

test("准备：普通文件夹团队、Mock 接入与岗位绑定", async ({ page }) => {
  team = await createTeam(page, "M404使用记录团队", plainWs, ROLE);
  await page.goto(`/#/teams/${team}/roles?tab=harness`);
  await createMockHarness(page, HARNESS);
  await bindRole(page, team, ROLE, HARNESS);
});

test("使用记录 2：成功但没有任何改动：任务详情与验收页醒目提示，通过需要二次确认", async ({
  page,
}) => {
  const task = await createTask(page, team, "没有改动 [mock:nochange]", ROLE);
  await runToReview(page);

  // 任务详情：醒目的提示，说明这次执行没有产生任何改动。
  const detail = page.locator(".run-outcome--success");
  await expect(detail).toBeVisible({ timeout: TIMEOUT });
  await expect(detail).toContainText("这次执行没有产生任何改动");
  await expect(detail).toContainText("Agent 可能提前结束");

  // 验收页：同样的醒目提示。
  await page
    .getByRole("region", { name: "验收入口" })
    .getByRole("button", { name: "去验收" })
    .click();
  await expect(page).toHaveURL(new RegExp(`/tasks/${task.id}/review$`));
  const warn = page.locator(".review__panel--warning");
  await expect(warn).toContainText("这次执行没有产生任何改动");
  await expect(warn).toContainText("Agent 可能提前结束（例如权限被拒）；建议退回并说明原因");
  await expect(page.locator(".review__panel", { hasText: "改动文件" })).toContainText(
    "本次执行没有改动文件。",
  );

  // 通过：先弹二次确认，确认后才真的通过。
  await page.getByRole("button", { name: "通过", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toContainText("确认通过（这次执行没有任何改动）");
  await expect(dialog).toContainText("Agent 可能提前结束");
  await dialog.getByRole("button", { name: "仍然通过" }).click();
  await expect(page.locator(".review-banner--success")).toBeVisible();
});

test("使用记录 5：人工接管后执行状态显示「已被接管」，送验收后验收页也是同一份文案", async ({
  page,
}) => {
  const task = await createTask(page, team, "接管文案 [mock:hang]", ROLE);
  await page
    .getByRole("button", { name: /^(开始执行|重新执行)$/ })
    .first()
    .click();
  await expect(page.locator(".run-msg--agent").first()).toContainText("正在长时间处理", {
    timeout: TIMEOUT,
  });

  await page.getByRole("button", { name: "人工接管", exact: true }).click();
  await page.getByRole("dialog").getByRole("button", { name: "确认接管" }).click();
  const taken = page.locator(".run-outcome--taken-over");
  await expect(taken).toBeVisible({ timeout: TIMEOUT });
  await expect(taken).toContainText("已由你接管");
  // 执行状态显示「已被接管」，不再显示「已取消」：信息条、历次执行记录与接管后的验收页都是同一份文案。
  const info = page.locator(".run-info");
  await expect(info).toContainText("已被接管");
  await expect(info).not.toContainText("已取消");
  await expect(page.locator(".run-side__panel", { hasText: "历次执行记录" })).toContainText(
    "已被接管",
  );

  // 接管后直接送验收：验收页的执行结果也是「已被接管」。
  await taken.getByRole("button", { name: "直接提交验收" }).click();
  await expect(page.locator(".task-detail__status")).toContainText("待验收");
  await page
    .getByRole("region", { name: "验收入口" })
    .getByRole("button", { name: "去验收" })
    .click();
  await expect(page).toHaveURL(new RegExp(`/tasks/${task.id}/review$`));
  const reviewInfo = page.getByLabel("执行信息");
  await expect(reviewInfo).toContainText("已被接管");
  await expect(reviewInfo).not.toContainText("已取消");
});
