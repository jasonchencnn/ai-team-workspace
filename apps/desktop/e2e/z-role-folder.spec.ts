// M1.2 岗位文件夹与工作日志（D14）端到端用例：真实界面 + 测试桥接 + 真实内核。
// 使用独立的临时工作空间（普通文件夹），Node 侧直接读文件断言；文件名 z- 开头，排在其他用例之后。
// 测试桥接的 reveal 只打印路径、不会打开 Finder。
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
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
    path: `${artifacts}rf-${String(shotNo).padStart(2, "0")}-${name}.png`,
    fullPage: false,
  });
}

const ROLE_A = "测试员";
const ROLE_A_RENAMED = "测试专员";
const ROLE_B = "校验员";
const ROLE_C = "报表/日期:汇总";
const ROLE_C_FOLDER = "报表_日期_汇总";
const BLOCK_REASON = "等待接口";
const NOTE = "用户自己放的草稿\n";

let workspace = "";
let teamId = "";
let prefixA = "";

test.describe.configure({ mode: "serial" });

test.beforeAll(() => {
  workspace = mkdtempSync(join(tmpdir(), "teamspace-e2e-role-folder-"));
  writeFileSync(join(workspace, "brief.txt"), "需求说明\n");
});

test.afterAll(() => {
  rmSync(workspace, { recursive: true, force: true });
});

function path(...parts: string[]): string {
  return join(workspace, ...parts);
}

function read(...parts: string[]): string {
  return readFileSync(path(...parts), "utf8");
}

/** 工作日志文件里的条目行：去掉 `- ` 与本地时间，剩下 `任务编号 · 描述` 或 `描述`，按文件顺序。 */
function journalLines(folder: string): string[] {
  return read("Teamspace", folder, "工作日志.md")
    .split("\n")
    .filter((line) => line.startsWith("- "))
    .map((line) => line.slice(2).replace(/^\S+ \S+ UTC[+-]\d{2}:\d{2} · /, ""));
}

async function openRole(page: Page, name: string): Promise<void> {
  await page.goto(`/#/teams/${teamId}/roles`);
  await page.locator(".role-chip", { hasText: name }).click();
  await expect(page.getByRole("heading", { level: 2, name })).toBeVisible();
}

/** 从岗位详情的「打开岗位工作台」进入工作台；工作日志与岗位文件夹路径都在工作台上。 */
async function openWorkbench(page: Page, name: string): Promise<void> {
  await openRole(page, name);
  await page.getByRole("link", { name: "打开岗位工作台" }).click();
  await expect(page).toHaveURL(/\/roles\/[^/]+\/workbench$/);
  await expect(page.getByRole("heading", { level: 1, name })).toBeVisible();
}

function folderSection(page: Page): Locator {
  return page.locator(".rw-panel", { has: page.getByRole("heading", { name: "岗位文件夹" }) });
}

function folderPath(page: Page): Locator {
  return folderSection(page).locator(".rw-path");
}

function journalItems(page: Page): Locator {
  return page.locator(".rw-journal-item .rw-journal-text");
}

test("创建团队后每个岗位都有文件夹、岗位说明、工作日志和过程文件", async ({ page }) => {
  await page.goto("/#/teams/new");
  await page.getByLabel("团队名称").fill("岗位文件夹团队");
  await page.getByLabel("手动输入工作空间路径").fill(workspace);
  await page.getByRole("button", { name: "检测", exact: true }).click();
  await expect(page.getByText("普通文件夹", { exact: true })).toBeVisible();
  prefixA = await addRole(page, ROLE_A, "负责回归测试与缺陷复现");
  await addRole(page, ROLE_B, "校验交付物");
  await addRole(page, ROLE_C, "");
  await page.getByRole("button", { name: "创建团队", exact: true }).click();
  await expect(page).toHaveURL(/\/teams\/(?!new$)[^/]+$/);
  await expect(page.getByRole("heading", { name: "团队空间" })).toBeVisible();
  teamId = /\/teams\/([^/?]+)/.exec(page.url())?.[1] ?? "";
  expect(teamId).not.toBe("");
  await shot(page, "team-home-after-create");

  expect(read("Teamspace", "README.md")).toContain("Teamspace 岗位工作区");
  for (const folder of [ROLE_A, ROLE_B, ROLE_C_FOLDER]) {
    expect(existsSync(path("Teamspace", folder, "岗位说明.md")), `${folder} 岗位说明`).toBe(true);
    expect(existsSync(path("Teamspace", folder, "工作日志.md")), `${folder} 工作日志`).toBe(true);
    expect(
      existsSync(path("Teamspace", folder, "过程文件", "README.md")),
      `${folder} 过程文件`,
    ).toBe(true);
  }

  // 岗位说明里的职责、岗位名、前缀与界面一致
  const brief = read("Teamspace", ROLE_A, "岗位说明.md");
  expect(brief).toContain(`# 岗位说明：${ROLE_A}`);
  expect(brief).toContain(`- 任务前缀：${prefixA}`);
  expect(brief).toContain("- 所属团队：岗位文件夹团队");
  expect(brief).toContain("- 状态：在岗");
  expect(brief).toContain("- 当前绑定 Harness：未绑定");
  expect(brief).toMatch(/## 职责\n\n负责回归测试与缺陷复现\n/);
  await openRole(page, ROLE_A);
  await expect(page.getByLabel("职责")).toHaveValue("负责回归测试与缺陷复现");

  // 刚创建的岗位日志只有一条「创建岗位」
  expect(journalLines(ROLE_A)).toEqual([`创建岗位「${ROLE_A}」（任务前缀 ${prefixA}）`]);
});

test("岗位详情显示相对路径与工作日志；绑定、派工、状态变化按顺序记入日志", async ({ page }) => {
  await openWorkbench(page, ROLE_A);
  const section = folderSection(page);
  await expect(folderPath(page)).toContainText(`Teamspace/${ROLE_A}`);
  await expect(journalItems(page)).toHaveText([`创建岗位「${ROLE_A}」（任务前缀 ${prefixA}）`]);
  await shot(page, "role-folder-initial", section);

  // 注册 Mock Harness 并绑定
  await page.goto(`/#/teams/${teamId}/roles?tab=harness`);
  await createMockHarness(page, "mock-folder");
  await expect(acpEntry(page, "mock-folder")).toBeVisible();

  await openRole(page, ROLE_A);
  await page.getByRole("button", { name: "绑定 Harness", exact: true }).click();
  const select = page.getByLabel("选择本团队的 Harness 实例");
  await select.selectOption(
    (await select.locator("option", { hasText: "mock-folder" }).getAttribute("value")) ?? "",
  );
  await page.getByRole("button", { name: "确认绑定" }).click();
  await expect(page.getByText("当前绑定 mock-folder")).toBeVisible();
  await openWorkbench(page, ROLE_A);
  await expect(journalItems(page).first()).toHaveText("绑定 Harness「mock-folder」");

  // 创建分派给该岗位的任务（TODO），再依次流转到受阻
  await page.goto(`/#/teams/${teamId}/tasks/new`);
  await page.getByLabel("任务名称").fill("回归测试");
  const owner = page.getByLabel("负责人岗位");
  await owner.selectOption(
    (await owner.locator("option", { hasText: ROLE_A }).getAttribute("value")) ?? "",
  );
  await page.locator("#task-file-search").fill("brief");
  await page.locator(".task-form__suggestion", { hasText: "brief.txt" }).click();
  await page.getByRole("radio", { name: "TODO", exact: true }).check();
  await page.getByRole("button", { name: "创建并分配" }).click();
  const status = page.locator(".task-detail__status");
  await expect(status).toContainText(`TODO / ${prefixA}-001`);
  await page.getByRole("button", { name: "READY", exact: true }).click();
  await expect(status).toContainText(`READY / ${prefixA}-001`);
  await page.getByRole("button", { name: "执行中", exact: true }).click();
  await expect(status).toContainText(`执行中 / ${prefixA}-001`);
  await page.getByRole("button", { name: "受阻", exact: true }).click();
  await page.getByLabel("受阻原因").fill(BLOCK_REASON);
  await page.getByRole("button", { name: "确认受阻" }).click();
  await expect(status).toContainText(`受阻 / ${prefixA}-001`);

  // 界面：最新的在最前
  await openWorkbench(page, ROLE_A);
  const key = `${prefixA}-001`;
  await expect(journalItems(page)).toHaveText([
    `${key} · 状态 执行中 → 受阻：${BLOCK_REASON}`,
    `${key} · 状态 READY → 执行中`,
    `${key} · 状态 TODO → READY`,
    `${key} · 创建任务 ${key}「回归测试」，负责岗位「${ROLE_A}」`,
    "绑定 Harness「mock-folder」",
    `创建岗位「${ROLE_A}」（任务前缀 ${prefixA}）`,
  ]);
  await shot(page, "role-journal-after-task-flow", folderSection(page));

  // 文件：同样的条目按时间先后追加
  expect(journalLines(ROLE_A)).toEqual([
    `创建岗位「${ROLE_A}」（任务前缀 ${prefixA}）`,
    "绑定 Harness「mock-folder」",
    `${key} · 创建任务 ${key}「回归测试」，负责岗位「${ROLE_A}」`,
    `${key} · 状态 TODO → READY`,
    `${key} · 状态 READY → 执行中`,
    `${key} · 状态 执行中 → 受阻：${BLOCK_REASON}`,
  ]);
  expect(read("Teamspace", ROLE_A, "岗位说明.md")).toContain(
    "- 当前绑定 Harness：mock-folder（Mock · ACP，启动命令 `teamspace-mock-agent`）",
  );
  // 其他岗位的日志不受影响
  expect(journalLines(ROLE_B)).toHaveLength(1);
});

test("岗位改名：文件夹与用户文件随之移动；与其他岗位同名的文件夹被拒绝", async ({ page }) => {
  writeFileSync(path("Teamspace", ROLE_A, "过程文件", "草稿.md"), NOTE);

  await openRole(page, ROLE_A);
  await page.getByLabel("岗位名称").fill(ROLE_A_RENAMED);
  await page.getByRole("button", { name: "保存岗位说明" }).click();
  await expect(page.getByRole("heading", { level: 2, name: ROLE_A_RENAMED })).toBeVisible();
  await expect(page.locator(".role-detail__folder-path")).toContainText(
    `Teamspace/${ROLE_A_RENAMED}`,
  );
  await page.getByRole("link", { name: "打开岗位工作台" }).click();
  await expect(folderPath(page)).toContainText(`Teamspace/${ROLE_A_RENAMED}`);
  await expect(journalItems(page).first()).toHaveText(
    `编辑岗位：岗位改名：「${ROLE_A}」→「${ROLE_A_RENAMED}」`,
  );
  await shot(page, "role-renamed", folderSection(page));

  expect(existsSync(path("Teamspace", ROLE_A))).toBe(false);
  expect(read("Teamspace", ROLE_A_RENAMED, "过程文件", "草稿.md")).toBe(NOTE);
  expect(read("Teamspace", ROLE_A_RENAMED, "岗位说明.md")).toContain(`- 岗位名：${ROLE_A_RENAMED}`);
  // 原有日志保留，并追加改名记录
  const lines = journalLines(ROLE_A_RENAMED);
  expect(lines[0]).toBe(`创建岗位「${ROLE_A}」（任务前缀 ${prefixA}）`);
  expect(lines.at(-1)).toBe(`编辑岗位：岗位改名：「${ROLE_A}」→「${ROLE_A_RENAMED}」`);

  // 改成与另一个岗位相同的文件夹名：被拒绝，岗位名与文件夹都不变
  await openRole(page, ROLE_B);
  await page.getByLabel("岗位名称").fill(ROLE_A_RENAMED);
  await page.getByRole("button", { name: "保存岗位说明" }).click();
  const alert = page.getByRole("alert");
  await expect(alert).toBeVisible();
  await expect(alert).toContainText(`Teamspace/${ROLE_A_RENAMED}`);
  await expect(alert).toContainText("无法把岗位改名");
  await shot(page, "rename-conflict-rejected", alert);
  expect(existsSync(path("Teamspace", ROLE_B, "岗位说明.md"))).toBe(true);
  expect(read("Teamspace", ROLE_B, "岗位说明.md")).toContain(`- 岗位名：${ROLE_B}`);
  expect(read("Teamspace", ROLE_A_RENAMED, "岗位说明.md")).toContain(`- 岗位名：${ROLE_A_RENAMED}`);
  await page.reload();
  await expect(page.locator(".role-chip", { hasText: ROLE_B })).toBeVisible();
});

test("岗位归档后文件夹仍在，岗位说明标注已归档", async ({ page }) => {
  await openRole(page, ROLE_B);
  await page.getByRole("button", { name: "归档岗位" }).click();
  await expect(page.locator(".role-chip", { hasText: ROLE_B })).toHaveCount(0);

  expect(existsSync(path("Teamspace", ROLE_B))).toBe(true);
  const brief = read("Teamspace", ROLE_B, "岗位说明.md");
  expect(brief).toContain("- 状态：已归档（");
  expect(brief).toContain("文件夹保留供查阅");
  expect(journalLines(ROLE_B).at(-1)).toBe(`归档岗位「${ROLE_B}」`);
  expect(existsSync(path("Teamspace", ROLE_B, "过程文件", "README.md"))).toBe(true);
  await page.goto(`/#/teams/${teamId}/roles`);
  await shot(page, "after-archive-role-list");
});

test("名称含 / : 的岗位：文件夹名被安全替换", async ({ page }) => {
  expect(existsSync(path("Teamspace", ROLE_C_FOLDER))).toBe(true);
  // 没有按 / 拆成多级目录
  expect(existsSync(path("Teamspace", "报表"))).toBe(false);
  expect(read("Teamspace", ROLE_C_FOLDER, "岗位说明.md")).toContain(`- 岗位名：${ROLE_C}`);

  await openWorkbench(page, ROLE_C);
  await expect(folderPath(page)).toContainText(`Teamspace/${ROLE_C_FOLDER}`);
  await shot(page, "sanitized-folder-name", folderSection(page));
});

test("工作台「在 Finder 中打开岗位文件夹」按钮可点击且无报错", async ({ page }) => {
  await openWorkbench(page, ROLE_A_RENAMED);
  const button = page.getByRole("button", { name: "在 Finder 中打开岗位文件夹" });
  await expect(button).toBeEnabled();
  const reply = page.waitForResponse(
    (response) =>
      response.url().endsWith("/invoke") &&
      (response.request().postData() ?? "").includes("reveal_role_folder"),
  );
  await button.click();
  const body = (await (await reply).json()) as { ok?: unknown; err?: unknown };
  expect(body.err).toBeUndefined();
  await expect(button).toBeEnabled();
  await expect(page.getByRole("alert")).toHaveCount(0);
  await shot(page, "reveal-button", folderSection(page));
});

/** 在创建团队页添加一个自定义岗位，返回自动填入的任务前缀。 */
async function addRole(page: Page, name: string, description: string): Promise<string> {
  await page.getByRole("button", { name: "+ 添加岗位" }).click();
  const card = page.locator(".tc-role").last();
  await card.getByLabel("岗位名称").fill(name);
  await card.getByLabel("岗位名称").blur();
  const prefix = card.getByLabel("任务编号前缀");
  await expect(prefix).toHaveValue(/^[A-Z][A-Z0-9]{1,3}$/);
  if (description !== "") {
    await card.getByLabel("职责").fill(description);
  }
  return await prefix.inputValue();
}
