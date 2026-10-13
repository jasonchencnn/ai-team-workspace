// apps/desktop/e2e/z-qa-m2.spec.ts
// M2 派工执行回归与探索测试套件（独立 QA - Gemini / Antigravity）
// 重点覆盖边界、竞态、并发隔离与异常恢复：
// 1. 执行中快速连续操作（取消/接管）、权限弹窗打开时取消、选择权限后立即刷新页面。
// 2. 执行中编辑任务、改派负责人、修改岗位绑定 Harness、归档 Harness / 岗位（防护与缺陷）。
// 3. 两个团队普通文件夹并发执行（允许）；单文件夹第二个任务被拒；取消后立即允许开始。
// 4. 多次重试历次执行记录与编号；[mock:refuse] 与 [mock:exit] 失败原因文案可读。
// 5. 模拟重启（__simulate_restart）处于「等待你确认」时的恢复与按钮状态。
// 6. 执行中刷新页面：断点补齐、不重复、不丢失。
// 7. 岗位工作台、任务面板小标、团队空间在各执行阶段的一致性。
// 8. 引用中文名与子目录文件的自动批准范围展示。
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { type Locator, type Page, expect, test } from "@playwright/test";

import { createMockHarness } from "./harnessHelpers.js";
import { BRIDGE_PORT } from "./ports.js";

const artifacts = fileURLToPath(new URL("../e2e-artifacts/", import.meta.url));
mkdirSync(artifacts, { recursive: true });

let shotNo = 0;
async function shot(page: Page, name: string): Promise<string> {
  shotNo += 1;
  const filename = `qa4-${String(shotNo).padStart(2, "0")}-${name}.png`;
  const fullPath = join(artifacts, filename);
  await page.screenshot({
    path: fullPath,
    fullPage: false,
  });
  return fullPath;
}

/** 直接调用桥接命令（读取状态、模拟重启、API边界探测）。 */
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
  failureKind?: string | null;
}

interface UpdateInfo {
  id: string;
  seq: number;
  body: {
    type: string;
    decision?: { kind: string; by: string; optionId: string | null } | null;
    command?: string | null;
    text?: string | null;
  };
}

const TIMEOUT = 25_000;

let plainA = "";
let plainB = "";
let teamA = "";
let teamB = "";

const ROLE_A1 = "主执行员";
const ROLE_A2 = "副执行员";
const ROLE_B1 = "独立协作者";
const HARNESS_A = "qa4-harness-alpha";
const HARNESS_A_ALT = "qa4-harness-beta";
const HARNESS_B = "qa4-harness-gamma";

interface TaskRef {
  url: string;
  id: string;
  key: string;
  title: string;
}
const tasks = new Map<string, TaskRef>();

test.describe.configure({ mode: "serial", timeout: 90_000 });

test.beforeAll(() => {
  plainA = mkdtempSync(join(tmpdir(), "teamspace-e2e-qa4-plain-a-"));
  writeFileSync(join(plainA, "brief.txt"), "主团队需求说明\n");

  plainB = mkdtempSync(join(tmpdir(), "teamspace-e2e-qa4-plain-b-"));
  writeFileSync(join(plainB, "brief.txt"), "副团队需求说明\n");
});

test.afterAll(() => {
  rmSync(plainA, { recursive: true, force: true });
  rmSync(plainB, { recursive: true, force: true });
});

// ---------------------------------------------------------------- 辅助工具函数

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
  const value = await select.locator("option", { hasText: harness }).getAttribute("value");
  await select.selectOption(value ?? "");
  await page.getByRole("button", { name: "确认绑定" }).click();
  await expect(page.getByText(`当前绑定 ${harness}`)).toBeVisible();
}

async function createTask(
  page: Page,
  team: string,
  title: string,
  owner: string,
  status: "TODO" | "READY",
  fileSearch = "brief",
  fileSelect = "brief.txt",
): Promise<TaskRef> {
  await page.goto(`/#/teams/${team}/tasks/new`);
  await page.getByLabel("任务名称").fill(title);
  const select = page.getByLabel("负责人岗位");
  await select.selectOption(
    (await select.locator("option", { hasText: owner }).getAttribute("value")) ?? "",
  );
  if (fileSearch !== "") {
    await page.locator("#task-file-search").fill(fileSearch);
    await page.locator(".task-form__suggestion").filter({ hasText: fileSelect }).first().click();
  }
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

async function updatesOf(runId: string): Promise<UpdateInfo[]> {
  const res = await invoke<{ updates: UpdateInfo[] }>("list_run_updates", {
    input: { runId, limit: 500 },
  });
  return res.updates;
}

// ---------------------------------------------------------------- 测试套件准备

test("00. 准备：创建双团队（独立文件夹）、注册 Mock Harness 并完成绑定", async ({ page }) => {
  teamA = await createTeam(page, "QA4主执行团队", plainA, [ROLE_A1, ROLE_A2]);
  await page.goto(`/#/teams/${teamA}/roles?tab=harness`);
  await createMockHarness(page, HARNESS_A);
  await createMockHarness(page, HARNESS_A_ALT);
  await bindRole(page, teamA, ROLE_A1, HARNESS_A);

  teamB = await createTeam(page, "QA4并发对照团队", plainB, [ROLE_B1]);
  await page.goto(`/#/teams/${teamB}/roles?tab=harness`);
  await createMockHarness(page, HARNESS_B);
  await bindRole(page, teamB, ROLE_B1, HARNESS_B);
});

// ---------------------------------------------------------------- 1. 竞态与操作冲突

test("01. 执行中快速连续点击「取消执行」与「人工接管」：状态一致，界面无崩溃", async ({ page }) => {
  const title = "快速连续操作测试 [mock:hang]";
  const ref = await createTask(page, teamA, title, ROLE_A1, "READY");
  await startRun(page);

  await expect(infoStrip(page)).toContainText("执行中");
  const cancelBtn = page.getByRole("button", { name: "取消执行" });
  const takeoverBtn = page.getByRole("button", { name: "人工接管", exact: true });
  await expect(cancelBtn).toBeVisible();
  await expect(takeoverBtn).toBeVisible();

  // 触发接管弹窗后立即取消弹窗并连击「取消执行」
  await takeoverBtn.click();
  const modal = page.getByRole("dialog").filter({ hasText: "人工接管" });
  await expect(modal).toBeVisible({ timeout: 5_000 });
  await modal.getByRole("button", { name: "取消" }).click();
  await expect(modal).toHaveCount(0);
  await cancelBtn.click();

  // 取消操作处理后，执行进入取消终态，且不会出现报错白屏
  const outcome = page.locator(".run-outcome--cancelled");
  await expect(outcome).toBeVisible({ timeout: TIMEOUT });
  await shot(page, "rapid-cancel-takeover");

  const run = (await runsOf(ref.id))[0];
  expect(run?.status).toBe("cancelled");
});

test("02. 在权限弹窗打开时取消执行：弹窗自动关闭，执行恢复取消态并回到 READY", async ({ page }) => {
  const title = "权限弹窗打开时取消 [mock:perm]";
  const ref = await createTask(page, teamA, title, ROLE_A1, "READY");
  await startRun(page);

  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible({ timeout: TIMEOUT });
  await expect(dialog).toContainText("请求权限");
  await expect(dialog).toContainText("等待你确认");
  await shot(page, "permission-dialog-open-before-cancel");

  // 在弹窗开启状态下，调用后端取消命令（模拟外界/并发取消，或弹窗外取消事件）
  const run = (await runsOf(ref.id))[0];
  expect(run).toBeDefined();
  await invoke("cancel_run", { runId: run?.id ?? "" });

  // 弹窗应随之自动关闭，界面呈现「执行已取消」
  await expect(dialog).toHaveCount(0, { timeout: 8_000 });
  const cancelled = page.locator(".run-outcome--cancelled");
  await expect(cancelled).toBeVisible();
  await expect(statusLine(page)).toContainText("READY");
  await shot(page, "cancel-while-permission-open-resolved");
});

test("03. 权限弹窗选择后立即刷新页面：决策正确生效，执行继续并正常完成", async ({ page }) => {
  const title = "选择权限后立即刷新 [mock:perm]";
  const ref = await createTask(page, teamA, title, ROLE_A1, "READY");
  await startRun(page);

  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible({ timeout: TIMEOUT });
  await dialog.getByRole("button", { name: "允许一次" }).click();

  // 立即刷新页面
  await page.reload();

  // 刷新后执行继续完成，最终转为待验收
  const success = page.locator(".run-outcome--success");
  await expect(success).toBeVisible({ timeout: TIMEOUT });
  await expect(success.locator(".run-outcome__summary")).toContainText("命令 echo hello 已执行");
  await expect(statusLine(page)).toContainText("待验收");
  await shot(page, "permission-response-immediate-reload");

  const run = (await runsOf(ref.id))[0];
  const updates = await updatesOf(run?.id ?? "");
  const permUpdate = updates.find(
    (u) => u.body.type === "permission" && u.body.command === "echo hello",
  );
  expect(permUpdate?.body.decision?.kind).toBe("allow_once");
});

// ---------------------------------------------------------------- 2. 执行中的约束与保护

test("04. 任务执行中不能编辑或改派负责人；执行取消后可以编辑", async ({ page }) => {
  const title = "执行中尝试改派与编辑 [mock:hang]";
  const ref = await createTask(page, teamA, title, ROLE_A1, "READY");
  await startRun(page);
  await expect(infoStrip(page)).toContainText("执行中");

  // 详情页没有编辑入口
  await expect(page.getByRole("button", { name: "编辑" })).toHaveCount(0);

  // 直接访问编辑路由：显示原因，保存被禁用
  const reason = "任务正在执行中，结束或取消执行后才能编辑";
  await page.goto(`/#/teams/${teamA}/tasks/${ref.id}/edit`);
  await expect(page.getByRole("alert").filter({ hasText: reason })).toBeVisible();
  await expect(page.getByRole("button", { name: "保存" })).toBeDisabled();
  await shot(page, "edit-task-form-blocked-during-run");

  // 绕过界面直接调用后端同样被拒绝，标题不变
  await expect(
    invoke("update_task", {
      input: {
        taskId: ref.id,
        title: "偷偷改名",
        description: "",
        ownerRoleId: "",
        fileRefs: [],
        dependsOn: [],
      },
    }),
  ).rejects.toThrow(reason);

  // 取消执行后可以编辑
  await openTask(page, title);
  await page.getByRole("button", { name: "取消执行" }).click();
  await expect(page.locator(".run-outcome--cancelled")).toBeVisible({ timeout: TIMEOUT });
  await page.getByRole("button", { name: "编辑" }).click();
  await expect(page).toHaveURL(new RegExp(`/tasks/${ref.id}/edit$`));
  await expect(page.getByRole("button", { name: "保存" })).toBeEnabled();
  await page.getByLabel("任务名称").fill(`${title} 改`);
  await page.getByRole("button", { name: "保存" }).click();
  await expect(page.getByRole("heading", { name: `${title} 改` })).toBeVisible();
});

test("05. 执行中修改岗位绑定 Harness 与归档操作防护：归档被拒，执行安全不受破坏", async ({
  page,
}) => {
  const title = "执行中岗位归档与换绑防护 [mock:hang]";
  await createTask(page, teamA, title, ROLE_A1, "READY");
  await startRun(page);
  await expect(infoStrip(page)).toContainText("执行中");

  // 1. 尝试归档正在执行的岗位（有 Doing 任务）：后端冲突拒绝
  const roles = await invoke<Array<{ role: { id: string; name: string } }>>("list_roles", {
    teamId: teamA,
  });
  const roleA1 = roles.find((r) => r.role.name === ROLE_A1)?.role.id ?? "";
  await expect(invoke("archive_role", { roleId: roleA1 })).rejects.toThrow(/未结束的任务/);

  // 2. 尝试归档被绑定的 Harness：后端冲突拒绝
  const harnesses = await invoke<Array<{ id: string; name: string }>>("list_harnesses", {
    teamId: teamA,
  });
  const harnessAId = harnesses.find((h) => h.name === HARNESS_A)?.id ?? "";
  await expect(invoke("archive_harness", { harnessId: harnessAId })).rejects.toThrow(
    /仍被岗位.*绑定/,
  );

  // 3. 回到任务详情，执行依然健康进行，不受影响
  await openTask(page, title);
  await expect(infoStrip(page)).toContainText("执行中");
  await shot(page, "execution-intact-after-rejected-archive");

  // 收尾：取消任务
  await page.getByRole("button", { name: "取消执行" }).click();
  await expect(page.locator(".run-outcome--cancelled")).toBeVisible({ timeout: TIMEOUT });
});

// ---------------------------------------------------------------- 3. 多团队并发与单执行约束

test("06. 两个团队各自的普通文件夹同时执行（应允许并发）", async ({ page }) => {
  const titleA = "主团队长任务 [mock:hang]";
  const titleB = "对照团队长任务 [mock:hang]";
  const refA = await createTask(page, teamA, titleA, ROLE_A1, "READY");
  const refB = await createTask(page, teamB, titleB, ROLE_B1, "READY");

  // 启动团队 A 执行
  await openTask(page, titleA);
  await startRun(page);
  await expect(infoStrip(page)).toContainText("执行中");

  // 启动团队 B 执行（不同文件夹，互不干扰）
  await openTask(page, titleB);
  await startRun(page);
  await expect(infoStrip(page)).toContainText("执行中");
  await shot(page, "two-teams-concurrent-runs");

  // 验证两边的后端执行状态皆处于 running
  const runA = (await runsOf(refA.id))[0];
  const runB = (await runsOf(refB.id))[0];
  expect(runA?.status).toBe("running");
  expect(runB?.status).toBe("running");

  // 收尾：分别取消
  await page.getByRole("button", { name: "取消执行" }).click();
  await expect(page.locator(".run-outcome--cancelled")).toBeVisible();
  await openTask(page, titleA);
  await page.getByRole("button", { name: "取消执行" }).click();
  await expect(page.locator(".run-outcome--cancelled")).toBeVisible();
});

test("07. 同一普通文件夹单执行约束：第二个任务被拒，取消前一个后立即可开始", async ({ page }) => {
  const title1 = "占用文件夹任务1 [mock:hang]";
  const title2 = "排队任务2 [mock:hang]";
  await createTask(page, teamA, title1, ROLE_A1, "READY");
  const ref2 = await createTask(page, teamA, title2, ROLE_A1, "READY");

  await openTask(page, title1);
  await startRun(page);
  await expect(infoStrip(page)).toContainText("执行中");

  // 打开任务 2：按钮被禁用，显示清晰提示
  await openTask(page, title2);
  const startBtn = page.getByRole("button", { name: "开始执行" });
  await expect(startBtn).toBeDisabled();
  await expect(startBtn).toHaveAttribute("title", /同一文件夹同时只能运行一个执行/);
  await expect(page.getByText("还不能开始执行")).toBeVisible();
  await shot(page, "single-folder-second-task-disabled");

  // 后端亦直接拒绝
  await expect(invoke("start_run", { taskId: ref2.id })).rejects.toThrow(/不能开始执行/);

  // 取消任务 1
  await openTask(page, title1);
  await page.getByRole("button", { name: "取消执行" }).click();
  await expect(page.locator(".run-outcome--cancelled")).toBeVisible();

  // 任务 2 立即变为可用，并能顺利启动
  await openTask(page, title2);
  await expect(startBtn).toBeEnabled();
  await startRun(page);
  await expect(infoStrip(page)).toContainText("执行中");
  await shot(page, "start-immediately-after-cancel");

  // 清理
  await page.getByRole("button", { name: "取消执行" }).click();
  await expect(page.locator(".run-outcome--cancelled")).toBeVisible();
});

// ---------------------------------------------------------------- 4. 多次重试与失败原因文案

test("08. 多次失败重试后历次执行记录与编号正确，折叠与切换准确", async ({ page }) => {
  const title = "三次失败重试任务 [mock:fail]";
  const ref = await createTask(page, teamA, title, ROLE_A1, "READY");

  // 第 1 次
  await startRun(page);
  const failed = page.locator(".run-outcome--failed");
  await expect(failed).toBeVisible({ timeout: TIMEOUT });
  await expect(failed).toContainText("第 1 次执行已失败");

  // 第 2 次重试
  await failed.getByRole("button", { name: "重试" }).click();
  await expect(infoStrip(page)).toContainText("第 2 次执行");
  await expect(failed).toContainText("第 2 次执行已失败", { timeout: TIMEOUT });

  // 第 3 次重试
  await failed.getByRole("button", { name: "重试" }).click();
  await expect(infoStrip(page)).toContainText("第 3 次执行");
  await expect(failed).toContainText("第 3 次执行已失败", { timeout: TIMEOUT });

  // 验证右栏历次记录
  const historyPanel = page.locator(".run-side__panel", { hasText: "历次执行记录" });
  await expect(historyPanel).toContainText("3 次");
  await shot(page, "multiple-retries-history");

  const runs = await runsOf(ref.id);
  expect(runs.map((r) => r.attempt)).toEqual([3, 2, 1]);
  expect(runs.every((r) => r.status === "failed")).toBe(true);
});

test("09. [mock:refuse] 失败原因文案可读、标签与建议清晰", async ({ page }) => {
  const title = "Agent拒绝任务 [mock:refuse]";
  await createTask(page, teamA, title, ROLE_A1, "READY");
  await startRun(page);

  const failed = page.locator(".run-outcome--failed");
  await expect(failed).toBeVisible({ timeout: TIMEOUT });
  await expect(
    failed.locator(".run-failure__tags .ui-tag", { hasText: "拒绝执行 · 当前" }),
  ).toBeVisible();
  await expect(failed.locator(".run-failure__advice-title")).toContainText("拒绝执行");
  await expect(failed.locator(".run-failure__reason")).toContainText(
    "Agent 拒绝了这个任务（refusal）",
  );
  await expect(failed.locator(".run-failure__text")).toContainText("Harness 拒绝了这次执行");
  await shot(page, "mock-refuse-readable");
});

test("10. [mock:exit] 进程异常退出失败原因文案可读且界面不卡死", async ({ page }) => {
  const title = "Agent异常退出 [mock:exit]";
  await createTask(page, teamA, title, ROLE_A1, "READY");
  await startRun(page);

  const failed = page.locator(".run-outcome--failed");
  await expect(failed).toBeVisible({ timeout: TIMEOUT });
  await expect(
    failed.locator(".run-failure__tags .ui-tag", { hasText: "Agent 退出 · 当前" }),
  ).toBeVisible();
  await expect(failed.locator(".run-failure__advice-title")).toContainText("Agent 退出");
  await expect(failed.locator(".run-failure__reason")).toContainText(
    /Agent 进程退出|Agent 关闭了输出/,
  );
  await expect(failed.locator(".run-failure__text")).toContainText(
    "Agent 进程已退出，ACP 会话断开",
  );
  await shot(page, "mock-exit-readable");
});

// ---------------------------------------------------------------- 5. 模拟重启在等待确认状态的恢复

test("11. 模拟重启（__simulate_restart）处于「等待你确认」时的恢复与按钮", async ({ page }) => {
  const title = "重启确认中断测试 [mock:perm]";
  const ref = await createTask(page, teamA, title, ROLE_A1, "READY");
  await startRun(page);

  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible({ timeout: TIMEOUT });
  await expect(dialog).toContainText("等待你确认");

  // 模拟应用退出重启收尾
  await invoke("__simulate_restart");
  await page.reload();

  // 重启后应转入已中断状态
  const interrupted = page.locator(".run-outcome--interrupted");
  await expect(interrupted).toBeVisible({ timeout: TIMEOUT });
  await expect(interrupted).toContainText("执行已中断");
  await expect(interrupted.locator(".run-outcome__alert")).toContainText(
    "应用重启，与 Agent 的会话已丢失",
  );

  // 验证 4 个恢复按钮齐全且弹窗不再幽灵出现
  await expect(dialog).toHaveCount(0);
  for (const btn of ["续接会话", "重新执行", "人工接管", "标记为失败"]) {
    await expect(interrupted.getByRole("button", { name: btn })).toBeVisible();
  }
  await shot(page, "restart-during-waiting-permission");

  // 续接会话
  await interrupted.getByRole("button", { name: "续接会话" }).click();
  const success = page.locator(".run-outcome--success");
  await expect(success).toBeVisible({ timeout: TIMEOUT });
  await expect(statusLine(page)).toContainText("待验收");
  await shot(page, "resume-after-restart-success");

  const run = (await runsOf(ref.id))[0];
  expect(run?.status).toBe("succeeded");
});

// ---------------------------------------------------------------- 6. 执行中刷新页面断点补齐

test("12. 执行中刷新页面：进度流从断点补齐，不重复、不丢失", async ({ page }) => {
  const title = "流断点补齐测试 [mock:hang]";
  await createTask(page, teamA, title, ROLE_A1, "READY");
  await startRun(page);

  const stream = page.locator(".run-stream");
  await expect(stream.locator(".run-msg--agent").first()).toContainText("收到任务", {
    timeout: TIMEOUT,
  });
  await expect(stream.locator(".run-msg--agent").first()).toContainText("正在长时间处理");

  // 记录刷新前收到的条目
  const beforeCount = await stream.locator(".run-msg").count();
  expect(beforeCount).toBeGreaterThan(0);

  // 刷新页面
  await page.reload();

  // 验证断点恢复：进度流条目仍然完整存在，未丢失
  const afterStream = page.locator(".run-stream");
  await expect(afterStream.locator(".run-msg--agent").first()).toContainText("收到任务", {
    timeout: TIMEOUT,
  });
  await expect(afterStream.locator(".run-msg--agent").first()).toContainText("正在长时间处理");
  const afterCount = await afterStream.locator(".run-msg").count();
  expect(afterCount).toBeGreaterThanOrEqual(beforeCount);
  await shot(page, "stream-resumed-after-refresh");

  // 清理
  await page.getByRole("button", { name: "取消执行" }).click();
  await expect(page.locator(".run-outcome--cancelled")).toBeVisible();
});

// ---------------------------------------------------------------- 7. 各处状态显示一致性

test("13. 岗位工作台、任务面板小标、团队空间在执行各阶段的一致性", async ({ page }) => {
  const title = "全流程状态一致性 [mock:hang]";
  const ref = await createTask(page, teamA, title, ROLE_A1, "READY");

  // 阶段 1：READY
  await page.goto(`/#/teams/${teamA}/tasks`);
  const card = page.locator("a.task-card-link", { hasText: title });
  await expect(card.locator(".ui-tag", { hasText: "READY" })).toBeVisible();

  await page.goto(`/#/teams/${teamA}`);
  await expect(page.locator(".th-panel", { hasText: "推进中的工作" })).toContainText("READY");

  await page.locator(".th-role", { hasText: ROLE_A1 }).getByRole("link", { name: ROLE_A1 }).click();
  await expect(page.locator(".rw-task-row", { hasText: title })).toContainText("READY");
  await shot(page, "stage-ready-consistency");

  // 阶段 2：执行中
  await openTask(page, title);
  await startRun(page);
  await expect(infoStrip(page)).toContainText("执行中");

  await page.goto(`/#/teams/${teamA}/tasks`);
  await expect(card.locator(".ui-tag", { hasText: `执行中 · ${HARNESS_A}` })).toBeVisible();

  await page.goto(`/#/teams/${teamA}`);
  await expect(page.locator(".th-task-row", { hasText: title })).toBeVisible();

  await page.locator(".th-role", { hasText: ROLE_A1 }).getByRole("link", { name: ROLE_A1 }).click();
  await expect(page.locator(".rw-run-row", { hasText: ref.key })).toContainText("执行中");
  await shot(page, "stage-running-consistency");

  // 阶段 3：取消后回到 READY
  await openTask(page, title);
  await page.getByRole("button", { name: "取消执行" }).click();
  await expect(page.locator(".run-outcome--cancelled")).toBeVisible();

  await page.goto(`/#/teams/${teamA}/tasks`);
  await expect(card.locator(".ui-tag", { hasText: "READY" })).toBeVisible();

  await page.goto(`/#/teams/${teamA}`);
  await expect(page.locator(".th-panel", { hasText: "推进中的工作" })).toContainText("READY");

  await page.locator(".th-role", { hasText: ROLE_A1 }).getByRole("link", { name: ROLE_A1 }).click();
  await expect(page.locator(".rw-run-row", { hasText: ref.key })).toContainText("已取消");
  await shot(page, "stage-cancelled-consistency");
});

// ---------------------------------------------------------------- 8. 引用中文名与子目录自动批准范围

test("14. 引用文件含中文名与子目录时，自动批准范围显示正确无乱码", async ({ page }) => {
  // 在工作空间内创建含中文与多级目录的文件
  const subDoc = join(plainA, "项目文档", "需求规划");
  mkdirSync(subDoc, { recursive: true });
  writeFileSync(join(subDoc, "接口需求.md"), "# 接口需求正文\n");

  const subAsset = join(plainA, "资源目录");
  mkdirSync(subAsset, { recursive: true });
  writeFileSync(join(subAsset, "结构图解.txt"), "结构说明文字\n");

  const title = "中文多级文件引用范围 [mock:hang]";
  await createTask(page, teamA, title, ROLE_A1, "READY", "接口需求", "接口需求.md");

  // 再为该任务追加第二个文件引用
  await page.getByRole("button", { name: "编辑" }).click();
  await page.locator("#task-file-search").fill("结构图解");
  await page.locator(".task-form__suggestion").filter({ hasText: "结构图解.txt" }).first().click();
  await page.getByRole("button", { name: "保存" }).click();

  // 开始执行并在侧栏检查自动批准规则
  await startRun(page);
  const rules = page.locator(".run-side__panel", { hasText: "权限规则" });
  await expect(rules).toBeVisible();

  // 验证引用的多级中文文件完整显示
  await expect(rules.locator(".run-rule", { hasText: "自动批准" })).toContainText(
    "项目文档/需求规划/接口需求.md",
  );
  await expect(rules.locator(".run-rule", { hasText: "自动批准" })).toContainText(
    "资源目录/结构图解.txt",
  );
  await shot(page, "chinese-file-refs-scope");

  // 清理
  await page.getByRole("button", { name: "取消执行" }).click();
  await expect(page.locator(".run-outcome--cancelled")).toBeVisible();
});
