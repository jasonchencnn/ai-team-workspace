// 测试轮次 2026-10-14 · QA R2：真实 Harness 冒烟。
// - RUN-SMK-001、RUN-SMK-002：MiniMax Code（mcode acp），需要 TEAMSPACE_REAL_SMOKE=1。
// - RUN-SMK-003：Claude Code（claude-code-acp），单独开关 TEAMSPACE_REAL_SMOKE_CLAUDE=1，消耗 Claude 额度。
// 没有对应环境变量时整组跳过，不调用真实 Harness、不消耗额度。前提：本机已安装并登录对应 CLI。
// 运行命令（会消耗额度，只在需要时执行）：
//   TEAMSPACE_REAL_SMOKE=1 TEAMSPACE_E2E_PORT_BASE=15781 \
//     pnpm --dir apps/desktop exec playwright test e2e/z-qa-r2-real-smoke.spec.ts -g "RUN-SMK-00[12]"
//   TEAMSPACE_REAL_SMOKE_CLAUDE=1 TEAMSPACE_E2E_PORT_BASE=15781 \
//     pnpm --dir apps/desktop exec playwright test e2e/z-qa-r2-real-smoke.spec.ts -g "RUN-SMK-003"
// 工作空间用临时普通文件夹，不碰仓库；任务编号由内核生成（岗位前缀 FE / CL），不是用例里写的编号。
import { mkdtempSync, existsSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { type Page, expect, test } from "@playwright/test";

import { acpEntry } from "./harnessHelpers.js";
import {
  type RunInfo,
  type TaskRef,
  type TeamSetup,
  TIMEOUT,
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
const REAL_CLAUDE = process.env.TEAMSPACE_REAL_SMOKE_CLAUDE === "1";

const ROLE = "前端开发";
/** 执行仍在进行中的状态。 */
const ACTIVE_STATUSES = ["queued", "running", "waiting_permission"];
/** MiniMax 一次真实执行的等待上限（毫秒）。 */
const RUN_LIMIT = 480_000;
/** 权限弹窗里的原因文案（写入岗位文件夹之外、或执行命令）。 */
const PERM_REASON =
  /写入本岗位文件夹与任务引用文件之外需要确认|读取工作空间之外的文件需要确认|执行命令需要确认/;

function journalOf(root: string, role: string): string {
  return readFileSync(join(root, "Teamspace", role, "工作日志.md"), "utf8");
}

/**
 * 一直处理权限弹窗（点「允许一次」）直到执行结束。
 * `maxApprovals` 为弹窗次数上限，超过仍出现弹窗即失败。返回结束时的执行与实际点击次数。
 */
async function approveUntilDone(
  page: Page,
  task: TaskRef,
  options: { limit: number; maxApprovals?: number },
): Promise<{ run: RunInfo; approvals: number }> {
  const deadline = Date.now() + options.limit;
  const maxApprovals = options.maxApprovals ?? Number.POSITIVE_INFINITY;
  const allow = page.getByRole("dialog").getByRole("button", { name: "允许一次" });
  let approvals = 0;
  for (;;) {
    const run = (await runsOf(task.id))[0];
    if (run !== undefined && !ACTIVE_STATUSES.includes(run.status)) {
      return { run, approvals };
    }
    if (await allow.isVisible()) {
      if (approvals >= maxApprovals) {
        throw new Error(`权限弹窗超过 ${String(maxApprovals)} 次，执行仍未结束`);
      }
      await allow.click();
      approvals += 1;
    }
    expect(Date.now(), "真实执行超时").toBeLessThan(deadline);
    await page.waitForTimeout(1_000);
  }
}

/**
 * 同时盯着权限弹窗与执行结束（每 500ms 各查一次），先出现的那个返回。不点击弹窗。
 */
async function waitPermissionOrEnd(
  page: Page,
  task: TaskRef,
  limit: number,
): Promise<"permission" | "ended"> {
  const deadline = Date.now() + limit;
  const allow = page.getByRole("dialog").getByRole("button", { name: "允许一次" });
  for (;;) {
    if (await allow.isVisible()) {
      return "permission";
    }
    const run = (await runsOf(task.id))[0];
    if (run !== undefined && !ACTIVE_STATUSES.includes(run.status)) {
      return "ended";
    }
    expect(Date.now(), "真实执行超时").toBeLessThan(deadline);
    await page.waitForTimeout(500);
  }
}

/**
 * 范围外改动（M2.1-03）的断言：执行成功但任务受阻，警示框与原因写明该文件，
 * 改动文件列表里该文件带「范围外」标签，后端清单与工作日志都有记录。
 */
async function expectOutOfScopeBlocked(
  page: Page,
  run: RunInfo,
  file: string,
  workspace: string,
): Promise<void> {
  expect(run.status, `执行未成功：${run.failure ?? ""}`).toBe("succeeded");
  expect(run.outOfScope, `${file} 未被标为范围外改动`).toContain(file);
  expect(run.changedFiles.map((f) => f.path)).toContain(file);

  const success = page.locator(".run-outcome--success");
  await expect(success).toBeVisible({ timeout: TIMEOUT });
  await expect(success).toContainText("已完成 · 受阻");
  await expect(success.locator(".run-outcome__alert--scope")).toContainText(
    "Agent 未经确认改动了范围外的文件",
  );
  await expect(success.locator(".run-outcome__alert--scope")).toContainText(file);
  await expect(statusLine(page)).toContainText("受阻");
  await expect(
    page.getByText(`Agent 未经确认改动了权限范围外的文件：${file}`).first(),
  ).toBeVisible();
  await expect(success.getByRole("button", { name: "去验收" })).toHaveCount(0);

  await success.getByRole("button", { name: "查看改动" }).click();
  await expect(success.locator(".run-outcome__files .run-file", { hasText: file })).toContainText(
    "范围外",
  );

  await expect
    .poll(() => journalOf(workspace, ROLE), { timeout: TIMEOUT })
    .toContain(`Agent 未经确认改动了范围外的文件：${file}`);
}

test.describe("真实冒烟（MiniMax Code，需要 TEAMSPACE_REAL_SMOKE=1）", () => {
  test.skip(!REAL, "未设置 TEAMSPACE_REAL_SMOKE=1：不调用真实 Harness，不消耗额度");
  test.describe.configure({ mode: "serial", timeout: 600_000 });

  const HARNESS = "MiniMax Code";

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

    const { run } = await approveUntilDone(page, task, { limit: RUN_LIMIT });
    expect(run.status, `执行未成功：${run.failure ?? ""}`).toBe("succeeded");
    await expect(page.locator(".run-outcome--success")).toBeVisible();
    await expect(statusLine(page)).toContainText("待验收");

    const file = join(workspace, "Teamspace", ROLE, "过程文件", "hello.txt");
    expect(existsSync(file), `没有生成 ${file}`).toBe(true);
    expect(readFileSync(file, "utf8")).toContain("Hello MiniMax");
    await expect
      .poll(() => journalOf(workspace, ROLE), { timeout: 30_000 })
      .toContain(`${task.key} · 第 1 次执行完成（${HARNESS}）`);
    expect(run.summary ?? "").not.toBe("");
  });

  test("RUN-SMK-002 MiniMax Code 写入工作空间根目录：弹窗允许后成功，或范围外改动转为受阻（二选一合规）", async ({
    page,
  }) => {
    const file = "output.txt";
    const task = await createTaskApi({
      teamId: team.teamId,
      roleId: team.roleIds[ROLE] ?? "",
      title: "探测命令",
      description:
        "在工作空间根目录（不是岗位文件夹）创建 output.txt，内容为 Smoke Testing；可以用 shell 命令完成。",
    });
    await openTask(page, task);
    await startRunUi(page);

    const signal = await waitPermissionOrEnd(page, task, RUN_LIMIT);

    if (signal === "permission") {
      // 路径 (a)：出现权限弹窗，说明原因，点「允许一次」后继续。
      test.info().annotations.push({ type: "smoke-path", description: "(a) 出现权限弹窗" });
      await expect(page.getByRole("dialog").locator(".perm-reason__text")).toContainText(
        PERM_REASON,
      );
      const { run, approvals } = await approveUntilDone(page, task, { limit: RUN_LIMIT });
      test.info().annotations.push({
        type: "permission approvals",
        description: String(approvals),
      });

      if (run.outOfScope.length === 0) {
        // (a1) 允许后执行成功，产出文件并进入待验收。
        test
          .info()
          .annotations.push({ type: "smoke-path-detail", description: "(a1) 允许后待验收" });
        expect(run.status, `执行未成功：${run.failure ?? ""}`).toBe("succeeded");
        expect(existsSync(join(workspace, file)), `没有生成 ${file}`).toBe(true);
        await expect(statusLine(page)).toContainText("待验收");
      } else {
        // (a2) 允许后仍有范围外改动，转为受阻。
        test.info().annotations.push({
          type: "smoke-path-detail",
          description: "(a2) 允许后因范围外改动受阻",
        });
        await expectOutOfScopeBlocked(page, run, file, workspace);
      }

      const decisions = (await updatesOf(run.id)).filter(
        (u) => u.body.type === "permission" && u.body.decision?.by === "human",
      );
      expect(decisions.length).toBeGreaterThan(0);
      expect(decisions[0]?.body.decision?.kind).toBe("allow_once");
    } else {
      // 路径 (b)：没有弹窗，执行直接结束。只有「范围外改动被标出、任务受阻」才合规；
      // 没有弹窗又进入待验收、且改动列表里没有该文件，判失败。
      test.info().annotations.push({ type: "smoke-path", description: "(b) 无弹窗，执行结束" });
      const run = (await runsOf(task.id))[0] as RunInfo;
      expect(
        run.outOfScope,
        `不合规：没有权限弹窗，且 ${file} 未被标为范围外改动（不应直接待验收）`,
      ).toContain(file);
      await expectOutOfScopeBlocked(page, run, file, workspace);
    }
  });
});

test.describe("真实冒烟（Claude Code，需要 TEAMSPACE_REAL_SMOKE_CLAUDE=1）", () => {
  test.skip(
    !REAL_CLAUDE,
    "未设置 TEAMSPACE_REAL_SMOKE_CLAUDE=1：不调用 Claude Code，不消耗 Claude 额度",
  );
  test.describe.configure({ mode: "serial", timeout: 480_000 });

  const HARNESS = "Claude Code 冒烟";
  /** 8 分钟总时限内，留出建接入、握手、绑定岗位的时间。 */
  const CLAUDE_LIMIT = 400_000;
  /** 默认模式下写入请求权限，弹窗可能出现多次；超过 5 次判失败。 */
  const MAX_APPROVALS = 5;

  let workspace = "";
  let team: TeamSetup;

  test.beforeAll(async () => {
    workspace = mkdtempSync(join(tmpdir(), "teamspace-claude-smoke-"));
    team = await setupTeam("Claude 冒烟团队", workspace, [{ name: ROLE, prefix: "CL" }]);
  });

  test.afterAll(() => {
    rmSync(workspace, { recursive: true, force: true });
  });

  test("RUN-SMK-003 Claude Code 真实执行：新建接入握手已连接、绑定岗位、权限弹窗允许后产出文件并待验收", async ({
    page,
  }) => {
    // 1) 新建 Claude Code 预设接入，点「保存并握手检查」，握手应为已连接。
    await page.goto(`/#/teams/${team.teamId}/roles?tab=harness`);
    await expect(page.getByRole("tab", { name: "Harness 接入" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await page.getByRole("button", { name: "新建接入" }).click();
    await page.locator(".acp-preset", { hasText: "Claude Code" }).click();
    await page.getByLabel("实例名称").fill(HARNESS);
    await page.getByRole("button", { name: "保存并握手检查" }).click();
    await expect(acpEntry(page, HARNESS), "Claude Code 接入未创建").toBeVisible({
      timeout: 120_000,
    });
    await expect(
      acpEntry(page, HARNESS),
      "Claude Code 握手未通过：请先安装 claude-code-acp 适配器并完成 claude 登录",
    ).toContainText("已连接", { timeout: 120_000 });

    // 2) 把岗位绑定到该接入。
    await page.goto(`/#/teams/${team.teamId}/roles`);
    await page.locator(".role-chip", { hasText: ROLE }).click();
    await page.getByRole("button", { name: "绑定 Harness", exact: true }).click();
    const select = page.getByLabel("选择本团队的 Harness 实例");
    await select.selectOption(
      (await select.locator("option", { hasText: HARNESS }).getAttribute("value")) ?? "",
    );
    await page.getByRole("button", { name: "确认绑定" }).click();
    await expect(page.getByText(`当前绑定 ${HARNESS}`)).toBeVisible();

    // 3) 建任务并开始执行，循环处理权限弹窗（最多 5 次）。
    const task = await createTaskApi({
      teamId: team.teamId,
      roleId: team.roleIds[ROLE] ?? "",
      title: "创建 hello.txt",
      description: "在岗位过程文件中创建 hello.txt，写入 Hello Claude",
    });
    await openTask(page, task);
    await startRunUi(page);
    await expect(infoStrip(page)).toContainText(HARNESS);

    const { run, approvals } = await approveUntilDone(page, task, {
      limit: CLAUDE_LIMIT,
      maxApprovals: MAX_APPROVALS,
    });
    test.info().annotations.push({ type: "permission approvals", description: String(approvals) });

    // 4) 结果：执行成功、待验收、文件内容正确、工作日志有完成条目。
    expect(run.status, `执行未成功：${run.failure ?? ""}`).toBe("succeeded");
    await expect(page.locator(".run-outcome--success")).toBeVisible();
    await expect(statusLine(page)).toContainText("待验收");

    const file = join(workspace, "Teamspace", ROLE, "过程文件", "hello.txt");
    expect(existsSync(file), `没有生成 ${file}`).toBe(true);
    expect(readFileSync(file, "utf8")).toContain("Hello Claude");
    await expect
      .poll(() => journalOf(workspace, ROLE), { timeout: 30_000 })
      .toContain(`${task.key} · 第 1 次执行完成（${HARNESS}）`);
    expect(run.summary ?? "").not.toBe("");
  });
});
