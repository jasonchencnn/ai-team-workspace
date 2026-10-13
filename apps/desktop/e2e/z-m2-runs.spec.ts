// M2 端到端用例：派工执行。真实界面 + 测试桥接 + 真实内核；Harness 是内置 Mock ACP Agent
// （任务标题里的 [mock:perm] / [mock:fail] / [mock:auth] / [mock:hang] 标记决定它的行为）。
// 使用两个独立的临时工作空间：一个普通文件夹（原地执行）、一个独立的 Git 仓库（worktree 隔离）。
// 「已中断」状态用桥接的测试专用命令 `__simulate_restart`（等同应用退出时的收尾）制造。
// 文件名 z- 开头，排在其他用例之后；关键状态截图保存到 e2e-artifacts/m2-xx-*.png。
import { execFileSync } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { type Locator, type Page, expect, test } from "@playwright/test";

import { acpEntry, createMockHarness } from "./harnessHelpers.js";
import { BRIDGE_PORT } from "./ports.js";

const artifacts = fileURLToPath(new URL("../e2e-artifacts/", import.meta.url));
mkdirSync(artifacts, { recursive: true });

let shotNo = 0;
async function shot(page: Page, name: string): Promise<void> {
  shotNo += 1;
  await page.screenshot({
    path: `${artifacts}m2-${String(shotNo).padStart(2, "0")}-${name}.png`,
    fullPage: false,
  });
}

/** 直接调用桥接命令（读取状态、模拟重启）。 */
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
  execDir: string;
  isolation: string;
  sessionId: string | null;
  failure: string | null;
  summary: string | null;
}

interface UpdateInfo {
  body: {
    type: string;
    decision?: { kind: string; by: string; optionId: string | null } | null;
    command?: string | null;
  };
}

const TEAM_P = "派工普通团队";
const TEAM_G = "派工Git团队";
const ROLE = "执行员";
const ROLE_FREE = "未绑定员";
const ROLE_GIT = "分支员";
const HARNESS = "m2-mock";
const HARNESS_GIT = "m2-mock-git";
const TIMEOUT = 25_000;

let plain = "";
let gitWs = "";
let teamP = "";
let teamG = "";

interface TaskRef {
  url: string;
  id: string;
  key: string;
  title: string;
}
const tasks = new Map<string, TaskRef>();

test.describe.configure({ mode: "serial", timeout: 90_000 });

function git(cwd: string, ...args: string[]): string {
  return execFileSync(
    "git",
    ["-c", "user.name=E2E", "-c", "user.email=e2e@example.invalid", ...args],
    { cwd, encoding: "utf8" },
  );
}

/** 递归列出目录下的所有文件（相对路径）。 */
function walk(root: string, base = root): string[] {
  const found: string[] = [];
  for (const entry of readdirSync(root, { withFileTypes: true })) {
    const full = join(root, entry.name);
    if (entry.name === ".git") {
      continue;
    }
    if (entry.isDirectory()) {
      found.push(...walk(full, base));
    } else {
      found.push(full.slice(base.length + 1));
    }
  }
  return found;
}

test.beforeAll(() => {
  plain = mkdtempSync(join(tmpdir(), "teamspace-e2e-m2-plain-"));
  writeFileSync(join(plain, "brief.txt"), "需求说明\n");
  gitWs = mkdtempSync(join(tmpdir(), "teamspace-e2e-m2-git-"));
  git(gitWs, "init", "-q", "-b", "main");
  writeFileSync(join(gitWs, "README.md"), "# 派工 Git 项目\n");
  writeFileSync(join(gitWs, "brief.txt"), "需求说明\n");
  git(gitWs, "add", ".");
  git(gitWs, "-c", "commit.gpgsign=false", "commit", "-q", "-m", "docs: seed");
});

test.afterAll(() => {
  rmSync(plain, { recursive: true, force: true });
  rmSync(gitWs, { recursive: true, force: true });
});

// ---------------------------------------------------------------- 辅助

async function addRole(page: Page, name: string, description: string): Promise<void> {
  await page.getByRole("button", { name: "+ 添加岗位" }).click();
  const card = page.locator(".tc-role").last();
  await card.getByLabel("岗位名称").fill(name);
  await card.getByLabel("岗位名称").blur();
  await expect(card.getByLabel("任务编号前缀")).toHaveValue(/^[A-Z][A-Z0-9]{1,3}$/);
  await card.getByLabel("职责").fill(description);
}

async function createTeam(
  page: Page,
  name: string,
  workspace: string,
  roles: string[],
): Promise<string> {
  await page.goto("/#/teams/new");
  await page.getByLabel("团队名称").fill(name);
  await page.getByLabel("手动输入工作空间路径").fill(workspace);
  await page.getByRole("button", { name: "检测", exact: true }).click();
  await expect(page.getByText(/普通文件夹|Git 仓库/).first()).toBeVisible();
  for (const role of roles) {
    await addRole(page, role, `${role}的职责`);
  }
  await page.getByRole("button", { name: "创建团队", exact: true }).click();
  await expect(page).toHaveURL(/\/teams\/(?!new$)[^/]+$/);
  const id = /\/teams\/([^/?]+)/.exec(page.url())?.[1] ?? "";
  expect(id).not.toBe("");
  return id;
}

async function bindRole(page: Page, team: string, role: string, harness: string): Promise<void> {
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

/** 创建任务（引用 brief.txt）并记录详情页地址与编号。 */
async function createTask(
  page: Page,
  team: string,
  title: string,
  owner: string,
  status: "TODO" | "READY",
): Promise<TaskRef> {
  await page.goto(`/#/teams/${team}/tasks/new`);
  await page.getByLabel("任务名称").fill(title);
  const select = page.getByLabel("负责人岗位");
  await select.selectOption(
    (await select.locator("option", { hasText: owner }).getAttribute("value")) ?? "",
  );
  await page.locator("#task-file-search").fill("brief");
  await page.locator(".task-form__suggestion", { hasText: "brief.txt" }).click();
  await page.getByRole("radio", { name: status, exact: true }).check();
  await page.getByRole("button", { name: "创建并分配" }).click();
  await expect(page.locator(".task-detail__status")).toContainText(status);
  const key = /\/ (\S+)$/.exec(
    (await page.locator(".task-detail__status").innerText()).trim(),
  )?.[1];
  expect(key).toBeTruthy();
  const url = page.url();
  const ref = { url, id: url.split("/").pop() ?? "", key: key ?? "", title };
  tasks.set(title, ref);
  return ref;
}

async function openTask(page: Page, title: string): Promise<TaskRef> {
  const ref = tasks.get(title);
  if (ref === undefined) {
    throw new Error(`未创建任务：${title}`);
  }
  await page.goto(ref.url);
  await expect(page.getByRole("heading", { level: 1, name: title })).toBeVisible();
  return ref;
}

const startButton = (page: Page): Locator =>
  page.getByRole("button", { name: /^(开始执行|重新执行)$/ });
const statusLine = (page: Page): Locator => page.locator(".task-detail__status");
const infoStrip = (page: Page): Locator => page.locator(".run-info");

async function startRun(page: Page): Promise<void> {
  await startButton(page).first().click();
  await expect(infoStrip(page)).toBeVisible();
}

async function runsOf(taskId: string): Promise<RunInfo[]> {
  return await invoke<RunInfo[]>("list_runs", { taskId });
}

async function waitRun(
  taskId: string,
  predicate: (runs: RunInfo[]) => boolean,
  what: string,
): Promise<RunInfo[]> {
  await expect
    .poll(async () => predicate(await runsOf(taskId)), { timeout: TIMEOUT, message: what })
    .toBe(true);
  return await runsOf(taskId);
}

async function updatesOf(runId: string): Promise<UpdateInfo[]> {
  const page = await invoke<{ updates: UpdateInfo[] }>("list_run_updates", {
    input: { runId, limit: 500 },
  });
  return page.updates;
}

function processFiles(root: string, role: string): string[] {
  const dir = join(root, "Teamspace", role, "过程文件");
  return existsSync(dir) ? readdirSync(dir).filter((name) => name.startsWith("mock-")) : [];
}

function journal(root: string, role: string): string {
  return readFileSync(join(root, "Teamspace", role, "工作日志.md"), "utf8");
}

// ---------------------------------------------------------------- 准备

test("准备：两个团队（普通文件夹 / Git）、Mock 接入与岗位绑定", async ({ page }) => {
  teamP = await createTeam(page, TEAM_P, plain, [ROLE, ROLE_FREE]);
  await page.goto(`/#/teams/${teamP}/roles?tab=harness`);
  await createMockHarness(page, HARNESS);
  await bindRole(page, teamP, ROLE, HARNESS);

  teamG = await createTeam(page, TEAM_G, gitWs, [ROLE_GIT]);
  await page.goto(`/#/teams/${teamG}/roles?tab=harness`);
  await createMockHarness(page, HARNESS_GIT);
  await bindRole(page, teamG, ROLE_GIT, HARNESS_GIT);
});

// ---------------------------------------------------------------- 普通文件夹

test("普通文件夹成功流程：原地执行，待验收，摘要与改动文件，过程文件与工作日志", async ({
  page,
}) => {
  const title = "整理素材清单";
  const ref = await createTask(page, teamP, title, ROLE, "READY");
  await expect(page.getByText("准备就绪后即可派给")).toBeVisible();
  await expect(startButton(page)).toBeEnabled();
  await shot(page, "task-ready-start");

  await startRun(page);
  await expect(infoStrip(page)).toContainText("原地：普通文件夹");
  await expect(infoStrip(page)).toContainText("第 1 次执行");
  await expect(infoStrip(page)).toContainText(plain);

  const success = page.locator(".run-outcome--success");
  await expect(success).toBeVisible({ timeout: TIMEOUT });
  await expect(success).toContainText("执行完成");
  await expect(success).toContainText("已完成 · 待验收");
  await expect(statusLine(page)).toContainText("待验收");
  await expect(success.locator(".run-outcome__summary")).toContainText(
    `已完成：在 Teamspace/${ROLE}/过程文件/ 写入了 mock-`,
  );
  await expect(success.locator(".run-outcome__facts")).toContainText("原地：普通文件夹");
  await expect(success.locator(".run-outcome__facts")).toContainText("工具调用");

  // 改动文件：右栏常驻，成功区点「查看改动」展开
  const side = page.locator(".run-side__panel", { hasText: "本次改动文件" });
  await expect(side).toContainText("1 个");
  await expect(side).toContainText(`Teamspace/${ROLE}/过程文件/mock-`);
  await success.getByRole("button", { name: "查看改动" }).click();
  await expect(success.locator(".run-outcome__files")).toContainText("mock-");
  await expect(success.locator(".run-outcome__files")).toContainText("原地执行直接改在工作空间里");
  await expect(page.locator(".run-side__panel", { hasText: "历次执行记录" })).toContainText("1 次");
  await shot(page, "run-succeeded");

  // 磁盘：Mock 写入的过程文件在岗位过程文件夹里
  const files = processFiles(plain, ROLE);
  expect(files).toHaveLength(1);
  expect(
    readFileSync(join(plain, "Teamspace", ROLE, "过程文件", files[0] ?? ""), "utf8"),
  ).toContain("# Mock 工作记录");

  // 工作日志：文件与工作台都出现这次执行的摘要
  const log = journal(plain, ROLE);
  expect(log).toContain(ref.key);
  expect(log).toContain("写入了 mock-");
  await page.goto(`/#/teams/${teamP}`);
  await page.locator(".th-role", { hasText: ROLE }).getByRole("link", { name: ROLE }).click();
  await expect(page.getByRole("heading", { level: 1, name: ROLE })).toBeVisible();
  await expect(
    page.locator(".rw-journal-text", { hasText: ref.key }).filter({ hasText: "完成" }).first(),
  ).toBeVisible();
  const recent = page.locator(".rw-run-row", { hasText: ref.key });
  await expect(recent).toContainText("第 1 次");
  await expect(recent).toContainText(HARNESS);
  await expect(recent).toContainText("已完成");
  await shot(page, "workbench-recent-run-and-journal");
});

test("[mock:perm]：权限弹窗自动出现，进度流有消息/计划/工具调用，允许一次后完成", async ({
  page,
}) => {
  const title = "运行脚本 [mock:perm] 允许";
  const ref = await createTask(page, teamP, title, ROLE, "READY");
  await startRun(page);

  // 弹窗自动出现
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible({ timeout: TIMEOUT });
  await expect(dialog).toContainText(`${ROLE} 请求权限`);
  await expect(dialog).toContainText("等待你确认");
  await expect(dialog.locator(".perm-request__code")).toHaveText("echo hello");
  await expect(dialog.locator(".perm-reason__title")).toHaveText("为什么需要你确认");
  await expect(dialog.locator(".perm-reason__text")).toContainText("执行命令需要确认");
  await expect(dialog.getByRole("button", { name: "允许一次" })).toBeVisible();
  await expect(dialog.getByRole("button", { name: "拒绝" })).toBeVisible();
  await shot(page, "permission-dialog");

  // 进度流（弹窗后面）：消息、计划、工具调用
  const stream = page.locator(".run-stream");
  await expect(stream.locator(".run-msg--agent").first()).toContainText("收到任务");
  await expect(stream.locator(".run-plan")).toContainText("读取任务说明与引用文件");
  await expect(stream.locator(".run-tool", { hasText: "读取引用文件" })).toBeVisible();
  await expect(stream.locator(".run-tool", { hasText: "写入过程文件" })).toBeVisible();
  await expect(stream.locator(".run-permission", { hasText: "运行 echo hello" })).toContainText(
    "等待你确认",
  );
  // 过程文件夹的编辑权限按规则自动批准，不打扰 Human
  await expect(stream.locator(".run-filewrite")).toContainText("已写入");

  // 先关掉：执行区提示、任务面板小标、重新打开
  await dialog.getByRole("button", { name: "稍后处理" }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.locator(".run-pending")).toContainText("有一个权限请求在等你确认");
  await expect(statusLine(page)).toContainText("执行中");
  await stream.locator(".run-permission").last().scrollIntoViewIfNeeded();
  await shot(page, "run-waiting-permission-closed");
  await page.goto(`/#/teams/${teamP}/tasks`);
  await expect(
    page.locator("a.task-card-link", { hasText: title }).locator(".ui-tag", {
      hasText: "等待你确认",
    }),
  ).toBeVisible();
  await shot(page, "board-waiting-badge");

  await openTask(page, title);
  const reopened = page.getByRole("dialog");
  await expect(reopened).toBeVisible();
  await reopened.getByRole("button", { name: "允许一次" }).click();

  const success = page.locator(".run-outcome--success");
  await expect(success).toBeVisible({ timeout: TIMEOUT });
  await expect(success.locator(".run-outcome__summary")).toContainText("命令 echo hello 已执行");
  await expect(statusLine(page)).toContainText("待验收");

  // 记录里有人工允许的决定
  const run = (await runsOf(ref.id))[0];
  const decisions = (await updatesOf(run?.id ?? ""))
    .map((u) => u.body)
    .filter((b) => b.type === "permission" && b.command === "echo hello");
  expect(decisions[0]?.decision?.kind).toBe("allow_once");
  expect(decisions[0]?.decision?.by).toBe("human");
});

test("[mock:perm] 选「拒绝」：记录拒绝决定，执行按 Mock 行为结束", async ({ page }) => {
  const title = "运行脚本 [mock:perm] 拒绝";
  const ref = await createTask(page, teamP, title, ROLE, "READY");
  await startRun(page);

  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible({ timeout: TIMEOUT });
  await dialog.getByRole("button", { name: "拒绝" }).click();

  const success = page.locator(".run-outcome--success");
  await expect(success).toBeVisible({ timeout: TIMEOUT });
  await expect(success.locator(".run-outcome__summary")).toContainText(
    "命令 echo hello 被拒绝，未执行",
  );
  await shot(page, "permission-rejected-finished");

  const run = (await runsOf(ref.id))[0];
  const permissions = (await updatesOf(run?.id ?? ""))
    .map((u) => u.body)
    .filter((b) => b.type === "permission");
  const exec = permissions.find((b) => b.command === "echo hello");
  expect(exec?.decision?.kind).toBe("reject_once");
  expect(exec?.decision?.by).toBe("human");
  expect(exec?.decision?.optionId).toBe("reject");
});

test("[mock:fail]：执行失败，任务受阻并写明原因；「重试」再次执行（第 2 次）", async ({ page }) => {
  const title = "会失败的任务 [mock:fail]";
  const ref = await createTask(page, teamP, title, ROLE, "READY");
  await startRun(page);

  const failed = page.locator(".run-outcome--failed");
  await expect(failed).toBeVisible({ timeout: TIMEOUT });
  await expect(failed).toContainText("第 1 次执行已失败");
  await expect(failed.locator(".run-failure__reason")).toContainText("Mock 模拟的内部错误");
  await expect(failed).toContainText("任务状态：已受阻");
  await expect(statusLine(page)).toContainText("受阻");
  await expect(page.getByText(/受阻原因：执行失败：/)).toBeVisible();
  await shot(page, "run-failed");

  // 重试：创建第 2 次执行（Mock 仍然失败）
  await failed.getByRole("button", { name: "重试" }).click();
  await expect(infoStrip(page)).toContainText("第 2 次执行");
  await expect(page.locator(".run-outcome--failed")).toContainText("第 2 次执行已失败", {
    timeout: TIMEOUT,
  });
  await expect(page.locator(".run-side__panel", { hasText: "历次执行记录" })).toContainText("2 次");
  const runs = await runsOf(ref.id);
  expect(runs.map((r) => r.status)).toEqual(["failed", "failed"]);
  expect(runs.map((r) => r.attempt)).toEqual([2, 1]);
});

test("[mock:auth]：失败原因「需要登录」，Harness 握手状态变为「需要登录」，重新握手后恢复", async ({
  page,
}) => {
  const title = "需要登录的任务 [mock:auth]";
  await createTask(page, teamP, title, ROLE, "READY");
  await startRun(page);

  const failed = page.locator(".run-outcome--failed");
  await expect(failed).toBeVisible({ timeout: TIMEOUT });
  await expect(
    failed.locator(".run-failure__tags .ui-tag", { hasText: "需要登录 · 当前" }),
  ).toBeVisible();
  await expect(failed.locator(".run-failure__advice-title")).toContainText("需要登录");
  await shot(page, "run-failed-needs-auth");

  await page.goto(`/#/teams/${teamP}/roles?tab=harness`);
  const entry = acpEntry(page, HARNESS);
  await expect(entry.locator(".ui-tag", { hasText: "需要登录" })).toBeVisible();
  await shot(page, "harness-needs-auth");

  // 重新握手后恢复，不影响后面的用例
  await entry.getByRole("button", { name: "握手检查" }).click();
  await expect(entry.locator(".ui-tag", { hasText: "已连接" })).toBeVisible();
});

test("[mock:hang] 取消执行：已取消，任务回到 READY；任务面板小标与工作台最近执行", async ({
  page,
}) => {
  const title = "长时间任务 [mock:hang] 取消";
  const ref = await createTask(page, teamP, title, ROLE, "READY");
  await startRun(page);

  // 执行中：进度流、取消与接管按钮
  await expect(page.locator(".run-progress")).toBeVisible();
  await expect(page.locator(".run-msg--agent").first()).toContainText("收到任务", {
    timeout: TIMEOUT,
  });
  await expect(page.locator(".run-msg--agent").first()).toContainText("正在长时间处理");
  await expect(infoStrip(page)).toContainText("执行中");
  await expect(page.getByRole("button", { name: "取消执行" })).toBeVisible();
  await expect(page.getByRole("button", { name: "人工接管", exact: true })).toBeVisible();
  await expect(statusLine(page)).toContainText("执行中");
  await page.locator(".run-msg--agent").last().scrollIntoViewIfNeeded();
  await shot(page, "run-in-progress");

  // 任务面板卡片小标 + 岗位工作台「最近执行」
  await page.goto(`/#/teams/${teamP}/tasks`);
  await expect(
    page.locator("a.task-card-link", { hasText: title }).locator(".ui-tag", {
      hasText: `执行中 · ${HARNESS}`,
    }),
  ).toBeVisible();
  await shot(page, "board-running-badge");
  await page.goto(`/#/teams/${teamP}`);
  await page.locator(".th-role", { hasText: ROLE }).getByRole("link", { name: ROLE }).click();
  const recent = page.locator(".rw-run-row", { hasText: ref.key });
  await expect(recent).toContainText("执行中");
  await expect(recent).toContainText(HARNESS);

  await openTask(page, title);
  await page.getByRole("button", { name: "取消执行" }).click();
  const cancelled = page.locator(".run-outcome--cancelled");
  await expect(cancelled).toBeVisible({ timeout: TIMEOUT });
  await expect(cancelled).toContainText("执行已取消");
  await expect(statusLine(page)).toContainText("READY");
  await expect(cancelled).toContainText("取消后任务回到");
  await shot(page, "run-cancelled");
  expect((await runsOf(ref.id))[0]?.status).toBe("cancelled");
  // 取消后可以再次执行
  await expect(startButton(page).first()).toBeEnabled();
});

test("[mock:hang] 人工接管：确认弹窗，已接管显示执行目录，任务保持执行中，直接提交验收", async ({
  page,
}) => {
  const title = "长时间任务 [mock:hang] 接管";
  const ref = await createTask(page, teamP, title, ROLE, "READY");
  await startRun(page);
  await expect(page.locator(".run-msg--agent").first()).toContainText("正在长时间处理", {
    timeout: TIMEOUT,
  });

  await page.getByRole("button", { name: "人工接管", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  await expect(dialog).toContainText("接管会停止这条执行");
  await expect(dialog.locator(".run-takeover__path")).toContainText(plain);
  await shot(page, "takeover-confirm");
  await dialog.getByRole("button", { name: "确认接管" }).click();

  const taken = page.locator(".run-outcome--taken-over");
  await expect(taken).toBeVisible({ timeout: TIMEOUT });
  await expect(taken).toContainText("已由你接管");
  await expect(taken.locator(".run-execdir__path")).toHaveText(plain);
  await expect(taken).toContainText("原地执行，这个目录就是工作空间本身");
  await expect(statusLine(page)).toContainText("执行中");
  await shot(page, "takeover-done");

  // 在 Finder 中打开：只触发 open_run_dir，后端不报错
  const reply = page.waitForResponse(
    (response) =>
      response.url().endsWith("/invoke") &&
      (response.request().postData() ?? "").includes("open_run_dir"),
  );
  await taken.getByRole("button", { name: "在 Finder 中打开" }).click();
  const body = (await (await reply).json()) as { ok?: unknown; err?: unknown };
  expect(body.err).toBeUndefined();

  await taken.getByRole("button", { name: "直接提交验收" }).click();
  await expect(statusLine(page)).toContainText("待验收");
  const run = (await runsOf(ref.id))[0];
  expect(run?.status).toBe("cancelled");
});

test("单执行约束：同一普通文件夹有活动执行时，另一个任务的「开始执行」不可用并说明原因", async ({
  page,
}) => {
  const busy = await createTask(page, teamP, "占用文件夹 [mock:hang]", ROLE, "READY");
  const waiting = await createTask(page, teamP, "排队等待的任务", ROLE, "READY");
  await expect(startButton(page).first()).toBeEnabled();

  await openTask(page, busy.title);
  await startRun(page);
  await expect(page.locator(".run-msg--agent").first()).toContainText("正在长时间处理", {
    timeout: TIMEOUT,
  });

  await openTask(page, waiting.title);
  const start = page.getByRole("button", { name: "开始执行" });
  await expect(start).toBeDisabled();
  await expect(start).toHaveAttribute("title", /同一文件夹同时只能运行一个执行/);
  await expect(page.getByText("还不能开始执行")).toBeVisible();
  await expect(page.getByText(new RegExp(`正在执行 ${busy.key}`))).toBeVisible();
  await shot(page, "start-disabled-single-run");

  // 后端同样拒绝
  await expect(invoke("start_run", { taskId: waiting.id })).rejects.toThrow(/不能开始执行/);

  // 释放文件夹后可以开始
  await openTask(page, busy.title);
  await page.getByRole("button", { name: "取消执行" }).click();
  await expect(page.locator(".run-outcome--cancelled")).toBeVisible({ timeout: TIMEOUT });
  await openTask(page, waiting.title);
  await expect(page.getByRole("button", { name: "开始执行" })).toBeEnabled();
});

test("未就绪任务（TODO）与岗位未绑定：「开始执行」不可用并显示原因", async ({ page }) => {
  await createTask(page, teamP, "还没就绪的任务", ROLE, "TODO");
  const start = page.getByRole("button", { name: "开始执行" });
  await expect(start).toBeDisabled();
  await expect(start).toHaveAttribute("title", /任务状态为「TODO」/);
  await expect(page.getByText("任务状态为「TODO」")).toBeVisible();
  await shot(page, "start-disabled-todo");

  await createTask(page, teamP, "没人接手的任务", ROLE_FREE, "READY");
  const unbound = page.getByRole("button", { name: "开始执行" });
  await expect(unbound).toBeDisabled();
  await expect(unbound).toHaveAttribute("title", /未绑定 Harness/);
  await expect(page.getByText(/负责岗位「未绑定员」未绑定 Harness/)).toBeVisible();
  await shot(page, "start-disabled-unbound");
});

// ---------------------------------------------------------------- 已中断

test("已中断（可续接）：续接会话后继续并完成", async ({ page }) => {
  const title = "应用重启 [mock:hang] 续接";
  const ref = await createTask(page, teamP, title, ROLE, "READY");
  await startRun(page);
  await expect(page.locator(".run-msg--agent").first()).toContainText("正在长时间处理", {
    timeout: TIMEOUT,
  });

  await invoke("__simulate_restart");
  await page.reload();
  const interrupted = page.locator(".run-outcome--interrupted");
  await expect(interrupted).toBeVisible({ timeout: TIMEOUT });
  await expect(interrupted).toContainText("执行已中断");
  await expect(interrupted).toContainText("支持续接会话");
  await expect(statusLine(page)).toContainText("执行中");
  for (const name of ["续接会话", "重新执行", "人工接管", "标记为失败"]) {
    await expect(interrupted.getByRole("button", { name })).toBeVisible();
  }
  await shot(page, "run-interrupted-resumable");
  expect((await runsOf(ref.id))[0]?.status).toBe("interrupted");

  await interrupted.getByRole("button", { name: "续接会话" }).click();
  // 续接后回放历史并发「继续」提示，Mock 走默认流程完成
  const success = page.locator(".run-outcome--success");
  await expect(success).toBeVisible({ timeout: TIMEOUT });
  await expect(statusLine(page)).toContainText("待验收");
  const runs = await runsOf(ref.id);
  expect(runs).toHaveLength(1);
  expect(runs[0]?.status).toBe("succeeded");
  await shot(page, "run-resumed-succeeded");
});

test("已中断：重新执行创建第 2 次执行（第 1 次记为失败）", async ({ page }) => {
  const title = "应用重启 [mock:hang] 重新执行";
  const ref = await createTask(page, teamP, title, ROLE, "READY");
  await startRun(page);
  await expect(page.locator(".run-msg--agent").first()).toContainText("正在长时间处理", {
    timeout: TIMEOUT,
  });
  await invoke("__simulate_restart");
  await page.reload();
  const interrupted = page.locator(".run-outcome--interrupted");
  await expect(interrupted).toBeVisible({ timeout: TIMEOUT });

  await interrupted.getByRole("button", { name: "重新执行" }).click();
  await expect(infoStrip(page)).toContainText("第 2 次执行", { timeout: TIMEOUT });
  await expect(page.locator(".run-msg--agent").first()).toContainText("正在长时间处理", {
    timeout: TIMEOUT,
  });
  const runs = await waitRun(
    ref.id,
    (list) => list.length === 2 && list[0]?.status === "running",
    "第 2 次执行进入执行中",
  );
  expect(runs[1]?.status).toBe("failed");
  expect(runs[1]?.failure ?? "").toContain("中断");

  // 收尾：取消第 2 次，释放文件夹
  await page.getByRole("button", { name: "取消执行" }).click();
  await expect(page.locator(".run-outcome--cancelled")).toBeVisible({ timeout: TIMEOUT });
  await expect(statusLine(page)).toContainText("READY");
});

test("已中断：标记为失败，任务受阻并写明原因", async ({ page }) => {
  const title = "应用重启 [mock:hang] 标记失败";
  const ref = await createTask(page, teamP, title, ROLE, "READY");
  await startRun(page);
  await expect(page.locator(".run-msg--agent").first()).toContainText("正在长时间处理", {
    timeout: TIMEOUT,
  });
  await invoke("__simulate_restart");
  await page.reload();
  const interrupted = page.locator(".run-outcome--interrupted");
  await expect(interrupted).toBeVisible({ timeout: TIMEOUT });

  await interrupted.getByRole("button", { name: "标记为失败" }).click();
  const failed = page.locator(".run-outcome--failed");
  await expect(failed).toBeVisible({ timeout: TIMEOUT });
  await expect(statusLine(page)).toContainText("受阻");
  await expect(page.getByText(/受阻原因：执行失败：.*中断/)).toBeVisible();
  expect((await runsOf(ref.id))[0]?.status).toBe("failed");
  await shot(page, "run-interrupted-marked-failed");
});

// ---------------------------------------------------------------- Git 工作空间

test("Git 工作空间：worktree 隔离，分支 teamspace/<编号>-r1，原工作空间文件不被改动", async ({
  page,
}) => {
  const title = "Git 整理文档";
  const ref = await createTask(page, teamG, title, ROLE_GIT, "READY");
  const before = git(gitWs, "rev-parse", "HEAD").trim();
  const readme = readFileSync(join(gitWs, "README.md"), "utf8");

  await startRun(page);
  await expect(infoStrip(page)).toContainText("Worktree：");
  const success = page.locator(".run-outcome--success");
  await expect(success).toBeVisible({ timeout: TIMEOUT });
  await expect(success.locator(".run-outcome__facts")).toContainText("Worktree 分支");
  await expect(statusLine(page)).toContainText("待验收");
  await shot(page, "git-run-succeeded");

  const run = (await runsOf(ref.id))[0];
  expect(run?.isolation).toBe("worktree");
  const branch = `teamspace/${ref.key}-r1`;
  expect(run?.branch).toBe(branch);
  // 执行信息条显示的执行目录就是 worktree，不是原工作空间
  await expect(infoStrip(page)).toContainText(run?.execDir ?? "");
  expect(run?.execDir).not.toBe(gitWs);
  expect(git(gitWs, "branch", "--list", branch)).toContain(branch);

  // Mock 的写入落在 worktree；原工作空间的已跟踪文件不变，也没有 Mock 写入的文件
  expect(processFiles(run?.execDir ?? "", ROLE_GIT)).toHaveLength(1);
  expect(readFileSync(join(gitWs, "README.md"), "utf8")).toBe(readme);
  expect(git(gitWs, "rev-parse", "HEAD").trim()).toBe(before);
  expect(git(gitWs, "status", "--porcelain", "--untracked-files=no")).toBe("");
  expect(walk(gitWs).filter((file) => /mock-.*\.md$/.test(file))).toEqual([]);
  expect(git(gitWs, "rev-parse", "--abbrev-ref", "HEAD").trim()).toBe("main");
});

test("Git 工作空间：人工接管后显示 worktree 分支与执行目录", async ({ page }) => {
  const title = "Git 长时间任务 [mock:hang]";
  const ref = await createTask(page, teamG, title, ROLE_GIT, "READY");
  await startRun(page);
  await expect(page.locator(".run-msg--agent").first()).toContainText("正在长时间处理", {
    timeout: TIMEOUT,
  });
  await expect(infoStrip(page)).toContainText("Worktree：");

  await page.getByRole("button", { name: "人工接管", exact: true }).click();
  await page.getByRole("dialog").getByRole("button", { name: "确认接管" }).click();
  const taken = page.locator(".run-outcome--taken-over");
  await expect(taken).toBeVisible({ timeout: TIMEOUT });
  await expect(taken).toContainText(`Git worktree 分支 teamspace/${ref.key}-r1`);
  const run = (await runsOf(ref.id))[0];
  await expect(taken.locator(".run-execdir__path")).toHaveText(run?.execDir ?? "");
  await shot(page, "git-takeover");
});

// 已知缺陷（M2-05 页面）：Git 执行时执行信息条只显示「Worktree：」加目录路径，没有分支名；
// 分支名只在人工接管后的区块里出现。修复后此用例会失败，届时去掉 test.fail()。
test("Git 工作空间：执行信息条显示 worktree 分支名（已知缺陷）", async ({ page }) => {
  test.fail();
  const ref = await openTask(page, "Git 整理文档");
  await expect(infoStrip(page)).toContainText(`teamspace/${ref.key}-r1`, { timeout: 3_000 });
});
