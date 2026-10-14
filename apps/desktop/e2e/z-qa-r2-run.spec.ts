// 测试轮次 2026-10-14 · QA R2：执行（RUN-*）用例中「需新增 e2e / 部分覆盖」的部分。
// 真实界面 + 测试桥接 + 真实内核；Harness 是内置 Mock ACP Agent（标记见 mock_agent.rs 的模块文档）。
// 准备数据直接走桥接，被测的界面行为才用页面操作。每个 test 标题以用例编号开头。
// 断言一律按用例预期写；确认是产品缺陷的用例用 test.fixme 保留，注释写明疑点编号。
import { execFileSync } from "node:child_process";
import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { type Locator, type Page, expect, test } from "@playwright/test";

import {
  type RunInfo,
  type TaskRef,
  type TeamSetup,
  TIMEOUT,
  bind,
  createTaskApi,
  infoStrip,
  invoke,
  invokeError,
  openTask,
  runsOf,
  setupMockHarness,
  setupTeam,
  sqlite,
  startRunUi,
  statusLine,
  updatesOf,
  waitLatest,
} from "./z-qa-r2-helpers.js";

test.describe.configure({ mode: "serial", timeout: 90_000 });

const HARNESS = "r2-mock";
const HARNESS_GIT = "r2-mock-git";
const ROLE = "执行员";
const ROLE_FREE = "未绑定员";
const ROLE_GIT = "分支员";
const SPEC_REF = "文档/需求说明_v1.md";

let plain = "";
let gitWs = "";
let teamP: TeamSetup;
let teamG: TeamSetup;

function git(cwd: string, ...args: string[]): string {
  return execFileSync(
    "git",
    ["-c", "user.name=E2E", "-c", "user.email=e2e@example.invalid", ...args],
    { cwd, encoding: "utf8" },
  );
}

function writeFile(root: string, relative: string, content: string): void {
  const target = join(root, relative);
  mkdirSync(dirname(target), { recursive: true });
  writeFileSync(target, content);
}

function seed(root: string): void {
  writeFile(root, "brief.txt", "需求说明\n");
  writeFile(root, SPEC_REF, "# 需求说明 v1\n");
  writeFile(root, "docs/spec.md", "# 规格\n");
}

function journal(root: string, role: string): string {
  return readFileSync(join(root, "Teamspace", role, "工作日志.md"), "utf8");
}

/** 创建任务并经后端开始执行，等执行结束，返回最新一次执行。 */
async function runToEnd(task: TaskRef): Promise<RunInfo> {
  await invoke("start_run", { taskId: task.id });
  return await waitLatest(
    task.id,
    (r) => !["queued", "running", "waiting_permission"].includes(r.status),
    "执行结束",
  );
}

test.beforeAll(async () => {
  plain = mkdtempSync(join(tmpdir(), "teamspace-e2e-r2-plain-"));
  seed(plain);
  gitWs = mkdtempSync(join(tmpdir(), "teamspace-e2e-r2-git-"));
  git(gitWs, "init", "-q", "-b", "main");
  seed(gitWs);
  git(gitWs, "add", ".");
  git(gitWs, "-c", "commit.gpgsign=false", "commit", "-q", "-m", "docs: seed");

  teamP = await setupTeam("R2 普通团队", plain, [
    { name: ROLE, prefix: "RP" },
    { name: ROLE_FREE, prefix: "RF" },
    { name: "归档绑定员", prefix: "RA" },
    { name: "异常握手员", prefix: "RE" },
    { name: "不续接员", prefix: "RN" },
    { name: "日志员", prefix: "RJ" },
  ]);
  const harness = await setupMockHarness(teamP.teamId, HARNESS);
  await bind(teamP.roleIds[ROLE] ?? "", harness);
  await bind(teamP.roleIds["日志员"] ?? "", harness);

  teamG = await setupTeam("R2 Git 团队", gitWs, [{ name: ROLE_GIT, prefix: "RG" }]);
  const harnessGit = await setupMockHarness(teamG.teamId, HARNESS_GIT);
  await bind(teamG.roleIds[ROLE_GIT] ?? "", harnessGit);
});

test.afterAll(() => {
  rmSync(plain, { recursive: true, force: true });
  rmSync(gitWs, { recursive: true, force: true });
});

async function plainTask(
  title: string,
  extra: { role?: string; status?: "todo" | "ready"; refs?: string[]; dependsOn?: string[] } = {},
): Promise<TaskRef> {
  return await createTaskApi({
    teamId: teamP.teamId,
    roleId: teamP.roleIds[extra.role ?? ROLE] ?? "",
    title,
    status: extra.status,
    refs: extra.refs ?? [SPEC_REF],
    dependsOn: extra.dependsOn,
  });
}

/**
 * 已确认的产品缺陷：按用例预期写断言并保留，默认标为 fixme 跳过，让门禁保持绿色。
 * 设置 R2_RUN_FIXME=1 可把它们当作普通用例运行，用来复核缺陷是否仍然存在。
 */
function defect(title: string, body: (args: { page: Page }) => Promise<void>): void {
  if (process.env.R2_RUN_FIXME === "1") {
    test(title, async ({ page }) => {
      await body({ page });
    });
  } else {
    test.fixme(title, async ({ page }) => {
      await body({ page });
    });
  }
}

// ================================================================ 前置条件

test("RUN-PRE-001 未绑定 Harness：按钮禁用、提示与警告卡片，后端 start_run 返回冲突", async ({
  page,
}) => {
  const task = await plainTask("未绑定的任务", { role: ROLE_FREE });
  await openTask(page, task);
  const reason = `负责岗位「${ROLE_FREE}」未绑定 Harness，请先在「岗位与绑定」中绑定`;
  const start = page.getByRole("button", { name: "开始执行" });
  await expect(start).toBeDisabled();
  await expect(start).toHaveAttribute("title", `还不能开始执行：${reason}`);
  const card = page.locator(".ui-card", { hasText: "还不能开始执行" });
  await expect(card).toBeVisible();
  await expect(card.locator("li", { hasText: reason })).toBeVisible();
  const error = await invokeError("start_run", { taskId: task.id });
  expect(error).toContain("conflict");
  expect(error).toContain("未绑定 Harness");
});

test("RUN-PRE-002 绑定的 Harness 已归档：按钮禁用，提示含「已归档，请重新绑定」", async ({
  page,
}) => {
  // 产品流程不允许归档仍被绑定的 Harness；这里直接改库制造该状态，验证前置检查本身。
  const name = "r2-archived";
  const harness = await invoke<{ id: string }>("create_harness", {
    input: { teamId: teamP.teamId, name, preset: "mock" },
  });
  const roleId = teamP.roleIds["归档绑定员"] ?? "";
  await bind(roleId, harness.id);
  const task = await plainTask("归档 Harness 的任务", { role: "归档绑定员" });
  sqlite(`UPDATE harnesses SET archived_at = datetime('now') WHERE id = '${harness.id}'`);
  await openTask(page, task);
  const start = page.getByRole("button", { name: "开始执行" });
  await expect(start).toBeDisabled();
  await expect(start).toHaveAttribute(
    "title",
    `还不能开始执行：Harness「${name}」已归档，请重新绑定`,
  );
  const card = page.locator(".ui-card", { hasText: "还不能开始执行" });
  await expect(
    card.locator("li", { hasText: `Harness「${name}」已归档，请重新绑定` }),
  ).toBeVisible();
  expect(await invokeError("start_run", { taskId: task.id })).toContain("已归档");
});

test("RUN-PRE-003 Harness 握手检查失败：按钮禁用，提示含失败说明", async ({ page }) => {
  const name = "r2-broken";
  const harness = await invoke<{ id: string }>("create_harness", {
    input: {
      teamId: teamP.teamId,
      name,
      preset: "custom",
      command: "/nonexistent/r2-agent",
      args: [],
    },
  });
  const checked = await invoke<{ handshake: { status: string; detail: string | null } }>(
    "check_harness",
    { harnessId: harness.id },
  );
  expect(checked.handshake.status).toBe("error");
  await bind(teamP.roleIds["异常握手员"] ?? "", harness.id);
  const task = await plainTask("握手失败的任务", { role: "异常握手员" });
  await openTask(page, task);
  const start = page.getByRole("button", { name: "开始执行" });
  await expect(start).toBeDisabled();
  const prefix = `Harness「${name}」握手检查失败，请先修正配置并重新检查：`;
  await expect(start).toHaveAttribute("title", new RegExp(`还不能开始执行：.*${prefix}.+`));
  const card = page.locator(".ui-card", { hasText: "还不能开始执行" });
  await expect(card.locator("li", { hasText: prefix })).toBeVisible();
});

test("RUN-PRE-005 前置依赖任务未完成：按钮禁用，提示依赖任务及其状态（执行中 / 受阻）", async ({
  page,
}) => {
  const dep = await plainTask("依赖任务 [mock:hang]");
  const waiting = await plainTask("等依赖的任务", { status: "todo", dependsOn: [dep.id] });
  await invoke("start_run", { taskId: dep.id });
  await waitLatest(dep.id, (r) => r.status === "running", "依赖任务进入执行中");

  await openTask(page, waiting);
  const start = page.getByRole("button", { name: "开始执行" });
  await expect(start).toBeDisabled();
  await expect(start).toHaveAttribute(
    "title",
    new RegExp(`依赖任务 ${dep.key} 尚未完成（执行中）`),
  );
  const card = page.locator(".ui-card", { hasText: "进入 READY 前还需要" });
  await expect(
    card.locator("li", { hasText: `依赖任务 ${dep.key} 尚未完成（执行中）` }),
  ).toBeVisible();

  // 依赖任务受阻：先取消，再让它失败
  const run = (await runsOf(dep.id))[0];
  await invoke("cancel_run", { runId: run?.id });
  const failing = await plainTask("会失败的依赖 [mock:fail]");
  const blockedWaiting = await plainTask("等失败依赖的任务", {
    status: "todo",
    dependsOn: [failing.id],
  });
  const ended = await runToEnd(failing);
  expect(ended.status).toBe("failed");
  await openTask(page, blockedWaiting);
  await expect(page.getByRole("button", { name: "开始执行" })).toBeDisabled();
  await expect(
    page
      .locator(".ui-card", { hasText: "进入 READY 前还需要" })
      .locator("li", { hasText: `依赖任务 ${failing.key} 尚未完成（受阻）` }),
  ).toBeVisible();
});

test("RUN-PRE-006 引用文件在工作区中不存在：标签「文件不存在」，按钮禁用，提示含路径", async ({
  page,
}) => {
  writeFile(plain, "docs/r2-missing.md", "# 将被删除\n");
  const task = await plainTask("引用文件会消失的任务", { refs: ["docs/r2-missing.md"] });
  rmSync(join(plain, "docs/r2-missing.md"));
  await openTask(page, task);
  const chip = page.locator(".file-chip--missing", { hasText: "docs/r2-missing.md" });
  await expect(chip).toBeVisible();
  await expect(chip).toContainText("文件不存在");
  const start = page.getByRole("button", { name: "开始执行" });
  await expect(start).toBeDisabled();
  await expect(start).toHaveAttribute("title", /引用的文件在工作区中不存在：docs\/r2-missing\.md/);
});

test("RUN-PRE-008 任务已完成或已取消：不能开始执行，提示当前状态与允许的状态", async ({ page }) => {
  const done = await plainTask("已完成的任务");
  const run = await runToEnd(done);
  expect(run.status).toBe("succeeded");
  await invoke("change_task_status", { input: { taskId: done.id, to: "done" } });
  await openTask(page, done);
  const doneReason =
    "任务状态为「已完成」：只有 READY、受阻，或执行中断、人工接管后的任务可以开始执行";
  await expect(page.getByRole("button", { name: /^(开始执行|重新执行)$/ })).toHaveCount(0);
  await expect(page.locator(".ui-card", { hasText: "还不能开始执行" })).toContainText(doneReason);
  expect(await invokeError("start_run", { taskId: done.id })).toContain(doneReason);

  const cancelled = await plainTask("已取消的任务", { status: "todo" });
  await invoke("change_task_status", { input: { taskId: cancelled.id, to: "cancelled" } });
  await openTask(page, cancelled);
  const start = page.getByRole("button", { name: "开始执行" });
  await expect(start).toBeDisabled();
  await expect(start).toHaveAttribute(
    "title",
    /任务状态为「已取消」：只有 READY、受阻，或执行中断、人工接管后的任务可以开始执行/,
  );
});

// ================================================================ 目录隔离

test("RUN-DIR-001 Git 工作空间：独立 worktree 与分支，路径在 worktrees/<团队>/ 下", async ({
  page,
}) => {
  const task = await createTaskApi({
    teamId: teamG.teamId,
    roleId: teamG.roleIds[ROLE_GIT] ?? "",
    title: "Git 目录隔离",
    refs: ["brief.txt"],
  });
  await openTask(page, task);
  await startRunUi(page);
  const run = await waitLatest(task.id, (r) => r.status === "succeeded", "执行成功");
  expect(run.isolation).toBe("worktree");
  const branch = `teamspace/${task.key}-r1`;
  expect(run.branch).toBe(branch);
  expect(run.execDir).toContain(`/worktrees/${teamG.teamId}/${task.key}-r1`);
  expect(existsSync(run.execDir)).toBe(true);
  expect(git(gitWs, "branch", "--list", branch)).toContain(branch);
  await expect(infoStrip(page)).toContainText(`Git worktree · 分支 ${branch}`);
  await expect(infoStrip(page)).toContainText(run.execDir);
});

test("RUN-DIR-005 普通文件夹：原地执行，exec_dir 即工作空间，分支与基线提交为空", async ({
  page,
}) => {
  const task = await plainTask("原地执行核对");
  await openTask(page, task);
  await startRunUi(page);
  const run = await waitLatest(task.id, (r) => r.status === "succeeded", "执行成功");
  expect(run.isolation).toBe("in_place");
  expect(run.execDir).toBe(plain);
  expect(run.branch).toBeNull();
  expect(run.baseCommit).toBeNull();
  await expect(infoStrip(page)).toContainText("原地：普通文件夹");
  await expect(infoStrip(page)).toContainText(plain);
});

// ================================================================ 权限与进度流

const dialogOf = (page: Page): Locator => page.getByRole("dialog");
const streamOf = (page: Page): Locator => page.locator(".run-stream");

function cleanOut(): void {
  rmSync(join(plain, "out.md"), { force: true });
}

/** 取消任务最新一次仍在进行的执行（释放普通文件夹）。 */
async function cancelActive(task: TaskRef): Promise<void> {
  const run = (await runsOf(task.id))[0];
  if (run !== undefined && ["queued", "running", "waiting_permission"].includes(run.status)) {
    await invoke("cancel_run", { runId: run.id });
  }
}

/** 开始执行并等权限弹窗出现。 */
async function startUntilDialog(page: Page, task: TaskRef): Promise<void> {
  await openTask(page, task);
  await startRunUi(page);
  await expect(dialogOf(page)).toBeVisible({ timeout: TIMEOUT });
}

test("RUN-EVT-001 Agent 消息：头像为岗位名首字，来源「岗位名 · Harness名」，分块合并为一条", async ({
  page,
}) => {
  const task = await plainTask("消息展示 [mock:perm]");
  await startUntilDialog(page, task);
  await dialogOf(page).getByRole("button", { name: "稍后处理" }).click();
  const message = streamOf(page).locator(".run-msg--agent").first();
  await expect(message.locator(".run-msg__avatar")).toHaveText(ROLE.slice(0, 1));
  await expect(message.locator(".run-msg__who")).toHaveText(`${ROLE} · ${HARNESS}`);
  await expect(message.locator(".run-msg__text")).toHaveText("收到任务，开始处理。");
  await cancelActive(task);
});

test("RUN-EVT-003 计划清单：「1 / 3 已完成」、状态标记与进行中标签、优先级", async ({ page }) => {
  const task = await plainTask("计划展示 [mock:perm]");
  await startUntilDialog(page, task);
  await dialogOf(page).getByRole("button", { name: "稍后处理" }).click();
  const plan = streamOf(page).locator(".run-plan");
  await expect(plan.locator(".run-plan__title")).toHaveText("计划");
  await expect(plan.locator(".run-plan__count")).toHaveText("1 / 3 已完成");
  const items = plan.locator(".run-plan__item");
  await expect(items).toHaveCount(3);
  await expect(items.nth(0).locator(".run-plan__marker")).toHaveText("✓");
  await expect(items.nth(1).locator(".run-plan__marker")).toHaveText("◉");
  await expect(items.nth(1).locator(".ui-tag", { hasText: "进行中" })).toBeVisible();
  await expect(items.nth(2).locator(".run-plan__marker")).toHaveText("○");
  await expect(items.nth(2).locator(".ui-tag", { hasText: "进行中" })).toHaveCount(0);
  for (let i = 0; i < 3; i += 1) {
    await expect(items.nth(i).locator(".run-plan__priority")).toContainText("中");
  }
  await cancelActive(task);
});

test("RUN-EVT-004 工具调用卡片：类型标签、状态胶囊、命令预格式化块与路径", async ({ page }) => {
  const task = await plainTask("工具调用展示 [mock:perm]");
  await startUntilDialog(page, task);
  await dialogOf(page).getByRole("button", { name: "稍后处理" }).click();
  const read = streamOf(page).locator(".run-tool", { hasText: "读取引用文件" });
  await expect(read.locator(".ui-tag").first()).toHaveText("读取");
  await expect(read.locator(".ui-tag", { hasText: "已完成" })).toBeVisible();
  await expect(read.locator(".run-tool__paths")).toContainText(SPEC_REF);

  const write = streamOf(page).locator(".run-tool", { hasText: "写入过程文件" });
  await expect(write.locator(".ui-tag").first()).toHaveText("编辑");
  await expect(write.locator(".ui-tag", { hasText: "已完成" })).toBeVisible();
  await expect(write.locator(".run-tool__paths")).toContainText(`Teamspace/${ROLE}/过程文件/mock-`);

  const exec = streamOf(page).locator(".run-tool", { hasText: "运行 echo hello" });
  await expect(exec.locator(".ui-tag").first()).toHaveText("执行命令");
  await expect(exec.locator("pre.run-tool__command")).toHaveText("echo hello");
  await expect(exec.locator(".ui-tag", { hasText: /等待中|进行中/ })).toBeVisible();
  await cancelActive(task);
});

test("RUN-EVT-005 工具调用 diff 摘要：默认折叠，展开显示文件名与行数统计，按钮文字切换", async ({
  page,
}) => {
  const task = await plainTask("diff 展示 [mock:perm]");
  await startUntilDialog(page, task);
  await dialogOf(page).getByRole("button", { name: "稍后处理" }).click();
  const write = streamOf(page).locator(".run-tool", { hasText: "写入过程文件" });
  const toggle = write.getByRole("button", { name: /diff 摘要/ });
  await expect(toggle).toContainText("展开 ▼");
  await expect(write.locator(".run-tool__diffs")).toHaveCount(0);
  await toggle.click();
  await expect(toggle).toContainText("收起 ▲");
  const diff = write.locator(".run-tool__diff");
  await expect(diff.locator(".run-tool__diff-path")).toContainText(
    `Teamspace/${ROLE}/过程文件/mock-`,
  );
  await expect(diff.locator(".run-tool__diff-add")).toHaveText("+3");
  await expect(diff.locator(".run-tool__diff-del")).toHaveText("-0");
  await toggle.click();
  await expect(toggle).toContainText("展开 ▼");
  await cancelActive(task);
});

test("RUN-EVT-006 文件写入事件：「已写入」带原因，越界被拒显示「已阻止」", async ({ page }) => {
  cleanOut();
  const task = await plainTask("写入事件 [mock:outside] [mock:linger]");
  await startUntilDialog(page, task);
  await dialogOf(page).getByRole("button", { name: "拒绝" }).click();
  await expect(dialogOf(page)).toHaveCount(0);
  const rows = streamOf(page).locator(".run-filewrite");
  const ok = rows.filter({ hasText: "已写入" });
  await expect(ok).toHaveCount(1);
  await expect(ok).toContainText(`Teamspace/${ROLE}/过程文件/mock-`);
  await expect(ok.locator(".run-filewrite__reason")).toHaveText("在自动批准范围内");
  const blocked = rows.filter({ hasText: "已阻止" });
  await expect(blocked).toHaveCount(1);
  await expect(blocked).toContainText("out.md");
  await expect(blocked.locator(".run-filewrite__reason")).toContainText(
    "写入本岗位文件夹与任务引用文件之外需要确认",
  );
  expect(existsSync(join(plain, "out.md"))).toBe(false);
  await cancelActive(task);
});

test("RUN-PRM-006 权限弹窗内容：标题、意图、命令、工作目录、工具类型、原因与选项", async ({
  page,
}) => {
  const task = await plainTask("弹窗内容 [mock:perm]");
  await startUntilDialog(page, task);
  const dialog = dialogOf(page);
  await expect(dialog).toContainText(`${ROLE} 请求权限`);
  await expect(dialog.locator(".ui-tag", { hasText: "等待你确认" }).first()).toBeVisible();
  await expect(dialog.locator(".perm-request__intent")).toHaveText("Agent 想执行一条命令");
  await expect(dialog.locator(".perm-request__code")).toHaveText("echo hello");
  const facts = dialog.locator(".perm-request__facts");
  await expect(facts).toContainText("工作目录");
  await expect(facts).toContainText(`${plain}（原地：普通文件夹）`);
  await expect(facts).toContainText("工具类型");
  await expect(facts).toContainText("execute · 执行命令");
  await expect(dialog.locator(".perm-reason__title")).toHaveText("为什么需要你确认");
  await expect(dialog.locator(".perm-reason__text")).toHaveText("执行命令需要确认");
  for (const name of ["允许一次", "总是允许", "拒绝", "稍后处理"]) {
    await expect(dialog.getByRole("button", { name, exact: true })).toBeVisible();
  }
  await expect(dialog).toContainText("「总是允许」只在本次执行内有效，执行结束即失效。");
  await cancelActive(task);
});

test("RUN-PRM-007 稍后处理：弹窗关闭、执行保持等待，提示条可重新打开且内容一致", async ({
  page,
}) => {
  const task = await plainTask("稍后处理 [mock:perm]");
  await startUntilDialog(page, task);
  const reason = await dialogOf(page).locator(".perm-reason__text").innerText();
  await dialogOf(page).getByRole("button", { name: "稍后处理" }).click();
  await expect(dialogOf(page)).toHaveCount(0);
  expect((await runsOf(task.id))[0]?.status).toBe("waiting_permission");
  const pending = page.locator(".run-pending");
  await expect(pending).toContainText("有一个权限请求在等你确认。");
  await pending.getByRole("button", { name: "重新打开权限请求" }).click();
  const dialog = dialogOf(page);
  await expect(dialog).toBeVisible();
  await expect(dialog.locator(".perm-reason__text")).toHaveText(reason);
  await expect(dialog.locator(".perm-request__code")).toHaveText("echo hello");
  await expect(dialog.getByRole("button", { name: "允许一次" })).toBeVisible();
  await expect(dialog.getByRole("button", { name: "拒绝" })).toBeVisible();
  await dialog.getByRole("button", { name: "拒绝" }).click();
  await expect(page.locator(".run-outcome--success")).toBeVisible({ timeout: TIMEOUT });
});

test("RUN-PRM-008 看板任务卡：等待权限时「等待你确认」，批准后改为「执行中 · Harness」", async ({
  page,
}) => {
  const task = await plainTask("看板徽标 [mock:perm] [mock:linger]");
  await startUntilDialog(page, task);
  await page.goto(`/#/teams/${teamP.teamId}/tasks`);
  const card = page.locator("a.task-card-link", { hasText: task.title });
  await expect(card.locator(".ui-tag", { hasText: "等待你确认" })).toBeVisible();
  const run = (await runsOf(task.id))[0];
  await invoke("respond_permission", {
    input: { runId: run?.id, requestId: run?.pendingPermission?.requestId, optionId: "allow" },
  });
  await expect(card.locator(".ui-tag", { hasText: `执行中 · ${HARNESS}` })).toBeVisible();
  await expect(card.locator(".ui-tag", { hasText: "等待你确认" })).toHaveCount(0);
  await cancelActive(task);
});

test("RUN-DEC-001 允许后：弹窗关闭、执行恢复，进度流记「你已允许」，随后的写入放行", async ({
  page,
}) => {
  cleanOut();
  const task = await plainTask("允许后继续 [mock:outside] [mock:linger]");
  await startUntilDialog(page, task);
  await dialogOf(page).getByRole("button", { name: "允许一次" }).click();
  await expect(dialogOf(page)).toHaveCount(0);
  await expect(infoStrip(page)).toContainText("执行中");
  const permission = streamOf(page).locator(".run-permission", { hasText: "写入 out.md" });
  await expect(permission.locator(".ui-tag", { hasText: "你已允许" })).toBeVisible();
  await expect(permission.locator(".run-permission__decision")).toContainText(
    "Human 选择「允许一次」",
  );
  const written = streamOf(page).locator(".run-filewrite", { hasText: "out.md" });
  await expect(written).toContainText("已写入");
  await expect(written.locator(".run-filewrite__reason")).toHaveText("Human 已允许对应的权限请求");
  expect(existsSync(join(plain, "out.md"))).toBe(true);
  await cancelActive(task);
  cleanOut();
});

test("RUN-DEC-002 拒绝后：进度流「已拒绝」，工具调用「失败」，执行收尾为待验收", async ({
  page,
}) => {
  const lingering = await plainTask("拒绝后观察 [mock:perm] [mock:linger]");
  await startUntilDialog(page, lingering);
  await dialogOf(page).getByRole("button", { name: "拒绝" }).click();
  const permission = streamOf(page).locator(".run-permission", { hasText: "运行 echo hello" });
  await expect(permission.locator(".ui-tag", { hasText: "已拒绝" })).toBeVisible();
  await expect(permission.locator(".run-permission__decision")).toContainText("Human 选择「拒绝」");
  const tool = streamOf(page).locator(".run-tool", { hasText: "运行 echo hello" });
  await expect(tool.locator(".ui-tag", { hasText: "失败" })).toBeVisible();
  await cancelActive(lingering);

  const finishing = await plainTask("拒绝后收尾 [mock:perm]");
  await startUntilDialog(page, finishing);
  await dialogOf(page).getByRole("button", { name: "拒绝" }).click();
  await expect(page.locator(".run-outcome--success")).toBeVisible({ timeout: TIMEOUT });
  await expect(statusLine(page)).toContainText("待验收");
  const run = (await runsOf(finishing.id))[0];
  expect(run?.status).toBe("succeeded");
  const tools = (await updatesOf(run?.id ?? "")).filter(
    (u) => u.body.type === "tool_call" && u.body.command === "echo hello",
  );
  expect(tools).toHaveLength(1);
});

test("RUN-DEC-003 等待权限时取消执行：弹窗关闭，权限请求被撤销（cancelled），任务回到 READY", async ({
  page,
}) => {
  const task = await plainTask("等待时取消 [mock:perm]");
  await startUntilDialog(page, task);
  const run = (await runsOf(task.id))[0];
  await invoke("cancel_run", { runId: run?.id });
  await expect(dialogOf(page)).toHaveCount(0);
  await expect(page.locator(".run-outcome--cancelled")).toBeVisible({ timeout: TIMEOUT });
  await expect(statusLine(page)).toContainText("READY");
  // 执行结束后界面不再显示进度流，「已撤销」标签只能在记录里核对：权限决定为 cancelled。
  const updates = await updatesOf(run?.id ?? "");
  const permission = updates.find(
    (u) => u.body.type === "permission" && u.body.command === "echo hello",
  );
  expect(permission?.body.decision?.kind).toBe("cancelled");
  expect(permission?.body.decision?.by).toBe("cancelled");
  const tool = updates.find((u) => u.body.type === "tool_call" && u.body.command === "echo hello");
  expect(tool).toBeDefined();
});

test("RUN-DEC-004 允许后立即刷新：弹窗不再出现，进度流保留决定，执行继续", async ({ page }) => {
  cleanOut();
  const task = await plainTask("允许后刷新 [mock:outside] [mock:linger]");
  await startUntilDialog(page, task);
  await dialogOf(page).getByRole("button", { name: "允许一次" }).click();
  await page.reload();
  await expect(infoStrip(page)).toContainText("执行中", { timeout: TIMEOUT });
  await expect(dialogOf(page)).toHaveCount(0);
  const permission = streamOf(page).locator(".run-permission", { hasText: "写入 out.md" });
  await expect(permission.locator(".ui-tag", { hasText: "你已允许" })).toBeVisible();
  await expect(permission.locator(".run-permission__decision")).toContainText(
    "Human 选择「允许一次」",
  );
  await expect(streamOf(page).locator(".run-filewrite", { hasText: "out.md" })).toContainText(
    "已写入",
  );
  expect((await runsOf(task.id))[0]?.status).toBe("running");
  await cancelActive(task);
  cleanOut();
});

test("RUN-DEC-005 引用文件含中文名与子目录：编辑请求按规则自动放行，不弹窗，路径显示正常", async ({
  page,
}) => {
  const target = join(plain, SPEC_REF);
  writeFileSync(target, "# 需求说明 v1\n");
  const task = await plainTask("编辑引用文件 [mock:editref] [mock:linger]");
  await openTask(page, task);
  await startRunUi(page);
  const written = streamOf(page).locator(".run-filewrite", { hasText: SPEC_REF });
  await expect(written).toContainText("已写入", { timeout: TIMEOUT });
  await expect(written.locator(".run-filewrite__reason")).toHaveText("在自动批准范围内");
  await expect(written.locator(".run-filewrite__path")).toHaveText(`${plain}/${SPEC_REF}`);
  const permission = streamOf(page).locator(".run-permission", { hasText: "编辑引用文件" });
  await expect(permission.locator(".ui-tag", { hasText: "自动批准" })).toBeVisible();
  await expect(permission.locator(".run-tool__paths")).toContainText(SPEC_REF);
  await expect(permission.locator(".run-permission__decision")).toContainText(
    "编辑本岗位文件夹或任务引用文件，自动允许",
  );
  await expect(dialogOf(page)).toHaveCount(0);
  expect((await runsOf(task.id))[0]?.status).toBe("running");
  expect(readFileSync(target, "utf8")).toContain("# Mock 越权写入");
  await cancelActive(task);
  writeFileSync(target, "# 需求说明 v1\n");
});

test("RUN-PRM-009 越界写入：弹窗说明原因，拒绝后磁盘无文件，允许后才落盘", async ({ page }) => {
  cleanOut();
  const rejected = await plainTask("越界写入 拒绝 [mock:outside]");
  await startUntilDialog(page, rejected);
  const dialog = dialogOf(page);
  await expect(dialog.locator(".perm-reason__text")).toContainText(
    "写入本岗位文件夹与任务引用文件之外需要确认",
  );
  await expect(dialog.locator(".perm-request__paths")).toContainText("out.md");
  await expect(dialog.locator(".perm-request__intent")).toHaveText("Agent 想写入或修改文件");
  await expect(infoStrip(page)).toContainText("等待确认");
  expect(existsSync(join(plain, "out.md"))).toBe(false);
  await dialog.getByRole("button", { name: "拒绝" }).click();
  await expect(page.locator(".run-outcome--success")).toBeVisible({ timeout: TIMEOUT });
  expect(existsSync(join(plain, "out.md"))).toBe(false);
  const run1 = (await runsOf(rejected.id))[0];
  const blocked = (await updatesOf(run1?.id ?? "")).filter(
    (u) => u.body.type === "file_write" && (u.body.path ?? "").endsWith("/out.md"),
  );
  expect(blocked).toHaveLength(1);
  expect(blocked[0]?.body.allowed).toBe(false);

  const allowed = await plainTask("越界写入 允许 [mock:outside]");
  await startUntilDialog(page, allowed);
  await expect(dialogOf(page).locator(".perm-reason__text")).toContainText(
    "写入本岗位文件夹与任务引用文件之外需要确认",
  );
  await dialogOf(page).getByRole("button", { name: "允许一次" }).click();
  await expect(page.locator(".run-outcome--success")).toBeVisible({ timeout: TIMEOUT });
  expect(readFileSync(join(plain, "out.md"), "utf8")).toContain("# Mock 越权写入");
  const run2 = (await runsOf(allowed.id))[0];
  const writes = (await updatesOf(run2?.id ?? "")).filter(
    (u) => u.body.type === "file_write" && (u.body.path ?? "").endsWith("/out.md"),
  );
  expect(writes).toHaveLength(1);
  expect(writes[0]?.body.allowed).toBe(true);
  expect(writes[0]?.body.reason).toBe("Human 已允许对应的权限请求");
  cleanOut();
});

test("RUN-GOAL-001 可用标志：派工、实时进度、批准权限请求、拿到结果、岗位日志留下记录", async ({
  page,
}) => {
  cleanOut();
  const task = await plainTask("可用标志串联 [mock:outside] [mock:perm]");
  await openTask(page, task);
  await expect(statusLine(page)).toContainText("READY");
  await startRunUi(page);
  await expect(statusLine(page)).toContainText("执行中");
  await expect(infoStrip(page)).toContainText("原地：普通文件夹");
  // 第一个权限请求：越界写入
  await expect(dialogOf(page)).toBeVisible({ timeout: TIMEOUT });
  await expect(dialogOf(page).locator(".perm-request__paths")).toContainText("out.md");
  await expect(streamOf(page).locator(".run-plan")).toBeVisible();
  await dialogOf(page).getByRole("button", { name: "允许一次" }).click();
  // 第二个权限请求：执行命令
  await expect(dialogOf(page).locator(".perm-request__code")).toHaveText("echo hello", {
    timeout: TIMEOUT,
  });
  await dialogOf(page).getByRole("button", { name: "允许一次" }).click();

  const success = page.locator(".run-outcome--success");
  await expect(success).toBeVisible({ timeout: TIMEOUT });
  await expect(statusLine(page)).toContainText("待验收");
  await expect(success.locator(".run-outcome__summary")).toContainText("命令 echo hello 已执行");
  await success.getByRole("button", { name: "查看改动" }).click();
  await expect(success.locator(".run-outcome__files")).toContainText("out.md");
  await expect(success.locator(".run-outcome__files")).toContainText(
    `Teamspace/${ROLE}/过程文件/mock-`,
  );
  expect(readFileSync(join(plain, "out.md"), "utf8")).toContain("# Mock 越权写入");

  await expect
    .poll(() => journal(plain, ROLE), { timeout: TIMEOUT })
    .toContain(`${task.key} · 第 1 次执行完成（${HARNESS}）；摘要：`);
  const log = journal(plain, ROLE);
  expect(log).toContain(`${task.key} · 开始第 1 次执行`);
  expect(log).toContain("改动文件：");
  expect(log).toContain("执行中 → 待验收");
  cleanOut();
});

// ---------------------------------------------------------------- 侧栏权限规则

test("RUN-PRM-010 侧栏「权限规则」：原地执行的自动批准 / 转交确认与单执行约束注脚", async ({
  page,
}) => {
  const task = await plainTask("规则面板 原地 [mock:perm]");
  await startUntilDialog(page, task);
  await dialogOf(page).getByRole("button", { name: "稍后处理" }).click();
  const panel = page.locator(".run-side__panel", { hasText: "权限规则" });
  const auto = panel.locator(".run-rule", { hasText: "自动批准" });
  await expect(auto).toContainText(`写入本岗位文件夹 Teamspace/${ROLE}/`);
  await expect(auto).toContainText(`写入任务引用文件：${SPEC_REF}`);
  await expect(auto).not.toContainText("worktree");
  const ask = panel.locator(".run-rule", { hasText: "转交你确认" });
  await expect(ask).toContainText("写入其他位置");
  await expect(ask).toContainText("执行命令");
  await expect(ask).toContainText("删除、移动文件，访问网络，切换 Agent 工作模式");
  await expect(panel.locator(".run-side__note")).toContainText("原地：普通文件夹");
  await expect(panel.locator(".run-side__note")).toContainText("同一文件夹同时只运行一个执行");
  await cancelActive(task);
});

defect(
  "RUN-PRM-010 [缺陷 N-03] Git worktree 的侧栏规则：不再写单执行约束；两种模式都说明例外与读取规则",
  async ({ page }) => {
    const task = await createTaskApi({
      teamId: teamG.teamId,
      roleId: teamG.roleIds[ROLE_GIT] ?? "",
      title: "规则面板 Git [mock:perm]",
      refs: ["brief.txt"],
    });
    await openTask(page, task);
    await startRunUi(page);
    await expect(dialogOf(page)).toBeVisible({ timeout: TIMEOUT });
    await dialogOf(page).getByRole("button", { name: "稍后处理" }).click();
    const panel = page.locator(".run-side__panel", { hasText: "权限规则" });
    await expect(panel.locator(".run-rule", { hasText: "自动批准" })).toContainText(
      "Git 工作空间内：写入本次 worktree 分支里的任何位置",
    );
    await expect(panel.locator(".run-side__note")).toContainText("Git worktree 分支");
    // 期望：Git 工作空间可并行，不再写单执行约束（现状仍写「同一文件夹同时只运行一个执行」）
    await expect(panel.locator(".run-side__note")).not.toContainText(
      "同一文件夹同时只运行一个执行",
    );
    // 期望：岗位说明与工作日志是例外，修改需确认；读取类请求一律自动允许（现状两处都没有）
    await expect(panel).toContainText("岗位说明");
    await expect(panel).toContainText("工作日志");
    await expect(panel.locator(".run-rule", { hasText: "自动批准" })).not.toContainText(
      "读取工作目录内的文件与搜索",
    );
    await cancelActive(task);
  },
);

// ================================================================ 取消、失败与中断

test("RUN-CTL-005 取消后点「重新执行」：第 2 次执行，任务回到执行中，历次记录列出两次", async ({
  page,
}) => {
  const task = await plainTask("取消后重做 [mock:hang]");
  await openTask(page, task);
  await startRunUi(page);
  await expect(page.locator(".run-msg--agent").first()).toContainText("正在长时间处理", {
    timeout: TIMEOUT,
  });
  await page.getByRole("button", { name: "取消执行" }).click();
  const cancelled = page.locator(".run-outcome--cancelled");
  await expect(cancelled).toBeVisible({ timeout: TIMEOUT });
  await expect(statusLine(page)).toContainText("READY");
  await cancelled.getByRole("button", { name: "重新执行" }).click();
  await expect(infoStrip(page)).toContainText("第 2 次执行", { timeout: TIMEOUT });
  await expect(statusLine(page)).toContainText("执行中");
  const runs = await runsOf(task.id);
  expect(runs.map((r) => r.attempt)).toEqual([2, 1]);
  const history = page.locator(".run-history__item");
  await expect(history).toHaveCount(2);
  await expect(history.nth(0)).toContainText("第 2 次执行");
  await expect(history.nth(0)).toContainText("执行中");
  await expect(history.nth(1)).toContainText("第 1 次执行");
  await expect(history.nth(1)).toContainText("已取消");
  await cancelActive(task);
});

test("RUN-FAIL-001 Agent 拒绝执行：失败类别「拒绝执行 · 当前」、建议全文、任务受阻原因", async ({
  page,
}) => {
  const task = await plainTask("会被拒绝 [mock:refuse]");
  await openTask(page, task);
  await startRunUi(page);
  const failed = page.locator(".run-outcome--failed");
  await expect(failed).toBeVisible({ timeout: TIMEOUT });
  const run = (await runsOf(task.id))[0];
  expect(run?.status).toBe("failed");
  expect(run?.failureKind).toBe("refused");
  await expect(failed).toContainText("第 1 次执行已失败");
  await expect(
    failed.locator(".run-failure__tags .ui-tag", { hasText: "拒绝执行 · 当前" }),
  ).toBeVisible();
  await expect(failed.locator(".run-failure__text")).toHaveText(
    "Harness 拒绝了这次执行，通常是提示内容触发了它的拒绝规则。检查任务说明后重试。",
  );
  await expect(statusLine(page)).toContainText("受阻");
  await expect(page.getByText("受阻原因：执行失败：Agent 拒绝了这个任务（refusal）")).toBeVisible();
});

test("RUN-FAIL-002 Agent 进程异常退出：失败类别、退出码与 stderr、建议与三个操作按钮", async ({
  page,
}) => {
  const task = await plainTask("会崩溃 [mock:exit]");
  await openTask(page, task);
  await startRunUi(page);
  const failed = page.locator(".run-outcome--failed");
  await expect(failed).toBeVisible({ timeout: TIMEOUT });
  expect((await runsOf(task.id))[0]?.failureKind).toBe("agent_exited");
  await expect(
    failed.locator(".run-failure__tags .ui-tag", { hasText: "Agent 退出 · 当前" }),
  ).toBeVisible();
  const reason = failed.locator(".run-failure__reason");
  await expect(reason).toContainText("Agent 进程退出（退出码");
  await expect(reason).toContainText("Mock 模拟的异常退出");
  await expect(failed.locator(".run-failure__text")).toContainText("可以点「重试」");
  for (const name of ["重试", "人工接管", "换绑 Harness"]) {
    await expect(failed.getByRole("button", { name })).toBeVisible();
    await expect(failed.getByRole("button", { name })).toBeEnabled();
  }
});

test("RUN-FAIL-007 失败画面保留已改动文件，并支持重试 / 人工接管 / 换绑 Harness", async ({
  page,
}) => {
  const task = await plainTask("写完才失败 [mock:failafter]");
  await openTask(page, task);
  await startRunUi(page);
  const failed = page.locator(".run-outcome--failed");
  await expect(failed).toBeVisible({ timeout: TIMEOUT });
  const kept = failed.locator(".run-failure__kept");
  await expect(kept).toContainText("已保留的改动");
  await expect(kept).toContainText("1 个文件");
  await expect(kept).toContainText("原地执行不会自动回滚");
  await expect(kept.locator(".run-file__path")).toContainText(`Teamspace/${ROLE}/过程文件/mock-`);
  await expect(failed.locator(".run-outcome__state")).toHaveText("任务状态：已受阻，等待你处理。");

  // 换绑 Harness：进入该岗位的绑定页面
  await failed.getByRole("button", { name: "换绑 Harness" }).click();
  await expect(page).toHaveURL(/\/roles/);
  await page.goBack();
  await expect(failed).toBeVisible();

  // 重试：第 2 次执行（标记仍在，仍会失败）
  await failed.getByRole("button", { name: "重试" }).click();
  await expect(page.locator(".run-outcome--failed")).toContainText("第 2 次执行已失败", {
    timeout: TIMEOUT,
  });
  expect((await runsOf(task.id)).map((r) => r.attempt)).toEqual([2, 1]);
});

// 缺陷（新）：失败画面上的「人工接管」按钮可点，但后端只允许接管活动或已中断的执行，
// 点击确认后报错「执行已结束（失败），不能接管」，界面停在失败画面。
defect("RUN-FAIL-007 [缺陷 新] 失败画面的「人工接管」确认后应进入已接管画面", async ({ page }) => {
  const task = await plainTask("失败后接管 [mock:failafter]");
  await openTask(page, task);
  await startRunUi(page);
  const failed = page.locator(".run-outcome--failed");
  await expect(failed).toBeVisible({ timeout: TIMEOUT });
  await failed.getByRole("button", { name: "人工接管" }).click();
  await dialogOf(page).getByRole("button", { name: "确认接管" }).click();
  await expect(page.locator(".run-outcome--taken-over")).toBeVisible({ timeout: TIMEOUT });
  await expect(page.getByText("执行已结束（失败），不能接管")).toHaveCount(0);
});

test("RUN-FAIL-007 Git 工作空间：失败后改动保留在 worktree 分支里", async ({ page }) => {
  const task = await createTaskApi({
    teamId: teamG.teamId,
    roleId: teamG.roleIds[ROLE_GIT] ?? "",
    title: "Git 写完才失败 [mock:failafter]",
    refs: ["brief.txt"],
  });
  await openTask(page, task);
  await startRunUi(page);
  const failed = page.locator(".run-outcome--failed");
  await expect(failed).toBeVisible({ timeout: TIMEOUT });
  await expect(failed.locator(".run-failure__kept")).toContainText("1 个文件");
  await expect(failed.locator(".run-failure__kept")).toContainText(
    "改动保留在本次执行的 worktree 分支里，重新执行时还能接着改。",
  );
  await expect(failed.locator(".run-file__path")).toContainText(
    `Teamspace/${ROLE_GIT}/过程文件/mock-`,
  );
});

/** 制造一次中断的执行：任务带 [mock:hang] / [mock:linger] 之类的标记，开始后等它在运行，再模拟应用重启。 */
async function interrupt(task: TaskRef, ready: string): Promise<RunInfo> {
  await invoke("start_run", { taskId: task.id });
  const run = await waitLatest(task.id, (r) => r.status === "running", "执行进入运行中");
  await expect
    .poll(async () =>
      (await updatesOf(run.id)).some(
        (u) => u.body.type === "message" && (u.body.text ?? "").includes(ready),
      ),
    )
    .toBe(true);
  await invoke("__simulate_restart");
  return await waitLatest(task.id, (r) => r.status === "interrupted", "执行被标记为已中断");
}

test("RUN-RCV-002 中断面板：中断时间、最后事件、已有改动取值", async ({ page }) => {
  const task = await plainTask("中断面板事实 [mock:linger]");
  const run = await interrupt(task, "步骤已处理完");
  expect(run.status).toBe("interrupted");
  await openTask(page, task);
  const panel = page.locator(".run-outcome--interrupted");
  await expect(panel).toBeVisible({ timeout: TIMEOUT });
  const info = panel.locator(".run-outcome__info");
  await expect(info.locator("div", { hasText: "中断时间" }).locator("dd")).toHaveText(
    /^\d{4}\/\d{1,2}\/\d{1,2} \d{2}:\d{2}:\d{2}$/,
  );
  await expect(info.locator("div", { hasText: "最后事件" }).locator("dd")).toHaveText(
    "Agent 消息：步骤已处理完，等待收尾……",
  );
  await expect(info.locator("div", { hasText: "已有改动" }).locator("dd")).toContainText(
    "个文件 · 原地：普通文件夹",
  );
  await invoke("mark_run_failed", { runId: run.id });
});

// 缺陷（新）：执行的改动文件只在正常收尾时统计，应用重启后中断的执行 changedFiles 为空，
// 中断面板「已有改动」显示「0 个文件」，即使 Agent 已经写入了过程文件。
defect("RUN-RCV-002 [缺陷 新] 中断面板「已有改动」应包含中断前已写入的文件", async ({ page }) => {
  const task = await plainTask("中断前已改文件 [mock:linger]");
  const run = await interrupt(task, "步骤已处理完");
  expect(run.changedFiles).toHaveLength(1);
  await openTask(page, task);
  const panel = page.locator(".run-outcome--interrupted");
  await expect(
    panel.locator(".run-outcome__info div", { hasText: "已有改动" }).locator("dd"),
  ).toHaveText("1 个文件 · 原地：普通文件夹");
  await invoke("mark_run_failed", { runId: run.id });
});

test("RUN-RCV-003 支持 loadSession：续接能力、「续接会话」为主按钮、图例说明", async ({ page }) => {
  const task = await plainTask("可续接 [mock:hang]");
  const run = await interrupt(task, "正在长时间处理");
  expect(run.canResume).toBe(true);
  await openTask(page, task);
  const panel = page.locator(".run-outcome--interrupted");
  await expect(panel).toBeVisible({ timeout: TIMEOUT });
  await expect(
    panel.locator(".run-outcome__info div", { hasText: "续接能力" }).locator("dd"),
  ).toHaveText(`${HARNESS} · 支持续接（loadSession）`);
  const resume = panel.getByRole("button", { name: "续接会话" });
  await expect(resume).toBeEnabled();
  await expect(resume).toHaveClass(/ui-button--primary/);
  await expect(panel.getByRole("button", { name: "重新执行" })).not.toHaveClass(
    /ui-button--primary/,
  );
  await expect(panel.locator(".run-outcome__legend")).toContainText(
    "「续接会话」：支持续接（loadSession），接着上次的上下文继续，不新建执行记录。",
  );
  await invoke("mark_run_failed", { runId: run.id });
});

test("RUN-RCV-005 不支持续接：「续接会话」置灰并说明原因，「重新执行」升为主按钮", async ({
  page,
}) => {
  const harness = await invoke<{ id: string }>("create_harness", {
    input: {
      teamId: teamP.teamId,
      name: "r2-no-load",
      preset: "custom",
      command: process.execPath,
      args: [fileURLToPath(new URL("./fixtures/r2-no-load-agent.mjs", import.meta.url))],
    },
  });
  const checked = await invoke<{ handshake: { status: string } }>("check_harness", {
    harnessId: harness.id,
  });
  expect(checked.handshake.status).toBe("connected");
  await bind(teamP.roleIds["不续接员"] ?? "", harness.id);
  const task = await plainTask("不可续接", { role: "不续接员" });
  const run = await interrupt(task, "收到，正在长时间处理");
  expect(run.canResume).toBe(false);
  expect(run.sessionId).not.toBeNull();
  await openTask(page, task);
  const panel = page.locator(".run-outcome--interrupted");
  await expect(panel).toBeVisible({ timeout: TIMEOUT });
  const resume = panel.getByRole("button", { name: "续接会话" });
  await expect(resume).toBeDisabled();
  await expect(resume).toHaveAttribute("title", "r2-no-load 不支持续接会话（loadSession）");
  await expect(panel.getByRole("button", { name: "重新执行" })).toHaveClass(/ui-button--primary/);
  await expect(
    panel.locator(".run-outcome__info div", { hasText: "续接能力" }).locator("dd"),
  ).toContainText("不支持续接");
  await invoke("mark_run_failed", { runId: run.id });
});

// ================================================================ 工作日志与工作台

test("RUN-LOG-002 成功执行后岗位工作日志新增「开始」「完成」「状态」条目，不含面板拼出的显示文字", async ({
  page,
}) => {
  const before = journal(plain, ROLE);
  const task = await plainTask("日志核对");
  await openTask(page, task);
  await startRunUi(page);
  await expect(page.locator(".run-outcome--success")).toBeVisible({ timeout: TIMEOUT });
  // 日志在执行结束后异步同步，等「完成」条目出现再核对
  await expect
    .poll(() => journal(plain, ROLE), { timeout: TIMEOUT })
    .toContain(`${task.key} · 第 1 次执行完成`);
  const after = journal(plain, ROLE);
  expect(after.startsWith(before)).toBe(true);
  const added = after
    .slice(before.length)
    .split("\n")
    .filter((line) => line.trim() !== "");
  expect(added.length).toBeGreaterThanOrEqual(3);
  for (const line of added) {
    expect(line.startsWith("- ")).toBe(true);
  }
  const mine = added.filter((line) => line.includes(task.key));
  expect(
    mine.some((l) =>
      l.includes(`· ${task.key} · 开始第 1 次执行（Harness「${HARNESS}」，原地执行）`),
    ),
  ).toBe(true);
  expect(
    mine.some(
      (l) =>
        l.includes(
          `· ${task.key} · 第 1 次执行完成（${HARNESS}）；摘要：已完成：在 Teamspace/${ROLE}/过程文件/`,
        ) && l.includes("；改动文件：Teamspace/"),
    ),
  ).toBe(true);
  expect(mine.some((l) => l.includes(`· ${task.key} · 状态 执行中 → 待验收（执行完成）`))).toBe(
    true,
  );
  for (const line of added) {
    expect(line).not.toContain("总时长");
    expect(line).not.toContain("执行摘要：");
    expect(line).not.toContain(`完成「${task.key}」`);
  }
});

test("RUN-LOG-005 日志写入失败：执行本身成功，工作台提示无法写入，恢复后补写开始与完成条目", async ({
  page,
}) => {
  const file = join(plain, "Teamspace", "日志员", "工作日志.md");
  const task = await plainTask("日志写不进去", { role: "日志员" });
  chmodSync(file, 0o444);
  try {
    await openTask(page, task);
    await startRunUi(page);
    await expect(page.locator(".run-outcome--success")).toBeVisible({ timeout: TIMEOUT });
    await expect(statusLine(page)).toContainText("待验收");
    expect(readFileSync(file, "utf8")).not.toContain("开始第 1 次执行");
    expect(readFileSync(file, "utf8")).not.toContain("执行完成");

    await page.goto(`/#/teams/${teamP.teamId}/roles/${teamP.roleIds["日志员"] ?? ""}/workbench`);
    const warning = page.locator(".ui-card", { hasText: "岗位文件夹无法写入" });
    await expect(warning).toBeVisible();
    await expect(warning).toContainText("无法写入工作日志");
  } finally {
    chmodSync(file, 0o644);
  }
  // 恢复权限后下一次同步补写
  await invoke("get_role_workbench", { roleId: teamP.roleIds["日志员"] ?? "" });
  await expect
    .poll(() => readFileSync(file, "utf8"), { timeout: TIMEOUT })
    .toContain(`${task.key} · 第 1 次执行完成（${HARNESS}）`);
  const log = readFileSync(file, "utf8");
  expect(log).toContain(`${task.key} · 开始第 1 次执行`);
});

// 缺陷 N-01：成功面板无条件写「已写入「岗位」工作日志」，日志写入失败时也一样。
defect(
  "RUN-LOG-005 [缺陷 N-01] 日志写入失败时成功面板不得显示「已写入工作日志」",
  async ({ page }) => {
    const file = join(plain, "Teamspace", "日志员", "工作日志.md");
    const task = await plainTask("日志写不进去 面板", { role: "日志员" });
    chmodSync(file, 0o444);
    try {
      await openTask(page, task);
      await startRunUi(page);
      const success = page.locator(".run-outcome--success");
      await expect(success).toBeVisible({ timeout: TIMEOUT });
      await expect(success).not.toContainText("已写入「日志员」工作日志");
    } finally {
      chmodSync(file, 0o644);
      await invoke("get_role_workbench", { roleId: teamP.roleIds["日志员"] ?? "" });
    }
  },
);

function workbenchUrl(team: TeamSetup, role: string): string {
  return `/#/teams/${team.teamId}/roles/${team.roleIds[role] ?? ""}/workbench`;
}

test("RUN-WRK-001 工作台「最近执行」：次数、Harness、状态胶囊与相对时间", async ({ page }) => {
  const done = await plainTask("工作台 已完成");
  await runToEnd(done);
  const running = await plainTask("工作台 执行中 [mock:hang]");
  await invoke("start_run", { taskId: running.id });
  await waitLatest(running.id, (r) => r.status === "running", "进入执行中");

  await page.goto(workbenchUrl(teamP, ROLE));
  const finished = page.locator(".rw-run-row", { hasText: done.key });
  await expect(finished.locator(".rw-run-row__key")).toHaveText(`第 1 次 · ${done.key}`);
  await expect(finished.locator(".rw-run-row__title")).toHaveText(HARNESS);
  await expect(finished.locator(".ui-tag--success", { hasText: "已完成" })).toBeVisible();
  await expect(finished.locator(".rw-run-row__time")).toHaveText(/^(刚刚|\d+ 分钟前)$/);

  const active = page.locator(".rw-run-row", { hasText: running.key });
  await expect(active.locator(".ui-tag--info", { hasText: "执行中" })).toBeVisible();
  await cancelActive(running);
});

// 缺陷 Q-23：人工接管后数据库状态是 cancelled，工作台「最近执行」直接显示后端的「已取消」，
// 与任务详情页的「已被接管」不一致。
defect("RUN-WRK-001 [缺陷 Q-23] 被人工接管的执行在工作台显示「已被接管」", async ({ page }) => {
  const task = await plainTask("工作台 接管 [mock:hang]");
  await invoke("start_run", { taskId: task.id });
  const run = await waitLatest(task.id, (r) => r.status === "running", "进入执行中");
  await invoke("takeover_run", { runId: run.id });
  await page.goto(workbenchUrl(teamP, ROLE));
  const row = page.locator(".rw-run-row", { hasText: task.key });
  await expect(row.locator(".ui-tag", { hasText: "已被接管" })).toBeVisible();
});

test("RUN-WRK-002 点击工作台最近执行行：跳转到任务详情页并显示该次执行", async ({ page }) => {
  const task = await plainTask("工作台 跳转");
  await runToEnd(task);
  await page.goto(workbenchUrl(teamP, ROLE));
  await page.locator(".rw-run-row", { hasText: task.key }).click();
  await expect(page).toHaveURL(new RegExp(`/teams/${teamP.teamId}/tasks/${task.id}$`));
  await expect(page.getByRole("heading", { level: 1, name: task.title })).toBeVisible();
  await expect(infoStrip(page)).toContainText("第 1 次执行");
  await expect(page.locator(".run-outcome--success")).toBeVisible();
});

test("RUN-WRK-004 执行结束后工作台日志预览联动出现新条目，「打开工作日志」可用", async ({
  page,
}) => {
  const task = await plainTask("工作台 日志联动");
  await page.goto(workbenchUrl(teamP, ROLE));
  await expect(page.getByRole("heading", { level: 1, name: ROLE })).toBeVisible();
  await expect(page.locator(".rw-journal-text", { hasText: task.key })).toHaveCount(1);
  await invoke("start_run", { taskId: task.id });
  await waitLatest(task.id, (r) => r.status === "succeeded", "执行成功");
  // 不刷新页面：预览随执行结束联动出现完成条目
  await expect(
    page.locator(".rw-journal-text", { hasText: task.key }).filter({ hasText: "第 1 次执行完成" }),
  ).toBeVisible();
  const reply = page.waitForResponse(
    (response) =>
      response.url().endsWith("/invoke") &&
      (response.request().postData() ?? "").includes("open_role_path"),
  );
  await page.getByRole("button", { name: "打开工作日志.md" }).click();
  const body = (await (await reply).json()) as { ok?: unknown; err?: unknown };
  expect(body.err).toBeUndefined();
});

// ================================================================ 人工接管

async function startHang(task: TaskRef): Promise<RunInfo> {
  await invoke("start_run", { taskId: task.id });
  return await waitLatest(task.id, (r) => r.status === "running", "进入执行中");
}

test("RUN-TKO-002 确认接管：Agent 停止，Run 记 taken_over 与「人工接管」，任务保持执行中", async ({
  page,
}) => {
  const task = await plainTask("接管 状态 [mock:hang]");
  await openTask(page, task);
  await startRunUi(page);
  await expect(page.locator(".run-msg--agent").first()).toContainText("正在长时间处理", {
    timeout: TIMEOUT,
  });
  await page.getByRole("button", { name: "人工接管", exact: true }).click();
  await dialogOf(page).getByRole("button", { name: "确认接管" }).click();
  await expect(page.locator(".run-outcome--taken-over")).toBeVisible({ timeout: TIMEOUT });
  const run = (await runsOf(task.id))[0];
  expect(run?.takenOver).toBe(true);
  expect(run?.failure).toBe("人工接管");
  expect(run?.status).toBe("cancelled");
  const detail = await invoke<{ task: { status: string }; owner: { id: string } }>("get_task", {
    taskId: task.id,
  });
  expect(detail.task.status).toBe("doing");
  expect(detail.owner.id).toBe(teamP.roleIds[ROLE]);
  // Agent 进程已停止：没有新的进度，文件夹已释放（另一个任务可以开始）
  const count = (await updatesOf(run?.id ?? "")).length;
  await page.waitForTimeout(800);
  expect((await updatesOf(run?.id ?? "")).length).toBe(count);
  const other = await plainTask("接管后释放文件夹 [mock:hang]");
  await startHang(other);
  await cancelActive(other);
});

test("RUN-TKO-003 已接管画面：标题、人工接管标签、执行目录与说明（普通文件夹 / Git）", async ({
  page,
}) => {
  const plainTaskRef = await plainTask("接管 画面 [mock:hang]");
  await startHang(plainTaskRef);
  await invoke("takeover_run", { runId: (await runsOf(plainTaskRef.id))[0]?.id });
  await openTask(page, plainTaskRef);
  const taken = page.locator(".run-outcome--taken-over");
  await expect(taken.locator(".run-outcome__title")).toHaveText("已由你接管");
  await expect(taken.locator(".run-outcome__head .ui-tag--info")).toHaveText("人工接管");
  await expect(taken.locator(".run-execdir__path")).toHaveText(plain);
  await expect(taken).toContainText("原地执行，这个目录就是工作空间本身。");
  await expect(infoStrip(page)).toContainText("已被接管");

  const gitTask = await createTaskApi({
    teamId: teamG.teamId,
    roleId: teamG.roleIds[ROLE_GIT] ?? "",
    title: "Git 接管画面 [mock:hang]",
    refs: ["brief.txt"],
  });
  const run = await startHang(gitTask);
  await invoke("takeover_run", { runId: run.id });
  await openTask(page, gitTask);
  const gitTaken = page.locator(".run-outcome--taken-over");
  await expect(gitTaken.locator(".run-execdir__path")).toHaveText(run.execDir);
  await expect(gitTaken).toContainText(
    `Git worktree 分支 teamspace/${gitTask.key}-r1，改动都在这里。`,
  );
  const reply = page.waitForResponse(
    (response) =>
      response.url().endsWith("/invoke") &&
      (response.request().postData() ?? "").includes("open_run_dir"),
  );
  await gitTaken.getByRole("button", { name: "在 Finder 中打开" }).click();
  const body = (await (await reply).json()) as { ok?: unknown; err?: unknown };
  expect(body.err).toBeUndefined();
});

test("RUN-TKO-004 「结束接管并重新执行」：普通文件夹保留接管期间的改动，第 2 次执行开始", async ({
  page,
}) => {
  const task = await plainTask("接管后重做 原地 [mock:hang]");
  const first = await startHang(task);
  await invoke("takeover_run", { runId: first.id });
  writeFileSync(join(plain, "takeover-note.txt"), "人工处理\n");
  await openTask(page, task);
  await page
    .locator(".run-outcome--taken-over")
    .getByRole("button", { name: "结束接管并重新执行" })
    .click();
  await expect(infoStrip(page)).toContainText("第 2 次执行", { timeout: TIMEOUT });
  const runs = await runsOf(task.id);
  expect(runs.map((r) => r.attempt)).toEqual([2, 1]);
  expect(["queued", "running"]).toContain(runs[0]?.status);
  await expect(statusLine(page)).toContainText("执行中");
  await expect(page.locator(".run-history__item")).toHaveCount(2);
  expect(readFileSync(join(plain, "takeover-note.txt"), "utf8")).toBe("人工处理\n");
  await cancelActive(task);
  rmSync(join(plain, "takeover-note.txt"), { force: true });
});

test("RUN-TKO-004 「结束接管并重新执行」：Git 新分支新 worktree，不带入上次 worktree 里的人工改动", async ({
  page,
}) => {
  const task = await createTaskApi({
    teamId: teamG.teamId,
    roleId: teamG.roleIds[ROLE_GIT] ?? "",
    title: "Git 接管后重做 [mock:hang]",
    refs: ["brief.txt"],
  });
  const first = await startHang(task);
  await invoke("takeover_run", { runId: first.id });
  writeFileSync(join(first.execDir, "manual.txt"), "人工改动\n");
  await openTask(page, task);
  await page
    .locator(".run-outcome--taken-over")
    .getByRole("button", { name: "结束接管并重新执行" })
    .click();
  await expect(infoStrip(page)).toContainText("第 2 次执行", { timeout: TIMEOUT });
  const second = await waitLatest(
    task.id,
    (r) => r.attempt === 2 && r.status === "running",
    "第 2 次运行",
  );
  expect(second.branch).toBe(`teamspace/${task.key}-r2`);
  expect(second.execDir).not.toBe(first.execDir);
  expect(second.baseCommit).toBe(git(gitWs, "rev-parse", "HEAD").trim());
  expect(existsSync(join(second.execDir, "manual.txt"))).toBe(false);
  // 旧分支与旧目录仍保留
  expect(existsSync(join(first.execDir, "manual.txt"))).toBe(true);
  expect(git(gitWs, "branch", "--list", `teamspace/${task.key}-r1`)).toContain(
    `teamspace/${task.key}-r1`,
  );
  await cancelActive(task);
});

test("RUN-TKO-006 Git 接管后 worktree 与分支完好，可在其中提交且不影响主分支", async ({ page }) => {
  const task = await createTaskApi({
    teamId: teamG.teamId,
    roleId: teamG.roleIds[ROLE_GIT] ?? "",
    title: "Git 接管后提交 [mock:hang]",
    refs: ["brief.txt"],
  });
  const run = await startHang(task);
  await invoke("takeover_run", { runId: run.id });
  const branch = `teamspace/${task.key}-r1`;
  const head = git(gitWs, "rev-parse", "main").trim();
  const listing = git(gitWs, "worktree", "list", "--porcelain");
  expect(listing).toContain(`branch refs/heads/${branch}`);
  expect(listing).toContain(run.execDir);

  writeFileSync(join(run.execDir, "manual.txt"), "人工提交\n");
  git(run.execDir, "add", "manual.txt");
  git(run.execDir, "-c", "commit.gpgsign=false", "commit", "-q", "-m", "docs: manual change");
  expect(git(gitWs, "log", "-1", "--format=%s", branch).trim()).toBe("docs: manual change");
  expect(git(gitWs, "rev-parse", "main").trim()).toBe(head);
  expect(git(gitWs, "rev-parse", "--abbrev-ref", "HEAD").trim()).toBe("main");
  await openTask(page, task);
  await expect(page.locator(".run-outcome--taken-over")).toContainText(branch);
});

// ================================================================ 其他

test("RUN-PKG-006 「去验收」只滚动到状态操作区，路由不变，控制台无错误", async ({ page }) => {
  const errors: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error") {
      errors.push(message.text());
    }
  });
  page.on("pageerror", (error) => {
    errors.push(error.message);
  });
  const task = await plainTask("去验收入口");
  await openTask(page, task);
  await startRunUi(page);
  const success = page.locator(".run-outcome--success");
  await expect(success).toBeVisible({ timeout: TIMEOUT });
  const url = page.url();
  await success.getByRole("button", { name: "去验收" }).click();
  const target = page.locator("#task-status-actions");
  await expect(target).toBeInViewport();
  expect(page.url()).toBe(url);
  await expect(target.locator("h2")).toHaveText("状态操作");
  await expect(target.getByRole("button", { name: "已完成" })).toBeVisible();
  expect(errors).toEqual([]);
});

test("RUN-ACP-003 内置预设的命令与参数预填且可编辑，保存后以修改值为准", async ({ page }) => {
  const presets =
    await invoke<{ preset: string; defaultCommand: string | null; defaultArgs: string[] }[]>(
      "list_acp_presets",
    );
  const claude = presets.find((p) => p.preset === "claude_code");
  expect(claude?.defaultCommand).toBe("claude-code-acp");
  await page.goto(`/#/teams/${teamP.teamId}/roles?tab=harness`);
  await page.getByRole("button", { name: "新建接入" }).click();
  await page.locator(".acp-preset", { hasText: "Claude Code" }).click();
  const command = page.getByLabel("启动命令");
  const args = page.getByLabel("参数", { exact: true });
  await expect(command).toHaveValue("claude-code-acp");
  await expect(args).toHaveValue((claude?.defaultArgs ?? []).join(" "));
  await expect(command).toBeEditable();
  await expect(args).toBeEditable();
  await expect(
    page.locator(".ui-field__hint", { hasText: "已按预设预填，可改成绝对路径。" }),
  ).toBeVisible();

  await command.fill("/opt/bin/claude-code-acp");
  await args.fill("--verbose");
  await page.getByLabel("实例名称").fill("r2-claude-edited");
  await page.getByRole("button", { name: "保存并握手检查" }).click();
  const entry = page.locator(".acp-entry", { hasText: "r2-claude-edited" });
  await expect(entry).toBeVisible();
  const list = await invoke<{ name: string; command: string; args: string[]; preset: string }[]>(
    "list_harnesses",
    { teamId: teamP.teamId },
  );
  const saved = list.find((h) => h.name === "r2-claude-edited");
  expect(saved?.preset).toBe("claude_code");
  expect(saved?.command).toBe("/opt/bin/claude-code-acp");
  expect(saved?.args).toEqual(["--verbose"]);

  await entry.getByRole("button", { name: "编辑" }).click();
  await expect(page.getByLabel("预设")).toBeDisabled();
  await expect(page.locator(".ui-field__hint", { hasText: "创建后不可修改。" })).toBeVisible();
  await expect(page.getByLabel("启动命令")).toHaveValue("/opt/bin/claude-code-acp");
  await expect(page.getByLabel("启动命令")).toBeEditable();
  await expect(page.getByLabel("参数", { exact: true })).toHaveValue("--verbose");
  await expect(page.getByLabel("参数", { exact: true })).toBeEditable();
});
