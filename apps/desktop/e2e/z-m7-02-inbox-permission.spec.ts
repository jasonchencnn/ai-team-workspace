// M7-02：待我处理页的「待答复权限」可就地允许 / 拒绝，不必跳到任务详情。
// 真实界面 + 测试桥接 + 真实内核；Harness 是内置 Mock ACP Agent。
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { type Page, expect, test } from "@playwright/test";

import {
  type TaskRef,
  type TeamSetup,
  TIMEOUT,
  bind,
  createTaskApi,
  invoke,
  setupMockHarness,
  setupTeam,
  waitLatest,
} from "./z-qa-r2-helpers.js";

test.describe.configure({ mode: "serial", timeout: 120_000 });

const HARNESS = "m7-02-mock";
const ROLE = "待答复员";

let workspace = "";
let team: TeamSetup;

test.beforeAll(async () => {
  workspace = mkdtempSync(join(tmpdir(), "teamspace-e2e-m7-02-"));
  writeFileSync(join(workspace, "brief.txt"), "需求说明\n");
  team = await setupTeam("M7-02 待答复团队", workspace, [{ name: ROLE, prefix: "M7" }]);
  const harness = await setupMockHarness(team.teamId, HARNESS);
  await bind(team.roleIds[ROLE] ?? "", harness);
});

test.afterAll(() => {
  rmSync(workspace, { recursive: true, force: true });
});

/** 建一个会申请命令权限的任务，并开始执行到「等待你确认」。 */
async function startWaitingPermission(title: string): Promise<TaskRef> {
  const task = await createTaskApi({
    teamId: team.teamId,
    roleId: team.roleIds[ROLE] ?? "",
    title,
  });
  await invoke("start_run", { taskId: task.id });
  await waitLatest(task.id, (run) => run.status === "waiting_permission", "执行等待权限确认");
  return task;
}

/** 打开待我处理页并选中权限类事项。 */
async function openPermissionItem(page: Page, task: TaskRef): Promise<void> {
  await page.goto(`/#/teams/${team.teamId}/inbox`);
  const group = page.locator(".inbox__group", { hasText: "待答复权限" });
  await expect(group).toContainText(task.key);
  await group.locator(".inbox__item-main").first().click();
  await expect(page.locator(".perm-approve")).toBeVisible();
}

test("待答复权限：审批卡就地显示动作与范围，可直接允许，执行继续", async ({ page }) => {
  const task = await startWaitingPermission("就地允许 [mock:perm]");

  await openPermissionItem(page, task);

  const card = page.locator(".perm-approve");
  await expect(card).toContainText("权限请求待答复");
  await expect(card).toContainText(`请求来自：${ROLE}`);
  await expect(card).toContainText(task.key);
  // 动作与范围：命令、理由都摆出来
  await expect(card).toContainText("请求动作与范围");
  await expect(card).toContainText("echo hello");
  await expect(card).toContainText("执行命令需要确认");
  await expect(card.getByRole("button", { name: "允许", exact: true })).toBeVisible();
  await expect(card.getByRole("button", { name: "拒绝", exact: true })).toBeVisible();

  await card.getByRole("button", { name: "允许", exact: true }).click();

  // 执行继续并跑完
  await waitLatest(
    task.id,
    (run) => !["queued", "running", "waiting_permission"].includes(run.status),
    "执行结束",
  );
  // 答复完的事项不再出现在待我处理
  await expect(page.locator(".inbox__group", { hasText: "待答复权限" })).toHaveCount(0, {
    timeout: TIMEOUT,
  });
});

test("待答复权限：就地拒绝，执行收到拒绝结果", async ({ page }) => {
  const task = await startWaitingPermission("就地拒绝 [mock:perm]");

  await openPermissionItem(page, task);
  await page.locator(".perm-approve").getByRole("button", { name: "拒绝", exact: true }).click();

  await waitLatest(
    task.id,
    (run) => !["queued", "running", "waiting_permission"].includes(run.status),
    "执行结束",
  );
  await expect(page.locator(".inbox__group", { hasText: "待答复权限" })).toHaveCount(0, {
    timeout: TIMEOUT,
  });
});
