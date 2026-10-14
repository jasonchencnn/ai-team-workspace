// 测试轮次 2026-10-14 · QA R2：真实 Harness 冒烟（RUN-SMK-001、RUN-SMK-002）。
// 会调用本机的 MiniMax Code（命令 mcode，参数 acp）并消耗额度，所以默认整体跳过；
// 只有设置了 TEAMSPACE_REAL_SMOKE=1 才运行。前提：本机已安装 mcode 并完成登录。
// 运行命令：
//   TEAMSPACE_REAL_SMOKE=1 TEAMSPACE_E2E_PORT_BASE=15781 \
//     pnpm --dir apps/desktop exec playwright test e2e/z-qa-r2-real-smoke.spec.ts
// RUN-SMK-003（Claude Code）未获 Human 同意，不在此实现。
// 工作空间用临时普通文件夹，不碰仓库；任务编号由内核生成（岗位前缀 FE），不是用例里写的 FE-SMOKE-01。
import { mkdtempSync, existsSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { type Page, expect, test } from "@playwright/test";

import {
  type RunInfo,
  type TaskRef,
  type TeamSetup,
  bind,
  createTaskApi,
  infoStrip,
  invoke,
  openTask,
  runsOf,
  setupTeam,
  startRunUi,
  statusLine,
  updatesOf,
} from "./z-qa-r2-helpers.js";

const REAL = process.env.TEAMSPACE_REAL_SMOKE === "1";

test.describe("真实冒烟（MiniMax Code，需要 TEAMSPACE_REAL_SMOKE=1）", () => {
  test.skip(!REAL, "未设置 TEAMSPACE_REAL_SMOKE=1：不调用真实 Harness，不消耗额度");
  test.describe.configure({ mode: "serial", timeout: 600_000 });

  const ROLE = "前端开发";
  const HARNESS = "MiniMax Code";
  const RUN_LIMIT = 480_000;

  let workspace = "";
  let team: TeamSetup;

  test.beforeAll(async () => {
    workspace = mkdtempSync(join(tmpdir(), "teamspace-real-smoke-"));
    team = await setupTeam("真实冒烟团队", workspace, [{ name: ROLE, prefix: "FE" }]);
    const harness = await invoke<{ id: string }>("create_harness", {
      input: { teamId: team.teamId, name: HARNESS, preset: "minimax_code" },
    });
    const checked = await invoke<{ handshake: { status: string; detail: string | null } }>(
      "check_harness",
      { harnessId: harness.id },
    );
    expect(
      checked.handshake.status,
      `MiniMax Code 握手未通过（${checked.handshake.detail ?? "无说明"}）：请先在终端安装 mcode 并登录`,
    ).toBe("connected");
    await bind(team.roleIds[ROLE] ?? "", harness.id);
  });

  test.afterAll(() => {
    rmSync(workspace, { recursive: true, force: true });
  });

  /** 一直处理权限弹窗（点「允许一次」）直到执行结束；返回结束时的执行。 */
  async function approveUntilDone(page: Page, task: TaskRef): Promise<RunInfo> {
    const deadline = Date.now() + RUN_LIMIT;
    for (;;) {
      const run = (await runsOf(task.id))[0];
      if (run !== undefined && !["queued", "running", "waiting_permission"].includes(run.status)) {
        return run;
      }
      const dialog = page.getByRole("dialog");
      if (await dialog.getByRole("button", { name: "允许一次" }).isVisible()) {
        await dialog.getByRole("button", { name: "允许一次" }).click();
      }
      expect(Date.now(), "真实执行超时").toBeLessThan(deadline);
      await page.waitForTimeout(1_000);
    }
  }

  test("RUN-SMK-001 MiniMax Code 真实执行简单任务：产出文件、待验收、岗位日志有记录", async ({
    page,
  }) => {
    const task = await createTaskApi({
      teamId: team.teamId,
      roleId: team.roleIds[ROLE] ?? "",
      title: "创建问候组件",
      description: "在岗位过程文件中创建一个 hello.txt，写入 Hello MiniMax",
    });
    await openTask(page, task);
    await startRunUi(page);
    await expect(infoStrip(page)).toContainText(HARNESS);
    // 实时进度流：Agent 的消息回传
    await expect(page.locator(".run-msg--agent").first()).toBeVisible({ timeout: 120_000 });

    const run = await approveUntilDone(page, task);
    expect(run.status, `执行未成功：${run.failure ?? ""}`).toBe("succeeded");
    await expect(page.locator(".run-outcome--success")).toBeVisible();
    await expect(statusLine(page)).toContainText("待验收");

    const file = join(workspace, "Teamspace", ROLE, "过程文件", "hello.txt");
    expect(existsSync(file), `没有生成 ${file}`).toBe(true);
    expect(readFileSync(file, "utf8")).toContain("Hello MiniMax");
    await expect
      .poll(() => readFileSync(join(workspace, "Teamspace", ROLE, "工作日志.md"), "utf8"), {
        timeout: 30_000,
      })
      .toContain(`${task.key} · 第 1 次执行完成（${HARNESS}）`);
    expect(run.summary ?? "").not.toBe("");
  });

  test("RUN-SMK-002 MiniMax Code 触发权限请求：弹窗说明原因，允许后继续并成功", async ({
    page,
  }) => {
    const task = await createTaskApi({
      teamId: team.teamId,
      roleId: team.roleIds[ROLE] ?? "",
      title: "探测命令",
      description:
        "在工作空间根目录（不是岗位文件夹）创建 output.txt，内容为 Smoke Testing；可以用 shell 命令完成。",
    });
    await openTask(page, task);
    await startRunUi(page);

    // 越出岗位文件夹的写入或执行命令，都要 Human 确认：弹窗自动出现，原因写明
    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible({ timeout: 300_000 });
    await expect(dialog.locator(".perm-reason__text")).toContainText(
      /写入本岗位文件夹与任务引用文件之外需要确认|执行命令需要确认/,
    );
    await dialog.getByRole("button", { name: "允许一次" }).click();

    const run = await approveUntilDone(page, task);
    expect(run.status, `执行未成功：${run.failure ?? ""}`).toBe("succeeded");
    expect(existsSync(join(workspace, "output.txt")), "没有生成 output.txt").toBe(true);
    const decisions = (await updatesOf(run.id)).filter(
      (u) => u.body.type === "permission" && u.body.decision?.by === "human",
    );
    expect(decisions.length).toBeGreaterThan(0);
    expect(decisions[0]?.body.decision?.kind).toBe("allow_once");
  });
});
