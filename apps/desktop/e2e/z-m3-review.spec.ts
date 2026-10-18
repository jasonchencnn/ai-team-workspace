// M3-06 端到端：验收页。真实界面 + 测试桥接 + 真实内核；Harness 是内置 Mock ACP Agent，
// 工作空间是临时 Git 仓库 / 普通文件夹。覆盖：Git 通过并合并、退回续接再验收、合并冲突后手动合并
// 标记完成、普通文件夹通过。「[mock:shellwrite=路径]」让 Mock 在执行目录写出确定内容的文件。
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";

import { type Page, expect, test } from "@playwright/test";

import { createMockHarness } from "./harnessHelpers.js";
import { BRIDGE_PORT } from "./ports.js";

async function invoke<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const response = await fetch(`http://127.0.0.1:${String(BRIDGE_PORT)}/invoke`, {
    method: "POST",
    headers: { "Content-Type": "text/plain" },
    body: JSON.stringify({ cmd, args }),
  });
  const reply = (await response.json()) as { ok?: T; err?: { message?: string } };
  if (reply.err !== undefined) {
    throw new Error(`${cmd} 失败：${reply.err.message ?? JSON.stringify(reply.err)}`);
  }
  return reply.ok as T;
}

interface RunInfo {
  id: string;
  attempt: number;
  status: string;
  branch: string | null;
}

const TIMEOUT = 25_000;
const HARNESS_G = "m3r-mock-git";
const HARNESS_P = "m3r-mock-plain";
const ROLE_G = "评审Git员";
const ROLE_P = "评审普通员";
const OK_MARK = "written by shell";

let gitWs = "";
let plainWs = "";
let teamG = "";
let teamP = "";

interface TaskRef {
  id: string;
  key: string;
  title: string;
  url: string;
}

test.describe.configure({ mode: "serial", timeout: 90_000 });

function git(cwd: string, ...args: string[]): string {
  return execFileSync(
    "git",
    [
      "-c",
      "user.name=E2E",
      "-c",
      "user.email=e2e@example.invalid",
      "-c",
      "commit.gpgsign=false",
      ...args,
    ],
    { cwd, encoding: "utf8" },
  );
}

function put(root: string, rel: string, content: string): void {
  const target = join(root, rel);
  mkdirSync(dirname(target), { recursive: true });
  writeFileSync(target, content);
}

test.beforeAll(() => {
  gitWs = mkdtempSync(join(tmpdir(), "teamspace-e2e-m3r-git-"));
  git(gitWs, "init", "-q", "-b", "main");
  put(gitWs, "README.md", "# 验收 Git 项目\n");
  put(gitWs, "brief.txt", "需求说明\n");
  put(gitWs, "notes/a.txt", "base\n");
  put(gitWs, "notes/c.txt", "base\n");
  git(gitWs, "add", ".");
  git(gitWs, "commit", "-q", "-m", "docs: seed");
  plainWs = mkdtempSync(join(tmpdir(), "teamspace-e2e-m3r-plain-"));
  put(plainWs, "brief.txt", "需求说明\n");
});

test.afterAll(() => {
  rmSync(gitWs, { recursive: true, force: true });
  rmSync(plainWs, { recursive: true, force: true });
});

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

async function bindRole(page: Page, team: string, role: string, harness: string) {
  await page.goto(`/#/teams/${team}/roles`);
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
  team: string,
  title: string,
  owner: string,
): Promise<TaskRef> {
  await page.goto(`/#/teams/${team}/tasks/new`);
  await page.getByLabel("任务名称").fill(title);
  const select = page.getByLabel("负责人岗位");
  await select.selectOption(
    (await select.locator("option", { hasText: owner }).getAttribute("value")) ?? "",
  );
  await page.locator("#task-file-search").fill("brief");
  await page.locator(".task-form__suggestion", { hasText: "brief.txt" }).click();
  await page.getByRole("radio", { name: "READY", exact: true }).check();
  await page.getByRole("button", { name: "创建并分配" }).click();
  await expect(page.locator(".task-detail__status")).toContainText("READY");
  const key = /\/ (\S+)$/.exec(
    (await page.locator(".task-detail__status").innerText()).trim(),
  )?.[1];
  const url = page.url();
  return { url, id: url.split("/").pop() ?? "", key: key ?? "", title };
}

/** 开始执行并等到「待验收」。 */
async function runToReview(page: Page): Promise<void> {
  await page
    .getByRole("button", { name: /^(开始执行|重新执行)$/ })
    .first()
    .click();
  await expect(page.locator(".run-outcome--success")).toBeVisible({ timeout: TIMEOUT });
  await expect(page.locator(".task-detail__status")).toContainText("待验收");
}

async function runsOf(taskId: string): Promise<RunInfo[]> {
  return await invoke<RunInfo[]>("list_runs", { taskId });
}

test("准备：Git 团队与普通文件夹团队、Mock 接入与绑定", async ({ page }) => {
  teamG = await createTeam(page, "验收Git团队", gitWs, ROLE_G);
  await page.goto(`/#/teams/${teamG}/roles?tab=harness`);
  await createMockHarness(page, HARNESS_G);
  await bindRole(page, teamG, ROLE_G, HARNESS_G);
  teamP = await createTeam(page, "验收普通团队", plainWs, ROLE_P);
  await page.goto(`/#/teams/${teamP}/roles?tab=harness`);
  await createMockHarness(page, HARNESS_P);
  await bindRole(page, teamP, ROLE_P, HARNESS_P);
});

test("Git：执行 → 验收页看到改动与 diff → 通过并合并 → 任务完成", async ({ page }) => {
  const task = await createTask(page, teamG, "新增说明 [mock:shellwrite=docs/ok.txt]", ROLE_G);
  await runToReview(page);

  // 任务详情的醒目入口
  const entry = page.getByRole("region", { name: "验收入口" });
  await expect(entry).toContainText("这个任务在等你验收");
  await entry.getByRole("button", { name: "去验收" }).click();
  await expect(page).toHaveURL(new RegExp(`/tasks/${task.id}/review$`));
  await expect(page.getByRole("heading", { level: 1 })).toContainText(`验收 ${task.key}`);

  // 信息条
  const info = page.getByLabel("执行信息");
  await expect(info).toContainText("第 1 次执行（当前）");
  await expect(info).toContainText(HARNESS_G);
  await expect(info).toContainText("main");
  await expect(info).toContainText(`teamspace/${task.key}-r1`);
  await expect(info).toContainText("独立 worktree");

  // 执行摘要 + 工作日志写入结果
  await expect(page.locator(".review__summary")).toContainText("已完成");
  await expect(page.locator(".review__journal")).toContainText("已写入");

  // 改动文件 + 逐文件 diff（按需读取）
  const row = page.locator(".review-file", { hasText: "docs/ok.txt" });
  await expect(row).toContainText("新增");
  await expect(row).toContainText("+1");
  await row.getByRole("button", { name: "docs/ok.txt" }).click();
  const card = page.locator(".diff-card", { hasText: "docs/ok.txt" });
  const added = card.locator(".diff-row--add");
  await expect(added).toContainText(OK_MARK);
  await expect(card.locator(".diff-row--hunk")).toBeVisible();

  // 通过并合并：确认弹窗 → 合并成功提示
  await page.getByRole("button", { name: "通过并合并到 main" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toContainText("确认合并");
  await expect(dialog).toContainText("--no-ff");
  await dialog.getByRole("button", { name: "确认合并" }).click();
  await expect(page.locator(".review-banner--success")).toContainText("合并成功");
  await expect(page.locator(".review-banner--success")).toContainText("已合并到 main");

  // 磁盘：基线分支有合并提交与文件，任务 DONE，不再有「去验收」
  expect(readFileSync(join(gitWs, "docs/ok.txt"), "utf8")).toContain(OK_MARK);
  expect(git(gitWs, "log", "-1", "--format=%p").trim().split(" ")).toHaveLength(2);
  await page.goto(task.url);
  await expect(page.locator(".task-detail__status")).toContainText("已完成");
  await expect(page.getByRole("region", { name: "验收入口" })).toHaveCount(0);
});

test("Git：退回需要意见 → 续接执行 → 再次验收（含历次退回意见）并通过", async ({ page }) => {
  const task = await createTask(page, teamG, "补充记录 [mock:shellwrite=docs/rej.txt]", ROLE_G);
  await runToReview(page);

  // 看板上待验收卡片可以进入验收页
  await page.goto(`/#/teams/${teamG}/tasks`);
  await page.getByRole("link", { name: "去验收" }).first().click();
  await expect(page).toHaveURL(new RegExp(`/tasks/${task.id}/review$`));

  const reject = page.getByRole("button", { name: "退回", exact: true });
  await expect(reject).toBeDisabled();
  await expect(page.getByText("意见为空时不能退回")).toBeVisible();
  await expect(page.locator(".review__panel", { hasText: "历次退回意见" })).toContainText(
    "还没有退回记录",
  );
  const opinion = "请补充回归说明，并保持 [mock:shellwrite=docs/rej2.txt]";
  await page.getByLabel("退回意见（必填）").fill("   ");
  await expect(reject).toBeDisabled();
  await page.getByLabel("退回意见（必填）").fill(opinion);
  await expect(reject).toBeEnabled();
  await reject.click();

  // 跳回任务详情，看到第 2 次执行开始并完成
  await expect(page).toHaveURL(new RegExp(`/tasks/${task.id}$`));
  await expect(page.locator(".run-info")).toContainText("第 2 次执行", { timeout: TIMEOUT });
  await expect(page.locator(".run-outcome--success")).toBeVisible({ timeout: TIMEOUT });
  await expect(page.locator(".task-detail__status")).toContainText("待验收");
  const runs = await runsOf(task.id);
  expect(runs.map((run) => run.attempt)).toEqual([2, 1]);

  // 再次验收：历次退回意见可见，两次执行的改动都在
  await page
    .getByRole("region", { name: "验收入口" })
    .getByRole("button", { name: "去验收" })
    .click();
  await expect(page.getByLabel("执行信息")).toContainText("第 2 次执行（当前）");
  await expect(page.locator(".review-rejection")).toContainText("第 1 次执行后退回");
  await expect(page.locator(".review-rejection")).toContainText("请补充回归说明");
  await expect(page.locator(".review-file", { hasText: "docs/rej.txt" })).toBeVisible();
  await expect(page.locator(".review-file", { hasText: "docs/rej2.txt" })).toBeVisible();
  await page.getByRole("button", { name: "通过并合并到 main" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "确认合并" }).click();
  await expect(page.locator(".review-banner--success")).toContainText("合并成功");
  expect(existsSync(join(gitWs, "docs/rej2.txt"))).toBe(true);
});

test("Git：合并冲突 → 任务受阻 → 手动合并后「已手动合并，标记完成」", async ({ page }) => {
  const task = await createTask(page, teamG, "改写笔记 [mock:shellwrite=notes/a.txt]", ROLE_G);
  await runToReview(page);
  const branch = (await runsOf(task.id))[0]?.branch ?? "";
  expect(branch).toBe(`teamspace/${task.key}-r1`);

  // 基线分支上同一个文件被另外改动并提交，通过时会冲突
  put(gitWs, "notes/a.txt", "main side\n");
  git(gitWs, "add", "notes/a.txt");
  git(gitWs, "commit", "-q", "-m", "docs: main side");

  await page
    .getByRole("region", { name: "验收入口" })
    .getByRole("button", { name: "去验收" })
    .click();
  await page.getByRole("button", { name: "通过并合并到 main" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "确认合并" }).click();
  await expect(page.locator(".review-banner--warning")).toContainText("合并有冲突");

  // 冲突状态：说明、冲突文件、复制命令、两个恢复按钮；没有通过 / 退回
  const panel = page.locator(".review__panel--warning");
  await expect(panel).toContainText("合并冲突");
  await expect(panel).toContainText("notes/a.txt");
  await expect(panel.locator("code")).toContainText(`git merge --no-ff ${branch}`);
  await expect(panel.getByRole("button", { name: "复制合并命令" })).toBeVisible();
  await expect(panel.getByRole("button", { name: "重新执行" })).toBeEnabled();
  await expect(page.getByRole("button", { name: "通过并合并到 main" })).toHaveCount(0);
  expect(git(gitWs, "rev-parse", "--abbrev-ref", "HEAD").trim()).toBe("main");

  // 任务详情：受阻，仍有醒目入口
  await page.goto(task.url);
  await expect(page.locator(".task-detail__status")).toContainText("受阻");
  await expect(page.getByRole("region", { name: "验收入口" })).toContainText("合并冲突");
  await page
    .getByRole("region", { name: "验收入口" })
    .getByRole("button", { name: "去验收" })
    .click();

  // 还没手动合并：标记完成被后端拒绝，任务仍受阻
  await page.getByRole("button", { name: "已手动合并，标记完成" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "确认标记完成" }).click();
  await expect(page.locator(".review__error")).toBeVisible();
  await page.keyboard.press("Escape");

  // 用 git 在临时仓库手动合并（冲突取执行分支一侧），再标记完成
  git(gitWs, "merge", "--no-ff", "-X", "theirs", branch, "-m", "chore: manual merge");
  await page.getByRole("button", { name: "已手动合并，标记完成" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "确认标记完成" }).click();
  await expect(page.locator(".review-banner--success")).toBeVisible();
  await page.goto(task.url);
  await expect(page.locator(".task-detail__status")).toContainText("已完成");
  expect(readFileSync(join(gitWs, "notes/a.txt"), "utf8")).toContain(OK_MARK);
});

test("Git：基线上同一文件被改动 → 冲突后刷新验收页，仍列出冲突文件", async ({ page }) => {
  const task = await createTask(page, teamG, "冲突后刷新 [mock:shellwrite=notes/c.txt]", ROLE_G);
  await runToReview(page);
  put(gitWs, "notes/c.txt", "main side c\n");
  git(gitWs, "add", "notes/c.txt");
  git(gitWs, "commit", "-q", "-m", "docs: main side c");

  await page
    .getByRole("region", { name: "验收入口" })
    .getByRole("button", { name: "去验收" })
    .click();
  await page.getByRole("button", { name: "通过并合并到 main" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "确认合并" }).click();
  await expect(page.locator(".review-banner--warning")).toContainText("合并有冲突");
  await expect(page.locator(".review__panel--warning")).toContainText("notes/c.txt");

  // 后端：验收数据里冲突状态与冲突文件取自执行记录，刷新后仍在。
  const view = await invoke<{ conflict: boolean; conflictFiles: string[] }>("get_review", {
    taskId: task.id,
  });
  expect(view.conflict).toBe(true);
  expect(view.conflictFiles).toEqual(["notes/c.txt"]);

  // 界面：刷新页面后仍处于冲突状态（两个恢复按钮在，没有通过 / 退回）。
  await page.reload();
  const panel = page.locator(".review__panel--warning");
  await expect(panel).toContainText("合并冲突");
  await expect(panel).toContainText("notes/c.txt");
  await expect(panel.getByRole("button", { name: "重新执行" })).toBeEnabled();
  await expect(page.getByRole("button", { name: "通过并合并到 main" })).toHaveCount(0);
  // 清理：手动合并并标记完成，不影响后面的用例。
  git(gitWs, "merge", "--no-ff", "-X", "theirs", `teamspace/${task.key}-r1`, "-m", "chore: merge");
  await page.getByRole("button", { name: "已手动合并，标记完成" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "确认标记完成" }).click();
  await expect(page.locator(".review-banner--success")).toBeVisible();
});

test("Git：验收页打开后执行分支又有新提交 → 通过被拒并提示刷新 → 刷新后可通过", async ({
  page,
}) => {
  const task = await createTask(page, teamG, "改动变化 [mock:shellwrite=docs/eh.txt]", ROLE_G);
  await runToReview(page);
  await page
    .getByRole("region", { name: "验收入口" })
    .getByRole("button", { name: "去验收" })
    .click();
  await expect(page.locator(".review-file", { hasText: "docs/eh.txt" })).toBeVisible();
  const before = (await runsOf(task.id))[0];
  expect(before?.branch).toBe(`teamspace/${task.key}-r1`);

  // 验收页已读到旧的 head；此时执行分支上又多了一个提交（模拟有人在 worktree 里继续改）。
  const execDir = (await invoke<{ execDir: string }[]>("list_runs", { taskId: task.id }))[0]
    ?.execDir;
  expect(execDir).toBeTruthy();
  const headBefore = git(execDir ?? "", "rev-parse", "HEAD").trim();
  put(execDir ?? "", "docs/eh-extra.txt", "late change\n");
  git(execDir ?? "", "add", "docs/eh-extra.txt");
  git(execDir ?? "", "commit", "-q", "-m", "docs: late change");
  expect(git(execDir ?? "", "rev-parse", "HEAD").trim()).not.toBe(headBefore);

  await page.getByRole("button", { name: "通过并合并到 main" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "确认合并" }).click();
  await expect(page.locator(".review__error")).toContainText("改动已变化，请刷新验收页");
  // 被拒后没有合并：基线分支上没有这个文件，任务仍待验收。
  expect(existsSync(join(gitWs, "docs/eh.txt"))).toBe(false);

  // 刷新后新的改动出现在列表里，再通过即成功。
  await page.reload();
  await expect(page.locator(".review-file", { hasText: "docs/eh-extra.txt" })).toBeVisible();
  await page.getByRole("button", { name: "通过并合并到 main" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "确认合并" }).click();
  await expect(page.locator(".review-banner--success")).toContainText("合并成功");
  expect(existsSync(join(gitWs, "docs/eh-extra.txt"))).toBe(true);
});

test("普通文件夹：改动文件列表、打开文件、无 diff，通过后任务完成", async ({ page }) => {
  const task = await createTask(page, teamP, "整理素材", ROLE_P);
  await runToReview(page);
  await page
    .getByRole("region", { name: "验收入口" })
    .getByRole("button", { name: "去验收" })
    .click();

  const info = page.getByLabel("执行信息");
  await expect(info).toContainText("原地：普通文件夹");
  await expect(page.getByRole("heading", { name: "逐文件 diff" })).toHaveCount(0);
  const row = page.locator(".review-file").first();
  await expect(row).toContainText("mock-");
  await expect(page.getByText("不提供 diff 预览")).toBeVisible();
  await row.getByRole("button", { name: /^打开文件/ }).click();
  await expect(page.locator(".review__error")).toHaveCount(0);

  await page.getByRole("button", { name: "通过", exact: true }).click();
  await expect(page.locator(".review-banner--success")).toContainText("已通过");
  await page.goto(task.url);
  await expect(page.locator(".task-detail__status")).toContainText("已完成");
});
