// M6-03 端到端用例：更换 Harness 流程、换绑待生效与撤销、换绑期间待投递消息、启动包预览。
//
// 走真实界面 + 测试桥接（真实内核）+ 内置 Mock ACP Agent：
// 任务标题里的 [mock:hang] 标记决定 Mock 的行为（发一条消息后一直等待，直到被取消）。
// 页面上的状态全部来自后端：待生效、已生效、投递中都以接口返回为准。
import { expect, test, type Page } from "@playwright/test";

import {
  bind,
  createTaskApi,
  invoke,
  setupMockHarness,
  setupTeam,
  type TaskRef,
} from "./z-qa-r2-helpers.js";

const TEAM = "换绑团队";
const ROLE = "换绑员";
const HARNESS_OLD = "m6-mock-old";
const HARNESS_NEW = "m6-mock-new";
const TIMEOUT = 30_000;

let teamId = "";
let roleId = "";
let harnessOld = "";
let harnessNew = "";

interface RunSnapshot {
  id: string;
  taskId: string;
  attempt: number;
  status: string;
  harness: { id: string; name: string };
}

interface RoleSnapshot {
  role: { id: string; name: string };
  harness: { id: string; name: string } | null;
  pendingBinding: { id: string; harnessName: string; mode: string } | null;
}

interface BoardSnapshot {
  columns: { status: string; tasks: { id: string; owner: { id: string } }[] }[];
}

/** 岗位当前的绑定与待生效换绑。 */
async function roleSnapshot(): Promise<RoleSnapshot> {
  const views = await invoke<RoleSnapshot[]>("list_roles", { teamId });
  const view = views.find((item) => item.role.id === roleId);
  if (view === undefined) {
    throw new Error(`找不到岗位 ${ROLE}`);
  }
  return view;
}

/** 取消岗位还在跑的执行，避免用例之间互相影响。 */
async function cancelActiveRuns(): Promise<void> {
  const board = await invoke<BoardSnapshot>("get_task_board", { input: { teamId } });
  for (const column of board.columns) {
    if (column.status !== "doing") {
      continue;
    }
    for (const task of column.tasks) {
      if (task.owner.id !== roleId) {
        continue;
      }
      for (const run of await runSnapshots(task.id)) {
        if (["queued", "running", "waiting_permission"].includes(run.status)) {
          await invoke("cancel_run", { runId: run.id });
        }
      }
    }
  }
}

/** 任务的执行记录（带 Harness 快照），最近的在前。 */
async function runSnapshots(taskId: string): Promise<RunSnapshot[]> {
  return await invoke<RunSnapshot[]>("list_runs", { taskId });
}

/** 把岗位恢复到「绑定旧实例、没有待生效换绑、没有在跑的执行」的起点。 */
async function resetRole(): Promise<void> {
  const view = await roleSnapshot();
  if (view.pendingBinding !== null) {
    await invoke("cancel_pending_rebind", { roleId });
  }
  await cancelActiveRuns();
  await bind(roleId, harnessOld);
}

/** 打开岗位页并选中岗位。 */
async function openRole(page: Page): Promise<void> {
  await page.goto(`/#/teams/${teamId}/roles`);
  await page.locator(".role-chip", { hasText: ROLE }).click();
  await expect(page.getByRole("heading", { level: 2, name: ROLE })).toBeVisible();
}

/** 打开「更换 Harness」弹窗并选好目标实例。 */
async function openRebindDialog(page: Page, target: string): Promise<void> {
  await page.getByRole("button", { name: "更换 Harness", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  const select = dialog.getByLabel("选择新的 Harness 实例");
  const value = await select.locator("option", { hasText: target }).getAttribute("value");
  expect(value).toBeTruthy();
  await select.selectOption(value ?? "");
}

/** 建一个挂着长执行的任务并启动它，返回任务与执行。 */
async function startHangingTask(
  page: Page,
  title: string,
): Promise<{
  task: TaskRef;
  run: RunSnapshot;
}> {
  const task = await createTaskApi({
    teamId,
    roleId,
    title,
    refs: ["README.md"],
    status: "ready",
  });
  await page.goto(task.url);
  await page.getByRole("button", { name: /^开始执行$/ }).click();
  await expect(page.locator(".run-progress")).toBeVisible();
  await expect
    .poll(async () => (await runSnapshots(task.id))[0]?.status, { timeout: TIMEOUT })
    .toBe("running");
  const run = (await runSnapshots(task.id))[0] as RunSnapshot;
  return { task, run };
}

test.describe.configure({ mode: "serial", timeout: 240_000 });

test.beforeAll(async () => {
  const setup = await setupTeam(TEAM, process.env.E2E_WORKSPACE ?? "", [
    { name: ROLE, prefix: "M6" },
  ]);
  teamId = setup.teamId;
  roleId = setup.roleIds[ROLE] ?? "";
  expect(roleId).not.toBe("");
  harnessOld = await setupMockHarness(teamId, HARNESS_OLD);
  harnessNew = await setupMockHarness(teamId, HARNESS_NEW);
  await bind(roleId, harnessOld);
});

test.afterAll(async () => {
  await cancelActiveRuns();
});

test.beforeEach(async () => {
  await resetRole();
});

test("没有进行中的执行：确认后立即换绑，岗位身份与历史不变", async ({ page }) => {
  await openRole(page);
  await expect(page.getByText(`当前绑定 ${HARNESS_OLD}`)).toBeVisible();

  await openRebindDialog(page, HARNESS_NEW);
  const dialog = page.getByRole("dialog");
  // 没有进行中的执行：说明会立即生效，不给方式选择。
  await expect(dialog.getByText("该岗位当前没有进行中的执行，确认后立即换绑。")).toBeVisible();
  await expect(dialog.getByRole("radio")).toHaveCount(0);
  await expect(dialog.getByText(`${HARNESS_OLD} → ${HARNESS_NEW}`)).toBeVisible();

  await dialog.getByRole("button", { name: "确认更换" }).click();

  // 立即生效：没有待生效卡片，当前绑定已经换成新实例。
  await expect(page.getByText(`当前绑定 ${HARNESS_NEW}`)).toBeVisible();
  await expect(page.getByText("换绑待生效")).toHaveCount(0);
  const view = await roleSnapshot();
  expect(view.harness?.name).toBe(HARNESS_NEW);
  expect(view.harness?.id).toBe(harnessNew);
  expect(view.pendingBinding).toBeNull();
});

test("执行进行中：选「当前任务完成后更换」→ 显示待生效，执行结束后才生效", async ({ page }) => {
  const { task, run } = await startHangingTask(page, "长任务待生效 [mock:hang]");

  await openRole(page);
  await openRebindDialog(page, HARNESS_NEW);
  const dialog = page.getByRole("dialog");

  // 有进行中的执行：显示这条执行对应的任务，并给出三种方式。
  await expect(dialog.getByText("有 1 个任务正在执行")).toBeVisible();
  await expect(dialog.getByText(task.key)).toBeVisible();
  await expect(dialog.getByText("长任务待生效 [mock:hang]")).toBeVisible();
  await expect(dialog.getByRole("radio", { name: /当前任务完成后更换/ })).toBeChecked();
  await expect(
    dialog.getByRole("radio", { name: /取消当前执行，由新 Harness 重新执行/ }),
  ).toBeVisible();
  await expect(dialog.getByRole("radio", { name: /立即人工接管，暂不自动执行/ })).toBeVisible();
  // 每种方式都写清后果。
  await expect(
    dialog.getByText("停止领取新任务，当前执行可以继续；新绑定在当前执行结束后生效。").first(),
  ).toBeVisible();

  await dialog.getByRole("button", { name: "确认更换" }).click();

  // 待生效：写明旧 → 新、方式与发起时间；当前绑定仍然是旧的，绝不显示为已生效。
  const pending = page.locator(".pending-binding");
  await expect(pending).toBeVisible();
  await expect(pending).toContainText(`待生效：${HARNESS_OLD} → ${HARNESS_NEW}`);
  await expect(pending).toContainText("当前执行完成后更换");
  await expect(page.getByText(`当前绑定 ${HARNESS_OLD}`)).toBeVisible();

  // 团队席位与岗位导航也标出待生效。
  await page.goto(`/#/teams/${teamId}`);
  await expect(page.locator(".th-role__pending")).toContainText(
    `待生效：${HARNESS_OLD} → ${HARNESS_NEW}`,
  );
  await page.goto(`/#/teams/${teamId}/roles`);
  await expect(page.locator(".role-chip__pending").first()).toHaveText("待生效");

  // 执行结束后自动生效：页面靠通知事件刷新，不用手动重载。
  await invoke("cancel_run", { runId: run.id });
  await expect(page.getByText(`当前绑定 ${HARNESS_NEW}`)).toBeVisible({ timeout: TIMEOUT });
  await expect(page.getByText("换绑待生效")).toHaveCount(0);
  const view = await roleSnapshot();
  expect(view.harness?.name).toBe(HARNESS_NEW);
  expect(view.pendingBinding).toBeNull();
});

test("待生效可以撤销：当前绑定保持不变", async ({ page }) => {
  const { run } = await startHangingTask(page, "长任务待撤销 [mock:hang]");

  await openRole(page);
  await openRebindDialog(page, HARNESS_NEW);
  await page.getByRole("dialog").getByRole("button", { name: "确认更换" }).click();
  await expect(page.locator(".pending-binding")).toBeVisible();

  await page.getByRole("button", { name: "撤销换绑" }).click();

  await expect(page.locator(".pending-binding")).toHaveCount(0);
  await expect(page.getByText(`当前绑定 ${HARNESS_OLD}`)).toBeVisible();
  const view = await roleSnapshot();
  expect(view.harness?.name).toBe(HARNESS_OLD);
  expect(view.pendingBinding).toBeNull();

  await invoke("cancel_run", { runId: run.id });
});

test("选「取消当前执行，由新 Harness 重新执行」：任务在新 Harness 上重新执行", async ({ page }) => {
  const { task, run } = await startHangingTask(page, "长任务取消重跑 [mock:hang]");
  expect(run.harness.name).toBe(HARNESS_OLD);

  await openRole(page);
  await openRebindDialog(page, HARNESS_NEW);
  const dialog = page.getByRole("dialog");
  await dialog.getByRole("radio", { name: /取消当前执行，由新 Harness 重新执行/ }).check();
  await dialog.getByRole("button", { name: "确认更换" }).click();

  // 立即换绑：没有待生效卡片。
  await expect(page.getByText(`当前绑定 ${HARNESS_NEW}`)).toBeVisible();
  await expect(page.getByText("换绑待生效")).toHaveCount(0);

  // 原执行被取消，任务在新 Harness 上重新开始执行。
  await expect
    .poll(async () => (await runSnapshots(task.id)).length, { timeout: TIMEOUT })
    .toBeGreaterThanOrEqual(2);
  const runs = await runSnapshots(task.id);
  const first = runs.find((item) => item.attempt === 1);
  const second = runs.find((item) => item.attempt === 2);
  expect(first?.status).toBe("cancelled");
  expect(first?.harness.name).toBe(HARNESS_OLD);
  expect(second?.harness.name).toBe(HARNESS_NEW);
  expect(["queued", "running"]).toContain(second?.status);
});

test("换绑期间待投递的消息列出来，并注明入队时的绑定", async ({ page }) => {
  // 发一条让 Mock 一直等待的消息：状态停在「投递中」。
  await invoke("send_message", {
    input: {
      teamId,
      toRoleId: roleId,
      body: "换绑期间的待投递消息 [mock:hang]",
      taskId: null,
    },
  });
  await expect
    .poll(
      async () => {
        const threads = await invoke<{ id: string; lastStatus: string }[]>("list_threads", {
          input: { teamId, roleId },
        });
        return threads.some((thread) => thread.lastStatus === "delivering");
      },
      { timeout: TIMEOUT },
    )
    .toBe(true);

  const { run } = await startHangingTask(page, "长任务待投递 [mock:hang]");

  await openRole(page);
  await openRebindDialog(page, HARNESS_NEW);
  await page.getByRole("dialog").getByRole("button", { name: "确认更换" }).click();
  await expect(page.locator(".pending-binding")).toBeVisible();

  // 待投递消息列表：说明入队时的绑定，并说明会按生效后的绑定投递。
  const panel = page.locator(".pending-messages");
  await expect(panel).toBeVisible();
  await expect(panel.getByText(`入队时绑定：${HARNESS_OLD}，将按生效后的绑定投递`)).toBeVisible();
  await expect(panel.locator(".ui-tag").first()).toHaveText("投递中");

  // 能跳到岗位通信里的原线程。
  await panel.getByRole("link", { name: "查看岗位通信线程" }).first().click();
  await expect(page).toHaveURL(new RegExp(`/teams/${teamId}/messages\\?`));
  await expect(page.getByText("换绑期间的待投递消息 [mock:hang]").first()).toBeVisible();

  await invoke("cancel_run", { runId: run.id });
});

test("启动包预览：当前实例已执行过时提示不会附加交接，换到新实例后有交接内容", async ({ page }) => {
  // 先让当前实例在这个岗位执行过一次：交接内容不再附加。
  const { run } = await startHangingTask(page, "长任务交接 [mock:hang]");
  await invoke("cancel_run", { runId: run.id });

  await openRole(page);
  const panel = page.locator(".handoff-preview");
  await expect(panel).toBeVisible();
  await expect(panel.getByText("当前 Harness 已在此岗位执行过，不会附加交接")).toBeVisible();
  // 默认收起，展开后还是这段说明。
  await expect(panel.locator(".handoff-preview__body")).toHaveCount(0);
  await panel.getByRole("button", { name: "展开" }).click();
  await expect(panel.locator(".handoff-preview__body")).toContainText("不会附加交接");

  // 换到没在这个岗位执行过的新实例：有交接内容。
  await openRebindDialog(page, HARNESS_NEW);
  await page.getByRole("dialog").getByRole("button", { name: "确认更换" }).click();
  await expect(page.getByText(`当前绑定 ${HARNESS_NEW}`)).toBeVisible();
  await expect(panel.getByText("当前 Harness 已在此岗位执行过，不会附加交接")).toHaveCount(0);
});
