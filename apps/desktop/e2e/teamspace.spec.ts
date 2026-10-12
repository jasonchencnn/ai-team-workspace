// 端到端用例：真实界面 + 测试桥接 + 真实内核 + 真实临时 Git 仓库。
// 断言基于界面文本，不依赖固定 id；用例按顺序共享同一个团队。
import { mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { type Locator, type Page, expect, test } from "@playwright/test";

const artifacts = fileURLToPath(new URL("../e2e-artifacts/", import.meta.url));
mkdirSync(artifacts, { recursive: true });

let shotNo = 0;
/** 截图；页面内部有滚动容器，所以不用 fullPage，需要时先把关注的元素滚进视口。 */
async function shot(page: Page, name: string, focus?: Locator): Promise<void> {
  if (focus !== undefined) {
    await focus.scrollIntoViewIfNeeded();
  }
  shotNo += 1;
  const prefix = String(shotNo).padStart(2, "0");
  await page.screenshot({ path: `${artifacts}${prefix}-${name}.png`, fullPage: false });
}

/** 进入当前（唯一的）团队的某个页面：先走 `/`（会跳到第一个团队），再取地址栏里的团队 id。 */
async function gotoTeam(page: Page, path = ""): Promise<void> {
  await page.goto("/");
  await page.waitForURL(/\/teams\/(?!new)[^/]+$/);
  const id = /\/teams\/([^/]+)/.exec(page.url())?.[1] ?? "";
  expect(id).not.toBe("");
  if (path !== "") {
    await page.goto(`/#/teams/${id}${path}`);
  }
}

async function createTask(
  page: Page,
  input: { title: string; owner: string; file?: string; status: "TODO" | "READY"; dep?: string },
): Promise<void> {
  await gotoTeam(page, "/tasks/new");
  await page.getByLabel("任务名称").fill(input.title);
  const ownerSelect = page.getByLabel("负责人岗位");
  const ownerValue = await ownerSelect
    .locator("option", { hasText: input.owner })
    .getAttribute("value");
  await ownerSelect.selectOption(ownerValue);
  if (input.file !== undefined) {
    await page.getByPlaceholder("搜索工作区文件").fill(input.file);
    await page.getByRole("button", { name: "添加", exact: true }).click();
  }
  if (input.dep !== undefined) {
    await page.locator("label.task-form__option", { hasText: input.dep }).locator("input").click();
  }
  await page.getByRole("radio", { name: input.status, exact: true }).check();
  await page.getByRole("button", { name: "创建并分配" }).click();
}

async function transition(page: Page, label: string): Promise<void> {
  await page.getByRole("button", { name: label, exact: true }).click();
}

/** 看板上的任务卡片链接（卡片整体是一个 <a>）。 */
function card(page: Page, text: RegExp): Locator {
  return page.locator("a.task-card-link", { hasText: text });
}

function statusLine(page: Page): Locator {
  return page.locator(".task-detail__status");
}

test.describe.configure({ mode: "serial" });

test("首次进入跳转到创建团队页", async ({ page }) => {
  await page.goto("/");
  await expect(page).toHaveURL(/\/teams\/new$/);
  await expect(page.getByRole("heading", { name: "创建 Team" })).toBeVisible();
  await expect(page.getByText("代码库状态：尚未检测")).toBeVisible();
  await shot(page, "first-run-create-team");
});

test("创建团队：选择工作空间、保留默认岗位、保存后进入岗位与绑定页", async ({ page }) => {
  await page.goto("/#/teams/new");
  await page.getByLabel("Team 名称").fill("账号系统迭代");
  await page.getByLabel("说明").first().fill("端到端测试团队");
  await page.getByRole("button", { name: "选择工作空间" }).click();
  await expect(page.getByText(/代码库已检测：main @ [0-9a-f]{7}/)).toBeVisible();
  // 默认勾选 Leader / 前端开发 / QA 测试
  await expect(page.locator(".tc-role__checkbox:checked")).toHaveCount(3);
  await shot(page, "create-team-workspace-detected");

  await page.getByRole("button", { name: "创建团队并配置岗位" }).click();
  await expect(page).toHaveURL(/\/teams\/[^/]+\/roles$/);
  await expect(page.getByRole("heading", { name: "岗位与绑定" })).toBeVisible();
  await expect(page.getByRole("button", { name: /^前端开发\s*\/\s*未绑定$/ })).toBeVisible();
  await shot(page, "roles-after-create");
});

test("新建 Mock Harness 实例并健康检查为正常，进入 Harness 详情", async ({ page }) => {
  await gotoTeam(page, "/roles?tab=harness");
  await expect(page.getByRole("tab", { name: "Harness 实例" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await expect(page.getByText("本团队还没有 Harness 实例")).toBeVisible();
  await page.getByRole("button", { name: "新建实例" }).click();
  await page.getByLabel("实例名称").fill("mock-local");
  await page.getByLabel("类型").selectOption("mock");
  await page.getByRole("button", { name: "注册实例" }).click();

  const card = page.locator(".harness-card", { hasText: "mock-local" });
  await expect(card).toBeVisible();
  // Mock 实例在卡片上标注「测试用」，避免误绑到正式岗位。
  await expect(card).toContainText("测试用");
  await card.getByRole("button", { name: "查看详情" }).click();
  await expect(page).toHaveURL(/\/harnesses\/[^/]+$/);
  await expect(page.getByRole("heading", { name: "健康检查结果" })).toBeVisible();
  await page.getByRole("button", { name: "重新健康检查" }).click();
  await expect(page.locator(".harness-detail__health .ui-tag")).toHaveText("在线");
  await expect(page.getByText("Mock Harness，无需检测")).toBeVisible();
  await shot(page, "harness-detail-healthy");

  // 详情页唯一的「返回」落回实例列表视图。
  await page.getByRole("button", { name: "返回实例列表" }).click();
  await expect(page).toHaveURL(/\/roles\?tab=harness$/);
  await expect(page.locator(".harness-card", { hasText: "mock-local" })).toContainText("在线");
  await shot(page, "harness-list-healthy");
});

test("把前端开发岗位绑定到该实例", async ({ page }) => {
  await gotoTeam(page, "/roles");
  await page.getByRole("button", { name: /^前端开发/ }).click();
  await page.getByRole("button", { name: "绑定 Harness", exact: true }).click();
  await page.getByLabel("选择本团队的 Harness 实例").selectOption({ index: 1 });
  await page.getByRole("button", { name: "确认绑定" }).click();
  await expect(page.getByText("当前绑定 mock-local")).toBeVisible();
  await expect(page.getByRole("button", { name: /^前端开发\s*\/\s*mock-local$/ })).toBeVisible();
  await shot(page, "frontend-role-bound");
});

test("空任务面板显示空状态", async ({ page }) => {
  await gotoTeam(page, "/tasks");
  await expect(page.getByText("团队已就绪，创建第一项任务")).toBeVisible();
  await shot(page, "tasks-empty");
});

test("创建任务 A（READY）与任务 B（TODO，依赖 A），看板与列表都能看到", async ({ page }) => {
  await createTask(page, {
    title: "实现注册界面",
    owner: "前端开发",
    file: "docs/plan.md",
    status: "READY",
  });
  await expect(page.locator(".task-detail__status")).toContainText("READY / FE-001");

  await createTask(page, {
    title: "接入注册接口",
    owner: "前端开发",
    file: "docs/api.md",
    status: "TODO",
    dep: "FE-001",
  });
  await expect(page.locator(".task-detail__status")).toContainText("TODO / FE-002");

  await gotoTeam(page, "/tasks");
  await expect(card(page, /FE-001 实现注册界面/)).toBeVisible();
  await expect(card(page, /FE-002 接入注册接口/)).toBeVisible();
  await expect(page.getByText("等待 1 项依赖完成")).toBeVisible();
  await shot(page, "tasks-board");

  await page.getByRole("button", { name: "列表", exact: true }).click();
  const rows = page.locator(".task-table tbody tr");
  await expect(rows).toHaveCount(2);
  await expect(rows.filter({ hasText: "FE-001" })).toContainText("前端开发");
  await shot(page, "tasks-list");
});

test("任务 A：READY → 执行中 → 受阻 → 待验收 → 已完成，随后 B 可转 READY", async ({ page }) => {
  await gotoTeam(page, "/tasks");
  await card(page, /FE-001 实现注册界面/).click();
  await expect(statusLine(page)).toContainText("READY / FE-001");

  await transition(page, "执行中");
  await expect(statusLine(page)).toContainText("执行中 / FE-001");

  await transition(page, "受阻");
  await page.getByLabel("受阻原因").fill("等待设计稿确认");
  await shot(page, "task-a-blocking-form");
  await page.getByRole("button", { name: "确认受阻" }).click();
  await expect(statusLine(page)).toContainText("受阻 / FE-001");
  await expect(page.getByText("受阻原因：等待设计稿确认")).toBeVisible();
  await shot(page, "task-a-blocked");

  // 受阻 → 执行中 → 待验收 → 已完成
  await transition(page, "执行中");
  await expect(statusLine(page)).toContainText("执行中 / FE-001");
  await expect(page.getByText("受阻原因：")).toHaveCount(0);
  await transition(page, "待验收");
  await expect(statusLine(page)).toContainText("待验收 / FE-001");
  await shot(page, "task-a-review");
  await transition(page, "已完成");
  await expect(statusLine(page)).toContainText("已完成 / FE-001");
  await expect(page.getByText("状态 受阻 → 执行中")).toBeVisible();
  await shot(page, "task-a-done");

  // B 的依赖已满足，现在可以转 READY
  await gotoTeam(page, "/tasks");
  await card(page, /FE-002 接入注册接口/).click();
  await expect(page.getByText("进入 READY 前还需要")).toHaveCount(0);
  await transition(page, "READY");
  await expect(statusLine(page)).toContainText("READY / FE-002");
  await shot(page, "task-b-ready");
});

test("A 在依赖未满足时，B 不能转 READY（用新任务验证后端拒绝）", async ({ page }) => {
  await createTask(page, {
    title: "回归测试占位",
    owner: "QA 测试",
    file: "docs/plan.md",
    status: "READY",
  });
  const qaKey = (await statusLine(page).textContent()) ?? "";
  expect(qaKey).toMatch(/READY \/ [A-Z]{2,4}-001/);
  await createTask(page, {
    title: "依赖未完成的任务",
    owner: "前端开发",
    file: "docs/plan.md",
    status: "TODO",
    dep: "回归测试占位",
  });
  await expect(page.getByText("进入 READY 前还需要")).toBeVisible();
  await expect(page.getByText(/依赖任务 .* 尚未完成/)).toBeVisible();
  await shot(page, "task-readiness-problem");
});

test("让任务依赖形成环时显示后端的环路错误", async ({ page }) => {
  // 当前 FE-004 依赖 QA 任务；让 QA 任务反过来依赖 FE-004
  await gotoTeam(page, "/tasks");
  await card(page, /回归测试占位/).click();
  await page.getByRole("button", { name: "编辑" }).click();
  await page
    .locator("label.task-form__option", { hasText: "依赖未完成的任务" })
    .locator("input")
    .click();
  await page.getByRole("button", { name: "保存", exact: true }).click();
  const banner = page.getByRole("alert");
  await expect(banner).toContainText(/依赖形成环路：.*→.*/);
  await shot(page, "dependency-cycle-error", banner);
});

test("团队空间页显示计数、进行中的任务和最近活动", async ({ page }) => {
  // 让一个任务处于执行中，便于出现在「推进中的工作」
  await gotoTeam(page, "/tasks");
  await card(page, /FE-002 接入注册接口/).click();
  await transition(page, "执行中");
  await expect(statusLine(page)).toContainText("执行中 / FE-002");

  await gotoTeam(page);
  await expect(page.getByRole("heading", { name: "团队空间" })).toBeVisible();
  await expect(page.getByText(/1 个执行中/)).toBeVisible();
  await expect(page.locator(".th-task-row", { hasText: "FE-002" })).toBeVisible();
  await expect(page.locator(".th-event", { hasText: "FE-002 状态 READY → 执行中" })).toBeVisible();
  await expect(page.getByText(/分支：main/)).toBeVisible();
  await shot(page, "team-home");
});

test("编辑任务：修改标题与说明后详情页立即更新", async ({ page }) => {
  await gotoTeam(page, "/tasks");
  await page.getByLabel(/^显示已完成（\d+）/).check(); // FE-001 已完成，默认不在看板上
  await card(page, /FE-001 实现注册界面/).click();
  await page.getByRole("button", { name: "编辑" }).click();
  await page.getByLabel("任务名称").fill("实现注册与登录界面");
  await page.getByLabel("说明", { exact: true }).fill("覆盖注册、登录两个页面");
  await shot(page, "task-edit-form");
  await page.getByRole("button", { name: "保存", exact: true }).click();
  await expect(page.getByRole("heading", { name: "实现注册与登录界面" })).toBeVisible();
  await expect(page.getByText("覆盖注册、登录两个页面")).toBeVisible();
  await expect(page.getByText(/编辑任务 FE-001「实现注册与登录界面」/)).toBeVisible();
  await shot(page, "task-edited");
});

test("看板可显示已完成列，非法输入由后端拒绝（创建 READY 任务但没有文件引用）", async ({
  page,
}) => {
  await gotoTeam(page, "/tasks");
  await page.getByLabel("显示已完成（1）").check();
  await expect(page.locator(".task-board__heading", { hasText: "已完成 1" })).toBeVisible();
  await shot(page, "tasks-board-with-done");

  await createTask(page, { title: "无引用的就绪任务", owner: "前端开发", status: "READY" });
  await expect(page.getByRole("heading", { name: "创建任务" })).toBeVisible();
  const banner = page.getByRole("alert");
  await expect(banner).toContainText("任务尚未就绪：至少需要一个文件引用");
  await shot(page, "task-create-rejected", banner);
});
