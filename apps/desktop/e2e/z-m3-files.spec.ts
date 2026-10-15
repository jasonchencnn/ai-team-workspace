// M3-05：共享文件页（teams/:teamId/files）的端到端用例。
// 真实界面 + 测试桥接 + 真实内核 + 临时工作空间（普通文件夹，放几个 Markdown）；
// 数据准备走桥接，被测的列表、渲染与双向跳转用页面操作。
// 文件名 z- 开头，排在其他用例之后。
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { type Page, expect, test } from "@playwright/test";

import type { TaskRef } from "./z-qa-r2-helpers.js";
import { createTaskApi, openTask, setupTeam } from "./z-qa-r2-helpers.js";

const TEAM = "共享文件组";
const ROLE_FE = "前端开发";
const ROLE_BE = "后端开发";
const TASK_TITLE = "账号系统任务拆解";
const TASK_REF_PATH = "plans/task-breakdown.md";
const UNREFERENCED_PATH = "docs/spec.md";

let workspace = "";
let teamId = "";
let roleFe = "";
let taskFe: TaskRef;

test.describe.configure({ mode: "serial", timeout: 180_000 });

test.beforeAll(() => {
  workspace = mkdtempSync(join(tmpdir(), "teamspace-e2e-m3-files-"));
  const write = (rel: string, content: string) => {
    const target = join(workspace, rel);
    mkdirSync(join(target, ".."), { recursive: true });
    writeFileSync(target, content);
  };
  write("README.md", "# 项目 Nova\n\n账号系统迭代的共享文件。\n");
  write(
    TASK_REF_PATH,
    [
      "# 账号系统任务拆解",
      "",
      "## 目标",
      "",
      "让用户完成登录，并在输入错误、网络异常和会话失效时得到明确反馈。",
      "",
      "| 任务 | 岗位 |",
      "| --- | --- |",
      "| FE-023 登录界面 | 前端 |",
      "",
      "- 验收：成功进入应用；失败原因可读",
      "- 提交期间防止重复提交",
      "",
      "接口约定见 [brief](brief.md)，设计稿见 [设计稿](https://example.com/design.png)。",
      "",
    ].join("\n"),
  );
  write("plans/brief.md", "# 任务简报\n\n只读预览。\n");
  write(UNREFERENCED_PATH, "# 接口说明\n\n还没有任务引用它。\n");
  // 非 Markdown：共享文件页不显示。
  write("素材清单.txt", "镜头 1\n镜头 2\n");
});

test.afterAll(() => {
  rmSync(workspace, { recursive: true, force: true });
});

/** 打开共享文件页；只改 hash 是同文档导航，重新加载才能保证拿到内核当前的返回。 */
async function gotoFiles(page: Page, path = ""): Promise<void> {
  const query = path === "" ? "" : `?path=${encodeURIComponent(path)}`;
  await page.goto(`/#/teams/${teamId}/files${query}`);
  await page.reload();
  await expect(page.getByRole("heading", { level: 1, name: "共享文件" })).toBeVisible();
}

const treeItem = (page: Page, name: string) =>
  page.locator(".files-tree__item").filter({ hasText: name });

test("FILE-001 文件页列出工作空间的 Markdown，非 md 不显示", async ({ page }) => {
  const team = await setupTeam(TEAM, workspace, [
    { name: ROLE_FE, prefix: "R1" },
    { name: ROLE_BE, prefix: "R2" },
  ]);
  teamId = team.teamId;
  roleFe = team.roleIds[ROLE_FE] ?? "";
  expect(roleFe).not.toBe("");

  await gotoFiles(page);
  await expect(treeItem(page, "README.md")).toBeVisible();
  await expect(page.locator(".files-meta")).toContainText(`${TEAM} · `);
  await expect(page.locator(".files-meta")).toContainText("个 Markdown 文件");
  // 目录分组带文件数（岗位文件夹 Teamspace/ 是建团队时自动生成的，这里只断言用户放的目录）。
  await expect(
    page.locator(".files-tree__item--dir", { hasText: "plans" }).locator(".files-tree__count"),
  ).toHaveText("2");
  await expect(
    page.locator(".files-tree__item--dir", { hasText: "docs" }).locator(".files-tree__count"),
  ).toHaveText("1");
  await expect(treeItem(page, "素材清单.txt")).toHaveCount(0);
  await expect(page.getByText("指向工作空间之外的符号链接")).toBeVisible();

  // 没指定文件时默认打开第一个，地址栏记下当前文件。
  await expect(page.locator(".files-doc__path")).toHaveText("README.md");
  await expect(page).toHaveURL(/path=README.md/);
  await expect(page.locator(".files-doc__body h1")).toHaveText("项目 Nova");
  // 面包屑按画板显示「工作空间 / 团队 / 共享文件 / 当前文件」。
  await expect(page.locator(".topbar__crumbs")).toHaveText(
    "工作空间/Project/共享文件/README.md".replace("Project", TEAM),
  );
});

test("FILE-002 目录树选文件，只读渲染 Markdown（表格、列表、代码块）", async ({ page }) => {
  await gotoFiles(page);
  // plans 默认收起；点开才能看到里面的文件。
  await expect(treeItem(page, "brief.md")).toHaveCount(0);
  await page.locator(".files-tree__item--dir", { hasText: "plans" }).click();
  await treeItem(page, "task-breakdown.md").click();

  await expect(page.locator(".files-doc__path")).toHaveText(TASK_REF_PATH);
  await expect(page).toHaveURL(new RegExp(`path=plans%2Ftask-breakdown.md`));
  await expect(page.locator(".files-doc__head")).toContainText("只读");
  await expect(page.locator(".files-doc__head")).toContainText("更新于 今天");
  await expect(page.locator(".files-doc__head")).toContainText(
    /更新于 今天 \d{2}:\d{2} · \d+(\.\d+)? (B|KB|MB)/,
  );
  await expect(page.locator(".files-doc__body h1")).toHaveText("账号系统任务拆解");
  await expect(page.locator(".files-doc__body table")).toContainText("FE-023 登录界面");
  await expect(page.locator(".files-doc__body li").first()).toContainText("验收：成功进入应用");
  // 选中的文件在树里高亮，地址栏与面包屑同步。
  await expect(page.locator(".files-tree__item--active")).toHaveText("task-breakdown.md");
  await expect(page.locator(".topbar__crumbs")).toContainText(TASK_REF_PATH);
});

test("FILE-003 正文里的工作空间内 .md 链接在本页打开，外链不可点", async ({ page }) => {
  await gotoFiles(page, TASK_REF_PATH);
  await page.locator(".files-doc__body .files-doc__link", { hasText: "brief" }).click();
  await expect(page.locator(".files-doc__path")).toHaveText("plans/brief.md");
  await expect(page.locator(".files-doc__body h1")).toHaveText("任务简报");

  await gotoFiles(page, TASK_REF_PATH);
  const external = page.locator(".files-doc__body .files-doc__link--blocked", {
    hasText: "设计稿",
  });
  await expect(external).toHaveCount(1);
  await expect(external).toHaveAttribute("title", "外部链接，应用内不打开");
});

test("FILE-004 搜索按文件名筛选，清空后恢复", async ({ page }) => {
  await gotoFiles(page);
  await page.getByLabel("搜索文件名").fill("brief");
  await expect(treeItem(page, "brief.md")).toBeVisible();
  await expect(treeItem(page, "README.md")).toHaveCount(0);
  // 命中的目录自动展开，否则筛出来也看不见。
  await expect(treeItem(page, "task-breakdown.md")).toHaveCount(0);

  await page.getByLabel("搜索文件名").fill("没有这个文件");
  await expect(page.getByText("没有匹配的文件。")).toBeVisible();

  await page.getByLabel("搜索文件名").fill("");
  await expect(treeItem(page, "README.md")).toBeVisible();
});

test("FILE-005 建一个引用该文件的任务，文件页右侧出现并可跳转", async ({ page }) => {
  taskFe = await createTaskApi({
    teamId,
    roleId: roleFe,
    title: TASK_TITLE,
    status: "ready",
    refs: [TASK_REF_PATH],
  });

  await gotoFiles(page, TASK_REF_PATH);
  const refs = page.locator(".files-refs .files-ref");
  await expect(page.locator(".files-panel__count")).toHaveText("1");
  await expect(refs).toHaveCount(1);
  await expect(refs).toContainText(taskFe.key);
  await expect(refs).toContainText(TASK_TITLE);
  await expect(refs).toContainText("负责岗位 前端开发");
  await expect(refs).toContainText("READY");

  await refs.getByRole("link", { name: "打开任务 →" }).click();
  await expect(page.getByRole("heading", { level: 1, name: TASK_TITLE })).toBeVisible();
});

test("FILE-006 任务详情的 .md 引用跳回文件页并选中该文件", async ({ page }) => {
  await openTask(page, taskFe);
  await page.locator(".task-detail__files .file-chip__link", { hasText: TASK_REF_PATH }).click();
  await expect(page.getByRole("heading", { level: 1, name: "共享文件" })).toBeVisible();
  await expect(page.locator(".files-doc__path")).toHaveText(TASK_REF_PATH);
  await expect(page.locator(".files-refs")).toContainText(taskFe.key);
});

test("FILE-007 没人引用的文件显示「暂无任务引用」", async ({ page }) => {
  await gotoFiles(page, UNREFERENCED_PATH);
  await expect(page.locator(".files-doc__path")).toHaveText(UNREFERENCED_PATH);
  await expect(page.locator(".files-muted")).toHaveText("暂无任务引用");
  await expect(page.locator(".files-panel__count")).toHaveText("0");
});

test("FILE-008 打开已被删掉的文件时提示「文件已被移动或删除」", async ({ page }) => {
  await gotoFiles(page, "plans/gone.md");
  await expect(page.getByRole("heading", { name: "文件已被移动或删除" })).toBeVisible();

  // 任务详情里丢失的引用同样说明原因。
  await openTask(page, taskFe);
  rmSync(join(workspace, TASK_REF_PATH));
  await page.reload();
  await expect(page.locator(".task-detail__files")).toContainText("文件已被移动或删除");
  await expect(page.locator(".file-chip__link")).toHaveCount(0);
});

test("FILE-009 侧边栏「共享文件」是可用入口并高亮", async ({ page }) => {
  await gotoFiles(page);
  await expect(page.locator(".sidebar__nav .nav-item")).toHaveText([
    "团队空间",
    "任务面板",
    "共享文件",
    "岗位与绑定",
    "活动与审计",
    "待我处理M5 提供",
  ]);
  await page.locator(".sidebar__nav a", { hasText: "共享文件" }).click();
  await expect(page.locator(".sidebar__nav .nav-item--active")).toHaveText("共享文件");
  await expect(page.getByRole("heading", { level: 1, name: "共享文件" })).toBeVisible();
});
