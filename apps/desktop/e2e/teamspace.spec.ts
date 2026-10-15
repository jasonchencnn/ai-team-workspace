// 端到端用例：真实界面 + 测试桥接 + 真实内核 + 真实临时 Git 仓库。
// 断言基于界面文本，不依赖固定 id；用例按顺序共享同一个团队。
import { mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { type Locator, type Page, expect, test } from "@playwright/test";

import { acpEntry, createMockHarness } from "./harnessHelpers.js";

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

/** 从地址栏取团队 id（地址形如 `#/teams/<id>` 或 `#/teams/<id>/...`）。 */
function teamIdOf(page: Page): string {
  const id = /\/teams\/([^/?]+)/.exec(page.url())?.[1] ?? "";
  expect(id).not.toBe("");
  return id;
}

/** 在创建团队页「从示例添加」若干岗位（示例面板保持展开）。 */
async function addExampleRoles(page: Page, names: string[]): Promise<void> {
  await page.getByRole("button", { name: "从示例添加" }).click();
  for (const name of names) {
    await page
      .locator(".tc-example", { hasText: name })
      .getByRole("button", { name: "添加" })
      .click();
  }
}

/** 点「+ 添加岗位」自定义一个岗位：填名称、失焦让界面向内核要前缀建议，返回自动填入的前缀。 */
async function addCustomRole(
  page: Page,
  name: string,
  description = "",
  expectedPrefix?: string,
): Promise<string> {
  await page.getByRole("button", { name: "+ 添加岗位" }).click();
  const card = page.locator(".tc-role").last();
  await card.getByLabel("岗位名称").fill(name);
  await card.getByLabel("岗位名称").blur();
  const prefix = card.getByLabel("任务编号前缀");
  if (expectedPrefix === undefined) {
    await expect(prefix).toHaveValue(/^[A-Z][A-Z0-9]{1,3}$/);
  } else {
    await expect(prefix).toHaveValue(expectedPrefix);
  }
  if (description !== "") {
    await card.getByLabel("职责").fill(description);
  }
  return await prefix.inputValue();
}

async function createTask(
  page: Page,
  input: {
    title: string;
    owner: string;
    file?: string;
    status: "TODO" | "READY";
    dep?: string;
    teamId?: string;
  },
): Promise<void> {
  if (input.teamId === undefined) {
    await gotoTeam(page, "/tasks/new");
  } else {
    await page.goto(`/#/teams/${input.teamId}/tasks/new`);
  }
  await page.getByLabel("任务名称").fill(input.title);
  const ownerSelect = page.getByLabel("负责人岗位");
  const ownerValue = await ownerSelect
    .locator("option", { hasText: input.owner })
    .getAttribute("value");
  await ownerSelect.selectOption(ownerValue);
  if (input.file !== undefined) {
    await page.locator("#task-file-search").fill(input.file);
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
  await expect(page.getByRole("heading", { name: "创建团队" })).toBeVisible();
  await expect(page.getByText("尚未检测")).toBeVisible();
  // 岗位默认为空：不预置任何岗位，示例也不自动加载
  await expect(page.getByText("还没有岗位")).toBeVisible();
  await expect(page.locator(".tc-role")).toHaveCount(0);
  await expect(page.locator(".tc-example")).toHaveCount(0);
  await shot(page, "first-run-create-team");
});

test("创建团队：检测 Git 工作空间，自定义岗位自动建议前缀，保存后进入团队空间", async ({
  page,
}) => {
  await page.goto("/#/teams/new");
  await page.getByLabel("团队名称").fill("账号系统迭代");
  await page.getByLabel("说明").first().fill("端到端测试团队");
  await page.getByRole("button", { name: "选择工作空间" }).click();
  await expect(page.getByText(/Git 仓库：main @ [0-9a-f]{7}/)).toBeVisible();

  // 次要入口：从示例添加（后续用例需要 Leader / 前端开发 / QA 测试）
  await addExampleRoles(page, ["Leader", "前端开发", "QA 测试"]);
  await expect(page.locator(".tc-role")).toHaveCount(3);
  await expect(page.locator(".tc-role").nth(0).getByLabel("任务编号前缀")).toHaveValue("LD");
  await expect(page.locator(".tc-role").nth(1).getByLabel("任务编号前缀")).toHaveValue("FE");
  await expect(page.locator(".tc-role").nth(2).getByLabel("任务编号前缀")).toHaveValue("QA");

  // 主入口：自定义非开发岗位；中文名称没有字母，前缀由内核建议为 R1、R2（避开表单内已有前缀）
  await addCustomRole(page, "剪辑师", "把素材剪成成片", "R1");
  await addCustomRole(page, "审片人", "审看成片并给出修改意见", "R2");
  await expect(page.locator(".tc-role")).toHaveCount(5);
  await shot(page, "create-team-workspace-detected");

  await page.getByRole("button", { name: "创建团队", exact: true }).click();
  // 创建成功后落到团队空间，而不是岗位页
  await expect(page).toHaveURL(/\/teams\/(?!new$)[^/]+$/);
  await expect(page.getByRole("heading", { name: "团队空间" })).toBeVisible();
  await expect(page.locator(".th-roles .th-role")).toHaveCount(5);
  await expect(page.locator(".th-role", { hasText: "剪辑师" })).toBeVisible();
  await expect(page.locator(".th-role", { hasText: "审片人" })).toBeVisible();
  await shot(page, "team-home-after-create");

  await page.goto(`/#/teams/${teamIdOf(page)}/roles`);
  await expect(page.getByRole("heading", { name: "岗位与绑定" })).toBeVisible();
  await expect(page.getByRole("button", { name: /^前端开发\s*\/\s*未绑定$/ })).toBeVisible();
  await expect(page.getByRole("button", { name: /^剪辑师\s*\/\s*未绑定$/ })).toBeVisible();
  await shot(page, "roles-after-create");
});

test("新建 Mock 接入并握手为已连接，进入接入设置", async ({ page }) => {
  await gotoTeam(page, "/roles?tab=harness");
  await expect(page.getByRole("tab", { name: "Harness 接入" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await expect(page.getByText("本团队还没有接入")).toBeVisible();
  await createMockHarness(page, "mock-local");

  const entry = acpEntry(page, "mock-local");
  // Mock 接入标注「测试用」，避免误绑到正式岗位。
  await expect(entry).toContainText("测试用");
  await entry.getByRole("button", { name: "编辑" }).click();
  await expect(page).toHaveURL(/\/harnesses\/[^/]+$/);
  await expect(page.getByRole("heading", { name: "握手结果" })).toBeVisible();
  await page.getByRole("button", { name: "握手检查" }).click();
  await expect(page.locator(".harness-detail__health-head .ui-tag")).toHaveText("已连接");
  await shot(page, "harness-detail-connected");

  // 详情页唯一的「返回」落回接入列表视图。
  await page.getByRole("button", { name: "返回接入列表" }).click();
  await expect(page).toHaveURL(/\/roles\?tab=harness$/);
  await expect(acpEntry(page, "mock-local")).toContainText("已连接");
  await shot(page, "harness-list-connected");
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

  // B 的依赖已满足：M3-03 起下游任务在前置任务完成时自动转 READY，详情页上已没有 READY 按钮可点。
  await gotoTeam(page, "/tasks");
  await card(page, /FE-002 接入注册接口/).click();
  await expect(page.getByText("进入 READY 前还需要")).toHaveCount(0);
  await expect(statusLine(page)).toContainText("READY / FE-002");
  await expect(page.getByRole("button", { name: "READY", exact: true })).toHaveCount(0);
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
  await expect(page.getByText(/Git 仓库：main @ [0-9a-f]{7}/)).toBeVisible();
  await expect(page.locator(".topbar__branch")).toContainText("main");
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

test("看板可显示已完成列；文件引用可选，引用不存在的文件由后端拒绝", async ({ page }) => {
  await gotoTeam(page, "/tasks");
  await page.getByLabel("显示已完成（1）").check();
  await expect(page.locator(".task-board__heading", { hasText: "已完成 1" })).toBeVisible();
  await shot(page, "tasks-board-with-done");

  // 没有文件引用也能创建 READY 任务。
  await createTask(page, { title: "无引用的就绪任务", owner: "前端开发", status: "READY" });
  await expect(page.getByRole("heading", { name: "无引用的就绪任务" })).toBeVisible();

  // 引用不存在的文件仍被拒绝。
  await createTask(page, {
    title: "引用缺失文件的就绪任务",
    owner: "前端开发",
    file: "docs/不存在的文件.md",
    status: "READY",
  });
  await expect(page.getByRole("heading", { name: "创建任务" })).toBeVisible();
  const banner = page.getByRole("alert");
  await expect(banner).toContainText(
    "任务尚未就绪：引用的文件在工作区中不存在：docs/不存在的文件.md",
  );
  await shot(page, "task-create-rejected", banner);
});

test("普通文件夹作为工作空间：不显示分支，READY 任务可引用其中文件并能搜到", async ({ page }) => {
  const plain = process.env.E2E_PLAIN_WORKSPACE ?? "";
  expect(plain).not.toBe("");

  await page.goto("/#/teams/new");
  await page.getByLabel("团队名称").fill("素材整理组");
  // 手动输入路径 + 检测（不经过系统对话框）
  await page.getByLabel("手动输入工作空间路径").fill(plain);
  await page.getByRole("button", { name: "检测", exact: true }).click();
  await expect(page.getByText("普通文件夹", { exact: true })).toBeVisible();
  await expect(page.getByText(/Git 仓库：/)).toHaveCount(0);
  await addCustomRole(page, "整理员", "整理素材并编号", "R1");
  await addCustomRole(page, "校对员", "", "R2");
  await shot(page, "plain-folder-detected");

  await page.getByRole("button", { name: "创建团队", exact: true }).click();
  await expect(page).toHaveURL(/\/teams\/(?!new$)[^/]+$/);
  await expect(page.getByRole("heading", { name: "团队空间" })).toBeVisible();
  const teamId = teamIdOf(page);
  const context = page.locator(".th-panel", { hasText: "团队共用的上下文" });
  await expect(context.getByText("普通文件夹", { exact: true })).toBeVisible();
  await expect(page.getByText(/Git 仓库：/)).toHaveCount(0);
  // 顶栏：有「工作区」标签，但没有分支与提交
  await expect(page.locator(".topbar__branch")).toContainText("工作区");
  await expect(page.locator(".topbar__commit")).toHaveCount(0);
  await shot(page, "plain-folder-team-home");

  // 任务表单的文件搜索能搜到普通文件夹里的文件（含中文文件名），搜不到文件夹外的文件
  await page.goto(`/#/teams/${teamId}/tasks/new`);
  await page.getByLabel("任务名称").fill("整理拍摄素材");
  const owner = page.getByLabel("负责人岗位");
  await owner.selectOption(
    await owner.locator("option", { hasText: "整理员" }).getAttribute("value"),
  );
  await page.locator("#task-file-search").fill("outline");
  const suggestion = page.locator(".task-form__suggestion", { hasText: "notes/outline.md" });
  await expect(suggestion).toBeVisible();
  await shot(page, "plain-folder-file-search");
  await suggestion.click();
  await page.locator("#task-file-search").fill("素材清单");
  await expect(page.locator(".task-form__suggestion", { hasText: "素材清单.txt" })).toBeVisible();
  // 用只存在于文件夹外（Git 仓库里的 docs/plan.md）的名字验证范围；不能搜 README——
  // Teamspace 会在工作空间里生成 Teamspace/README.md，是否已生成取决于时机。
  await page.locator("#task-file-search").fill("plan");
  await expect(page.locator(".task-form__suggestion")).toHaveCount(0);
  await page.locator("#task-file-search").fill("");
  await expect(page.locator(".task-form__chip", { hasText: "notes/outline.md" })).toBeVisible();
  await page.getByRole("radio", { name: "READY", exact: true }).check();
  await page.getByRole("button", { name: "创建并分配" }).click();
  await expect(statusLine(page)).toContainText("READY / R1-001");
  await expect(page.getByRole("heading", { name: "整理拍摄素材" })).toBeVisible();
  await expect(page.getByText("notes/outline.md")).toBeVisible();
  await shot(page, "plain-folder-task-ready");
});

test("不建任何岗位直接创建团队：团队空间显示添加岗位引导", async ({ page }) => {
  await page.goto("/#/teams/new");
  await page.getByLabel("团队名称").fill("暂无岗位团队");
  await page.getByRole("button", { name: "选择工作空间" }).click();
  await expect(page.getByText(/Git 仓库：main @ [0-9a-f]{7}/)).toBeVisible();
  await page.getByRole("button", { name: "创建团队", exact: true }).click();

  await expect(page).toHaveURL(/\/teams\/(?!new$)[^/]+$/);
  const teamId = teamIdOf(page);
  await expect(page.getByRole("heading", { name: "团队空间" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "还没有岗位" })).toBeVisible();
  await expect(page.getByText("团队里还没有岗位，添加岗位后才能创建和派发任务。")).toBeVisible();
  await expect(page.locator(".th-role")).toHaveCount(0);
  await shot(page, "team-home-no-roles");

  // 任务表单同样引导先添加岗位
  await page.goto(`/#/teams/${teamId}/tasks/new`);
  await expect(
    page.getByText("任务必须指定负责岗位，请先在「岗位与绑定」里添加岗位。"),
  ).toBeVisible();

  // 引导按钮进入岗位页，在那里新增第一个岗位
  await page.goto(`/#/teams/${teamId}`);
  await page.getByRole("button", { name: "添加岗位", exact: true }).click();
  await expect(page).toHaveURL(/\/teams\/[^/]+\/roles$/);
  await expect(page.getByRole("heading", { name: "岗位与绑定" })).toBeVisible();
  await page.getByRole("button", { name: "新增岗位" }).first().click();
  const form = page.locator(".role-create");
  await form.getByLabel("岗位名称").fill("策划");
  await form.getByLabel("岗位名称").blur();
  await expect(form.getByLabel("任务编号前缀")).toHaveValue("R1");
  await form.getByRole("button", { name: "创建岗位" }).click();
  await expect(page.locator(".role-chip", { hasText: "策划" })).toBeVisible();
  await shot(page, "roles-first-role-added");
});

test("Git 仓库子目录作为工作空间：路径保持子目录，同时显示 Git 信息", async ({ page }) => {
  const subdir = process.env.E2E_SUBDIR_WORKSPACE ?? "";
  expect(subdir).toMatch(/packages[/\\]app$/);

  await page.goto("/#/teams/new");
  await page.getByLabel("团队名称").fill("子目录工作组");
  await page.getByLabel("手动输入工作空间路径").fill(subdir);
  await page.getByRole("button", { name: "检测", exact: true }).click();
  await expect(page.getByText(/Git 仓库：main @ [0-9a-f]{7}/)).toBeVisible();
  // 路径仍是所选子目录，没有被提升成仓库根
  await expect(page.getByText(/packages\/app$/)).toBeVisible();
  await addCustomRole(page, "App 开发", "", "AP");
  await page.getByRole("button", { name: "创建团队", exact: true }).click();

  await expect(page).toHaveURL(/\/teams\/(?!new$)[^/]+$/);
  const teamId = teamIdOf(page);
  const context = page.locator(".th-panel", { hasText: "团队共用的上下文" });
  await expect(context.locator(".th-path")).toContainText(/packages\/app$/);
  await expect(context.getByText(/Git 仓库：main @ [0-9a-f]{7}/)).toBeVisible();
  await expect(page.locator(".topbar__branch")).toContainText("main");
  await shot(page, "subdir-team-home");

  // 文件搜索以子目录为根：路径相对子目录，搜不到仓库根的文件
  await page.goto(`/#/teams/${teamId}/tasks/new`);
  await page.locator("#task-file-search").fill("main");
  await expect(
    page.locator(".task-form__suggestion", { hasText: /^src\/main\.ts$/ }),
  ).toBeVisible();
  await page.locator("#task-file-search").fill("plan");
  await expect(page.locator(".task-form__suggestion")).toHaveCount(0);
  await shot(page, "subdir-file-search");
});
