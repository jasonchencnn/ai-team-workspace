// M5-04 端到端：待我处理、决策记录、通知中心与通知订阅。真实界面 + 测试桥接 + 真实内核；
// Harness 是内置 Mock ACP Agent，工作空间是临时 Git 仓库。覆盖：执行到待验收后出现在待我处理
// 并能跳到验收页；受阻任务记录决策并恢复为 READY、任务详情能看到决策；关掉某类通知后
// 不再出现在通知中心。
import { type Page, expect, test } from "@playwright/test";

import { createMockHarness } from "./harnessHelpers.js";

const TIMEOUT = 25_000;
const HARNESS = "m5-mock";
const ROLE = "待我处理员";

let team = "";

interface TaskRef {
  id: string;
  key: string;
  title: string;
  url: string;
}

test.describe.configure({ mode: "serial", timeout: 90_000 });

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
  await page.locator("#task-file-search").fill("plan");
  await page.locator(".task-form__suggestion", { hasText: "plan.md" }).click();
  await page.getByRole("radio", { name: "READY", exact: true }).check();
  await page.getByRole("button", { name: "创建并分配" }).click();
  await expect(page.locator(".task-detail__status")).toContainText("READY");
  const key = /\/ (\S+)$/.exec(
    (await page.locator(".task-detail__status").innerText()).trim(),
  )?.[1];
  const url = page.url();
  return { url, id: url.split("/").pop() ?? "", key: key ?? "", title };
}

/** 开始执行并等到执行结束。 */
async function startRun(page: Page): Promise<void> {
  await page
    .getByRole("button", { name: /^(开始执行|重新执行)$/ })
    .first()
    .click();
}

test("准备：创建团队、Mock 接入与绑定", async ({ page }) => {
  const workspace = process.env.E2E_WORKSPACE ?? "";
  team = await createTeam(page, "待我处理团队", workspace, ROLE);
  await page.goto(`/#/teams/${team}/roles?tab=harness`);
  await createMockHarness(page, HARNESS);
  await bindRole(page, team, ROLE, HARNESS);
});

test("执行到待验收：出现在待我处理，能跳到验收页", async ({ page }) => {
  const task = await createTask(page, team, "待验收用例", ROLE);
  await startRun(page);
  await expect(page.locator(".run-outcome--success")).toBeVisible({ timeout: TIMEOUT });
  await expect(page.locator(".task-detail__status")).toContainText("待验收");

  // 侧边栏角标：待处理总数
  await expect(page.locator(".nav-item__badge")).toContainText("1");

  // 待我处理页：按类别分组，条目里有任务编号与操作按钮
  await page.goto(`/#/teams/${team}/inbox`);
  await expect(page.getByRole("heading", { name: "待我处理" })).toBeVisible();
  await expect(page.getByText("1 项需要你的判断，团队其余工作继续推进。")).toBeVisible();
  const group = page.locator(".inbox__group", { hasText: "待验收" });
  await expect(group).toContainText(task.key);
  await expect(group.getByRole("button", { name: "去验收" })).toBeVisible();

  // 跳到验收页
  await group.getByRole("button", { name: "去验收" }).click();
  await expect(page).toHaveURL(new RegExp(`/tasks/${task.id}/review$`));
});

test("受阻任务：记录决策并恢复为 READY，任务详情能看到决策", async ({ page }) => {
  const task = await createTask(page, team, "会失败的任务 [mock:fail]", ROLE);
  await startRun(page);
  await expect(page.locator(".run-outcome--failed")).toBeVisible({ timeout: TIMEOUT });
  await expect(page.locator(".task-detail__status")).toContainText("受阻");

  // 受阻任务出现在待我处理
  await page.goto(`/#/teams/${team}/inbox`);
  const group = page.locator(".inbox__group", { hasText: "执行失败或受阻" });
  await expect(group).toContainText(task.key);

  // 记录决策：内容为空不可提交
  await group.getByRole("button", { name: "记录决策" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toContainText("记录决策");
  const submit = dialog.getByRole("button", { name: "保存决策并恢复任务" });
  await expect(submit).toBeDisabled();
  await dialog.getByLabel("决策内容").fill("先补充测试配置再重试");
  await expect(submit).toBeEnabled();
  await submit.click();

  // 决策已记录
  await expect(page.locator(".decision-recorded__title")).toContainText("决策已记录");
  await expect(page.locator(".decision-recorded__text")).toContainText("先补充测试配置再重试");

  // 任务恢复为 READY
  await page.goto(task.url);
  await expect(page.locator(".task-detail__status")).toContainText("READY");

  // 任务详情里能看到决策列表
  await expect(page.locator(".decisions")).toContainText("先补充测试配置再重试");

  // 处理完就不再出现在待我处理
  await page.goto(`/#/teams/${team}/inbox`);
  await expect(page.locator(".inbox__group", { hasText: "执行失败或受阻" })).toHaveCount(0);
});

test("通知订阅：关掉某类通知后不再出现在通知中心", async ({ page }) => {
  // 默认订阅：执行到待验收会收到「任务待验收」通知
  const first = await createTask(page, team, "通知用例甲", ROLE);
  await page.goto(first.url);
  await startRun(page);
  await expect(page.locator(".run-outcome--success")).toBeVisible({ timeout: TIMEOUT });
  const bell = page.getByRole("button", { name: /通知中心/ });
  await expect(bell).toContainText("1", { timeout: TIMEOUT });
  await bell.click();
  await expect(page.locator(".notify-center__item").first()).toContainText("任务待验收");
  await expect(page.locator(".notify-center__item").first()).toContainText(first.key);

  // 关掉这一类通知
  await page.getByRole("button", { name: "管理订阅" }).click();
  await expect(page).toHaveURL(new RegExp(`/teams/${team}/notifications$`));
  const row = page.locator("tr", { hasText: "任务待验收" });
  await row.locator(".notify-settings__master input").uncheck();
  await page.getByRole("button", { name: "保存策略" }).click();
  await expect(page.locator(".notify-settings__saved")).toBeVisible();

  // 再执行一次：不再有通知（通知按会话累积，第一条仍在，第二条不再出现）
  const second = await createTask(page, team, "通知用例乙", ROLE);
  await page.goto(second.url);
  await startRun(page);
  await expect(page.locator(".run-outcome--success")).toBeVisible({ timeout: TIMEOUT });
  await page.getByRole("button", { name: /通知中心/ }).click();
  const items = page.locator(".notify-center__item");
  await expect(items).toHaveCount(1);
  await expect(items.first()).toContainText(first.key);
  await expect(items.first()).not.toContainText(second.key);
});
