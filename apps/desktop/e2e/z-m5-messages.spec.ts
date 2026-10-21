// M5-03 岗位通信：发送消息 → 排队 → 投递 → 回复写回；失败原因与重新发送；从任务详情发起消息。
//
// 用例走真实内核（测试桥接）与内置 Mock Agent：消息正文里的 [mock:...] 标记决定 Mock 的行为，
// 页面上的状态全部来自后端，界面不造状态。
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { type Page, expect, test } from "@playwright/test";

import { createMockHarness } from "./harnessHelpers.js";

const artifacts = fileURLToPath(new URL("../e2e-artifacts/", import.meta.url));
mkdirSync(artifacts, { recursive: true });

const TEAM = "岗位通信团队";
const ROLE_REPLY = "回复岗位";
const ROLE_FAIL = "失败岗位";
const ROLE_UNBOUND = "待绑定岗位";
const ROLE_SLOW = "慢岗位";
const HARNESS = "m5-mock";
const TIMEOUT = 60_000;

let workspace = "";
let team = "";

test.describe.configure({ mode: "serial", timeout: 300_000 });

/** 在建团队页上加一个岗位。 */
async function addRole(page: Page, name: string, description: string): Promise<void> {
  await page.getByRole("button", { name: "+ 添加岗位" }).click();
  const card = page.locator(".tc-role").last();
  await card.getByLabel("岗位名称").fill(name);
  await card.getByLabel("岗位名称").blur();
  await expect(card.getByLabel("任务编号前缀")).toHaveValue(/^[A-Z][A-Z0-9]{1,3}$/);
  await card.getByLabel("职责").fill(description);
}

/** 给岗位绑定 Mock 接入。 */
async function bindHarness(page: Page, roleName: string): Promise<void> {
  await page.goto(`/#/teams/${team}/roles`);
  await page.locator(".role-chip", { hasText: roleName }).click();
  await page.getByRole("button", { name: "绑定 Harness", exact: true }).click();
  const select = page.getByLabel("选择本团队的 Harness 实例");
  await select.selectOption(
    (await select.locator("option", { hasText: HARNESS }).getAttribute("value")) ?? "",
  );
  await page.getByRole("button", { name: "确认绑定" }).click();
  await expect(page.getByText(`当前绑定 ${HARNESS}`)).toBeVisible();
}

/** 在岗位通信页发一条消息，返回发送后所在线程的地址。 */
async function sendMessage(page: Page, roleName: string, body: string): Promise<string> {
  await page.locator(".msg-list__actions").getByRole("button", { name: "发送消息" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  const roleSelect = dialog.getByLabel("接收岗位");
  await roleSelect.selectOption(
    (await roleSelect.locator("option", { hasText: roleName }).getAttribute("value")) ?? "",
  );
  await dialog.getByLabel("消息内容").fill(body);
  await dialog.getByRole("button", { name: "排队发送" }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page).toHaveURL(/\/messages\?thread=/);
  return page.url();
}

test.beforeAll(() => {
  workspace = mkdtempSync(join(tmpdir(), "teamspace-e2e-m5-msg-"));
  writeFileSync(join(workspace, "brief.txt"), "需求说明\n");
});

test.afterAll(() => {
  rmSync(workspace, { recursive: true, force: true });
});

test("准备：建团队、四个岗位与 Mock 接入", async ({ page }) => {
  await page.goto("/#/teams/new");
  await page.getByLabel("团队名称").fill(TEAM);
  await page.getByLabel("手动输入工作空间路径").fill(workspace);
  await page.getByRole("button", { name: "检测", exact: true }).click();
  await expect(page.getByText(/普通文件夹|Git 仓库/).first()).toBeVisible();
  await addRole(page, ROLE_REPLY, "负责回消息");
  await addRole(page, ROLE_FAIL, "负责回失败");
  await addRole(page, ROLE_UNBOUND, "先不绑定接入");
  await addRole(page, ROLE_SLOW, "慢回复");
  await page.getByRole("button", { name: "创建团队", exact: true }).click();
  await expect(page).toHaveURL(/\/teams\/(?!new$)[^/]+$/);
  team = /\/teams\/([^/?]+)/.exec(page.url())?.[1] ?? "";
  expect(team).not.toBe("");

  await page.goto(`/#/teams/${team}/roles?tab=harness`);
  await createMockHarness(page, HARNESS);
  await bindHarness(page, ROLE_REPLY);
  await bindHarness(page, ROLE_FAIL);
  await bindHarness(page, ROLE_SLOW);
});

test("侧边栏能进岗位通信页；发出的消息不刷新页面就变成已回复并显示回复正文", async ({ page }) => {
  await page.goto(`/#/teams/${team}/tasks`);
  await page.getByRole("link", { name: "岗位通信" }).click();
  await expect(page).toHaveURL(/\/messages$/);
  await expect(page.getByRole("heading", { level: 1, name: "岗位通信" })).toBeVisible();

  // 页面没有整页刷新：打个标记，发送后它还在，就说明状态是页面自己更新出来的。
  await page.evaluate(() => {
    (window as unknown as Record<string, unknown>).__m5_no_reload = true;
  });

  await sendMessage(page, ROLE_REPLY, "[mock:reply=已收到]");

  const bubble = page.locator(".msg-detail__bubbles .msg-bubble").first();
  await expect(bubble).toContainText("[mock:reply=已收到]");
  // 排队 → 投递中 → 已回复，页面靠轮询跟上，不用刷新。
  await expect(bubble.locator(".ui-tag")).toHaveText("已回复", { timeout: TIMEOUT });
  await expect(page.getByTestId("message-reply")).toContainText("已收到", { timeout: TIMEOUT });
  await expect
    .poll(() => page.evaluate(() => (window as unknown as Record<string, unknown>).__m5_no_reload))
    .toBe(true);
  await page.screenshot({ path: `${artifacts}m5-messages-replied.png`, fullPage: false });
});

test("Agent 出错时消息失败，写明原因并给出重新发送", async ({ page }) => {
  await page.goto(`/#/teams/${team}/messages`);
  await sendMessage(page, ROLE_FAIL, "[mock:replyfail]");

  const bubble = page.locator(".msg-detail__bubbles .msg-bubble").first();
  await expect(bubble.locator(".ui-tag")).toHaveText("失败", { timeout: TIMEOUT });
  await expect(bubble).toContainText("失败原因：");
  await expect(bubble).toContainText("Agent 出错");
  await expect(bubble.getByRole("button", { name: "重新发送" })).toBeVisible();
  await page.screenshot({ path: `${artifacts}m5-messages-failed.png`, fullPage: false });
});

test("未绑定接入时消息失败；绑上之后重新发送即可投递并收到回复", async ({ page }) => {
  await page.goto(`/#/teams/${team}/messages`);
  const threadUrl = await sendMessage(page, ROLE_UNBOUND, "[mock:reply=补上绑定后收到]");

  const bubble = page.locator(".msg-detail__bubbles .msg-bubble").first();
  await expect(bubble.locator(".ui-tag")).toHaveText("失败", { timeout: TIMEOUT });
  await expect(bubble).toContainText("未绑定 Harness");

  await bindHarness(page, ROLE_UNBOUND);
  await page.goto(threadUrl);
  const retryBubble = page.locator(".msg-detail__bubbles .msg-bubble").first();
  await retryBubble.getByRole("button", { name: "重新发送" }).click();
  await expect(retryBubble.locator(".ui-tag")).toHaveText("已回复", { timeout: TIMEOUT });
  await expect(page.getByTestId("message-reply")).toContainText("补上绑定后收到", {
    timeout: TIMEOUT,
  });
});

test("从任务详情给负责岗位发消息，线程挂在任务上", async ({ page }) => {
  await page.goto(`/#/teams/${team}/tasks/new`);
  await page.getByLabel("任务名称").fill("岗位通信入口任务");
  const owner = page.getByLabel("负责人岗位");
  await owner.selectOption(
    (await owner.locator("option", { hasText: ROLE_REPLY }).getAttribute("value")) ?? "",
  );
  await page.getByRole("radio", { name: "READY", exact: true }).check();
  await page.getByRole("button", { name: "创建并分配" }).click();
  await expect(page.locator(".task-detail__status")).toContainText("READY");

  await page.getByRole("button", { name: "给负责岗位发消息" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  // 岗位与关联任务都预选好了。
  await expect(dialog.locator("option:checked", { hasText: ROLE_REPLY })).toHaveCount(1);
  await expect(dialog.locator("option:checked", { hasText: "岗位通信入口任务" })).toHaveCount(1);
  await dialog.getByLabel("消息内容").fill("[mock:reply=任务里的消息收到]");
  await dialog.getByRole("button", { name: "排队发送" }).click();
  await expect(dialog).toHaveCount(0);

  // 任务详情里出现这个线程的链接。
  const threadLink = page.locator(".task-detail__thread-list a").first();
  await expect(threadLink).toBeVisible();
  await threadLink.click();
  await expect(page).toHaveURL(/\/messages\?thread=/);
  const bubble = page.locator(".msg-detail__bubbles .msg-bubble").first();
  await expect(bubble.locator(".ui-tag")).toHaveText("已回复", { timeout: TIMEOUT });
  await expect(page.getByTestId("message-reply")).toContainText("任务里的消息收到", {
    timeout: TIMEOUT,
  });
});

test("Agent 还没给回复时，消息停在投递中", async ({ page }) => {
  await page.goto(`/#/teams/${team}/messages`);
  await sendMessage(page, ROLE_SLOW, "[mock:hang]");

  const bubble = page.locator(".msg-detail__bubbles .msg-bubble").first();
  await expect(bubble.locator(".ui-tag")).toHaveText("投递中", { timeout: TIMEOUT });
  // 列表里也能看到这条线程的最近状态。
  await expect(page.locator(".msg-thread").first().locator(".ui-tag")).toHaveText("投递中", {
    timeout: TIMEOUT,
  });
  await page.screenshot({ path: `${artifacts}m5-messages-delivering.png`, fullPage: false });
});
