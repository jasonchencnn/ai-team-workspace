// M7-04 端到端：数据备份与恢复。真实界面 + 测试桥接 + 真实内核；工作空间是临时 Git 仓库，
// 备份写到临时目录。覆盖：设置页导出备份（用「选择目录」按钮，桥接返回 E2E_BACKUP_DIR）
// → 看到导出结果与团队/工作空间路径；查看备份摘要 → 恢复走到高风险确认弹窗 → 确认后恢复完成。
import { type Page, expect, test } from "@playwright/test";

const TIMEOUT = 25_000;

let team = "";
let teamName = "";

/**
 * 准备一个团队用于备份。
 *
 * 已有团队时直接复用：新建团队会排在团队列表最前面，把 `/` 的默认落点抢走，
 * 让后面依赖默认团队的用例（如 z-qa-exploratory）跑到别的团队上去。
 */
async function ensureTeam(page: Page, name: string, workspace: string, role: string) {
  await page.goto("/");
  await page.waitForURL(/\/teams\/.+/, { timeout: TIMEOUT });
  const existing = /\/teams\/([^/?]+)/.exec(page.url())?.[1] ?? "";
  if (existing !== "" && !page.url().endsWith("/teams/new")) {
    return existing;
  }
  await page.goto("/#/teams/new");
  await page.getByLabel("团队名称").fill(name);
  await page.getByLabel("手动输入工作空间路径").fill(workspace);
  await page.getByRole("button", { name: "检测", exact: true }).click();
  await expect(page.getByText(/普通文件夹|Git 仓库/).first()).toBeVisible();
  await page.getByRole("button", { name: "+ 添加岗位" }).click();
  const card = page.locator(".tc-role").last();
  await card.getByLabel("岗位名称").fill(role);
  await card.getByLabel("岗位名称").blur();
  await page.getByRole("button", { name: "创建团队", exact: true }).click();
  await expect(page).toHaveURL(/\/teams\/(?!new$)[^/]+$/);
  return /\/teams\/([^/?]+)/.exec(page.url())?.[1] ?? "";
}

/** 顶栏面包屑里的团队名（工作空间 / 团队名）。 */
async function currentTeamName(page: Page): Promise<string> {
  const crumb = page.locator("[aria-label='当前位置']");
  return (await crumb.innerText()).split("/").pop()?.trim() ?? "";
}

test.describe.configure({ mode: "serial", timeout: 120_000 });

test("准备：准备一个带岗位文件夹的团队", async ({ page }) => {
  const workspace = process.env.E2E_WORKSPACE ?? "";
  team = await ensureTeam(page, "备份团队", workspace, "备份员");
  expect(team).not.toBe("");
  await page.goto(`/#/teams/${team}`);
  teamName = await currentTeamName(page);
  expect(teamName).not.toBe("");
});

test("设置页导出备份：清单、数据库快照与岗位文件夹副本", async ({ page }) => {
  await page.goto(`/#/teams/${team}/settings`);
  await expect(page.getByRole("heading", { name: "数据备份" })).toBeVisible();

  // 选目录：桥接返回 E2E_BACKUP_DIR（模拟用户在系统对话框里选了这个目录）。
  await page.getByRole("button", { name: "选择目录" }).click();
  const destInput = page.getByLabel("备份目标目录");
  await expect(destInput).toHaveValue(/\S/);
  const destDir = await destInput.inputValue();
  expect(destDir).not.toBe("");

  await page.getByRole("button", { name: "导出备份" }).click();
  const result = page.getByTestId("backup-export-result");
  await expect(result).toBeVisible({ timeout: TIMEOUT });
  await expect(result).toContainText("已导出");
  await expect(result).toContainText(teamName);
  // 岗位文件夹也在清单里（每个岗位一行）。
  await expect(result.locator(".settings__list--nested li").first()).toBeVisible();

  // 备份目录建在目标目录下，清单与数据库快照都在里面。
  const backupDir = (await result.locator("dd").first().innerText()).trim();
  expect(backupDir.startsWith(destDir)).toBe(true);
  expect(backupDir).toContain("teamspace-backup-");
});

test("恢复：查看摘要 → 高风险确认 → 恢复完成", async ({ page }) => {
  await page.goto(`/#/teams/${team}/settings`);

  // 备份目录从上一次导出的结果里拿：先导出一份，再把目录填进去。
  await page.getByRole("button", { name: "选择目录" }).click();
  await expect(page.getByLabel("备份目标目录")).toHaveValue(/\S/);
  await page.getByRole("button", { name: "导出备份" }).click();
  const result = page.getByTestId("backup-export-result");
  await expect(result).toBeVisible({ timeout: TIMEOUT });
  const backupDir = (await result.locator("dd").first().innerText()).trim();

  // 查看备份摘要。
  await page.getByLabel("备份目录").fill(backupDir);
  await page.getByRole("button", { name: "查看备份" }).click();
  const summary = page.getByTestId("backup-summary");
  await expect(summary).toBeVisible({ timeout: TIMEOUT });
  await expect(summary).toContainText(teamName);

  // 恢复必须先经过高风险确认弹窗。
  await page.getByRole("button", { name: "恢复这份备份" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  await expect(dialog).toContainText("确认从备份恢复");
  await expect(dialog).toContainText("覆盖当前数据");
  await expect(dialog).toContainText("恢复前备份");

  await dialog.getByRole("button", { name: "确认恢复" }).click();
  const restored = page.getByTestId("backup-restore-result");
  await expect(restored).toBeVisible({ timeout: TIMEOUT });
  await expect(restored).toContainText("已恢复");
  await expect(restored).toContainText("恢复前备份");
});

test("备份目录为空时给出中文提示", async ({ page }) => {
  await page.goto(`/#/teams/${team}/settings`);
  await page.getByLabel("备份目标目录").fill("");
  await page.getByRole("button", { name: "导出备份" }).click();
  await expect(page.getByText("请填写备份目标目录")).toBeVisible();
});
