// apps/desktop/e2e/z-qa-m3.spec.ts
// M3 验收闭环独立回归与边界探索测试套件（独立 QA - Gemini / Antigravity）
// 覆盖：
// 1. 验收：Git diff 大文件截断与二进制、退回意见边界（空白、超长 2000 字）、多轮退回后历次意见、
//    普通文件夹「打开文件」对可执行文件改为在 Finder 中显示、验收合并中对其他操作的拒绝、已通过的执行不能再次验收。
// 2. 合并：基线有未提交改动时「无法合并」且任务仍待验收、冲突后两种恢复、合并后 worktree 与分支已清理、基线分支名特殊字符。
// 3. 依赖：链式与菱形自动就绪、重新打开前置任务时下游不回退、表单环路预检与保存禁用。
// 4. 活动与审计：多条件筛选组合、URL 保持、加载更多、清除筛选。
// 5. 共享文件：越界路径与符号链接、非 md、超大文件截断提示、任务 ↔ 文件双向跳转、文件被删除后的提示。

import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";

import { type Page, expect, test } from "@playwright/test";

import {
  type RunInfo,
  bind,
  createTaskApi,
  invoke,
  invokeError,
  openTask,
  runsOf,
  setupMockHarness,
  setupTeam,
  TIMEOUT,
} from "./z-qa-r2-helpers.js";

test.describe.configure({ mode: "serial", timeout: 120_000 });

let gitWs = "";
let plainWs = "";
let teamGitId = "";
let teamPlainId = "";
let roleGitId = "";
let rolePlainId = "";

const ROLE_GIT = "全能Git员";
const ROLE_PLAIN = "全能普通员";
const HARNESS_GIT = "qa-m3-mock-git";
const HARNESS_PLAIN = "qa-m3-mock-plain";

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

function put(root: string, rel: string, content: string | Buffer): void {
  const target = join(root, rel);
  mkdirSync(dirname(target), { recursive: true });
  writeFileSync(target, content);
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

/** 从任务详情点击「去验收」进入验收页。 */
async function goReview(page: Page): Promise<void> {
  await page
    .getByRole("region", { name: "验收入口" })
    .getByRole("button", { name: "去验收" })
    .click();
}

/** 在验收页通过并确认合并。 */
async function approveAndConfirm(page: Page): Promise<void> {
  await page.getByRole("button", { name: /^通过并合并到/ }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toContainText("确认合并");
  await dialog.getByRole("button", { name: "确认合并" }).click();
}

test.beforeAll(() => {
  gitWs = mkdtempSync(join(tmpdir(), "teamspace-e2e-qa-m3-git-"));
  git(gitWs, "init", "-q", "-b", "main");
  put(gitWs, "README.md", "# QA M3 Git 仓库\n");
  put(gitWs, "docs/spec.md", "# 共享设计说明\n\n核心需求文档。\n");
  put(gitWs, "notes/seed.txt", "seed note\n");
  git(gitWs, "add", ".");
  git(gitWs, "commit", "-q", "-m", "docs: initial commit");

  plainWs = mkdtempSync(join(tmpdir(), "teamspace-e2e-qa-m3-plain-"));
  put(plainWs, "README.md", "# QA M3 普通文件夹\n");
  put(plainWs, "notes/info.txt", "普通文件夹说明\n");
});

test.afterAll(() => {
  rmSync(gitWs, { recursive: true, force: true });
  rmSync(plainWs, { recursive: true, force: true });
});

// ---------------------------------------------------------------- 00. 套件准备

test("00. 准备：创建 Git 团队与普通文件夹团队、注册 Mock Harness 并完成绑定", async () => {
  const tGit = await setupTeam("QA3-Git团队", gitWs, [{ name: ROLE_GIT, prefix: "G1" }]);
  teamGitId = tGit.teamId;
  roleGitId = tGit.roleIds[ROLE_GIT] ?? "";
  const hGit = await setupMockHarness(teamGitId, HARNESS_GIT);
  await bind(roleGitId, hGit);

  const tPlain = await setupTeam("QA3-普通团队", plainWs, [{ name: ROLE_PLAIN, prefix: "P1" }]);
  teamPlainId = tPlain.teamId;
  rolePlainId = tPlain.roleIds[ROLE_PLAIN] ?? "";
  const hPlain = await setupMockHarness(teamPlainId, HARNESS_PLAIN);
  await bind(rolePlainId, hPlain);

  expect(roleGitId).toBeTruthy();
  expect(rolePlainId).toBeTruthy();
});

// ---------------------------------------------------------------- 1. 验收（Review）边界

test("01. 验收：Git diff 大文件截断（>256KB 截断提示）与二进制文件（不显示 diff，标为二进制）", async ({
  page,
}) => {
  const task = await createTaskApi({
    teamId: teamGitId,
    roleId: roleGitId,
    title: "改动大文件与二进制 [mock:shellwrite=docs/run.txt]",
  });
  await openTask(page, task);
  await runToReview(page);

  // 在执行目录（worktree）中人工制造超大文本文件（>256KB）与二进制文件
  const run = (await runsOf(task.id))[0] as RunInfo;
  expect(run).toBeDefined();

  // 二进制文件（含不可打印字节 \0\1\2\3）
  put(run.execDir, "assets/logo.png", Buffer.from([0, 1, 2, 3, 255, 0, 128, 64]));
  // 超大文本文件（单个文件补丁上限 256KB = 262,144 字节，这里写 >300KB）
  const largeChunk = "line for testing truncation in review patch: " + "X".repeat(60) + "\n";
  put(run.execDir, "docs/large.txt", largeChunk.repeat(4000));

  await goReview(page);
  await expect(page).toHaveURL(new RegExp(`/tasks/${task.id}/review$`));

  // 1. 二进制文件检查
  const binRow = page.locator(".review-file", { hasText: "assets/logo.png" });
  await expect(binRow).toBeVisible();
  await expect(binRow.getByText("二进制")).toBeVisible();
  await binRow.getByRole("button", { name: "assets/logo.png" }).click();
  const binCard = page.locator(".diff-card", { hasText: "assets/logo.png" });
  await expect(binCard.locator(".diff-card__note-row")).toContainText("二进制文件，不显示 diff。");

  // 2. 超大文件截断检查
  const largeRow = page.locator(".review-file", { hasText: "docs/large.txt" });
  await expect(largeRow).toBeVisible();
  await largeRow.getByRole("button", { name: "docs/large.txt" }).click();
  const largeCard = page.locator(".diff-card", { hasText: "docs/large.txt" });
  await expect(largeCard.locator(".diff-row--add").first()).toBeVisible();
  const truncatedNote = largeCard.locator(".diff-card__truncated");
  await expect(truncatedNote).toBeVisible();
  await expect(truncatedNote.locator(".ui-tag")).toHaveText("diff 已截断");
  await expect(truncatedNote).toContainText(
    "这个文件改动很大，只显示了前一部分。完整内容请在仓库中用 git 查看。",
  );
});

test("02. 验收：退回意见边界——纯空白禁用退回，超长 2000 字支持提交，超出 2000 字被拦截", async ({
  page,
}) => {
  const task = await createTaskApi({
    teamId: teamGitId,
    roleId: roleGitId,
    title: "退回边界测试 [mock:shellwrite=docs/bnd.txt]",
  });
  await openTask(page, task);
  await runToReview(page);
  await goReview(page);

  const rejectBtn = page.getByRole("button", { name: "退回", exact: true });
  const commentArea = page.getByLabel("退回意见（必填）");

  // 1. 纯空白时退回被禁用，提示「意见为空时不能退回」
  await commentArea.fill("     \n   \t  ");
  await expect(rejectBtn).toBeDisabled();
  await expect(page.getByText("意见为空时不能退回")).toBeVisible();

  // 2. 刚好 2000 字：输入框允许并启用按钮
  const comment2000 = "评".repeat(2000);
  await commentArea.fill(comment2000);
  await expect(rejectBtn).toBeEnabled();

  // 3. 超出 2000 字（2001 字）经 API 提交被后端拒绝
  const run = (await runsOf(task.id))[0] as RunInfo;
  const errOver = await invokeError("reject_review", {
    input: { runId: run.id, comment: "超".repeat(2001) },
  });
  expect(errOver).toContain("退回意见");
  expect(errOver).toContain("2000");

  // 4. 前端使用 2000 字退回成功提交
  await rejectBtn.click();
  await expect(page).toHaveURL(new RegExp(`/tasks/${task.id}$`));
  await expect(page.locator(".run-info")).toContainText("第 2 次执行", { timeout: TIMEOUT });
});

test("03. 验收：多轮退回后历次意见按时序完整展示并保留每次意见内容", async ({ page }) => {
  const task = await createTaskApi({
    teamId: teamGitId,
    roleId: roleGitId,
    title: "多轮退回任务 [mock:shellwrite=docs/multi.txt]",
  });
  await openTask(page, task);
  await runToReview(page);

  // 第 1 次退回
  await goReview(page);
  const opinion1 = "第一轮意见：请补齐文档注释 [mock:shellwrite=docs/multi-r2.txt]";
  await page.getByLabel("退回意见（必填）").fill(opinion1);
  await page.getByRole("button", { name: "退回", exact: true }).click();
  await expect(page.locator(".run-info")).toContainText("第 2 次执行", { timeout: TIMEOUT });
  await expect(page.locator(".run-outcome--success")).toBeVisible({ timeout: TIMEOUT });

  // 第 2 次退回
  await goReview(page);
  const opinion2 = "第二轮意见：请完善单元测试用例";
  await page.getByLabel("退回意见（必填）").fill(opinion2);
  await page.getByRole("button", { name: "退回", exact: true }).click();
  await expect(page.locator(".run-info")).toContainText("第 3 次执行", { timeout: TIMEOUT });
  await expect(page.locator(".run-outcome--success")).toBeVisible({ timeout: TIMEOUT });

  // 第 3 次验收：验证历次退回记录
  await goReview(page);
  await expect(page.getByLabel("执行信息")).toContainText("第 3 次执行（当前）");

  const panel = page.locator(".review__panel", { hasText: "历次退回意见" });
  await expect(panel.locator(".review__caption").first()).toHaveText("2 次");

  const rejections = panel.locator(".review-rejection");
  await expect(rejections).toHaveCount(2);

  // 第一条记录：第 1 次退回
  await expect(rejections.nth(0)).toContainText("第 1 次执行后退回");
  await expect(rejections.nth(0)).toContainText(opinion1);

  // 第二条记录：第 2 次退回
  await expect(rejections.nth(1)).toContainText("第 2 次执行后退回");
  await expect(rejections.nth(1)).toContainText(opinion2);
});

test("04. 验收：普通文件夹「打开文件」对可执行文件改为在 Finder 中显示（且目录越界被拒绝）", async ({
  page,
}) => {
  // 在普通文件夹的原地执行目录下创建脚本并赋予可执行权限，并在任务中引用它（D15 / D16：引用文件内的写入自动批准）
  const scriptPath = join(plainWs, "scripts", "deploy.sh");
  put(plainWs, "scripts/deploy.sh", "#!/bin/sh\necho deployed\n");
  try {
    execFileSync("chmod", ["+x", scriptPath]);
  } catch {
    // 忽略在某些只读环境的 chmod 异常
  }

  const task = await createTaskApi({
    teamId: teamPlainId,
    roleId: rolePlainId,
    title: "普通文件夹可执行脚本 [mock:shellwrite=scripts/deploy.sh]",
    refs: ["scripts/deploy.sh"],
  });
  await openTask(page, task);
  await runToReview(page);

  await goReview(page);
  const info = page.getByLabel("执行信息");
  await expect(info).toContainText("原地：普通文件夹");

  const row = page.locator(".review-file", { hasText: "scripts/deploy.sh" });
  await expect(row).toBeVisible();

  // 点击「打开文件」：对可执行文件内部调用 reveal_path（在 Finder 中显示），不报错
  await row.getByRole("button", { name: /^打开文件/ }).click();
  await expect(page.locator(".review__error")).toHaveCount(0);

  const run = (await runsOf(task.id))[0] as RunInfo;
  // 直接经 bridge 调用 open_run_file 验证合法文件打开正常
  await invoke("open_run_file", { runId: run.id, path: "scripts/deploy.sh" });

  // 越界路径（如 .. 或绝对路径）尝试打开被拒绝
  const errOut = await invokeError("open_run_file", {
    runId: run.id,
    path: "../../../outside.txt",
  });
  expect(errOut).toContain("只能打开执行目录内的文件");
});

test("05. 验收：验收合并进行中（持有验收锁）拒绝对该任务的其他并发操作", async ({ page }) => {
  const task = await createTaskApi({
    teamId: teamGitId,
    roleId: roleGitId,
    title: "验收锁并发防护 [mock:shellwrite=notes/delay.txt]",
  });
  await openTask(page, task);
  await runToReview(page);

  const run = (await runsOf(task.id))[0] as RunInfo;
  const view = await invoke<{ headCommit: string }>("get_review", { taskId: task.id });
  const headCommit = view.headCommit;
  expect(headCommit).toBeTruthy();

  // 利用 git filter 延迟机制让合并操作在 git 层面执行约 1.5 秒
  put(gitWs, ".git/info/attributes", "notes/delay.txt filter=delay\n");
  git(gitWs, "config", "filter.delay.clean", "sh -c 'sleep 1.2; cat'");
  git(gitWs, "config", "filter.delay.smudge", "sh -c 'sleep 1.2; cat'");

  // 发起 approve_review，它会取得该任务的验收锁并进入 Git 合并
  const approvePromise = invoke("approve_review", {
    input: { runId: run.id, expectedHead: headCommit },
  });

  // 在合并进行期间，发起对该任务的其他操作，验证均被锁定拦截（conflict: 正在验收合并，请稍候）
  await new Promise((resolve) => setTimeout(resolve, 200));

  const [errStatus, errUpdate, errRun, errApprove] = await Promise.all([
    invokeError("change_task_status", {
      input: { taskId: task.id, to: "ready", blockedReason: null },
    }),
    invokeError("update_task", {
      input: {
        taskId: task.id,
        title: "并发修改标题",
        description: "",
        ownerRoleId: roleGitId,
        fileRefs: [],
        dependsOn: [],
      },
    }),
    invokeError("start_run", { taskId: task.id }),
    invokeError("approve_review", {
      input: { runId: run.id, expectedHead: headCommit },
    }),
  ]);

  expect(errStatus).toContain("正在验收合并，请稍候");
  expect(errUpdate).toContain("正在验收合并，请稍候");
  expect(errRun).toContain("正在验收合并，请稍候");
  expect(errApprove).toContain("正在验收合并，请稍候");

  // 等待最初的合并顺利完成
  await approvePromise;

  // 清理 filter
  git(gitWs, "config", "--unset", "filter.delay.clean");
  git(gitWs, "config", "--unset", "filter.delay.smudge");
  rmSync(join(gitWs, ".git/info/attributes"), { force: true });
});

test("06. 验收：已通过的执行不能再次验收（接口拒绝且界面无入口）", async ({ page }) => {
  const task = await createTaskApi({
    teamId: teamGitId,
    roleId: roleGitId,
    title: "一次性验收 [mock:shellwrite=notes/once.txt]",
  });
  await openTask(page, task);
  await runToReview(page);

  const run = (await runsOf(task.id))[0] as RunInfo;
  await goReview(page);
  await approveAndConfirm(page);
  await expect(page.locator(".review-banner--success")).toBeVisible();

  // 1. 尝试再次调用 approve_review 被拒绝
  const errApprove = await invokeError("approve_review", {
    input: { runId: run.id, expectedHead: "any" },
  });
  expect(errApprove).toMatch(/不在待验收|已有验收结果/);

  // 2. 尝试调用 reject_review 被拒绝
  const errReject = await invokeError("reject_review", {
    input: { runId: run.id, comment: "已通过后企图退回" },
  });
  expect(errReject).toMatch(/不在待验收|已有验收结果/);

  // 3. 回到任务详情：状态为「已完成」，且不再有「验收入口」
  await openTask(page, task);
  await expect(page.locator(".task-detail__status")).toContainText("已完成");
  await expect(page.getByRole("region", { name: "验收入口" })).toHaveCount(0);
});

// ---------------------------------------------------------------- 2. 合并（Merge）边界

test("07. 合并：基线有未提交已跟踪改动时，验收通过提示「无法合并」且任务仍待验收", async ({
  page,
}) => {
  const task = await createTaskApi({
    teamId: teamGitId,
    roleId: roleGitId,
    title: "基线未提交阻断合并 [mock:shellwrite=notes/dirty.txt]",
  });
  await openTask(page, task);
  await runToReview(page);

  // 在基线主分支上制造未提交的已跟踪文件改动
  put(gitWs, "README.md", "# 污染基线未提交内容\n");

  await goReview(page);
  await approveAndConfirm(page);

  // 界面显示「无法合并」且明确告知原因
  const warningBanner = page.locator(".review-banner--warning");
  await expect(warningBanner).toContainText("无法合并");
  await expect(warningBanner).toContainText("基线工作目录存在已跟踪文件的未提交改动");
  await expect(warningBanner).toContainText("任务仍保持待验收，处理后可再次通过。");

  // 任务在详情页仍保持待验收
  await openTask(page, task);
  await expect(page.locator(".task-detail__status")).toContainText("待验收");
  await expect(page.getByRole("region", { name: "验收入口" })).toBeVisible();

  // 恢复基线并再次去验收，确认能成功合并
  git(gitWs, "checkout", "--", "README.md");
  await goReview(page);
  await approveAndConfirm(page);
  await expect(page.locator(".review-banner--success")).toContainText("合并成功");
});

test("08. 合并：冲突后恢复路径一——手动在 Git 合并后「已手动合并，标记完成」成功转 DONE", async ({
  page,
}) => {
  const task = await createTaskApi({
    teamId: teamGitId,
    roleId: roleGitId,
    title: "冲突手动恢复 [mock:shellwrite=notes/conflict-a.txt]",
  });
  await openTask(page, task);
  await runToReview(page);
  const branch = (await runsOf(task.id))[0]?.branch ?? "";

  // 基线制造冲突提交
  put(gitWs, "notes/conflict-a.txt", "main branch conflicting text\n");
  git(gitWs, "add", "notes/conflict-a.txt");
  git(gitWs, "commit", "-q", "-m", "docs: conflict a on main");

  await goReview(page);
  await approveAndConfirm(page);
  await expect(page.locator(".review-banner--warning")).toContainText("合并有冲突");

  // 在未合并前点击「已手动合并，标记完成」应被拒绝报错
  await page.getByRole("button", { name: "已手动合并，标记完成" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "确认标记完成" }).click();
  await expect(page.locator(".review__error")).toContainText("还没有并入");
  await page.keyboard.press("Escape");

  // 在本地仓库手动合并分支
  git(gitWs, "merge", "--no-ff", "-X", "theirs", branch, "-m", "merge: manual resolve conflict");

  // 再次标记完成：成功
  await page.getByRole("button", { name: "已手动合并，标记完成" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "确认标记完成" }).click();
  await expect(page.locator(".review-banner--success")).toBeVisible();

  // 确认分支与目录已自动清理
  expect(git(gitWs, "branch", "--list", branch).trim()).toBe("");
  await openTask(page, task);
  await expect(page.locator(".task-detail__status")).toContainText("已完成");
});

test("09. 合并：冲突后恢复路径二——「重新执行」丢弃旧分支与 worktree，任务回到 READY", async ({
  page,
}) => {
  const task = await createTaskApi({
    teamId: teamGitId,
    roleId: roleGitId,
    title: "冲突重新执行恢复 [mock:shellwrite=notes/conflict-b.txt]",
  });
  await openTask(page, task);
  await runToReview(page);
  const firstRun = (await runsOf(task.id))[0] as RunInfo;

  // 基线制造冲突提交
  put(gitWs, "notes/conflict-b.txt", "main conflicting content b\n");
  git(gitWs, "add", "notes/conflict-b.txt");
  git(gitWs, "commit", "-q", "-m", "docs: conflict b on main");

  await goReview(page);
  await approveAndConfirm(page);
  await expect(page.locator(".review-banner--warning")).toContainText("合并有冲突");

  // 点击「重新执行」
  await page.getByRole("button", { name: "重新执行" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toContainText("会删除旧的执行目录与执行分支");
  await dialog.getByRole("button", { name: "确认，丢弃旧执行" }).click();

  // 验证回到任务详情且状态为 READY
  await expect(page).toHaveURL(new RegExp(`/tasks/${task.id}$`));
  await expect(page.locator(".task-detail__status")).toContainText("READY");
  await expect(page.getByRole("button", { name: "重新执行", exact: true })).toBeVisible();

  // 验证旧分支和旧 worktree 目录均已被删除
  expect(git(gitWs, "branch", "--list", firstRun.branch ?? "").trim()).toBe("");
  expect(existsSync(firstRun.execDir)).toBe(false);
});

test("10. 合并：正常合并通过后，执行分支与 worktree 彻底清理", async ({ page }) => {
  const task = await createTaskApi({
    teamId: teamGitId,
    roleId: roleGitId,
    title: "合并清理验证 [mock:shellwrite=notes/clean.txt]",
  });
  await openTask(page, task);
  await runToReview(page);

  const run = (await runsOf(task.id))[0] as RunInfo;
  expect(existsSync(run.execDir)).toBe(true);

  await goReview(page);
  await approveAndConfirm(page);
  await expect(page.locator(".review-banner--success")).toContainText("合并成功");

  // 1. 验证磁盘 worktree 目录已被删除
  expect(existsSync(run.execDir)).toBe(false);
  // 2. 验证执行分支已被删除
  expect(git(gitWs, "branch", "--list", run.branch ?? "").trim()).toBe("");
  // 3. 验证基线生成了合并提交（2 个父提交）
  const parents = git(gitWs, "log", "-1", "--format=%p").trim().split(" ");
  expect(parents.length).toBe(2);
});

test("11. 合并：基线分支名含合法特殊字符能正常合并，含非法字符（如 =）被安全拦截", async ({
  page,
}) => {
  // 创建并切到包含特殊字符的合法分支：feat/v1.0-alpha_1
  const specBranch = "feat/v1.0-alpha_1";
  git(gitWs, "checkout", "-b", specBranch);

  const task = await createTaskApi({
    teamId: teamGitId,
    roleId: roleGitId,
    title: "特殊分支名任务 [mock:shellwrite=notes/branch-spec.txt]",
  });
  await openTask(page, task);
  await runToReview(page);

  await goReview(page);
  await expect(page.getByLabel("执行信息")).toContainText(specBranch);
  await approveAndConfirm(page);
  await expect(page.locator(".review-banner--success")).toContainText(`已合并到 ${specBranch}`);

  // 验证非法分支名（包含 =）的基线分支在合并时被后端安全拦截（blocked: 分支名包含 =，无法安全合并）
  const badBranch = "feat=bad_branch";
  git(gitWs, "checkout", "-b", badBranch);

  const taskInvalid = await createTaskApi({
    teamId: teamGitId,
    roleId: roleGitId,
    title: "非法分支任务 [mock:shellwrite=notes/inv.txt]",
  });
  await openTask(page, taskInvalid);
  await runToReview(page);
  const runInv = (await runsOf(taskInvalid.id))[0] as RunInfo;
  const viewInv = await invoke<{ headCommit: string }>("get_review", { taskId: taskInvalid.id });

  const res = await invoke<{ outcome: string; reason?: string }>("approve_review", {
    input: { runId: runInv.id, expectedHead: viewInv.headCommit },
  });
  expect(res.outcome).toBe("blocked");
  expect(res.reason).toContain("分支名包含 =，无法安全合并");

  // 切回 main
  git(gitWs, "checkout", "main");
});

// ---------------------------------------------------------------- 3. 依赖（Dependencies）推进

test("12. 依赖：链式依赖（A → B → C）自动就绪流转", async ({ page }) => {
  const taskA = await createTaskApi({
    teamId: teamGitId,
    roleId: roleGitId,
    title: "链式A [mock:shellwrite=notes/chain-a.txt]",
    status: "ready",
  });
  const taskB = await createTaskApi({
    teamId: teamGitId,
    roleId: roleGitId,
    title: "链式B [mock:shellwrite=notes/chain-b.txt]",
    status: "todo",
    dependsOn: [taskA.id],
  });
  const taskC = await createTaskApi({
    teamId: teamGitId,
    roleId: roleGitId,
    title: "链式C",
    status: "todo",
    dependsOn: [taskB.id],
  });

  // 初始：B 和 C 均为 TODO 且无法执行
  await openTask(page, taskB);
  await expect(page.getByRole("button", { name: "开始执行", exact: true })).toBeDisabled();

  // A 执行并验收合并
  await openTask(page, taskA);
  await runToReview(page);
  await goReview(page);
  await approveAndConfirm(page);
  await expect(page.locator(".review-banner--success")).toContainText("下游已自动就绪");

  // B 自动转为 READY
  await openTask(page, taskB);
  await expect(page.locator(".task-detail__status")).toContainText("READY");
  await expect(page.getByRole("button", { name: "开始执行", exact: true })).toBeEnabled();

  // C 仍为 TODO
  await openTask(page, taskC);
  await expect(page.locator(".task-detail__status")).toContainText("TODO");

  // B 执行并验收合并
  await openTask(page, taskB);
  await runToReview(page);
  await goReview(page);
  await approveAndConfirm(page);

  // C 自动转为 READY
  await openTask(page, taskC);
  await expect(page.locator(".task-detail__status")).toContainText("READY");
});

test("13. 依赖：菱形依赖（A → B, A → C, B+C → D）就绪条件与自动推进", async ({ page }) => {
  const taskA = await createTaskApi({
    teamId: teamGitId,
    roleId: roleGitId,
    title: "菱形根A [mock:shellwrite=notes/dia-a.txt]",
    status: "ready",
  });
  const taskB = await createTaskApi({
    teamId: teamGitId,
    roleId: roleGitId,
    title: "菱形左B [mock:shellwrite=notes/dia-b.txt]",
    status: "todo",
    dependsOn: [taskA.id],
  });
  const taskC = await createTaskApi({
    teamId: teamGitId,
    roleId: roleGitId,
    title: "菱形右C [mock:shellwrite=notes/dia-c.txt]",
    status: "todo",
    dependsOn: [taskA.id],
  });
  const taskD = await createTaskApi({
    teamId: teamGitId,
    roleId: roleGitId,
    title: "菱形汇合D",
    status: "todo",
    dependsOn: [taskB.id, taskC.id],
  });

  // A 完成 → B 与 C 同时变为 READY，D 仍为 TODO
  await openTask(page, taskA);
  await runToReview(page);
  await goReview(page);
  await approveAndConfirm(page);

  await openTask(page, taskB);
  await expect(page.locator(".task-detail__status")).toContainText("READY");
  await openTask(page, taskC);
  await expect(page.locator(".task-detail__status")).toContainText("READY");
  await openTask(page, taskD);
  await expect(page.locator(".task-detail__status")).toContainText("TODO");

  // 仅 B 完成 → D 依然是 TODO（因为 C 未完成）
  await openTask(page, taskB);
  await runToReview(page);
  await goReview(page);
  await approveAndConfirm(page);

  await openTask(page, taskD);
  await expect(page.locator(".task-detail__status")).toContainText("TODO");
  await expect(page.getByRole("button", { name: "开始执行", exact: true })).toBeDisabled();

  // C 也完成 → D 自动就绪转为 READY
  await openTask(page, taskC);
  await runToReview(page);
  await goReview(page);
  await approveAndConfirm(page);

  await openTask(page, taskD);
  await expect(page.locator(".task-detail__status")).toContainText("READY");
  await expect(page.getByRole("button", { name: "开始执行", exact: true })).toBeEnabled();
});

test("14. 依赖：重新打开前置任务时下游不回退且禁用开始执行", async ({ page }) => {
  const taskPre = await createTaskApi({
    teamId: teamGitId,
    roleId: roleGitId,
    title: "前置待重开A [mock:shellwrite=notes/reopen-a.txt]",
    status: "ready",
  });
  const taskDown = await createTaskApi({
    teamId: teamGitId,
    roleId: roleGitId,
    title: "下游不回退B",
    status: "todo",
    dependsOn: [taskPre.id],
  });

  // 完成前置任务
  await openTask(page, taskPre);
  await runToReview(page);
  await goReview(page);
  await approveAndConfirm(page);

  // 下游进入 READY
  await openTask(page, taskDown);
  await expect(page.locator(".task-detail__status")).toContainText("READY");
  await expect(page.getByRole("button", { name: "开始执行", exact: true })).toBeEnabled();

  // 重新打开前置任务（回到 READY）
  await openTask(page, taskPre);
  await page.getByRole("button", { name: "READY", exact: true }).click();
  await expect(page.locator(".task-detail__events")).toContainText("未回退");

  // 检查下游：状态保持 READY（不回退为 TODO），但开始执行被禁用并有明确依赖未完成提示
  await openTask(page, taskDown);
  await page.reload();
  await expect(page.locator(".task-detail__status")).toContainText("READY");
  await expect(page.getByRole("button", { name: "开始执行", exact: true })).toBeDisabled();
  await expect(page.getByText(new RegExp(`依赖任务 ${taskPre.key} 尚未完成`))).toBeVisible();
});

test("15. 依赖：表单环路预检实时报错且禁用保存", async ({ page }) => {
  const task1 = await createTaskApi({
    teamId: teamGitId,
    roleId: roleGitId,
    title: "环路测试任务1",
  });
  const task2 = await createTaskApi({
    teamId: teamGitId,
    roleId: roleGitId,
    title: "环路测试任务2",
    status: "todo",
    dependsOn: [task1.id],
  });

  // 编辑 task1，试图添加 task2 为依赖（形成 1 -> 2 -> 1 环路）
  await page.goto(`/#/teams/${teamGitId}/tasks/${task1.id}/edit`);
  await page.getByRole("checkbox", { name: new RegExp(task2.title) }).check();

  const alert = page.getByRole("alert");
  await expect(alert).toContainText("依赖形成环路");
  await expect(alert).toContainText(`${task1.key} → ${task2.key} → ${task1.key}`);
  await expect(page.getByRole("button", { name: "保存", exact: true })).toBeDisabled();

  // 移除环路依赖，恢复正常
  await page.getByRole("button", { name: new RegExp(`移除依赖 ${task2.key}`) }).click();
  await expect(alert).toHaveCount(0);
  await expect(page.getByRole("button", { name: "保存", exact: true })).toBeEnabled();
});

// ---------------------------------------------------------------- 4. 活动与审计（Activity & Audit）

test("16. 活动与审计：多条件筛选组合（岗位 + 任务 + 种类 + 时间范围求交集）", async ({ page }) => {
  const taskAct1 = await createTaskApi({
    teamId: teamGitId,
    roleId: roleGitId,
    title: "审计专用任务1",
    status: "ready",
  });

  await page.goto(`/#/teams/${teamGitId}/activity`);
  await page.reload();

  // 筛选：岗位
  await page.getByLabel("岗位", { exact: true }).selectOption({ label: ROLE_GIT });
  // 筛选：任务
  await page.getByLabel("任务", { exact: true }).selectOption({
    label: `${taskAct1.key} ${taskAct1.title}`,
  });
  // 筛选：活动种类
  const open = await page.locator(".act-multi").evaluate((el) => (el as HTMLDetailsElement).open);
  if (!open) {
    await page.locator(".act-multi__trigger").click();
  }
  await page.getByRole("checkbox", { name: "创建任务", exact: true }).click();
  // 筛选：时间范围「今天」
  await page.getByRole("group", { name: "时间范围" }).getByRole("button", { name: "今天" }).click();

  // 断言：符合交集条件的记录精准显示
  const rows = page.locator(".act-table tbody tr");
  await expect(rows).toHaveCount(1);
  await expect(rows.first()).toContainText(`创建任务 ${taskAct1.key}`);
});

test("17. 活动与审计：URL 保持（筛选参数写入 URL，刷新后严格保持）", async ({ page }) => {
  const taskAct2 = await createTaskApi({
    teamId: teamGitId,
    roleId: roleGitId,
    title: "URL保持任务",
  });

  const query = `?task=${taskAct2.id}&role=${roleGitId}&range=today`;
  await page.goto(`/#/teams/${teamGitId}/activity${query}`);
  await page.reload();

  // 验证地址栏依然保持参数
  await expect(page).toHaveURL(new RegExp(`task=${taskAct2.id}`));
  await expect(page).toHaveURL(new RegExp(`role=${roleGitId}`));
  await expect(page).toHaveURL(/range=today/);

  // 验证输入项回填
  await expect(page.getByLabel("任务", { exact: true })).toHaveValue(taskAct2.id);
  await expect(page.getByLabel("岗位", { exact: true })).toHaveValue(roleGitId);
  await expect(page.getByRole("button", { name: "今天" })).toHaveAttribute("aria-pressed", "true");
});

test("18. 活动与审计：加载更多（存在下一页时展示按钮，点击追加加载）", async ({ page }) => {
  let callCount = 0;
  // 拦截 list_events 模拟分页 cursor
  await page.route("**/invoke", async (route) => {
    const body = route.request().postDataJSON() as { cmd: string; args?: unknown };
    if (body.cmd === "list_events") {
      callCount += 1;
      if (callCount === 1) {
        await route.fulfill({
          json: {
            ok: {
              events: [
                {
                  id: "e-mock-1",
                  teamId: teamGitId,
                  actor: "human",
                  kind: "task_created",
                  summary: "第一页事件模拟 1",
                  createdAt: new Date().toISOString(),
                },
              ],
              nextCursor: "cursor-page-2",
            },
          },
        });
        return;
      } else {
        await route.fulfill({
          json: {
            ok: {
              events: [
                {
                  id: "e-mock-2",
                  teamId: teamGitId,
                  actor: "human",
                  kind: "task_created",
                  summary: "第二页追加事件 2",
                  createdAt: new Date().toISOString(),
                },
              ],
              nextCursor: null,
            },
          },
        });
        return;
      }
    }
    await route.continue();
  });

  await page.goto(`/#/teams/${teamGitId}/activity`);
  await expect(page.locator(".act-table tbody tr")).toHaveCount(1);
  const loadMoreBtn = page.getByRole("button", { name: "加载更多" });
  await expect(loadMoreBtn).toBeVisible();

  await loadMoreBtn.click();
  await expect(page.locator(".act-table tbody tr")).toHaveCount(2);
  await expect(page.locator(".act-table")).toContainText("第二页追加事件 2");
  await expect(loadMoreBtn).toHaveCount(0);
});

test("19. 活动与审计：清除筛选（一键清除全部条件，恢复默认列表与 URL）", async ({ page }) => {
  await page.goto(`/#/teams/${teamGitId}/activity`);
  await page.reload();

  // 设定筛选条件
  await page.getByRole("group", { name: "时间范围" }).getByRole("button", { name: "今天" }).click();
  const clearBtn = page.getByRole("button", { name: "清除筛选" }).first();
  await expect(clearBtn).toBeVisible();

  // 点击清除
  await clearBtn.click();
  await expect(page).not.toHaveURL(/range=/);
  await expect(page.locator(".act-multi__trigger")).toHaveText("全部种类（可多选）");
});

// ---------------------------------------------------------------- 5. 共享文件（Shared Files）

test("20. 共享文件：越界路径（..）与指向工作空间外的符号链接被拦截", async ({ page }) => {
  // 1. 尝试使用 .. 越界读取
  await page.goto(`/#/teams/${teamGitId}/files?path=../outside.md`);
  await page.reload();
  await expect(page.getByRole("heading", { name: "文件已被移动或删除" })).toBeVisible();
  await expect(page.getByText("文件路径不能包含「..」")).toBeVisible();

  // 2. 尝试在工作空间创建指向外部的符号链接
  const outsideFile = join(tmpdir(), "outside-leak.md");
  writeFileSync(outsideFile, "# 敏感外部内容\n");
  const linkPath = join(gitWs, "sym_leak.md");
  try {
    symlinkSync(outsideFile, linkPath);
  } catch {
    // 忽略软链接创建限制
  }

  // 访问该符号链接，内核拒绝展示
  await page.goto(`/#/teams/${teamGitId}/files?path=sym_leak.md`);
  await page.reload();
  await expect(page.getByRole("heading", { name: "文件已被移动或删除" })).toBeVisible();
  await expect(page.getByText("文件在工作空间之外，不予显示")).toBeVisible();

  rmSync(outsideFile, { force: true });
  rmSync(linkPath, { force: true });
});

test("21. 共享文件：非 md 文件（如 .txt、.png）不出现在列表且尝试读取被拒绝", async ({ page }) => {
  put(gitWs, "docs/image.png", Buffer.from([0, 1, 2, 3]));
  put(gitWs, "docs/notes.txt", "普通文本文件\n");

  await page.goto(`/#/teams/${teamGitId}/files`);
  await page.reload();

  // 目录树中不列出非 .md 文件
  await expect(page.locator(".files-tree__item").filter({ hasText: "image.png" })).toHaveCount(0);
  await expect(page.locator(".files-tree__item").filter({ hasText: "notes.txt" })).toHaveCount(0);

  // 直接在 URL 访问非 md 路径被提示拒绝
  await page.goto(`/#/teams/${teamGitId}/files?path=docs%2Fnotes.txt`);
  await page.reload();
  await expect(page.getByRole("heading", { name: "文件已被移动或删除" })).toBeVisible();
  await expect(page.getByText("共享文件只显示 Markdown（.md）文件")).toBeVisible();
});

// 超大文件（1.2 MB，远超 256 KiB 的读取上限）只渲染前一部分并明确标注截断，页面不再整页卡住。
test("22. 共享文件：超大 Markdown 文件（>256KiB 读取上限）截断提示「（内容已截断）」且页面可交互", async ({
  page,
}) => {
  // 创建 1.2 MB 的 Markdown 文件：仍是后端读取上限（256 KiB）的 4 倍以上
  const hugeContent = "# 超大需求规格书\n\n" + "M3 specification detail line.\n".repeat(40000);
  put(gitWs, "docs/huge.md", hugeContent);
  expect(Buffer.byteLength(hugeContent)).toBeGreaterThan(256 * 1024);

  await page.goto(`/#/teams/${teamGitId}/files?path=docs%2Fhuge.md`);
  await page.reload();

  // 列表与正文互不阻塞：文件树先出来，正文随后跟上（默认 8 秒超时内完成）
  await expect(page.locator(".files-tree__item").filter({ hasText: "huge.md" })).toBeVisible();
  await expect(page.locator(".files-doc__path")).toHaveText("docs/huge.md");
  await expect(page.locator(".files-doc__body h1")).toHaveText("超大需求规格书");
  // 验证顶部元数据明确标注「（内容已截断）」
  await expect(page.locator(".files-doc__meta")).toContainText("（内容已截断）");
  // 正文之前有独立的截断说明，并如实报告真实大小（1.1 MB > 上限）
  const notice = page.locator(".files-doc__truncated");
  await expect(notice).toBeVisible();
  await expect(notice.locator(".ui-tag")).toHaveText("内容已截断");
  await expect(notice).toContainText("只显示前一部分");
  await expect(page.locator(".files-doc__meta")).toContainText(/1\.\d MB/);

  // 渲染被截断的文档后页面仍然可交互：搜索框能立刻筛出文件
  await page.getByLabel("搜索文件名").fill("huge");
  await expect(page.locator(".files-tree__item").filter({ hasText: "huge.md" })).toBeVisible();
  await page.getByLabel("搜索文件名").fill("");
  await expect(page.locator(".files-tree__item").filter({ hasText: "spec.md" })).toBeVisible();
});

test("23. 共享文件：任务 ↔ 文件双向跳转", async ({ page }) => {
  put(gitWs, "plans/jump-target.md", "# 双向跳转验证文档\n\n此文档被任务关联。\n");

  const taskJump = await createTaskApi({
    teamId: teamGitId,
    roleId: roleGitId,
    title: "双向跳转任务",
    status: "ready",
    refs: ["plans/jump-target.md"],
  });

  // 1. 任务详情 → 文件页跳转
  await openTask(page, taskJump);
  await page
    .locator(".task-detail__files .file-chip__link", { hasText: "plans/jump-target.md" })
    .click();
  await expect(page.getByRole("heading", { level: 1, name: "共享文件" })).toBeVisible();
  await expect(page.locator(".files-doc__path")).toHaveText("plans/jump-target.md");
  await expect(page.locator(".files-doc__body h1")).toHaveText("双向跳转验证文档");

  // 2. 文件页右侧「引用任务」→ 任务详情跳转
  const refsPanel = page.locator(".files-refs .files-ref");
  await expect(refsPanel).toHaveCount(1);
  await expect(refsPanel).toContainText(taskJump.key);
  await expect(refsPanel).toContainText(taskJump.title);

  await refsPanel.getByRole("link", { name: "打开任务 →" }).click();
  await expect(page.getByRole("heading", { level: 1, name: taskJump.title })).toBeVisible();
});

test("24. 共享文件：文件被删除后的提示（文件页与任务详情引用均提示已被移动或删除）", async ({
  page,
}) => {
  const goneRel = "docs/to-be-deleted.md";
  put(gitWs, goneRel, "# 即将被删除的文件\n");

  const taskDel = await createTaskApi({
    teamId: teamGitId,
    roleId: roleGitId,
    title: "引用即将删除文件的任务",
    status: "ready",
    refs: [goneRel],
  });

  // 删除文件
  rmSync(join(gitWs, goneRel), { force: true });

  // 1. 共享文件页访问已删除文件：提示「文件已被移动或删除」
  await page.goto(`/#/teams/${teamGitId}/files?path=docs%2Fto-be-deleted.md`);
  await page.reload();
  await expect(page.getByRole("heading", { name: "文件已被移动或删除" })).toBeVisible();

  // 2. 任务详情页：引用区域提示「文件已被移动或删除」
  await openTask(page, taskDel);
  await page.reload();
  await expect(page.locator(".task-detail__files")).toContainText("文件已被移动或删除");
});
