// M1.3 端到端用例：ACP 接入（预设、握手、编辑）与岗位工作台（绑定、任务、日志、最近文件、换绑）。
// 真实界面 + 测试桥接 + 真实内核；Mock 接入的握手会真的启动内置 Mock ACP Agent 子进程。
// 使用独立的临时工作空间（普通文件夹），Node 侧直接写过程文件；文件名 z- 开头，排在其他用例之后。
import { mkdirSync, mkdtempSync, rmSync, utimesSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";
import { fileURLToPath } from "node:url";

import { type Locator, type Page, expect, test } from "@playwright/test";

import { acpEntry, createMockHarness } from "./harnessHelpers.js";

const artifacts = fileURLToPath(new URL("../e2e-artifacts/", import.meta.url));
mkdirSync(artifacts, { recursive: true });

let shotNo = 0;
async function shot(page: Page, name: string, focus?: Locator): Promise<void> {
  if (focus !== undefined) {
    await focus.scrollIntoViewIfNeeded();
  }
  shotNo += 1;
  await page.screenshot({
    path: `${artifacts}m13-${String(shotNo).padStart(2, "0")}-${name}.png`,
    fullPage: false,
  });
}

const TEAM = "工作台团队";
const ROLE_BOUND = "编码员";
const ROLE_FREE = "观察员";
const DUTY = "负责实现需求并自测";
const GUIDELINES = "提交前先跑检查";
const MOCK = "mock-acp";
const MOCK_RENAMED = "mock-acp-edited";
const BAD = "bad-agent";
const BAD_COMMAND = "/nonexistent/teamspace-no-such-agent";

let workspace = "";
let teamId = "";
let prefixBound = "";

test.describe.configure({ mode: "serial" });

test.beforeAll(() => {
  workspace = mkdtempSync(join(tmpdir(), "teamspace-e2e-m13-"));
  writeFileSync(join(workspace, "brief.txt"), "需求说明\n");
});

test.afterAll(() => {
  rmSync(workspace, { recursive: true, force: true });
});

/** 在创建团队页添加一个自定义岗位，返回自动填入的任务前缀。 */
async function addRole(page: Page, name: string, description: string): Promise<string> {
  await page.getByRole("button", { name: "+ 添加岗位" }).click();
  const card = page.locator(".tc-role").last();
  await card.getByLabel("岗位名称").fill(name);
  await card.getByLabel("岗位名称").blur();
  const prefix = card.getByLabel("任务编号前缀");
  await expect(prefix).toHaveValue(/^[A-Z][A-Z0-9]{1,3}$/);
  await card.getByLabel("职责").fill(description);
  return await prefix.inputValue();
}

async function openHarnessTab(page: Page): Promise<void> {
  await page.goto(`/#/teams/${teamId}/roles?tab=harness`);
  await expect(page.getByRole("tab", { name: "Harness 接入" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
}

/** 从团队空间的岗位卡进入岗位工作台。 */
async function openWorkbenchFromHome(page: Page, name: string): Promise<void> {
  await page.goto(`/#/teams/${teamId}`);
  await expect(page.getByRole("heading", { name: "团队空间" })).toBeVisible();
  await page.locator(".th-role", { hasText: name }).getByRole("link", { name }).click();
  await expect(page).toHaveURL(/\/teams\/[^/]+\/roles\/[^/]+\/workbench$/);
  await expect(page.getByRole("heading", { level: 1, name })).toBeVisible();
}

function bindingPanel(page: Page): Locator {
  return page.locator(".rw-panel", { has: page.getByRole("heading", { name: "当前绑定" }) });
}

function filesPanel(page: Page): Locator {
  return page.locator(".rw-panel", { has: page.getByRole("heading", { name: "最近文件" }) });
}

test("创建工作台团队：两个岗位，职责与工作约定写入", async ({ page }) => {
  await page.goto("/#/teams/new");
  await page.getByLabel("团队名称").fill(TEAM);
  await page.getByLabel("手动输入工作空间路径").fill(workspace);
  await page.getByRole("button", { name: "检测", exact: true }).click();
  await expect(page.getByText("普通文件夹", { exact: true })).toBeVisible();
  prefixBound = await addRole(page, ROLE_BOUND, DUTY);
  await addRole(page, ROLE_FREE, "旁观并记录");
  await page.getByRole("button", { name: "创建团队", exact: true }).click();
  await expect(page).toHaveURL(/\/teams\/(?!new$)[^/]+$/);
  teamId = /\/teams\/([^/?]+)/.exec(page.url())?.[1] ?? "";
  expect(teamId).not.toBe("");

  // 工作约定在岗位详情里填写
  await page.goto(`/#/teams/${teamId}/roles`);
  await page.locator(".role-chip", { hasText: ROLE_BOUND }).click();
  await page.getByLabel("工作约定").fill(GUIDELINES);
  await page.getByRole("button", { name: "保存岗位说明" }).click();
  await expect(page.getByLabel("工作约定")).toHaveValue(GUIDELINES);
  await shot(page, "team-created-roles");
});

test("ACP 接入：新建接入的预设网格显示 12 个预设与本机检测标签", async ({ page }) => {
  await openHarnessTab(page);
  await expect(page.getByText("本团队还没有接入")).toBeVisible();
  await page.getByRole("button", { name: "新建接入" }).click();

  const grid = page.locator(".acp-preset-grid");
  await expect(grid.locator(".acp-preset")).toHaveCount(12);
  for (const label of [
    "Claude Code",
    "Codex",
    "MiniMax Code",
    "Gemini CLI",
    "CodeBuddy",
    "Qoder CLI",
    "Grok Build",
    "Qwen Code",
    "Kimi CLI",
    "OpenCode",
  ]) {
    await expect(grid.locator(".acp-preset", { hasText: label })).toContainText(
      /本机已检测到|未检测到/,
    );
  }
  await expect(grid.locator(".acp-preset", { hasText: "自定义 ACP Agent" })).toContainText(
    "自由配置",
  );
  await expect(grid.locator(".acp-preset", { hasText: "Mock" })).toContainText("内置");
  await shot(page, "acp-preset-grid", grid);

  // 选中预设后进入第二步并预填启动命令
  await grid.locator(".acp-preset", { hasText: "Mock" }).click();
  await expect(page.getByRole("heading", { name: /第二步 确认配置（已选：Mock）/ })).toBeVisible();
  await expect(page.getByLabel("启动命令")).toHaveValue("teamspace-mock-agent");
  await shot(page, "acp-create-step-two");
});

test("ACP 接入：新建 Mock 接入后握手为已连接，显示协议版本与 Agent 名称", async ({ page }) => {
  await openHarnessTab(page);
  await createMockHarness(page, MOCK);

  const entry = acpEntry(page, MOCK);
  await expect(entry).toContainText("测试用");
  await expect(entry).toContainText(/ACP v\d+/);
  await expect(entry).toContainText("teamspace-mock-agent");
  await expect(entry).toContainText("暂未被任何岗位绑定");
  // 握手成功是常规信息：说明行用灰色，不用警告色
  await expect(entry.locator(".acp-entry__detail--muted")).toContainText("握手成功");
  await expect(entry.locator(".acp-entry__detail--warning")).toHaveCount(0);
  await expect(entry.locator(".acp-entry__detail--danger")).toHaveCount(0);
  await shot(page, "acp-mock-connected", entry);

  await entry.getByRole("button", { name: "编辑" }).click();
  await expect(page.getByRole("heading", { name: "握手结果" })).toBeVisible();
  const facts = page.locator(".harness-detail__facts");
  await expect(page.locator(".harness-detail__health-head .ui-tag")).toHaveText("已连接");
  await expect(facts).toContainText(/ACP v\d+/);
  await expect(facts.locator(".harness-detail__fact", { hasText: "Agent 名称" })).toContainText(
    "teamspace-mock-agent",
  );
  await shot(page, "acp-mock-detail");
});

test("ACP 接入：自定义 ACP Agent 指向不存在的命令，握手异常并显示原因", async ({ page }) => {
  await openHarnessTab(page);
  await page.getByRole("button", { name: "新建接入" }).click();
  await page.locator(".acp-preset", { hasText: "自定义 ACP Agent" }).click();
  // 自定义预设必填启动命令：留空时保存按钮不可用
  await page.getByLabel("实例名称").fill(BAD);
  await expect(page.getByRole("button", { name: "保存并握手检查" })).toBeDisabled();
  await page.getByLabel("启动命令").fill(BAD_COMMAND);
  await page.getByRole("button", { name: "保存并握手检查" }).click();

  const entry = acpEntry(page, BAD);
  await expect(entry).toBeVisible();
  await expect(entry.locator(".ui-tag", { hasText: "异常" })).toBeVisible();
  await expect(entry.locator(".acp-entry__detail--danger")).toContainText(
    "启动命令不存在或不可执行",
  );
  await expect(entry.locator(".acp-entry__code")).toHaveText(BAD_COMMAND);
  await shot(page, "acp-custom-error", entry);
});

test("ACP 接入：编辑接入后握手状态变回未检查，重新握手后恢复已连接", async ({ page }) => {
  await openHarnessTab(page);
  await acpEntry(page, MOCK).getByRole("button", { name: "编辑" }).click();
  await page.getByLabel("实例名称").fill(MOCK_RENAMED);
  await page.getByRole("button", { name: "保存启动配置" }).click();
  await expect(page.locator(".harness-detail__health-head .ui-tag")).toHaveText("未检查");
  await shot(page, "acp-edit-unchecked");

  await page.getByRole("button", { name: "返回接入列表" }).click();
  const entry = acpEntry(page, MOCK_RENAMED);
  await expect(entry.locator(".ui-tag", { hasText: "未检查" })).toBeVisible();
  await entry.getByRole("button", { name: "握手检查" }).click();
  await expect(entry.locator(".ui-tag", { hasText: "已连接" })).toBeVisible();
});

test("ACP 接入：把岗位绑定到接入后，接入列表显示被绑定岗位", async ({ page }) => {
  await page.goto(`/#/teams/${teamId}/roles`);
  await page.locator(".role-chip", { hasText: ROLE_BOUND }).click();
  await page.getByRole("button", { name: "绑定 Harness", exact: true }).click();
  const select = page.getByLabel("选择本团队的 Harness 实例");
  await select.selectOption(
    (await select.locator("option", { hasText: MOCK_RENAMED }).getAttribute("value")) ?? "",
  );
  await page.getByRole("button", { name: "确认绑定" }).click();
  await expect(page.getByText(`当前绑定 ${MOCK_RENAMED}`)).toBeVisible();

  await openHarnessTab(page);
  await expect(acpEntry(page, MOCK_RENAMED)).toContainText(`被绑定${ROLE_BOUND}`);
  await expect(acpEntry(page, BAD)).toContainText("暂未被任何岗位绑定");
  await shot(page, "acp-list-bound", acpEntry(page, MOCK_RENAMED));
});

test("岗位工作台：从团队空间进入，显示岗位信息、绑定与握手状态、职责与工作约定", async ({
  page,
}) => {
  await openWorkbenchFromHome(page, ROLE_BOUND);

  await expect(page.locator(".topbar__crumbs")).toContainText(TEAM);
  await expect(page.locator(".topbar__crumbs")).toContainText(ROLE_BOUND);

  // 侧边导航：工作台属于「团队空间」，不点亮「岗位与绑定」
  const nav = page.locator(".sidebar__nav");
  await expect(nav.locator(".nav-item--active")).toHaveText(["团队空间"]);

  // 顶栏：普通文件夹工作区显示「工作区 <文件夹名>」，完整路径放在 title
  const topbarBranch = page.locator(".topbar__branch");
  await expect(topbarBranch.locator(".topbar__branch-label")).toHaveText("工作区");
  await expect(topbarBranch.locator(".topbar__folder")).toHaveText(basename(workspace));
  await expect(topbarBranch).toHaveAttribute("title", workspace);

  await expect(page.locator(".rw-meta")).toContainText(prefixBound);
  await expect(page.locator(".rw-meta")).toContainText("普通岗位");

  const binding = bindingPanel(page);
  await expect(binding).toContainText(MOCK_RENAMED);
  await expect(binding).toContainText("Mock 测试用");
  await expect(binding.locator(".rw-fact", { hasText: "握手状态" })).toContainText("已连接");
  await expect(binding.locator(".rw-fact", { hasText: "Agent 版本" })).toContainText(
    "teamspace-mock-agent",
  );
  // 握手成功是常规信息：说明行用灰色，不用警告色
  await expect(binding.locator(".rw-note--muted")).toContainText("握手成功");
  await expect(binding.locator(".rw-note--warning")).toHaveCount(0);
  await expect(binding.locator(".rw-note--danger")).toHaveCount(0);
  // 换绑说明的措辞
  await expect(binding).toContainText("进行中的任务完成后生效");

  const folder = page.locator(".rw-panel", {
    has: page.getByRole("heading", { name: "岗位文件夹" }),
  });
  await expect(folder.locator(".rw-path")).toContainText(`Teamspace/${ROLE_BOUND}`);
  await expect(folder).toContainText("职责");
  await expect(folder).toContainText(DUTY);
  await expect(folder).toContainText("工作约定");
  await expect(folder).toContainText(GUIDELINES);
  await shot(page, "workbench-overview");
});

test("岗位工作台：创建 2 个任务后最近任务出现，点击进入详情", async ({ page }) => {
  await openWorkbenchFromHome(page, ROLE_BOUND);
  await expect(page.getByText("暂无任务")).toBeVisible();

  for (const title of ["工作台任务一", "工作台任务二"]) {
    await page.goto(`/#/teams/${teamId}/tasks/new`);
    await page.getByLabel("任务名称").fill(title);
    const owner = page.getByLabel("负责人岗位");
    await owner.selectOption(
      (await owner.locator("option", { hasText: ROLE_BOUND }).getAttribute("value")) ?? "",
    );
    await page.locator("#task-file-search").fill("brief");
    await page.locator(".task-form__suggestion", { hasText: "brief.txt" }).click();
    await page.getByRole("radio", { name: "TODO", exact: true }).check();
    await page.getByRole("button", { name: "创建并分配" }).click();
    await expect(page.locator(".task-detail__status")).toContainText("TODO");
  }

  await openWorkbenchFromHome(page, ROLE_BOUND);
  const rows = page.locator(".rw-task-row");
  await expect(rows).toHaveCount(2);
  await expect(rows.nth(0)).toContainText("工作台任务二");
  await expect(rows.nth(1)).toContainText("工作台任务一");
  await expect(rows.nth(0)).toContainText(`${prefixBound}-002`);
  await shot(page, "workbench-recent-tasks");

  await rows.filter({ hasText: "工作台任务一" }).click();
  await expect(page).toHaveURL(/\/teams\/[^/]+\/tasks\/[^/]+$/);
  await expect(page.getByRole("heading", { name: "工作台任务一" })).toBeVisible();
  await shot(page, "workbench-task-detail");
});

test("岗位工作台：工作日志显示创建岗位、绑定与创建任务条目", async ({ page }) => {
  await openWorkbenchFromHome(page, ROLE_BOUND);
  const items = page.locator(".rw-journal-item .rw-journal-text");
  await expect(items.first()).toContainText("工作台任务二");
  await expect(items.filter({ hasText: `创建岗位「${ROLE_BOUND}」` })).toHaveCount(1);
  await expect(items.filter({ hasText: `绑定 Harness「${MOCK_RENAMED}」` })).toHaveCount(1);
  await expect(items.filter({ hasText: `${prefixBound}-001 · 创建任务` })).toHaveCount(1);
  await expect(page.locator(".rw-journal-time").first()).toBeVisible();
  await shot(page, "workbench-journal", page.locator(".rw-journal"));
});

test("岗位工作台：无过程文件时显示空状态；写入两个文件后按修改时间倒序，点击无报错", async ({
  page,
}) => {
  await openWorkbenchFromHome(page, ROLE_BOUND);
  await expect(filesPanel(page)).toContainText("暂无过程文件");
  await shot(page, "workbench-no-files", filesPanel(page));

  const processDir = join(workspace, "Teamspace", ROLE_BOUND, "过程文件");
  const older = join(processDir, "较早的草稿.md");
  const newer = join(processDir, "较新的结果.md");
  writeFileSync(older, "早\n");
  writeFileSync(newer, "晚\n");
  const now = Date.now() / 1000;
  utimesSync(older, now - 7200, now - 7200);
  utimesSync(newer, now - 60, now - 60);

  await page.reload();
  const names = filesPanel(page).locator(".rw-file-row__name");
  await expect(names).toHaveText(["较新的结果.md", "较早的草稿.md"]);
  await expect(filesPanel(page).locator(".rw-file-row__time").first()).not.toHaveText("");
  await shot(page, "workbench-recent-files", filesPanel(page));

  // 点击文件：触发 open_role_path 且后端不报错（桥接只打印路径，不会真的打开 Finder）
  const reply = page.waitForResponse(
    (response) =>
      response.url().endsWith("/invoke") &&
      (response.request().postData() ?? "").includes("open_role_path"),
  );
  await filesPanel(page).locator(".rw-file-row", { hasText: "较新的结果.md" }).click();
  const body = (await (await reply).json()) as { ok?: unknown; err?: unknown };
  expect(body.err).toBeUndefined();
  await expect(page.getByRole("alert")).toHaveCount(0);
});

test("岗位工作台：未绑定岗位显示「未绑定 · 暂不派工」，绑定 Harness 跳到岗位页并展开绑定选择", async ({
  page,
}) => {
  await openWorkbenchFromHome(page, ROLE_FREE);
  const binding = bindingPanel(page);
  await expect(binding.locator(".rw-binding__unbound")).toContainText("未绑定");
  await expect(binding.locator(".rw-binding__unbound")).toContainText("暂不派工");
  await expect(filesPanel(page)).toContainText("暂无过程文件");
  await shot(page, "workbench-unbound");

  await binding.getByRole("button", { name: "绑定 Harness" }).click();
  await expect(page).toHaveURL(/\/roles\?tab=roles&role=[^&]+$/);
  await expect(page.getByRole("heading", { level: 2, name: ROLE_FREE })).toBeVisible();
  await expect(page.getByLabel("选择本团队的 Harness 实例")).toBeVisible();
  await shot(page, "workbench-bind-jump");
});

test("岗位工作台：换绑 Harness 跳到岗位与绑定页并选中该岗位；返回团队空间", async ({ page }) => {
  await openWorkbenchFromHome(page, ROLE_BOUND);
  await bindingPanel(page).getByRole("button", { name: "换绑 Harness" }).click();
  await expect(page).toHaveURL(/\/roles\?tab=roles&role=[^&]+$/);
  await expect(page.getByRole("heading", { name: "岗位与绑定" })).toBeVisible();
  await expect(page.getByRole("heading", { level: 2, name: ROLE_BOUND })).toBeVisible();
  await expect(page.getByLabel("选择本团队的 Harness 实例")).toBeVisible();
  await shot(page, "workbench-rebind-jump");

  await openWorkbenchFromHome(page, ROLE_BOUND);
  await page.getByRole("button", { name: "返回团队空间" }).click();
  await expect(page).toHaveURL(new RegExp(`/teams/${teamId}$`));
  await expect(page.getByRole("heading", { name: "团队空间" })).toBeVisible();
  await shot(page, "workbench-back-home");
});
