// 测试轮次 2026-10-14（QA R2）：RUN 用例共用的辅助。准备数据直接走测试桥接（真实内核），
// 被测的界面行为才用页面操作。
import { execFileSync } from "node:child_process";
import { dirname, join } from "node:path";

import { type Locator, type Page, expect } from "@playwright/test";

import { BRIDGE_PORT } from "./ports.js";

export const TIMEOUT = 25_000;

/** 直接调用桥接命令。 */
export async function invoke<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
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

/** 桥接返回错误时的消息（用于断言后端拒绝）。 */
export async function invokeError(cmd: string, args: Record<string, unknown>): Promise<string> {
  const response = await fetch(`http://127.0.0.1:${String(BRIDGE_PORT)}/invoke`, {
    method: "POST",
    headers: { "Content-Type": "text/plain" },
    body: JSON.stringify({ cmd, args }),
  });
  const reply = (await response.json()) as {
    ok?: unknown;
    err?: { message?: string; kind?: string };
  };
  if (reply.err === undefined) {
    throw new Error(`${cmd} 本应被拒绝，但成功了`);
  }
  return `${reply.err.kind ?? ""}:${reply.err.message ?? ""}`;
}

export interface ChangedFileInfo {
  path: string;
  change: string;
}

export interface RunInfo {
  id: string;
  taskId: string;
  attempt: number;
  status: string;
  statusLabel: string;
  branch: string | null;
  baseCommit: string | null;
  execDir: string;
  isolation: string;
  sessionId: string | null;
  failure: string | null;
  failureKind: string | null;
  summary: string | null;
  takenOver: boolean;
  canResume: boolean;
  changedFiles: ChangedFileInfo[];
  outOfScope: string[];
  changeNote: string | null;
  pendingPermission: { requestId: string; reason: string; paths: string[] } | null;
}

export interface UpdateInfo {
  seq: number;
  body: {
    type: string;
    text?: string;
    role?: string;
    allowed?: boolean;
    path?: string;
    files?: string[];
    reason?: string;
    status?: string;
    command?: string | null;
    toolKind?: string | null;
    decision?: { kind: string; by: string; optionId: string | null; reason: string } | null;
  };
}

export interface TeamSetup {
  teamId: string;
  roleIds: Record<string, string>;
}

/** 创建团队与岗位（岗位编号前缀依次为 R1、R2…，避免和别的用例的前缀冲突）。 */
export async function setupTeam(
  name: string,
  workspace: string,
  roles: { name: string; prefix: string }[],
): Promise<TeamSetup> {
  const team = await invoke<{ id: string }>("create_team", {
    input: {
      name,
      description: "",
      workspacePath: workspace,
      roles: roles.map((role) => ({
        name: role.name,
        taskPrefix: role.prefix,
        description: `${role.name}的职责`,
        guidelines: "",
        isLeader: false,
      })),
    },
  });
  const views = await invoke<{ role: { id: string; name: string } }[]>("list_roles", {
    teamId: team.id,
  });
  const roleIds: Record<string, string> = {};
  for (const view of views) {
    roleIds[view.role.name] = view.role.id;
  }
  return { teamId: team.id, roleIds };
}

/** 新建 Mock 接入并握手，返回实例 id。 */
export async function setupMockHarness(teamId: string, name: string): Promise<string> {
  const harness = await invoke<{ id: string }>("create_harness", {
    input: { teamId, name, preset: "mock" },
  });
  await invoke("check_harness", { harnessId: harness.id });
  return harness.id;
}

export async function bind(roleId: string, harnessId: string): Promise<void> {
  await invoke("bind_role_harness", { roleId, harnessId });
}

export interface TaskRef {
  id: string;
  key: string;
  title: string;
  url: string;
}

export interface TaskOptions {
  teamId: string;
  roleId: string;
  title: string;
  description?: string;
  status?: "todo" | "ready" | undefined;
  refs?: string[] | undefined;
  dependsOn?: string[] | undefined;
}

/** 直接经桥接创建任务；`status` 默认 ready。 */
export async function createTaskApi(options: TaskOptions): Promise<TaskRef> {
  const detail = await invoke<{ task: { id: string; key: string } }>("create_task", {
    input: {
      teamId: options.teamId,
      title: options.title,
      description: options.description ?? "",
      ownerRoleId: options.roleId,
      status: options.status ?? "ready",
      fileRefs: (options.refs ?? []).map((path) => ({ path, anchor: null })),
      dependsOn: options.dependsOn ?? [],
    },
  });
  return {
    id: detail.task.id,
    key: detail.task.key,
    title: options.title,
    url: `/#/teams/${options.teamId}/tasks/${detail.task.id}`,
  };
}

export async function runsOf(taskId: string): Promise<RunInfo[]> {
  return await invoke<RunInfo[]>("list_runs", { taskId });
}

export async function updatesOf(runId: string): Promise<UpdateInfo[]> {
  const page = await invoke<{ updates: UpdateInfo[] }>("list_run_updates", {
    input: { runId, limit: 500 },
  });
  return [...page.updates].sort((a, b) => a.seq - b.seq);
}

/** 等某个任务的最新执行满足条件。 */
export async function waitLatest(
  taskId: string,
  predicate: (run: RunInfo) => boolean,
  what: string,
): Promise<RunInfo> {
  await expect
    .poll(
      async () => {
        const run = (await runsOf(taskId))[0];
        return run !== undefined && predicate(run);
      },
      { timeout: TIMEOUT, message: what },
    )
    .toBe(true);
  const run = (await runsOf(taskId))[0];
  if (run === undefined) {
    throw new Error(what);
  }
  return run;
}

export const startButton = (page: Page): Locator =>
  page.getByRole("button", { name: /^(开始执行|重新执行)$/ });
export const statusLine = (page: Page): Locator => page.locator(".task-detail__status");
export const infoStrip = (page: Page): Locator => page.locator(".run-info");

export async function openTask(page: Page, task: TaskRef): Promise<void> {
  await page.goto(task.url);
  await expect(page.getByRole("heading", { level: 1, name: task.title })).toBeVisible();
}

/** 在任务详情页点「开始执行」。 */
export async function startRunUi(page: Page): Promise<void> {
  await startButton(page).first().click();
  await expect(infoStrip(page)).toBeVisible();
}

/** 全局准备写在环境变量里的临时根目录（数据库就在这里）。 */
export function e2eRoot(): string {
  return dirname(process.env.E2E_WORKSPACE ?? "");
}

/** 直接改桥接的数据库（只用来制造产品界面无法制造的状态，如已归档但仍被绑定的 Harness）。 */
export function sqlite(statement: string): void {
  execFileSync("sqlite3", [join(e2eRoot(), "teamspace.db"), statement], { stdio: "ignore" });
}
