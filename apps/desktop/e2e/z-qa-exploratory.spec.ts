// 端到端探索性测试：覆盖未测边界场景、异常拒绝与多团队隔离。
// 真实界面 + 测试桥接 + 真实内核 + 临时 Git 仓库。
import { mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { type Locator, type Page, expect, test } from "@playwright/test";

import { acpEntry, createMockHarness } from "./harnessHelpers.js";
import { tickTaskBoardBox } from "./taskBoardHelpers.js";

const artifacts = fileURLToPath(new URL("../e2e-artifacts/", import.meta.url));
mkdirSync(artifacts, { recursive: true });

async function qaShot(page: Page, filename: string, focus?: Locator): Promise<void> {
  if (focus !== undefined) {
    await focus.scrollIntoViewIfNeeded();
  }
  await page.screenshot({ path: `${artifacts}${filename}.png`, fullPage: false });
}

let baseTeamId = "";

/** 保证团队中常驻一项基线活动任务，使得看板工具栏（包含各过滤器）常态保持可见 */
async function ensureBaselineTask(page: Page): Promise<void> {
  await gotoTeam(page, "/tasks");
  const baselineCard = page.locator("a.task-card-link", { hasText: "基线常规待办任务" });
  if ((await baselineCard.count()) === 0) {
    await gotoTeam(page, "/tasks/new");
    await page.getByLabel("任务名称").fill("基线常规待办任务");
    const ownerSelect = page.getByLabel("负责人岗位");
    const ownerVal = await ownerSelect
      .locator("option", { hasText: "Leader" })
      .getAttribute("value");
    await ownerSelect.selectOption(ownerVal ?? "");
    await page.locator("#task-file-search").fill("README.md");
    await page.getByRole("button", { name: "添加", exact: true }).click();
    await page.getByRole("button", { name: "创建并分配" }).click();
    await expect(page.getByRole("heading", { name: "基线常规待办任务" })).toBeVisible();
  }
}

/** 创建团队页：填名称、选工作空间、可选从示例添加岗位；只填表，不提交。 */
async function fillCreateTeam(
  page: Page,
  input: { name: string; description?: string; examples?: string[] },
): Promise<void> {
  await page.goto("/#/teams/new");
  await page.getByLabel("团队名称").fill(input.name);
  if (input.description !== undefined) {
    await page.getByLabel("说明").first().fill(input.description);
  }
  await page.getByRole("button", { name: "选择工作空间" }).click();
  await expect(page.getByText(/Git 仓库：main @ [0-9a-f]{7}/)).toBeVisible();
  const examples = input.examples ?? [];
  if (examples.length > 0) {
    await page.getByRole("button", { name: "从示例添加" }).click();
    for (const example of examples) {
      await page
        .locator(".tc-example", { hasText: example })
        .getByRole("button", { name: "添加" })
        .click();
    }
  }
}

/** 创建团队并等待落到团队空间（`/teams/<id>`），返回团队 id。 */
async function createTeamViaUi(
  page: Page,
  input: { name: string; description?: string; examples?: string[] },
): Promise<string> {
  await fillCreateTeam(page, input);
  await page.getByRole("button", { name: "创建团队", exact: true }).click();
  await expect(page).toHaveURL(/\/teams\/(?!new$)[^/]+$/);
  await expect(page.getByRole("heading", { name: "团队空间" })).toBeVisible();
  return /\/teams\/([^/]+)/.exec(page.url())?.[1] ?? "";
}

/** 获取或进入基础团队，并保证基础团队存在且有基线数据。 */
async function gotoTeam(page: Page, path = ""): Promise<string> {
  if (baseTeamId !== "") {
    if (path !== "") {
      await page.goto(`/#/teams/${baseTeamId}${path}`);
    } else {
      await page.goto(`/#/teams/${baseTeamId}`);
    }
    return baseTeamId;
  }

  await page.goto("/");
  await page.waitForURL(/\/teams\/.+$/);
  if (page.url().endsWith("/teams/new")) {
    await createTeamViaUi(page, {
      name: "账号系统迭代",
      description: "端到端测试团队",
      examples: ["Leader", "前端开发", "QA 测试"],
    });
  }
  const id = /\/teams\/([^/]+)/.exec(page.url())?.[1] ?? "";
  expect(id).not.toBe("");
  baseTeamId = id;
  if (path !== "") {
    await page.goto(`/#/teams/${id}${path}`);
  }
  return id;
}

test.describe.configure({ mode: "serial" });

test("01. 新建接入：预设网格显示 12 个预设与本机检测标签", async ({ page }) => {
  await gotoTeam(page, "/roles?tab=harness");
  await expect(page.getByRole("tab", { name: "Harness 接入" })).toHaveAttribute(
    "aria-selected",
    "true",
  );

  // 保证基础 mock-local 存在，方便后续用例使用
  if ((await acpEntry(page, "mock-local").count()) === 0) {
    await createMockHarness(page, "mock-local");
  }

  await page.getByRole("button", { name: "新建接入" }).click();
  const grid = page.locator(".acp-preset-grid");
  await expect(grid).toBeVisible();
  await expect(grid.locator(".acp-preset")).toHaveCount(12);
  for (const label of [
    "Claude Code",
    "Codex",
    "MiniMax Code",
    "Gemini CLI",
    "CodeBuddy",
    "Qoder CLI",
    "Grok Build",
    "Qwen Code",
    "Kimi CLI",
    "OpenCode",
  ]) {
    await expect(grid.locator(".acp-preset", { hasText: label })).toContainText(
      /本机已检测到|未检测到/,
    );
  }
  await expect(grid.locator(".acp-preset", { hasText: "自定义 ACP Agent" })).toContainText(
    "自由配置",
  );
  await expect(grid.locator(".acp-preset", { hasText: "Mock" })).toContainText("内置");

  await qaShot(page, "qa-01-cli-detection", grid);
});

test("02. 归档 Harness：仍被岗位绑定时被拒绝，解绑后成功归档", async ({ page }) => {
  await gotoTeam(page, "/roles?tab=harness");

  // 1. 新建 Mock 实例 qa-mock-harness
  await createMockHarness(page, "qa-mock-harness");

  // 2. 绑定到 QA 测试岗位
  await page.getByRole("tab", { name: "岗位" }).click();
  await page.locator(".role-chip", { hasText: "QA 测试" }).click();
  await page.getByRole("button", { name: /绑定 Harness|更换 Harness/ }).click();
  const select = page.getByLabel("选择本团队的 Harness 实例");
  const harnessOption = await select
    .locator("option", { hasText: "qa-mock-harness" })
    .getAttribute("value");
  expect(harnessOption).toBeTruthy();
  await select.selectOption(harnessOption ?? "");
  await page.getByRole("button", { name: "确认绑定" }).click();
  await expect(page.getByText("当前绑定 qa-mock-harness")).toBeVisible();

  // 3. 尝试归档该 Harness -> 预期被后端拒绝 (conflict)
  await page.getByRole("tab", { name: "Harness 接入" }).click();
  await acpEntry(page, "qa-mock-harness").getByRole("button", { name: "编辑" }).click();
  await expect(page).toHaveURL(/\/harnesses\/[^/]+$/);

  await page.getByRole("button", { name: "归档接入" }).click();
  const alert = page.getByRole("alert");
  await expect(alert).toBeVisible();
  await expect(alert).toContainText(/仍被岗位「QA 测试」绑定，请先解绑/);
  await qaShot(page, "qa-02-archive-harness-refused", alert);

  // 4. 回到岗位页解绑
  await page.getByRole("button", { name: "返回接入列表" }).click();
  await page.getByRole("tab", { name: "岗位" }).click();
  await page.locator(".role-chip", { hasText: "QA 测试" }).click();
  await page.getByRole("button", { name: "解绑" }).click();
  await expect(page.locator(".role-detail__unbound")).toBeVisible();

  // 5. 再次归档该 Harness -> 成功
  await page.getByRole("tab", { name: "Harness 接入" }).click();
  await acpEntry(page, "qa-mock-harness").getByRole("button", { name: "编辑" }).click();
  await page.getByRole("button", { name: "归档接入" }).click();

  // 归档成功后落回实例列表，qa-mock-harness 不再存在
  await expect(page).toHaveURL(/\/roles\?tab=harness$/);
  await expect(acpEntry(page, "qa-mock-harness")).toHaveCount(0);
  await qaShot(page, "qa-03-archive-harness-success");
});

test("03. 归档岗位：有未结束任务时被拒绝，任务结束后可归档且绑定清空", async ({ page }) => {
  await gotoTeam(page, "/roles");

  // 1. 创建临时岗位「架构审查」
  await page.getByRole("button", { name: "新增岗位" }).click();
  const roleCreateForm = page.locator(".role-create");
  await roleCreateForm.getByLabel("岗位名称").fill("架构审查");
  await roleCreateForm.getByLabel("岗位名称").blur();
  // 中文名称没有字母，内核建议 R 系列前缀（避开团队已用前缀）；这里再手动改成 ARCH
  await expect(roleCreateForm.getByLabel("任务编号前缀")).toHaveValue(/^R\d+$/);
  await roleCreateForm.getByLabel("任务编号前缀").fill("ARCH");
  await roleCreateForm.getByLabel("职责").fill("负责代码架构把关");
  await roleCreateForm.getByRole("button", { name: "创建岗位" }).click();

  await expect(page.locator(".role-chip", { hasText: "架构审查" })).toBeVisible();

  // 2. 为该岗位绑定 mock-local
  await page.locator(".role-chip", { hasText: "架构审查" }).click();
  await page.getByRole("button", { name: /绑定 Harness|更换 Harness/ }).click();
  const select = page.getByLabel("选择本团队的 Harness 实例");
  const optVal = await select.locator("option", { hasText: "mock-local" }).getAttribute("value");
  await select.selectOption(optVal ?? "");
  await page.getByRole("button", { name: "确认绑定" }).click();
  await expect(page.getByText("当前绑定 mock-local")).toBeVisible();

  // 3. 创建一个分配给「架构审查」的任务（状态 TODO）
  await gotoTeam(page, "/tasks/new");
  await page.getByLabel("任务名称").fill("临时架构审查任务");
  const ownerSelect = page.getByLabel("负责人岗位");
  const ownerVal = await ownerSelect
    .locator("option", { hasText: "架构审查" })
    .getAttribute("value");
  await ownerSelect.selectOption(ownerVal ?? "");
  await page.locator("#task-file-search").fill("README.md");
  await page.getByRole("button", { name: "添加", exact: true }).click();
  await page.getByRole("button", { name: "创建并分配" }).click();

  await expect(page.locator(".task-detail__status")).toContainText("ARCH-001");

  // 4. 回到岗位页尝试归档「架构审查」 -> 预期被拒绝
  await gotoTeam(page, "/roles");
  await page.locator(".role-chip", { hasText: "架构审查" }).click();
  await page.getByRole("button", { name: "归档岗位" }).click();

  const alert = page.getByRole("alert");
  await expect(alert).toBeVisible();
  await expect(alert).toContainText(/还负责 1 个未结束的任务，请先转交或结束后再归档/);
  await qaShot(page, "qa-04-archive-role-refused", alert);

  // 5. 结束该任务（转为已取消）
  await gotoTeam(page, "/tasks");
  await page.locator("a.task-card-link", { hasText: "临时架构审查任务" }).click();
  await page.getByRole("button", { name: "已取消", exact: true }).click();
  await expect(page.locator(".task-detail__status")).toContainText("已取消 / ARCH-001");

  // 6. 任务已结束，再次尝试归档「架构审查」 -> 成功
  await gotoTeam(page, "/roles");
  await page.locator(".role-chip", { hasText: "架构审查" }).click();
  await page.getByRole("button", { name: "归档岗位" }).click();

  await expect(page.locator(".role-chip", { hasText: "架构审查" })).toHaveCount(0);
  await qaShot(page, "qa-05-archive-role-success");
});

test("04. 受阻任务：编辑后恢复；受阻 → 执行中须满足就绪条件", async ({ page }) => {
  // 1. 创建就绪任务并转为执行中 -> 受阻
  await gotoTeam(page, "/tasks/new");
  await page.getByLabel("任务名称").fill("受阻流转校验任务");
  const ownerSelect = page.getByLabel("负责人岗位");
  const ownerVal = await ownerSelect
    .locator("option", { hasText: "前端开发" })
    .getAttribute("value");
  await ownerSelect.selectOption(ownerVal ?? "");
  await page.locator("#task-file-search").fill("docs/plan.md");
  await page.getByRole("button", { name: "添加", exact: true }).click();
  await page.getByRole("radio", { name: "READY", exact: true }).check();
  await page.getByRole("button", { name: "创建并分配" }).click();

  await expect(page.locator(".task-detail__status")).toContainText("READY");
  await page.getByRole("button", { name: "执行中", exact: true }).click();
  await expect(page.locator(".task-detail__status")).toContainText("执行中");

  await page.getByRole("button", { name: "受阻", exact: true }).click();
  await page.getByLabel("受阻原因").fill("等待前后端契约冻结");
  await page.getByRole("button", { name: "确认受阻" }).click();
  await expect(page.locator(".task-detail__status")).toContainText("受阻");
  await expect(page.getByText("受阻原因：等待前后端契约冻结")).toBeVisible();

  // 2. 编辑受阻任务：修改说明后详情页更新，任务仍处于受阻
  await page.getByRole("button", { name: "编辑" }).click();
  await page.getByLabel("说明", { exact: true }).fill("契约已在会议中对齐，待最终补充文档");
  await page.getByRole("button", { name: "保存", exact: true }).click();

  await expect(page.getByText("契约已在会议中对齐，待最终补充文档")).toBeVisible();
  await expect(page.locator(".task-detail__status")).toContainText("受阻");
  await expect(page.getByText("受阻原因：等待前后端契约冻结")).toBeVisible();
  await qaShot(page, "qa-06-edit-blocked-task");

  // 3. 受阻 → 执行中就绪条件校验：
  // 编辑任务，添加一个不存在的文件引用并删除有效引用
  await page.getByRole("button", { name: "编辑" }).click();
  await page.getByRole("button", { name: "移除引用 docs/plan.md" }).click();
  await page.locator("#task-file-search").fill("docs/nonexistent-spec.md");
  await page.getByRole("button", { name: "添加", exact: true }).click();
  await page.getByRole("button", { name: "保存", exact: true }).click();

  // 详情页展示未满足就绪问题，且因为未就绪，allowedTransitions 不包含「执行中」
  await expect(page.getByText("进入 READY 前还需要")).toBeVisible();
  await expect(
    page.getByText("引用的文件在工作区中不存在：docs/nonexistent-spec.md"),
  ).toBeVisible();
  await expect(page.getByRole("button", { name: "执行中", exact: true })).toHaveCount(0);
  await qaShot(page, "qa-07-blocked-readiness-check");

  // 4. 修复文件引用后，就绪条件满足，出现「执行中」按钮，点击成功恢复为执行中，受阻原因清除
  await page.getByRole("button", { name: "编辑" }).click();
  await page.getByRole("button", { name: "移除引用 docs/nonexistent-spec.md" }).click();
  await page.locator("#task-file-search").fill("docs/plan.md");
  await page.getByRole("button", { name: "添加", exact: true }).click();
  await page.getByRole("button", { name: "保存", exact: true }).click();

  await expect(page.getByText("进入 READY 前还需要")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "执行中", exact: true })).toBeVisible();
  await page.getByRole("button", { name: "执行中", exact: true }).click();
  await expect(page.locator(".task-detail__status")).toContainText("执行中");
  await expect(page.getByText("受阻原因：")).toHaveCount(0);
  await qaShot(page, "qa-08-blocked-to-doing");
});

test("05. 任务取消与恢复（CANCELLED → TODO）并在看板「显示已取消」", async ({ page }) => {
  // 保证团队中常驻一项基线活动任务，使得看板工具栏（包含各过滤器）常态保持可见
  await ensureBaselineTask(page);

  await gotoTeam(page, "/tasks");
  await page.locator("a.task-card-link", { hasText: "受阻流转校验任务" }).click();

  // 1. 取消任务
  await page.getByRole("button", { name: "已取消", exact: true }).click();
  await expect(page.locator(".task-detail__status")).toContainText("已取消");
  await qaShot(page, "qa-09-task-cancelled");

  // 2. 看板默认隐藏已取消任务
  await gotoTeam(page, "/tasks");
  await expect(page.locator("a.task-card-link", { hasText: "受阻流转校验任务" })).toHaveCount(0);
  await expect(page.locator(".task-board__heading--cancelled")).toHaveCount(0);

  // 3. 勾选「显示已取消」后展示已取消列和卡片
  await tickTaskBoardBox(page, "显示已取消");
  await expect(page.locator(".task-board__heading--cancelled")).toBeVisible();
  const cancelledCard = page.locator("a.task-card-link", { hasText: "受阻流转校验任务" });
  await expect(cancelledCard).toBeVisible();
  await qaShot(page, "qa-10-kanban-show-cancelled");

  // 4. 恢复取消任务为 TODO
  await cancelledCard.click();
  await expect(page.getByRole("button", { name: "TODO", exact: true })).toBeVisible();
  await page.getByRole("button", { name: "TODO", exact: true }).click();
  await expect(page.locator(".task-detail__status")).toContainText("TODO");
  await qaShot(page, "qa-11-task-restored-to-todo");
});

test("06. 岗位规则与边界：非法前缀、前缀重复、第二个 Leader 被拒绝", async ({ page }) => {
  await gotoTeam(page, "/roles");
  await page.getByRole("button", { name: "新增岗位" }).click();
  const roleCreateForm = page.locator(".role-create");
  const prefixInput = roleCreateForm.getByLabel("任务编号前缀");

  // 1. 非法前缀：1 位字符
  await roleCreateForm.getByLabel("岗位名称").fill("非法前缀测试");
  // 名称失焦后建议在途，立刻手填前缀；建议返回时不得覆盖手填值。
  const lateSuggestion = page.waitForResponse(
    (response) =>
      response.url().includes("/invoke") &&
      (response.request().postData() ?? "").includes("suggest_task_prefix"),
  );
  await roleCreateForm.getByLabel("岗位名称").blur();
  await prefixInput.fill("X");
  await lateSuggestion;
  await expect(prefixInput).toHaveValue("X");
  await roleCreateForm.getByRole("button", { name: "创建岗位" }).click();

  const alert = roleCreateForm.getByRole("alert");
  await expect(alert).toBeVisible();
  await expect(alert).toContainText("任务前缀「X」格式不正确：需为 2–4 位大写字母或数字");
  await qaShot(page, "qa-12-role-invalid-prefix", alert);

  // 2. 前缀与现有岗位重复（岗位代码已由系统自动生成，不再由用户填写）
  await prefixInput.fill("FE");
  await roleCreateForm.getByRole("button", { name: "创建岗位" }).click();
  await expect(alert).toContainText("任务前缀「FE」已被岗位「前端开发」使用");
  await qaShot(page, "qa-13-role-duplicate-prefix", alert);

  // 3. 第二个 Leader 被拒绝
  await roleCreateForm.getByLabel("岗位名称").fill("副团队长");
  await prefixInput.fill("VLD");
  await roleCreateForm.getByLabel("设为 Leader").check();
  await roleCreateForm.getByRole("button", { name: "创建岗位" }).click();
  await expect(alert).toContainText(/团队已有 Leader「.*」，每个团队至多一个 Leader/);
  await qaShot(page, "qa-14-role-second-leader-rejected", alert);

  await roleCreateForm.getByRole("button", { name: "取消" }).click();
});

test("07. 输入边界：超长标题、控制字符、首尾空格、重复名称（Harness/团队）", async ({ page }) => {
  // 1. 超长标题（>200 字）
  await gotoTeam(page, "/tasks/new");
  const longTitle = "长".repeat(201);
  await page.getByLabel("任务名称").fill(longTitle);
  const ownerSelect = page.getByLabel("负责人岗位");
  const ownerVal = await ownerSelect
    .locator("option", { hasText: "前端开发" })
    .getAttribute("value");
  await ownerSelect.selectOption(ownerVal ?? "");
  await page.locator("#task-file-search").fill("README.md");
  await page.getByRole("button", { name: "添加", exact: true }).click();
  await page.getByRole("button", { name: "创建并分配" }).click();

  const alert = page.getByRole("alert");
  await expect(alert).toBeVisible();
  await expect(alert).toContainText("任务标题不能超过 200 个字符");
  await qaShot(page, "qa-15-input-boundary-title-length", alert);

  // 2. 控制字符
  await page.getByLabel("任务名称").fill("任务标题\u0007包含警报符");
  await page.getByRole("button", { name: "创建并分配" }).click();
  await expect(alert).toContainText("任务标题包含不允许的控制字符");
  await qaShot(page, "qa-16-input-boundary-control-char", alert);

  // 3. 首尾空格自动去除
  await page.getByLabel("任务名称").fill("   首尾带空格的有效任务   ");
  await page.getByRole("button", { name: "创建并分配" }).click();
  await expect(page.getByRole("heading", { name: "首尾带空格的有效任务" })).toBeVisible();
  await qaShot(page, "qa-17-input-boundary-trimmed-title");

  // 4. 重复 Harness 名称
  await gotoTeam(page, "/roles?tab=harness");
  await page.getByRole("button", { name: "新建接入" }).click();
  const harnessCreateForm = page.locator(".acp-create");
  await harnessCreateForm.locator(".acp-preset", { hasText: "Mock" }).click();
  await harnessCreateForm.getByLabel("实例名称").fill("mock-local");
  await harnessCreateForm.getByRole("button", { name: "保存并握手检查" }).click();
  const harnessAlert = harnessCreateForm.getByRole("alert");
  await expect(harnessAlert).toContainText("团队中已有名为「mock-local」的 Harness");
  await qaShot(page, "qa-18-input-boundary-duplicate-harness", harnessAlert);
  await harnessCreateForm.getByRole("button", { name: "取消" }).click();

  // 5. 重复团队名称
  await fillCreateTeam(page, { name: "账号系统迭代" });
  await page.getByRole("button", { name: "创建团队", exact: true }).click();
  const teamAlert = page.getByRole("alert");
  await expect(teamAlert).toContainText("已有同名团队「账号系统迭代」");
  await qaShot(page, "qa-19-input-boundary-duplicate-team", teamAlert);
});

test("08. 文件引用边界：父级目录 ..、绝对路径、超过 50 个引用上限", async ({ page }) => {
  test.setTimeout(60_000);
  await gotoTeam(page, "/tasks/new");
  await page.getByLabel("任务名称").fill("文件引用边界测试任务");
  const ownerSelect = page.getByLabel("负责人岗位");
  const ownerVal = await ownerSelect
    .locator("option", { hasText: "前端开发" })
    .getAttribute("value");
  await ownerSelect.selectOption(ownerVal ?? "");

  // 1. 文件引用包含 ..
  await page.locator("#task-file-search").fill("docs/../secret.txt");
  await page.getByRole("button", { name: "添加", exact: true }).click();
  await page.getByRole("button", { name: "创建并分配" }).click();
  const alert = page.getByRole("alert");
  await expect(alert).toBeVisible();
  await expect(alert).toContainText("文件引用不能包含「..」：docs/../secret.txt");
  await qaShot(page, "qa-20-file-refs-parent-dir", alert);

  // 2. 文件引用为绝对路径
  await page.getByRole("button", { name: "移除引用 docs/../secret.txt" }).click();
  await page.locator("#task-file-search").fill("/etc/passwd");
  await page.getByRole("button", { name: "添加", exact: true }).click();
  await page.getByRole("button", { name: "创建并分配" }).click();
  await expect(alert).toContainText("文件引用必须是相对工作区的路径：/etc/passwd");
  await qaShot(page, "qa-21-file-refs-absolute-path", alert);

  // 3. 超过 50 个引用
  await page.getByRole("button", { name: "移除引用 /etc/passwd" }).click();
  for (let i = 1; i <= 51; i++) {
    const filename = `docs/ref_${String(i).padStart(2, "0")}.md`;
    await page.locator("#task-file-search").fill(filename);
    await page.getByRole("button", { name: "添加", exact: true }).click();
  }
  await expect(page.locator(".task-form__chip")).toHaveCount(51);
  await page.getByRole("button", { name: "创建并分配" }).click();
  await expect(alert).toContainText("文件引用最多 50 个");
  await qaShot(page, "qa-22-file-refs-over-limit", alert);
});

test("09. 团队切换与数据隔离：创建第二个团队并验证数据互不串", async ({ page }) => {
  // 1. 创建第二个团队
  const team2Id = await createTeamViaUi(page, {
    name: "研发效能保障组",
    description: "独立测试团队数据隔离",
    examples: ["Leader"],
  });
  expect(team2Id).not.toBe(baseTeamId);
  await qaShot(page, "qa-23-second-team-created");

  // 2. 在团队 2 中创建专有任务
  await page.goto(`/#/teams/${team2Id}/tasks/new`);
  await page.getByLabel("任务名称").fill("团队2独立隔离任务");
  const ownerSelect = page.getByLabel("负责人岗位");
  const ownerVal = await ownerSelect.locator("option", { hasText: "Leader" }).getAttribute("value");
  await ownerSelect.selectOption(ownerVal ?? "");
  await page.locator("#task-file-search").fill("README.md");
  await page.getByRole("button", { name: "添加", exact: true }).click();
  await page.getByRole("button", { name: "创建并分配" }).click();
  await expect(page.locator(".task-detail__status")).toContainText("LD-001");

  // 3. 使用 TeamSwitcher 切换回团队 1
  const trigger = page.locator(".team-switcher__trigger");
  await trigger.click();
  const optionTeam1 = page.locator(".team-switcher__option", { hasText: "账号系统迭代" });
  await expect(optionTeam1).toBeVisible();
  await optionTeam1.click();

  // 4. 验证团队 1 看板与列表中完全没有团队 2 的任务
  await gotoTeam(page, "/tasks");
  await expect(page.locator("a.task-card-link", { hasText: "团队2独立隔离任务" })).toHaveCount(0);

  await page.getByRole("button", { name: "列表", exact: true }).click();
  await expect(page.locator(".task-table tr", { hasText: "团队2独立隔离任务" })).toHaveCount(0);
  await qaShot(page, "qa-24-team-switcher-isolation");
});

test("10. 重新加载页面后状态保持", async ({ page }) => {
  await gotoTeam(page, "/roles?tab=harness");
  await expect(page.getByRole("tab", { name: "Harness 接入" })).toHaveAttribute(
    "aria-selected",
    "true",
  );

  // 重新加载页面
  await page.reload();

  // 验证重新加载后仍然在 Harness 接入标签页
  await expect(page).toHaveURL(/\/roles\?tab=harness$/);
  await expect(page.getByRole("tab", { name: "Harness 接入" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await expect(page.getByRole("heading", { name: "接入列表" })).toBeVisible();

  // 再在任务面板验证刷新
  await gotoTeam(page, "/tasks");
  await expect(page.getByRole("heading", { name: "任务面板" })).toBeVisible();
  await page.reload();
  await expect(page).toHaveURL(/\/tasks$/);
  await expect(page.getByRole("heading", { name: "任务面板" })).toBeVisible();

  await qaShot(page, "qa-25-reload-state-persistence");
});

test("11. 当团队所有任务均已取消时，看板保留工具栏并提示勾选「显示已取消」", async ({ page }) => {
  // 创建一个专属团队用于复现此缺陷
  const bugTeamId = await createTeamViaUi(page, {
    name: "缺陷复现-仅已取消团队",
    examples: ["Leader"],
  });

  // 在该团队创建 1 个任务并立即取消它
  await page.goto(`/#/teams/${bugTeamId}/tasks/new`);
  await page.getByLabel("任务名称").fill("将被取消的唯一任务");
  const ownerSelect = page.getByLabel("负责人岗位");
  const ownerVal = await ownerSelect.locator("option", { hasText: "Leader" }).getAttribute("value");
  await ownerSelect.selectOption(ownerVal ?? "");
  await page.locator("#task-file-search").fill("README.md");
  await page.getByRole("button", { name: "添加", exact: true }).click();
  await page.getByRole("button", { name: "创建并分配" }).click();

  // 取消该任务
  await page.getByRole("button", { name: "已取消", exact: true }).click();
  await expect(page.locator(".task-detail__status")).toContainText("已取消");

  // 进入该团队看板页面
  await page.goto(`/#/teams/${bugTeamId}/tasks`);

  // 用户预期：虽然没有进行中的任务，但团队已有已取消任务，看板工具栏应保留，允许勾选「显示已取消」以恢复任务
  // 实际缺陷表现：页面渲染了 EmptyState，没有工具栏，显示已取消复选框不可见（预期超时失败）
  await qaShot(page, "qa-26-bug-all-cancelled-empty-state");
  await expect(page.getByLabel("显示已取消")).toBeVisible({ timeout: 2000 });
  await expect(page.getByText("当前只有已取消的任务")).toBeVisible();
  await expect(page.getByText("创建第一项任务")).toHaveCount(0);
  await tickTaskBoardBox(page, "显示已取消");
  await expect(page.getByRole("link", { name: /将被取消的唯一任务/ })).toBeVisible();
});
