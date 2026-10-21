// M4-03 一键接入的端到端：团队空间页的「接入 Leader」卡片，三段配置可见、复制按钮可用，
// 确认后工作空间里出现 .mcp.json 且内容正确。
// 真实界面 + 测试桥接 + 真实内核；工作空间是临时 Git 仓库。
import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { type Page, expect, test } from "@playwright/test";

import { UI_PORT } from "./ports.js";

test.describe.configure({ mode: "serial", timeout: 90_000 });

let workspace = "";
let team = "";

test.beforeAll(() => {
  workspace = mkdtempSync(join(tmpdir(), "teamspace-e2e-m403-"));
  writeFileSync(join(workspace, "README.md"), "# 项目\n");
  execFileSync("git", ["init", "-q", "-b", "main"], { cwd: workspace, stdio: "ignore" });
});

test.afterAll(() => {
  rmSync(workspace, { recursive: true, force: true });
});

async function createTeam(page: Page, name: string): Promise<string> {
  await page.goto("/#/teams/new");
  await page.getByLabel("团队名称").fill(name);
  await page.getByLabel("手动输入工作空间路径").fill(workspace);
  await page.getByRole("button", { name: "检测", exact: true }).click();
  await expect(page.getByText(/普通文件夹|Git 仓库/).first()).toBeVisible();
  await page.getByRole("button", { name: "创建团队", exact: true }).click();
  await expect(page).toHaveURL(/\/teams\/(?!new$)[^/]+$/);
  return /\/teams\/([^/?]+)/.exec(page.url())?.[1] ?? "";
}

test("准备：建一个团队，工作空间是临时 Git 仓库", async ({ page }) => {
  team = await createTeam(page, "M403接入团队");
  expect(team).not.toBe("");
});

test("接入 Leader 卡片：三段配置都能看到，命令带 socket 环境变量", async ({ page }) => {
  await page.goto(`/#/teams/${team}`);
  const card = page.locator(".li-panel");
  await expect(card).toBeVisible();
  await expect(card.getByRole("heading", { name: "接入 Leader" })).toBeVisible();

  // 三段配置各一块文本。
  await expect(card.locator(".li-section")).toHaveCount(3);
  await expect(card.locator(".li-section__title").nth(0)).toHaveText("Claude Code");
  await expect(card.locator(".li-section__title").nth(1)).toHaveText("工作空间 .mcp.json");
  await expect(card.locator(".li-section__title").nth(2)).toContainText("config.toml");

  // 第一段是 Claude Code 的登记命令；测试用的数据目录不是默认位置，所以要带 socket 环境变量。
  const claude = card.locator(".li-code").nth(0);
  await expect(claude).toContainText("claude mcp add teamspace");
  await expect(claude).toContainText("TEAMSPACE_CONTROL_SOCKET");

  // 第二段是 `.mcp.json` 片段，第三段是 Codex 的 TOML 片段。
  await expect(card.locator(".li-code").nth(1)).toContainText("mcpServers");
  await expect(card.locator(".li-code").nth(2)).toContainText("[mcp_servers.teamspace]");

  // 权限边界写在卡片上：合并与权限答复只能由人来做。
  await expect(card).toContainText("只能你在应用里做");
  await expect(card).toContainText("通过验收并合并到主分支");
  await expect(card).toContainText("答复 Agent 的权限请求");

  // Git 工作空间：提示文件会出现在改动里。
  await expect(card).toContainText("此文件会出现在 Git 改动中");
});

test("复制按钮：点一下就复制对应那段配置", async ({ page, context }) => {
  // 无头浏览器默认不给剪贴板权限，这里显式放行，才能验证真的复制到了内容。
  await context.grantPermissions(["clipboard-write", "clipboard-read"], {
    origin: `http://127.0.0.1:${String(UI_PORT)}`,
  });
  await page.goto(`/#/teams/${team}`);
  const card = page.locator(".li-panel");
  const first = card.locator(".li-section").nth(0);
  await first.getByRole("button", { name: "复制" }).click();
  await expect(first.getByRole("button", { name: "已复制" })).toBeVisible();

  const copied = await page.evaluate(() => navigator.clipboard.readText());
  expect(copied).toContain("claude mcp add teamspace");
  expect(copied).toContain("teamspace-mcp");
});

test("写入工作空间 .mcp.json：确认后文件出现且内容正确", async ({ page }) => {
  await page.goto(`/#/teams/${team}`);
  const card = page.locator(".li-panel");
  await card.getByRole("button", { name: "写入工作空间 .mcp.json" }).click();

  // 写入前先确认，弹窗里说明写到哪里。
  const dialog = page.getByRole("dialog");
  await expect(dialog).toContainText(".mcp.json");
  await expect(dialog).toContainText("保留");
  await dialog.getByRole("button", { name: "确认写入" }).click();

  await expect(card.getByText(/已新建 .*\.mcp\.json/)).toBeVisible();

  const file = join(workspace, ".mcp.json");
  expect(existsSync(file)).toBe(true);
  const content = JSON.parse(readFileSync(file, "utf8")) as {
    mcpServers: Record<string, { command: string; env?: Record<string, string> } | undefined>;
  };
  const server = content.mcpServers.teamspace;
  expect(server).toBeDefined();
  expect(server?.command ?? "").toContain("teamspace-mcp");
  expect(server?.env?.TEAMSPACE_CONTROL_SOCKET ?? "").toContain("teamspace-control.sock");
});
