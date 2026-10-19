// QA 轮次 R2：任务看板 / 任务表单 / 任务详情的端到端回归。
// 真实界面 + 测试桥接 + 真实内核 + 临时工作空间；只用 Mock 预设接入，不启动任何真实 Agent。
// 用例按顺序共享同一个团队（每次 pnpm run e2e 都是全新的临时数据库，团队由第一条用例创建）。
import { mkdirSync, mkdtempSync, rmSync, unlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { type Locator, type Page, expect, test } from "@playwright/test";

import { createMockHarness } from "./harnessHelpers.js";

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
  await page.screenshot({ path: `${artifacts}r2ui-${prefix}-${name}.png`, fullPage: false });
}

// ---------------------------------------------------------------- 团队与工作空间

let workspace = "";
/** 看板 / 表单 / 详情的主团队：两个岗位，主创员绑定 Mock，复核员不绑定。 */
let teamBoard = "";
/** 只有 READY 任务、没有 TODO 任务的团队（UI-TASK-009 的前置条件）。 */
let teamReady = "";
/** 没有任何岗位的团队（UI-FORM-003）。 */
let teamNoRole = "";

const ROLE_MAIN = "主创员";
const ROLE_REVIEW = "复核员";
const HARNESS = "qa-ui-mock";

/** 主团队里各任务的标题，供后续用例定位。 */
const T_READY = "就绪任务甲";
const T_TODO = "待办任务乙";
const T_DOING = "执行中任务丙";
const T_BLOCKED = "受阻任务丁";
const T_REVIEW = "待验收任务戊";
const T_DONE = "已完成任务己";
const T_DEPS = "依赖未完成的任务";
const BLOCKED_REASON = "等待接口冻结";
const TODO_DESC = "等接口冻结后联调，先留好验收口径。";

test.describe.configure({ mode: "serial", timeout: 180_000 });

test.beforeAll(() => {
  workspace = mkdtempSync(join(tmpdir(), "teamspace-e2e-qa-ui4-"));
  writeFileSync(join(workspace, "a.md"), "# A 文档\n\n## 章节\n\n正文\n");
  mkdirSync(join(workspace, "docs"));
  writeFileSync(join(workspace, "docs", "doc.md"), "# 设计文档\n");
  mkdirSync(join(workspace, "素材"));
  writeFileSync(join(workspace, "素材", "镜头 01.mov"), "假素材\n");
  // UI-DETAIL-002 用：任务建好后删掉它，详情页应出现「文件不存在」。
  writeFileSync(join(workspace, "gone.md"), "# 稍后删除\n");
  mkdirSync(join(workspace, "notes"));
  writeFileSync(join(workspace, "notes", "待删文件.md"), "# 待删\n");
});

test.afterAll(() => {
  rmSync(workspace, { recursive: true, force: true });
});

// ---------------------------------------------------------------- 辅助

function teamIdOf(page: Page): string {
  const id = /\/teams\/([^/?]+)/.exec(page.url())?.[1] ?? "";
  expect(id).not.toBe("");
  return id;
}

/** 创建团队页：填名称、检测工作空间、可选自定义岗位；返回团队 id。 */
async function createTeam(page: Page, name: string, roles: string[]): Promise<string> {
  await page.goto("/#/teams/new");
  await page.getByLabel("团队名称").fill(name);
  await page.getByLabel("手动输入工作空间路径").fill(workspace);
  await page.getByRole("button", { name: "检测", exact: true }).click();
  await expect(page.getByText("普通文件夹", { exact: true })).toBeVisible();
  for (const role of roles) {
    await page.getByRole("button", { name: "+ 添加岗位" }).click();
    const card = page.locator(".tc-role").last();
    await card.getByLabel("岗位名称").fill(role);
    await card.getByLabel("岗位名称").blur();
    await expect(card.getByLabel("任务编号前缀")).toHaveValue(/^[A-Z][A-Z0-9]{1,3}$/);
  }
  await page.getByRole("button", { name: "创建团队", exact: true }).click();
  await expect(page).toHaveURL(/\/teams\/(?!new$)[^/]+$/);
  return teamIdOf(page);
}

/** 看板上的任务卡片链接。 */
function card(page: Page, text: RegExp): Locator {
  return page.locator("a.task-card-link", { hasText: text });
}

/** 看板容器（role=region，aria-label「任务看板」，可聚焦以键盘滚动）。 */
function board(page: Page): Locator {
  return page.getByRole("region", { name: "任务看板" });
}

/** 看板上的某一列（如「TODO」「执行中」），按列标题里的计数数字精确匹配。 */
function column(page: Page, title: string): Locator {
  return page.locator(".task-board__column").filter({
    has: page.locator("h2", { hasText: new RegExp(`^${title} \\d+$`) }),
  });
}

function statusLine(page: Page): Locator {
  return page.locator(".task-detail__status");
}

async function transition(page: Page, label: string): Promise<void> {
  await page.getByRole("button", { name: label, exact: true }).click();
}

/** 断言详情页状态行同时带文字标签与状态色类（不只靠颜色区分）。 */
async function expectStatus(page: Page, label: string, key: string, status: string): Promise<void> {
  const line = statusLine(page);
  await expect(line).toHaveText(`${label} / ${key}`);
  await expect(line).toHaveClass(new RegExp(`task-detail__status--${status}`));
}

interface NewTask {
  teamId: string;
  title: string;
  owner: string;
  status?: "TODO" | "READY";
  files?: string[];
  deps?: string[];
  description?: string;
}

/** 创建任务并等落到详情页，返回任务 KEY。 */
async function newTask(page: Page, input: NewTask): Promise<string> {
  await page.goto(`/#/teams/${input.teamId}/tasks/new`);
  await expect(page.getByRole("heading", { name: "创建任务" })).toBeVisible();
  await page.getByLabel("任务名称").fill(input.title);
  const owner = page.getByLabel("负责人岗位");
  const ownerValue = await owner.locator("option", { hasText: input.owner }).getAttribute("value");
  await owner.selectOption(ownerValue ?? "");
  for (const file of input.files ?? []) {
    await page.locator("#task-file-search").fill(file);
    await page.getByRole("button", { name: "添加", exact: true }).click();
  }
  for (const dep of input.deps ?? []) {
    await page.locator("label.task-form__option", { hasText: dep }).locator("input").check();
  }
  if (input.description !== undefined) {
    await page.getByLabel("说明", { exact: true }).fill(input.description);
  }
  await page.getByRole("radio", { name: input.status ?? "TODO", exact: true }).check();
  await page.getByRole("button", { name: "创建并分配" }).click();
  await expect(statusLine(page)).toBeVisible();
  const key = /\/ (\S+)$/.exec((await statusLine(page).innerText()).trim())?.[1] ?? "";
  expect(key).not.toBe("");
  return key;
}

/** 打开某任务详情。 */
async function openTask(page: Page, teamId: string, title: string): Promise<void> {
  await page.goto(`/#/teams/${teamId}/tasks`);
  await card(page, new RegExp(title)).click();
  await expect(page.getByRole("heading", { level: 1, name: title })).toBeVisible();
}

/** 断言某条 invoke 命令在接下来的 `ms` 毫秒内没有被发起。 */
async function expectNoInvoke(page: Page, cmd: string, ms: number): Promise<void> {
  const fired = await page
    .waitForRequest((request) => (request.postData() ?? "").includes(`"${cmd}"`), { timeout: ms })
    .then(
      () => true,
      () => false,
    );
  expect(fired, `不应发起 ${cmd} 请求`).toBe(false);
}

// ---------------------------------------------------------------- UI-TASK-002

test("UI-TASK-002 看板列构成与首列筛选：默认 4 列且 TODO 列合并 todo 与 ready", async ({
  page,
}) => {
  // 准备：主团队 + 一个绑定了 Mock 的岗位 + 一个未绑定的岗位。
  teamBoard = await createTeam(page, "看板结构验证组", [ROLE_MAIN, ROLE_REVIEW]);
  await page.goto(`/#/teams/${teamBoard}/roles?tab=harness`);
  await createMockHarness(page, HARNESS);
  await page.goto(`/#/teams/${teamBoard}/roles`);
  await page.getByRole("button", { name: new RegExp(`^${ROLE_MAIN}`) }).click();
  await page.getByRole("button", { name: "绑定 Harness", exact: true }).click();
  const select = page.getByLabel("选择本团队的 Harness 实例");
  await select.selectOption(
    (await select.locator("option", { hasText: HARNESS }).getAttribute("value")) ?? "",
  );
  await page.getByRole("button", { name: "确认绑定" }).click();
  await expect(page.getByText(`当前绑定 ${HARNESS}`)).toBeVisible();

  // 各状态任务齐备：todo、ready、doing、blocked、review，外加 1 个 DONE。
  await newTask(page, {
    teamId: teamBoard,
    title: T_READY,
    owner: ROLE_MAIN,
    status: "READY",
    files: ["a.md"],
  });
  await newTask(page, {
    teamId: teamBoard,
    title: T_TODO,
    owner: ROLE_MAIN,
    files: ["a.md"],
    description: TODO_DESC,
  });
  await newTask(page, { teamId: teamBoard, title: T_DOING, owner: ROLE_MAIN, status: "READY" });
  await newTask(page, { teamId: teamBoard, title: T_REVIEW, owner: ROLE_MAIN, status: "READY" });
  await newTask(page, { teamId: teamBoard, title: T_BLOCKED, owner: ROLE_MAIN, status: "READY" });
  await newTask(page, { teamId: teamBoard, title: T_DONE, owner: ROLE_MAIN, status: "READY" });

  await openTask(page, teamBoard, T_DOING);
  await transition(page, "执行中");
  await expect(statusLine(page)).toContainText("执行中");

  await openTask(page, teamBoard, T_DONE);
  await transition(page, "执行中");
  await transition(page, "待验收");
  await transition(page, "已完成");
  await expect(statusLine(page)).toContainText("已完成");

  await openTask(page, teamBoard, T_REVIEW);
  await transition(page, "执行中");
  await transition(page, "待验收");

  await openTask(page, teamBoard, T_BLOCKED);
  await transition(page, "执行中");
  await transition(page, "受阻");
  await page.getByLabel("受阻原因").fill(BLOCKED_REASON);
  await page.getByRole("button", { name: "确认受阻" }).click();
  await expect(statusLine(page)).toContainText("受阻");

  // 打开看板，不勾选任何开关：默认 4 列。
  await page.goto(`/#/teams/${teamBoard}/tasks`);
  const columns = page.locator(".task-board__column");
  await expect(columns).toHaveCount(4);
  await expect(page.locator(".task-board__heading")).toHaveText([
    /^TODO \d+$/,
    /^执行中 \d+$/,
    /^受阻 \d+$/,
    /^待验收 \d+$/,
  ]);
  // 已完成列默认不显示、已取消列同样默认收起。
  await expect(page.locator(".task-board__heading--done")).toHaveCount(0);
  await expect(page.locator(".task-board__heading--cancelled")).toHaveCount(0);
  await expect(page.getByLabel("显示已取消")).not.toBeChecked();
  await expect(page.getByLabel(/^显示已完成（\d+）/)).not.toBeChecked();
  await shot(page, "task-002-default-columns");

  // 第一列为 TODO，合并 todo 与 ready。
  const first = column(page, "TODO");
  await expect(first).toHaveCount(1);
  await expect(first.locator("a.task-card-link")).toHaveCount(2);
  await expect(card(page, new RegExp(T_READY))).toBeVisible();
  await expect(card(page, new RegExp(T_TODO))).toBeVisible();

  // 三档筛选：全部 → 仅 TODO → 仅 READY，其他列不受影响。
  const filter = page.getByLabel("TODO / READY");
  await filter.selectOption("all");
  await expect(first.locator("a.task-card-link")).toHaveCount(2);
  await expect(column(page, "执行中").locator("a.task-card-link")).toHaveCount(1);

  await filter.selectOption("todo");
  await expect(card(page, new RegExp(T_READY))).toHaveCount(0);
  await expect(card(page, new RegExp(T_TODO))).toHaveCount(1);
  await expect(column(page, "执行中").locator("a.task-card-link")).toHaveCount(1);
  await shot(page, "task-002-filter-todo");

  await filter.selectOption("ready");
  await expect(card(page, new RegExp(T_READY))).toHaveCount(1);
  await expect(card(page, new RegExp(T_TODO))).toHaveCount(0);
  await expect(column(page, "执行中").locator("a.task-card-link")).toHaveCount(1);
  await expect(column(page, "受阻").locator("a.task-card-link")).toHaveCount(1);
  await expect(column(page, "待验收").locator("a.task-card-link")).toHaveCount(1);
  await shot(page, "task-002-filter-ready");
});

// ---------------------------------------------------------------- UI-TASK-003

test("UI-TASK-003 任务卡信息层级：五类卡片的副文案、引用与可点击进入详情", async ({ page }) => {
  // 有未完成依赖的 TODO 任务（依赖项处于 READY，未完成）。
  await newTask(page, {
    teamId: teamBoard,
    title: T_DEPS,
    owner: ROLE_MAIN,
    deps: [T_READY],
    files: ["a.md"],
  });

  await page.goto(`/#/teams/${teamBoard}/tasks`);
  const todoColumn = column(page, "TODO");

  // blocked 卡第二行是 blockedReason 原文（第一行是负责人）。
  const blockedCard = card(page, new RegExp(T_BLOCKED));
  await expect(blockedCard.locator(".task-card__lines p").nth(1)).toHaveText(BLOCKED_REASON);
  // 有未完成依赖的卡显示等待数量。
  await expect(card(page, new RegExp(T_DEPS))).toContainText("等待 1 项依赖完成");
  // ready 卡显示已就绪、可派工。
  await expect(card(page, new RegExp(T_READY))).toContainText("已就绪，可派工");
  // 无依赖的 todo 卡提示可以标记 READY。
  await expect(card(page, new RegExp(T_TODO))).toContainText("可尝试标记为 READY");
  // doing 卡在无依赖行时显示当前绑定的 Harness 名。
  await expect(card(page, new RegExp(T_DOING))).toContainText(`当前绑定 ${HARNESS}`);
  await shot(page, "task-003-cards");

  // 卡片含 KEY、负责人岗位与引用文件数。
  const readyCard = card(page, new RegExp(T_READY));
  await expect(readyCard.locator(".task-card__name")).toHaveText(
    /^[A-Z0-9]{2,4}-\d{3} 就绪任务甲$/,
  );
  await expect(readyCard).toContainText(`负责人 ${ROLE_MAIN}`);
  await expect(readyCard).toContainText("引用 1 个文件");

  // 卡片可点击进入任务详情。
  await blockedCard.click();
  await expect(page.getByRole("heading", { level: 1, name: T_BLOCKED })).toBeVisible();
  await expect(statusLine(page)).toContainText("受阻");

  // 首列按更新时间倒序：最后创建的依赖任务排在最前。
  await page.goto(`/#/teams/${teamBoard}/tasks`);
  const names = await todoColumn.locator(".task-card__name").allInnerTexts();
  expect(names[0]).toContain(T_DEPS);
  expect(names[1]).toContain(T_TODO);
  await shot(page, "task-003-todo-order");
});

// ---------------------------------------------------------------- UI-TASK-008

test("UI-TASK-008 看板与列表视图切换：看板区域与列表表头、超长标题 title 提示", async ({
  page,
}) => {
  const longTitle = `超长标题任务${"很长的需求描述".repeat(6)}`;
  await newTask(page, { teamId: teamBoard, title: longTitle, owner: ROLE_MAIN });

  await page.goto(`/#/teams/${teamBoard}/tasks`);
  await expect(board(page)).toBeVisible();

  // 切到列表：表头依次为 KEY、标题、负责人、状态、更新时间。
  await page.getByRole("button", { name: "列表", exact: true }).click();
  await expect(board(page)).toHaveCount(0);
  await expect(page.locator(".task-table thead th")).toHaveText([
    "KEY",
    "标题",
    "负责人",
    "状态",
    "更新时间",
  ]);

  // 行含 KEY、标题（超长以 title 提示）、负责人、状态标签与更新时间。
  const longRow = page.locator(".task-table tbody tr", { hasText: "超长标题任务" });
  await expect(longRow.locator(".task-table__link")).toHaveText(/^[A-Z0-9]{2,4}-\d{3}$/);
  const titleCell = longRow.locator("td.task-table__title");
  await expect(titleCell).toHaveText(longTitle);
  await expect(titleCell).toHaveAttribute("title", longTitle);
  await expect(longRow.locator("td").nth(2)).toHaveText(ROLE_MAIN);
  await expect(longRow.locator(".ui-tag")).toHaveText("TODO");
  await expect(longRow.locator("td").last()).not.toHaveText("");
  await shot(page, "task-008-list-view");

  // 点击行进入任务详情。
  await longRow.click();
  await expect(page.getByRole("heading", { level: 1, name: longTitle })).toBeVisible();
  await expect(statusLine(page)).toContainText("TODO");

  // 切回看板。
  await page.goto(`/#/teams/${teamBoard}/tasks`);
  await expect(board(page)).toBeVisible();
});

// ---------------------------------------------------------------- UI-TASK-009

test("UI-TASK-009 筛选无结果与可访问性：空筛选提示与看板横向键盘滚动", async ({ page }) => {
  // 前置条件：团队没有 TODO 任务。
  teamReady = await createTeam(page, "仅就绪对照组", ["就绪岗"]);
  await newTask(page, {
    teamId: teamReady,
    title: "对照组就绪任务",
    owner: "就绪岗",
    status: "READY",
  });

  await page.goto(`/#/teams/${teamReady}/tasks`);
  await page.getByLabel("TODO / READY").selectOption("todo");
  await page.getByRole("button", { name: "列表", exact: true }).click();
  await expect(page.getByText("当前筛选下没有任务。")).toBeVisible();
  await shot(page, "task-009-empty-filter");

  // 窄窗口下看板可横向滚动，键盘也能滚动。
  await page.getByRole("button", { name: "看板", exact: true }).click();
  await page.setViewportSize({ width: 760, height: 900 });
  const region = board(page);
  await expect(region).toHaveCount(1);
  const metrics = await region.evaluate((el) => ({
    scrollWidth: el.scrollWidth,
    clientWidth: el.clientWidth,
  }));
  expect(metrics.scrollWidth).toBeGreaterThan(metrics.clientWidth);
  await region.focus();
  await page.keyboard.press("ArrowRight");
  await expect.poll(async () => await region.evaluate((el) => el.scrollLeft)).toBeGreaterThan(0);
  await shot(page, "task-009-narrow-board");
});

// ---------------------------------------------------------------- UI-FORM-002

test("UI-FORM-002 任务名称为空提交：字段报错且不发起创建请求", async ({ page }) => {
  await page.goto(`/#/teams/${teamBoard}/tasks/new`);
  await expect(page.getByRole("heading", { name: "创建任务" })).toBeVisible();
  const name = page.getByLabel("任务名称");
  await expect(name).toHaveAttribute("aria-invalid", "false");

  await page.getByLabel("负责人岗位").selectOption("");

  const call = expectNoInvoke(page, "create_task", 1500);
  await page.getByRole("button", { name: "创建并分配" }).click();
  await call;

  await expect(page.locator(".ui-field__error")).toHaveText("请填写任务名称");
  await expect(name).toHaveAttribute("aria-invalid", "true");
  await expect(page.getByRole("heading", { name: "创建任务" })).toBeVisible();
  await shot(page, "form-002-empty-title", page.locator(".ui-field--invalid"));
});

// ---------------------------------------------------------------- UI-FORM-003

test("UI-FORM-003 无岗位时阻断负责人选择：加载中提示与空岗位引导", async ({ page }) => {
  teamNoRole = await createTeam(page, "无岗位验证组", []);

  // 加载中显示「正在加载岗位…」：把岗位查询延后到能看见加载态。
  await page.route("**/invoke", async (route) => {
    if ((route.request().postData() ?? "").includes('"list_roles"')) {
      await new Promise((resolve) => setTimeout(resolve, 1500));
    }
    await route.continue();
  });
  await page.goto(`/#/teams/${teamNoRole}/tasks/new`);
  await page.reload();
  await expect(page.getByRole("status", { name: "正在加载岗位…" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "还没有可用岗位" })).toBeVisible({
    timeout: 20_000,
  });
  await page.unroute("**/invoke");

  // 负责人岗位区显示引导文案，且没有可选的负责人。
  await expect(
    page.getByText("任务必须指定负责岗位，请先在「岗位与绑定」里添加岗位。"),
  ).toBeVisible();
  await expect(page.getByLabel("负责人岗位")).toHaveCount(0);
  await shot(page, "form-003-no-role");
});

test("UI-FORM-003 无岗位时无法提交", async ({ page }) => {
  await page.goto(`/#/teams/${teamNoRole}/tasks/new`);
  await expect(page.getByRole("heading", { name: "创建任务" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "还没有可用岗位" })).toBeVisible();
  await expect(page.getByRole("button", { name: "创建并分配" })).toBeDisabled();
});

// ---------------------------------------------------------------- UI-FORM-004

test("UI-FORM-004 负责人必选：不选负责人直接提交不发起创建请求", async ({ page }) => {
  await page.goto(`/#/teams/${teamBoard}/tasks/new`);
  await expect(page.getByRole("heading", { name: "创建任务" })).toBeVisible();
  await page.getByLabel("任务名称").fill("没有负责人的任务");

  // 负责人字段标记为必填。
  await expect(page.getByLabel("负责人岗位")).toHaveAttribute("required", "");
  await expect(page.locator(".ui-field__required")).toHaveCount(2);

  const call = expectNoInvoke(page, "create_task", 1500);
  await page.getByRole("button", { name: "创建并分配" }).click();
  await call;
  await expect(page.getByRole("heading", { name: "创建任务" })).toBeVisible();
});

// ---------------------------------------------------------------- UI-FORM-006

test("UI-FORM-006 文件搜索、锚点与去重移除：实时搜索、# 锚点、同路径去重与移除", async ({
  page,
}) => {
  await page.route("**/invoke", async (route) => {
    if ((route.request().postData() ?? "").includes('"search_workspace_files"')) {
      await new Promise((resolve) => setTimeout(resolve, 1200));
    }
    await route.continue();
  });
  await page.goto(`/#/teams/${teamBoard}/tasks/new`);
  await expect(page.getByRole("heading", { name: "创建任务" })).toBeVisible();

  // 1. 输入 doc 触发实时搜索：先看到「正在搜索文件…」，再看到匹配结果。
  await page.locator("#task-file-search").fill("doc");
  await expect(page.getByRole("status", { name: "正在搜索文件…" })).toBeVisible();
  await expect(page.locator(".task-form__suggestion", { hasText: "docs/doc.md" })).toBeVisible({
    timeout: 20_000,
  });
  await expect(
    page.getByText(
      "输入时实时搜索工作空间内的文件；用 # 追加锚点可直达具体章节，引用文件需包含验收标准。",
    ),
  ).toBeVisible();
  await shot(page, "form-006-search");

  // 2. 输入 a.md#章节 生成带锚点的引用。
  await page.locator("#task-file-search").fill("a.md#章节");
  await page.getByRole("button", { name: "添加", exact: true }).click();
  await expect(page.locator(".task-form__chip")).toHaveCount(1);
  await expect(page.locator(".task-form__chip")).toContainText("a.md#章节");
  await page.unroute("**/invoke");

  // 3. 再次输入 a.md：同一路径不再作为搜索结果出现，引用也不重复。
  await page.locator("#task-file-search").fill("a.md");
  await expect(page.locator(".task-form__suggestion")).toHaveCount(0);
  await expect(page.locator(".task-form__chip")).toHaveCount(1);

  // 4. 移除按钮的 aria-label 为「移除引用 <文件名>」，点击后该项消失。
  const remove = page.getByRole("button", { name: "移除引用 a.md#章节" });
  await expect(remove).toBeVisible();
  await remove.click();
  await expect(page.locator(".task-form__chip")).toHaveCount(0);
  await expect(page.getByText("尚未添加引用。")).toBeVisible();

  // 中文名 + 空格的路径同样能搜到并引用。
  await page.locator("#task-file-search").fill("镜头");
  await page.locator(".task-form__suggestion", { hasText: "素材/镜头 01.mov" }).click();
  await expect(page.locator(".task-form__chip")).toHaveCount(1);
  await expect(page.locator(".task-form__chip")).toContainText("素材/镜头 01.mov");
});

// ---------------------------------------------------------------- UI-FORM-007

test("UI-FORM-007 引用不存在的文件被拒：错误原因为文件不存在且表单保留内容", async ({ page }) => {
  await page.goto(`/#/teams/${teamBoard}/tasks/new`);
  await expect(page.getByRole("heading", { name: "创建任务" })).toBeVisible();
  await page.getByLabel("任务名称").fill("引用缺失文件的任务");
  const owner = page.getByLabel("负责人岗位");
  await owner.selectOption(
    (await owner.locator("option", { hasText: ROLE_MAIN }).getAttribute("value")) ?? "",
  );
  await page.locator("#task-file-search").fill("missing.md");
  await page.getByRole("button", { name: "添加", exact: true }).click();
  await expect(page.locator(".task-form__chip")).toContainText("missing.md");
  await page.getByRole("radio", { name: "READY", exact: true }).check();
  await page.getByRole("button", { name: "创建并分配" }).click();

  const banner = page.getByRole("alert");
  await expect(banner).toContainText("引用的文件在工作区中不存在：missing.md");
  await shot(page, "form-007-missing-file", banner);

  // 表单保留已填内容。
  await expect(page.getByRole("heading", { name: "创建任务" })).toBeVisible();
  await expect(page.getByLabel("任务名称")).toHaveValue("引用缺失文件的任务");
  await expect(owner).not.toHaveValue("");
  await expect(page.locator(".task-form__chip")).toContainText("missing.md");
});

// ---------------------------------------------------------------- UI-FORM-008

test("UI-FORM-008 说明字段可选：留空说明可保存，详情显示暂无说明。", async ({ page }) => {
  await page.goto(`/#/teams/${teamBoard}/tasks/new`);
  await expect(page.getByRole("heading", { name: "创建任务" })).toBeVisible();
  await expect(page.getByText("详细需求保留在文件中，这里只登记协作信息。")).toBeVisible();
  await page.getByLabel("任务名称").fill("没有说明的任务");
  const owner = page.getByLabel("负责人岗位");
  await owner.selectOption(
    (await owner.locator("option", { hasText: ROLE_MAIN }).getAttribute("value")) ?? "",
  );
  await page.getByLabel("说明", { exact: true }).fill("");
  await page.getByRole("button", { name: "创建并分配" }).click();

  await expect(page.getByRole("heading", { level: 1, name: "没有说明的任务" })).toBeVisible();
  const section = page.locator(".task-detail__section", {
    has: page.getByRole("heading", { name: "任务说明" }),
  });
  await expect(section.getByText("暂无说明。")).toBeVisible();
  await shot(page, "form-008-no-description", section);
});

// ---------------------------------------------------------------- UI-FORM-009

test("UI-FORM-009 依赖选择与循环依赖：依赖勾选列表、编辑排除自身、空态与环路拒绝", async ({
  page,
}) => {
  // 无可依赖任务时的空态。
  await page.goto(`/#/teams/${teamNoRole}/tasks/new`);
  await expect(page.getByRole("heading", { name: "本团队还没有任务" })).toBeVisible();
  await expect(page.getByText("先创建前置任务，再回来把它设为依赖。")).toBeVisible();
  await shot(page, "form-009-no-dependency-empty");

  // 依赖勾选项显示任务标识与标题。
  await page.goto(`/#/teams/${teamBoard}/tasks/new`);
  await expect(page.getByRole("heading", { name: "创建任务" })).toBeVisible();
  const option = page.locator("label.task-form__option", { hasText: T_READY });
  await expect(option.locator(".task-form__option-key")).toHaveText(/^[A-Z0-9]{2,4}-\d{3}$/);
  await expect(option.locator(".task-form__option-title")).toHaveText(T_READY);
  await expect(option.locator(".ui-tag")).toHaveText("READY");
  await option.locator("input").check();
  await expect(page.locator(".task-form__chip")).toContainText(
    `${await option.locator(".task-form__option-key").innerText()} · ${T_READY}`,
  );
  await expect(page.locator(".task-form__summary").first()).toContainText("依赖：");

  // 编辑任务 X 时依赖选项不含 X 自身。
  await page.goto(`/#/teams/${teamBoard}/tasks`);
  await card(page, new RegExp(T_READY)).click();
  await page.getByRole("button", { name: "编辑" }).click();
  await expect(page.getByRole("heading", { name: "编辑任务" })).toBeVisible();
  await expect(page.locator("label.task-form__option", { hasText: T_READY })).toHaveCount(0);
  await expect(page.locator("label.task-form__option", { hasText: T_TODO })).toHaveCount(1);

  // 构造 A→B、B→A：这里选中依赖 T_DEPS 的任务，再让 T_DEPS 反过来依赖它，保存被拒。
  await page.locator("label.task-form__option", { hasText: T_DEPS }).locator("input").check();
  // M3-07 起环路在选择依赖时就预检出来：显示错误并禁用保存（不再等保存时才被后端拒绝）。
  const banner = page.getByRole("alert");
  await expect(banner).toContainText(/依赖形成环路：.*→.*/);
  await expect(page.getByRole("button", { name: "保存", exact: true })).toBeDisabled();
  await shot(page, "form-009-cycle-error", banner);
  // 表单内容保留。
  await expect(page.getByRole("heading", { name: "编辑任务" })).toBeVisible();
  await expect(page.getByLabel("任务名称")).toHaveValue(T_READY);
});

// ---------------------------------------------------------------- UI-FORM-010

test("UI-FORM-010 底部规则说明与编辑保存：规则文案不含文件引用强制，编辑预填并可保存", async ({
  page,
}) => {
  await page.goto(`/#/teams/${teamBoard}/tasks/new`);
  await expect(page.getByRole("heading", { name: "创建任务" })).toBeVisible();
  await expect(page.locator(".task-form__note--rules")).toHaveText(
    "READY 必须具备负责人与已满足的依赖；填写的引用文件必须存在。循环依赖会阻止保存。",
  );
  await expect(page.getByText("必须有文件引用")).toHaveCount(0);
  await shot(page, "form-010-rules", page.locator(".task-form__note--rules"));

  // 编辑页头与字段预填。
  await page.goto(`/#/teams/${teamBoard}/tasks`);
  await card(page, new RegExp(T_TODO)).click();
  const taskId = page.url().split("/").pop() ?? "";
  await page.goto(`/#/teams/${teamBoard}/tasks/${taskId}/edit`);
  await expect(page.getByRole("heading", { name: "编辑任务" })).toBeVisible();
  await expect(page.getByLabel("任务名称")).toHaveValue(T_TODO);
  await expect(page.getByLabel("说明", { exact: true })).toHaveValue(TODO_DESC);
  await expect(page.locator(".task-form__chip")).toContainText("a.md");
  const owner = page.getByLabel("负责人岗位");
  await expect(owner.locator("option:checked")).toHaveText(ROLE_MAIN);

  // 修改说明保存，详情与刷新后一致。
  await page.getByLabel("说明", { exact: true }).fill(`${TODO_DESC}（已更新）`);
  await page.getByRole("button", { name: "保存", exact: true }).click();
  await expect(page.getByRole("heading", { level: 1, name: T_TODO })).toBeVisible();
  await expect(page.getByText(`${TODO_DESC}（已更新）`)).toBeVisible();
  await page.reload();
  await expect(page.getByText(`${TODO_DESC}（已更新）`)).toBeVisible();
  await shot(page, "form-010-edited");
});

// ---------------------------------------------------------------- UI-DETAIL-002

test("UI-DETAIL-002 任务说明与文件引用展示：锚点、失效引用、无引用与无说明", async ({ page }) => {
  const withRefs = "带引用的任务";
  await page.goto(`/#/teams/${teamBoard}/tasks/new`);
  await page.getByLabel("任务名称").fill(withRefs);
  const owner = page.getByLabel("负责人岗位");
  await owner.selectOption(
    (await owner.locator("option", { hasText: ROLE_MAIN }).getAttribute("value")) ?? "",
  );
  await page.getByLabel("说明", { exact: true }).fill("引用三个文件，其中一个稍后被删除。");
  for (const ref of ["a.md#章节", "素材/镜头 01.mov", "gone.md"]) {
    await page.locator("#task-file-search").fill(ref);
    await page.getByRole("button", { name: "添加", exact: true }).click();
  }
  await expect(page.locator(".task-form__chip")).toHaveCount(3);
  await page.getByRole("button", { name: "创建并分配" }).click();
  await expect(page.getByRole("heading", { level: 1, name: withRefs })).toBeVisible();

  // 文件建好后被删掉：引用应显示「文件已被移动或删除」（画板 4:435）并用 missing 样式。
  unlinkSync(join(workspace, "gone.md"));
  await page.reload();
  await expect(page.getByRole("heading", { level: 1, name: withRefs })).toBeVisible();

  const files = page.locator(".task-detail__files");
  const anchored = files.locator("li", { hasText: "a.md" });
  await expect(anchored).toHaveText("a.md #章节");
  await expect(anchored).toHaveClass(/file-chip(?!.*--missing)/);
  const chinese = files.locator("li", { hasText: "素材/镜头 01.mov" });
  await expect(chinese).toHaveText("素材/镜头 01.mov");
  await expect(chinese).not.toHaveText(/#/);
  const missing = files.locator("li", { hasText: "gone.md" });
  await expect(missing).toHaveClass(/file-chip--missing/);
  await expect(missing).toContainText("文件已被移动或删除");
  await expect(missing).not.toHaveText(/#/);
  await expect(page.getByText("引用三个文件，其中一个稍后被删除。")).toBeVisible();
  await shot(page, "detail-002-file-refs", files);

  // 无引用：不渲染引用列表且没有空白占位。
  const bare = await newTask(page, {
    teamId: teamBoard,
    title: "无引用无说明任务",
    owner: ROLE_MAIN,
  });
  expect(bare).toMatch(/^[A-Z0-9]{2,4}-\d{3}$/);
  await expect(page.locator(".task-detail__files")).toHaveCount(0);
  await expect(page.getByText("尚未添加引用。")).toHaveCount(0);
  const section = page.locator(".task-detail__section", {
    has: page.getByRole("heading", { name: "任务说明" }),
  });
  await expect(section.getByText("暂无说明。")).toBeVisible();
  await shot(page, "detail-002-bare", section);
});

// ---------------------------------------------------------------- UI-DETAIL-005

test("UI-DETAIL-005 标记受阻需填原因：留空不提交，填原因后显示受阻原因", async ({ page }) => {
  const key = await newTask(page, {
    teamId: teamBoard,
    title: "受阻原因校验任务",
    owner: ROLE_MAIN,
    status: "READY",
  });
  await transition(page, "执行中");
  await expect(statusLine(page)).toContainText("执行中");

  // 点「受阻」后留空提交：不提交，字段为必填。
  await transition(page, "受阻");
  const reason = page.getByLabel("受阻原因");
  await expect(reason).toHaveAttribute("required", "");
  await expect(page.getByRole("button", { name: "确认受阻" })).toBeDisabled();
  await expect(statusLine(page)).toContainText(`执行中 / ${key}`);
  await shot(page, "detail-005-reason-required", page.locator(".task-detail__block-form"));

  // 填原因后提交：状态变为受阻，且状态操作区显示受阻原因。
  await reason.fill("等待联调环境就绪");
  await page.getByRole("button", { name: "确认受阻" }).click();
  await expect(statusLine(page)).toHaveText(`受阻 / ${key}`);
  const actions = page.locator("#task-status-actions");
  await expect(actions.getByText("受阻原因：等待联调环境就绪")).toBeVisible();
  await shot(page, "detail-005-blocked", actions);
});

// ---------------------------------------------------------------- UI-DETAIL-006

test("UI-DETAIL-006 流转失败与无可用操作：失败显示后端错误文案，无操作时给出提示", async ({
  page,
}) => {
  // 1. 注入流转失败：后端返回错误时界面展示错误文案且状态不变。
  await page.goto(`/#/teams/${teamBoard}/tasks`);
  await card(page, new RegExp(T_READY)).click();
  await expect(statusLine(page)).toContainText("READY");
  const before = await statusLine(page).innerText();
  await page.route("**/invoke", async (route) => {
    if ((route.request().postData() ?? "").includes('"change_task_status"')) {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        headers: { "access-control-allow-origin": "*" },
        body: JSON.stringify({
          err: { kind: "conflict", message: "注入的流转失败：任务正在被执行" },
        }),
      });
      return;
    }
    await route.continue();
  });
  await transition(page, "执行中");
  await expect(page.locator(".task-detail__error")).toHaveText("注入的流转失败：任务正在被执行");
  await expect(statusLine(page)).toHaveText(before);
  await shot(page, "detail-006-transition-failed", page.locator("#task-status-actions"));
  await page.unroute("**/invoke");

  // 2. 已完成且就绪条件不满足的任务没有任何可执行的状态操作。
  const stale = "引用已删文件的任务";
  const staleKey = await newTask(page, {
    teamId: teamBoard,
    title: stale,
    owner: ROLE_MAIN,
    status: "READY",
    files: ["notes/待删文件.md"],
  });
  await transition(page, "执行中");
  await transition(page, "待验收");
  await transition(page, "已完成");
  await expect(statusLine(page)).toHaveText(`已完成 / ${staleKey}`);
  unlinkSync(join(workspace, "notes", "待删文件.md"));
  await page.reload();

  await expect(page.getByText("进入 READY 前还需要")).toBeVisible();
  await expect(page.getByText("当前没有可执行的状态操作。")).toBeVisible();
  await expect(page.locator(".task-detail__actions")).toHaveCount(0);
  await shot(page, "detail-006-no-actions", page.locator("#task-status-actions"));
});

// ---------------------------------------------------------------- UI-DETAIL-007

test("UI-DETAIL-007 依赖区块与跳转：前置与后续依赖分区显示并可跳转", async ({ page }) => {
  await openTask(page, teamBoard, T_DEPS);
  const side = page.locator(".task-detail__side");
  await expect(side.getByText("前置依赖")).toBeVisible();
  await expect(side.getByText("后续依赖")).toBeVisible();
  // 前置依赖：显示任务标识与标题，并带上状态标签。
  const beforeBox = side.locator("div", { has: page.getByText("前置依赖") });
  const before = beforeBox.locator(".task-detail__dep-list");
  await expect(before.locator("a")).toHaveCount(1);
  await expect(before.locator("a")).toHaveText(/^[A-Z0-9]{2,4}-\d{3} 就绪任务甲$/);
  await expect(before.locator(".ui-tag")).toHaveText("READY");
  // 后续依赖：没有后续任务时显示「暂无」。
  const afterBox = side.locator("div", { has: page.getByText("后续依赖") });
  await expect(afterBox.locator(".task-detail__dep-list")).toHaveCount(0);
  await expect(afterBox.getByText("暂无")).toBeVisible();
  await shot(page, "detail-007-dependencies", side);

  // 点击前置依赖项跳转到该任务详情。
  await before.locator("a").click();
  await expect(page.getByRole("heading", { level: 1, name: T_READY })).toBeVisible();
  await expect(statusLine(page)).toContainText("READY");
  const reverse = page.locator(".task-detail__side");
  await expect(
    reverse.locator("div", { has: page.getByText("后续依赖") }).locator(".task-detail__dep-list a"),
  ).toHaveText(new RegExp(T_DEPS));
});

// ---------------------------------------------------------------- UI-DETAIL-008

test("UI-DETAIL-008 岗位未绑定时的阻断提示：页头原因与警告卡", async ({ page }) => {
  await newTask(page, {
    teamId: teamBoard,
    title: "岗位未绑定的任务",
    owner: ROLE_REVIEW,
    status: "READY",
  });
  const start = page.getByRole("button", { name: "开始执行" });
  await expect(start).toBeDisabled();
  await expect(start).toHaveAttribute(
    "title",
    `还不能开始执行：负责岗位「${ROLE_REVIEW}」未绑定 Harness，请先在「岗位与绑定」中绑定`,
  );
  await expect(page.getByRole("heading", { name: "还不能开始执行" })).toBeVisible();
  await expect(
    page.getByText(`负责岗位「${ROLE_REVIEW}」未绑定 Harness，请先在「岗位与绑定」中绑定`),
  ).toBeVisible();
  await shot(page, "detail-008-unbound-role", page.locator(".ui-card--warning"));
});

// ---------------------------------------------------------------- UI-DETAIL-009

test("UI-DETAIL-009 状态色、刷新保持与异常兜底：七种状态文字标签、刷新一致与不存在的任务", async ({
  page,
}) => {
  const colors: Record<string, string> = {};
  const seen = async (label: string, taskKey: string, status: string): Promise<void> => {
    await expectStatus(page, label, taskKey, status);
    colors[status] = await statusLine(page).evaluate((el) => getComputedStyle(el).color);
  };

  const key = await newTask(page, { teamId: teamBoard, title: "状态巡检任务", owner: ROLE_MAIN });
  await seen("TODO", key, "todo");
  await transition(page, "READY");
  await seen("READY", key, "ready");
  await transition(page, "执行中");
  await seen("执行中", key, "doing");
  await transition(page, "受阻");
  await page.getByLabel("受阻原因").fill("等待数据");
  await page.getByRole("button", { name: "确认受阻" }).click();
  await seen("受阻", key, "blocked");
  await transition(page, "执行中");
  await transition(page, "待验收");
  await seen("待验收", key, "review");
  await shot(page, "detail-009-status-review");
  await transition(page, "已取消");
  await seen("已取消", key, "cancelled");

  // 已完成：另一条任务走到 DONE。
  const doneKey = await newTask(page, {
    teamId: teamBoard,
    title: "状态巡检完成任务",
    owner: ROLE_MAIN,
    status: "READY",
  });
  await transition(page, "执行中");
  await transition(page, "待验收");
  await transition(page, "已完成");
  await seen("已完成", doneKey, "done");
  await shot(page, "detail-009-status-done");

  // 七种状态都有文字标签；cancelled 与 todo 一样是中性色，blocked 是警告色。
  expect(Object.keys(colors).sort()).toEqual(
    ["blocked", "cancelled", "doing", "done", "ready", "review", "todo"].sort(),
  );
  expect(colors.cancelled).toBe(colors.todo);
  expect(colors.blocked).not.toBe(colors.cancelled);

  // 刷新后状态、说明、依赖、引用展示一致。
  await page.goto(`/#/teams/${teamBoard}/tasks`);
  await card(page, new RegExp(T_DEPS)).click();
  const snapshot = async () => ({
    status: await statusLine(page).innerText(),
    // 「任务说明」区同时含文件引用与说明正文。
    section: await page
      .locator(".task-detail__section", { has: page.getByRole("heading", { name: "任务说明" }) })
      .innerText(),
    deps: await page.locator(".task-detail__dep-list").first().innerText(),
  });
  const before = await snapshot();
  await page.reload();
  await expect(statusLine(page)).toHaveText(before.status);
  expect(await snapshot()).toEqual(before);
  await shot(page, "detail-009-after-reload");

  // 加载中显示「正在加载任务」。
  await page.route("**/invoke", async (route) => {
    if ((route.request().postData() ?? "").includes('"get_task"')) {
      await new Promise((resolve) => setTimeout(resolve, 1500));
    }
    await route.continue();
  });
  await page.reload();
  await expect(page.getByRole("status", { name: "正在加载任务" })).toBeVisible();
  await page.unroute("**/invoke");

  // 不存在的 taskId 不出现空白页，给出错误并可回退任务面板。
  await page.goto(`/#/teams/${teamBoard}/tasks/不存在任务-001`);
  const alert = page.getByRole("alert");
  await expect(alert).toBeVisible();
  await expect(page.getByRole("heading", { name: "任务详情" })).toBeVisible();
  await expect(alert).not.toHaveText("");
  await shot(page, "detail-009-missing-task", alert);
  await page.getByRole("button", { name: "返回任务面板" }).click();
  await expect(page).toHaveURL(new RegExp(`/teams/${teamBoard}/tasks$`));
  await expect(page.getByRole("heading", { name: "任务面板" })).toBeVisible();
  await expect(board(page)).toBeVisible();
});

test("UI-DETAIL-009 返回任务面板后保持之前的视图与筛选", async ({ page }) => {
  await page.goto(`/#/teams/${teamBoard}/tasks`);
  await page.getByLabel("TODO / READY").selectOption("ready");
  await page.getByRole("button", { name: "列表", exact: true }).click();
  await expect(page.locator(".task-table")).toBeVisible();
  // 视图与筛选写进地址栏：进详情时带在链接上。
  await expect(page).toHaveURL(/view=list&filter=ready$/);

  await page.locator(".task-table tbody tr").first().click();
  await expect(statusLine(page)).toBeVisible();
  await page.getByRole("button", { name: "返回任务面板" }).click();
  await expect(page).toHaveURL(new RegExp(`/teams/${teamBoard}/tasks\\?view=list&filter=ready$`));

  await expect(page.getByRole("button", { name: "列表", exact: true })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await expect(page.getByLabel("TODO / READY")).toHaveValue("ready");

  // 刷新页面仍是这个视图和筛选（状态在地址栏里，不靠组件内存）。
  await page.reload();
  await expect(page.getByRole("button", { name: "列表", exact: true })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await expect(page.getByLabel("TODO / READY")).toHaveValue("ready");
});
