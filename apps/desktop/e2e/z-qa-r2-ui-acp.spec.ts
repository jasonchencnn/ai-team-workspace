// apps/desktop/e2e/z-qa-r2-ui-acp.spec.ts
// QA 第二轮（ACP 接入）：UI-ACP-002/003/004/006/008/009/010、UI-HARNESS-002/003/004/005。
// 只用内置 Mock 预设、/usr/bin/false 这类惰性命令，以及本文件临时写出的「假 Agent」脚本
// （只回一条 JSON-RPC 错误消息，不联网、不启动任何真实 ACP Agent）。
// 失败注入走测试桥接的 /invoke：page.route 拦截并回错误体，产品代码与正式构建完全一致。
import { chmodSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { type Page, expect, test } from "@playwright/test";

import { acpEntry, createMockHarness } from "./harnessHelpers.js";

const TEAM_NAME = "QA二轮接入团队";
const ROLE_DEV = "后端研发";
const ROLE_ARCH = "系统架构师";

/** 自定义预设 + 参数 + 环境配置档案：用于验证编辑时字段保持原值、不回显密钥。 */
const KEEP = "custom-keep";
const KEEP_ARGS = "--qa-keep --verbose";
/** 环境配置档案里的一段「密钥」：任何界面位置都不得出现。 */
const SECRET = "sk-live-QAUI3-MUST-NOT-BE-SHOWN";

/** UI-ACP-009 用例前置里已存在的实例名，用于重名拒绝。 */
const FRONTEND_LOCAL = "frontend-local";
const MOCK_NEW = "mock-retry-ok";

/** UI-ACP-004/010：被岗位绑定的 Mock 接入。 */
const MOCK_BOUND = "mock-bound";
/** UI-HARNESS-004：握手成功后编辑成未检查状态。 */
const MOCK_CHECKED = "mock-checked";
const MOCK_UNCHECKED = "mock-unchecked";
/** UI-HARNESS-005：归档后回访其详情路由。 */
const MOCK_ARCHIVED = "mock-archived";

/** UI-ACP-010：握手返回 needs_auth 的接入。 */
const NEEDS_AUTH = "agent-needs-auth";

let workspace = "";
let teamId = "";
let envProfile = "";
let needsAuthAgent = "";

test.describe.configure({ mode: "serial" });

test.beforeAll(() => {
  workspace = mkdtempSync(join(tmpdir(), "teamspace-e2e-qa-ui3-"));
  envProfile = join(workspace, "agent.env");
  // 档案里放一段密钥明文：详情页与编辑表单只能回显路径。
  writeFileSync(envProfile, `ANTHROPIC_API_KEY=${SECRET}\n`);
  // 假 Agent：读到 initialize 后回一条 auth_required 的 JSON-RPC 错误，不做任何别的事。
  needsAuthAgent = join(workspace, "needs-auth-agent.sh");
  writeFileSync(
    needsAuthAgent,
    [
      "#!/bin/sh",
      "read -r _line || exit 0",
      `echo '{"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"Authentication required","data":{"authMethods":[{"id":"oauth","name":"OAuth 登录"}]}}}'`,
      "",
    ].join("\n"),
  );
  chmodSync(needsAuthAgent, 0o755);
});

test.afterAll(() => {
  rmSync(workspace, { recursive: true, force: true });
});

// ---------------------------------------------------------------- 辅助

async function addRoleDraft(page: Page, name: string, desc: string): Promise<void> {
  await page.getByRole("button", { name: "+ 添加岗位" }).click();
  const card = page.locator(".tc-role").last();
  await card.getByLabel("岗位名称").fill(name);
  await card.getByLabel("职责").fill(desc);
}

async function openHarnessTab(page: Page): Promise<void> {
  await page.goto(`/#/teams/${teamId}/roles?tab=harness`);
  await expect(page.getByRole("tab", { name: "Harness 接入" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
}

/** 从测试桥接的 /invoke 请求体里取出命令名。 */
function cmdOf(postData: string | null): string | null {
  if (postData === null) {
    return null;
  }
  try {
    const parsed = JSON.parse(postData) as { cmd?: unknown };
    return typeof parsed.cmd === "string" ? parsed.cmd : null;
  } catch {
    return null;
  }
}

/** 记录页面上依次发出的 invoke 命令（用于断言「先创建再握手」的顺序）。 */
function recordInvokes(page: Page): string[] {
  const calls: string[] = [];
  page.on("request", (request) => {
    if (!request.url().endsWith("/invoke")) {
      return;
    }
    const cmd = cmdOf(request.postData());
    if (cmd !== null) {
      calls.push(cmd);
    }
  });
  return calls;
}

/**
 * 注入后端失败：指定命令的前 `times` 次 /invoke 直接回错误体，其余照常透传。
 * 返回注销函数，测试里用它恢复真实后端。
 */
async function failInvoke(
  page: Page,
  cmd: string,
  message: string,
  times = 1,
): Promise<() => Promise<void>> {
  const handler = async (route: import("@playwright/test").Route): Promise<void> => {
    if (cmdOf(route.request().postData()) !== cmd) {
      await route.fallback();
      return;
    }
    if (times <= 0) {
      await route.fallback();
      return;
    }
    times -= 1;
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ err: { kind: "internal", message } }),
    });
  };
  await page.route("**/invoke", handler);
  return async () => {
    await page.unroute("**/invoke", handler);
  };
}

/** 注入加载中：指定命令的 /invoke 延迟 `ms` 再透传，用于观察加载文案。 */
async function delayInvoke(page: Page, cmd: string, ms: number): Promise<() => Promise<void>> {
  const handler = async (route: import("@playwright/test").Route): Promise<void> => {
    if (cmdOf(route.request().postData()) !== cmd) {
      await route.fallback();
      return;
    }
    await new Promise((resolve) => {
      setTimeout(resolve, ms);
    });
    await route.continue();
  };
  await page.route("**/invoke", handler);
  return async () => {
    await page.unroute("**/invoke", handler);
  };
}

// --------------------------------------------------------------------------
// UI-ACP-003 空状态、加载与失败重试（本文件第一个用例：在这里建自己的团队）
// --------------------------------------------------------------------------

test("UI-ACP-003 列表空状态、加载与失败重试：空状态与加载文案、错误条带「重试」且重试后恢复", async ({
  page,
}) => {
  await page.goto("/#/teams/new");
  await page.getByLabel("团队名称").fill(TEAM_NAME);
  await page.getByLabel("手动输入工作空间路径").fill(workspace);
  await page.getByRole("button", { name: "检测", exact: true }).click();
  await expect(page.getByText("普通文件夹", { exact: true })).toBeVisible();
  await addRoleDraft(page, ROLE_DEV, "负责核心微服务开发");
  await addRoleDraft(page, ROLE_ARCH, "负责架构评审");
  await page.getByRole("button", { name: "创建团队", exact: true }).click();
  await expect(page).toHaveURL(/\/teams\/(?!new$)[^/]+$/);
  teamId = /\/teams\/([^/?]+)/.exec(page.url())?.[1] ?? "";
  expect(teamId).not.toBe("");

  // 1) 空状态
  await openHarnessTab(page);
  await expect(page.getByText("本团队还没有接入")).toBeVisible();
  await expect(
    page.getByText("新建接入后即可为岗位绑定执行者；Mock 接入用于本地验证，不消耗额度。"),
  ).toBeVisible();

  // 2) 加载中
  const undoDelay = await delayInvoke(page, "list_harnesses", 3000);
  await page.reload();
  await expect(page.getByText("正在加载接入列表")).toBeVisible();
  await undoDelay();
  await expect(page.getByText("本团队还没有接入")).toBeVisible();

  // 3) 失败：错误条 + 「重试」，重试后恢复
  const undoFail = await failInvoke(page, "list_harnesses", "注入的接入列表加载失败");
  await page.reload();
  const banner = page.locator(".ui-error-banner");
  await expect(banner).toContainText("注入的接入列表加载失败");
  await banner.getByRole("button", { name: "重试" }).click();
  await undoFail();
  await expect(page.getByText("本团队还没有接入")).toBeVisible();
  await expect(page.locator(".ui-error-banner")).toHaveCount(0);
});

// --------------------------------------------------------------------------
// UI-ACP-006 新建接入两步结构与选中切换
// --------------------------------------------------------------------------

test("UI-ACP-006 新建接入两步结构与选中切换：区块标题与说明、选中标题变化、再次点击取消选择", async ({
  page,
}) => {
  await openHarnessTab(page);
  await page.getByRole("button", { name: "新建接入" }).click();
  await expect(page.getByRole("heading", { name: "新建接入", exact: true })).toBeVisible();
  await expect(page.getByText("两步：选择预设，再确认配置")).toBeVisible();
  await expect(page.getByRole("heading", { name: "第一步 选择预设" })).toBeVisible();

  const mock = page.locator(".acp-preset", { hasText: "Mock" });
  const save = page.getByRole("button", { name: "保存并握手检查" });
  await expect(page.getByRole("heading", { name: "第二步 确认配置", exact: true })).toBeVisible();
  await expect(save).toBeDisabled();

  // 选中 Mock：第二步标题带上已选预设，提交按钮可用
  await mock.click();
  await expect(
    page.getByRole("heading", { name: "第二步 确认配置（已选：Mock）", exact: true }),
  ).toBeVisible();
  await expect(mock).toHaveAttribute("aria-pressed", "true");
  await expect(save).toBeEnabled();

  // 再次点击同一张卡片：取消选择并回到「第二步 确认配置」，提交按钮重新禁用
  await mock.click();
  await expect(page.getByRole("heading", { name: "第二步 确认配置", exact: true })).toBeVisible();
  await expect(mock).toHaveAttribute("aria-pressed", "false");
  await expect(save).toBeDisabled();

  await page.getByRole("button", { name: "取消" }).click();
  await expect(page.getByRole("heading", { name: "新建接入", exact: true })).toHaveCount(0);
});

// --------------------------------------------------------------------------
// UI-ACP-008 自定义 Agent 与预设的命令预填
// --------------------------------------------------------------------------

test("UI-ACP-008 自定义 Agent 与预设的命令预填：名称按「预设（默认命令）」预填、启动命令必填、Claude Code 预填且可改", async ({
  page,
}) => {
  await openHarnessTab(page);
  await page.getByRole("button", { name: "新建接入" }).click();

  const name = page.getByLabel("实例名称");
  const command = page.getByLabel("启动命令");
  const args = page.getByLabel("参数");
  const save = page.getByRole("button", { name: "保存并握手检查" });

  // 1) 自定义 ACP Agent：名称预填为「<预设名>（自定义）」，启动命令必填且为空
  await page.locator(".acp-preset", { hasText: "自定义 ACP Agent" }).click();
  await expect(name).toHaveValue("自定义 ACP Agent（自定义）");
  await expect(
    page.locator(".ui-field__hint", { hasText: "自定义 ACP Agent 必填；命令名或绝对路径。" }),
  ).toBeVisible();
  await expect(command).toHaveValue("");

  // 清空预填名称：名称为空时提交按钮禁用
  await name.fill("");
  await expect(save).toBeDisabled();
  // 补上名称但启动命令仍为空（commandRequired）：提交按钮仍禁用
  await name.fill("自定义预填校验");
  await expect(save).toBeDisabled();

  // 3) 改选 Claude Code：名称与启动命令按预设预填，参数按预设预填（默认为空）
  await page.locator(".acp-preset", { hasText: "Claude Code" }).click();
  await expect(name).toHaveValue("Claude Code（claude-code-acp）");
  await expect(command).toHaveValue("claude-code-acp");
  await expect(args).toHaveValue("");
  await expect(
    page.locator(".ui-field__hint", { hasText: "已按预设预填，可改成绝对路径。" }),
  ).toBeVisible();
  await expect(save).toBeEnabled();

  // 4) 启动命令与参数均可手动修改
  await command.fill("/usr/bin/false");
  await args.fill("--qa-preset");
  await expect(command).toHaveValue("/usr/bin/false");
  await expect(args).toHaveValue("--qa-preset");

  // 只验证预填与可改，不保存：避免以真实预设启动任何 Agent
  await page.getByRole("button", { name: "取消" }).click();
  await expect(page.getByRole("heading", { name: "新建接入", exact: true })).toHaveCount(0);
});

// --------------------------------------------------------------------------
// UI-ACP-002 预设只读（顺带建好带参数与环境档案的自定义接入）
// --------------------------------------------------------------------------

test("UI-ACP-002 预设只读：预设下拉禁用并提示「创建后不可修改。」，编辑时名称、启动命令、参数、配置档案保持原值", async ({
  page,
}) => {
  await openHarnessTab(page);
  await page.getByRole("button", { name: "新建接入" }).click();
  await page.locator(".acp-preset", { hasText: "自定义 ACP Agent" }).click();
  await page.getByLabel("实例名称").fill(KEEP);
  await page.getByLabel("启动命令").fill("/usr/bin/false");
  await page.getByLabel("参数").fill(KEEP_ARGS);
  await page.getByLabel("环境配置档案（可选）").fill(envProfile);
  await page.getByRole("button", { name: "保存并握手检查" }).click();
  await expect(acpEntry(page, KEEP)).toBeVisible();

  await acpEntry(page, KEEP).getByRole("button", { name: "编辑" }).click();

  // 预设只读
  await expect(page.getByLabel("预设")).toBeDisabled();
  await expect(page.locator(".ui-field__hint", { hasText: "创建后不可修改。" })).toBeVisible();

  // 已创建接入的四个字段在编辑时保持原值，不被重置
  await expect(page.getByLabel("实例名称")).toHaveValue(KEEP);
  await expect(page.getByLabel("启动命令")).toHaveValue("/usr/bin/false");
  await expect(page.getByLabel("参数")).toHaveValue(KEEP_ARGS);
  await expect(page.getByLabel("环境配置档案")).toHaveValue(envProfile);
  // 没有改动时保存按钮不可用，反证表单未被悄悄重置
  await expect(page.getByRole("button", { name: "保存启动配置" })).toBeDisabled();

  await page.getByRole("button", { name: "返回接入列表" }).click();
});

// --------------------------------------------------------------------------
// UI-ACP-004 接入列表字段与被绑定标识
// --------------------------------------------------------------------------

test("UI-ACP-004 接入列表字段与被绑定标识：每行展示名称/预设/握手状态与「启动命令」「Agent 版本」「能力」「被绑定」标签", async ({
  page,
}) => {
  await openHarnessTab(page);
  await createMockHarness(page, MOCK_BOUND);

  // 把岗位绑定到该接入
  await page.goto(`/#/teams/${teamId}/roles`);
  await page.locator(".role-chip", { hasText: ROLE_DEV }).click();
  await page.getByRole("button", { name: "绑定 Harness", exact: true }).click();
  const select = page.getByLabel("选择本团队的 Harness 实例");
  await select.selectOption(
    (await select.locator("option", { hasText: MOCK_BOUND }).getAttribute("value")) ?? "",
  );
  await page.getByRole("button", { name: "确认绑定" }).click();
  await expect(page.getByText(`当前绑定 ${MOCK_BOUND}`)).toBeVisible();

  await openHarnessTab(page);
  const entry = acpEntry(page, MOCK_BOUND);
  await expect(entry.locator(".acp-entry__name")).toContainText(MOCK_BOUND);
  await expect(entry.locator(".ui-tag", { hasText: "Mock 测试用" })).toBeVisible();
  await expect(entry.locator(".ui-tag", { hasText: "已连接" })).toBeVisible();

  // 字段标签
  await expect(entry.locator(".acp-entry__command")).toContainText("启动命令");
  await expect(entry.locator(".acp-entry__command .acp-entry__code")).toContainText(
    "teamspace-mock-agent",
  );
  const meta = entry.locator(".acp-entry__meta");
  await expect(meta).toContainText("Agent 版本");
  await expect(meta).toContainText("能力");
  await expect(meta).toContainText("被绑定");

  // 「被绑定」显示岗位名，可据此判断能否归档
  await expect(meta).toContainText(ROLE_DEV);
  await expect(meta).not.toContainText("暂未被任何岗位绑定");
});

// --------------------------------------------------------------------------
// UI-ACP-009 必填校验、重名拒绝与保存后握手
// --------------------------------------------------------------------------

test("UI-ACP-009 必填校验、重名拒绝与保存后握手：空名禁用不发请求、重名被拒不重复、先创建再握手、失败保留弹窗且取消不创建", async ({
  page,
}) => {
  await openHarnessTab(page);
  const calls = recordInvokes(page);
  await createMockHarness(page, FRONTEND_LOCAL);

  await page.getByRole("button", { name: "新建接入" }).click();
  await page.locator(".acp-preset", { hasText: "Mock" }).click();
  const save = page.getByRole("button", { name: "保存并握手检查" });
  const name = page.getByLabel("实例名称");

  // 1) 清空自动预填的实例名称：提交按钮禁用，且不发起创建请求
  const before = calls.filter((cmd) => cmd === "create_harness").length;
  await name.fill("");
  await expect(save).toBeDisabled();
  await expect(save).toBeDisabled();
  expect(calls.filter((cmd) => cmd === "create_harness")).toHaveLength(before);
  await expect(acpEntry(page, FRONTEND_LOCAL)).toHaveCount(1);

  // 2) 以 frontend-local 重名保存：后端拒绝，列表不出现重复项
  await name.fill(FRONTEND_LOCAL);
  await expect(save).toBeEnabled();
  await save.click();
  await expect(page.locator(".ui-error-banner")).toContainText(
    "团队中已有名为「frontend-local」的 Harness",
  );
  await expect(acpEntry(page, FRONTEND_LOCAL)).toHaveCount(1);

  // 3) 以新名称保存 Mock：先创建实例再执行握手检查
  await name.fill(MOCK_NEW);
  const mark = calls.length;
  await save.click();
  const created = acpEntry(page, MOCK_NEW);
  await expect(created).toBeVisible();
  expect(
    calls.slice(mark).filter((cmd) => cmd === "create_harness" || cmd === "check_harness"),
  ).toEqual(["create_harness", "check_harness"]);
  await expect(created.locator(".ui-tag", { hasText: "已连接" })).toBeVisible();
  await expect(created.locator(".acp-entry__detail--muted")).toContainText("握手成功");
  await expect(created).not.toContainText("尚未握手。");

  // 4) 注入创建失败：错误条出现在第二步，弹窗保持打开可修改重试
  await page.getByRole("button", { name: "新建接入" }).click();
  await page.locator(".acp-preset", { hasText: "Mock" }).click();
  const failedName = `${MOCK_NEW}-inject`;
  await page.getByLabel("实例名称").fill(failedName);
  const undo = await failInvoke(page, "create_harness", "注入的创建失败：存储不可用");
  await page.getByRole("button", { name: "保存并握手检查" }).click();
  await expect(page.locator(".ui-error-banner")).toContainText("注入的创建失败：存储不可用");
  await undo();
  await expect(page.getByRole("heading", { name: "新建接入", exact: true })).toBeVisible();
  await expect(page.getByLabel("实例名称")).toHaveValue(failedName);
  await expect(page.getByRole("button", { name: "保存并握手检查" })).toBeEnabled();

  // 点「取消」：关闭弹窗且不创建实例
  await page.getByRole("button", { name: "取消" }).click();
  await expect(page.getByRole("heading", { name: "新建接入", exact: true })).toHaveCount(0);
  await expect(acpEntry(page, failedName)).toHaveCount(0);
});

// --------------------------------------------------------------------------
// UI-HARNESS-002 启动配置字段与提示
// --------------------------------------------------------------------------

test("UI-HARNESS-002 启动配置字段与提示：五个字段齐备，启动命令/参数/环境配置档案三段提示文案一致", async ({
  page,
}) => {
  await openHarnessTab(page);
  await acpEntry(page, KEEP).getByRole("button", { name: "编辑" }).click();

  const panel = page.locator(".harness-detail__connection");
  await expect(panel.getByRole("heading", { name: "启动配置" })).toBeVisible();
  for (const label of ["实例名称", "预设", "启动命令", "参数", "环境配置档案"]) {
    await expect(panel.getByLabel(label)).toBeVisible();
  }

  await expect(
    panel.locator(".ui-field__hint", { hasText: "命令名或绝对路径；留空则使用预设默认命令。" }),
  ).toBeVisible();
  await expect(panel.locator(".ui-field__hint", { hasText: "空格分隔。" })).toBeVisible();
  await expect(
    panel.locator(".ui-field__hint", {
      hasText: "可留空；填写时须为绝对路径，只记录路径，不保存密钥。",
    }),
  ).toBeVisible();

  await page.getByRole("button", { name: "返回接入列表" }).click();
});

// --------------------------------------------------------------------------
// UI-HARNESS-003 不展示任何密钥明文
// --------------------------------------------------------------------------

test("UI-HARNESS-003 不展示任何密钥明文：详情页与编辑表单只回显配置文件路径，页面不出现档案内容", async ({
  page,
}) => {
  await openHarnessTab(page);
  await acpEntry(page, KEEP).getByRole("button", { name: "编辑" }).click();

  // 只回显路径
  await expect(page.getByLabel("环境配置档案")).toHaveValue(envProfile);
  await expect(page.locator("body")).not.toContainText(SECRET);
  await expect(page.locator("body")).not.toContainText("ANTHROPIC_API_KEY");

  // 编辑表单（同页的启动配置区）同样不含任何凭据/令牌内容
  await expect(page.getByLabel("启动命令")).toHaveValue("/usr/bin/false");
  await expect(page.getByLabel("参数")).toHaveValue(KEEP_ARGS);
  await expect(page.locator("body")).not.toContainText(SECRET);
  await expect(page.getByText("平台不保存任何密钥，登录由各 Agent 自己管理。")).toBeVisible();

  await page.getByRole("button", { name: "返回接入列表" }).click();
});

// --------------------------------------------------------------------------
// UI-HARNESS-004 握手结果字段与未握手态
// --------------------------------------------------------------------------

test("UI-HARNESS-004 握手结果字段与未握手态：握手成功展示真实值，未握手不伪造成功值", async ({
  page,
}) => {
  await openHarnessTab(page);
  await createMockHarness(page, MOCK_CHECKED);
  await acpEntry(page, MOCK_CHECKED).getByRole("button", { name: "编辑" }).click();

  const facts = page.locator(".harness-detail__facts");
  await expect(page.locator(".harness-detail__health-head .ui-tag")).toHaveText("已连接");
  // 值为握手返回的真实值
  await expect(facts.locator(".harness-detail__fact", { hasText: "协议版本" })).toContainText(
    "ACP v1",
  );
  await expect(facts.locator(".harness-detail__fact", { hasText: "Agent 名称" })).toContainText(
    "teamspace-mock-agent",
  );
  await expect(facts.locator(".harness-detail__fact", { hasText: "Agent 版本" })).toContainText(
    "teamspace-mock-agent",
  );
  // Mock Agent 的 initialize 没有声明任何登录方式：如实显示占位符
  await expect(facts.locator(".harness-detail__fact", { hasText: "登录方式" })).toContainText("—");
  await expect(facts.locator(".harness-detail__fact", { hasText: "能力" })).toContainText(
    "加载会话",
  );

  // 未握手：保存启动配置后握手重置为 unknown
  await page.getByLabel("实例名称").fill(MOCK_UNCHECKED);
  await page.getByRole("button", { name: "保存启动配置" }).click();
  await expect(page.locator(".harness-detail__health-head .ui-tag")).toHaveText("未检查");
  await expect(page.locator(".harness-detail__checked")).toHaveText("从未检查");
  for (const key of ["协议版本", "Agent 名称", "Agent 版本", "登录方式", "能力"]) {
    await expect(facts.locator(".harness-detail__fact", { hasText: key })).toContainText("—");
  }
});

// --------------------------------------------------------------------------
// UI-ACP-010 握手检查、状态色与归档
// --------------------------------------------------------------------------

test("UI-ACP-010 握手检查、状态色与归档：needs_auth 警告色、Mock「测试用」标签、被绑定归档被拒", async ({
  page,
}) => {
  // 1) 建一个握手返回 needs_auth 的接入（假 Agent 脚本，只回一条 JSON-RPC 错误）
  await openHarnessTab(page);
  await page.getByRole("button", { name: "新建接入" }).click();
  await page.locator(".acp-preset", { hasText: "自定义 ACP Agent" }).click();
  await page.getByLabel("实例名称").fill(NEEDS_AUTH);
  await page.getByLabel("启动命令").fill(needsAuthAgent);
  await page.getByRole("button", { name: "保存并握手检查" }).click();

  // 2) needs_auth 为警告色，不是危险色
  const authEntry = acpEntry(page, NEEDS_AUTH);
  await expect(authEntry).toBeVisible();
  await expect(authEntry.locator(".ui-tag--warning", { hasText: "需要登录" })).toBeVisible();
  await expect(authEntry.locator(".acp-entry__detail--warning")).toContainText("Agent 要求先登录");
  await expect(authEntry.locator(".acp-entry__detail--danger")).toHaveCount(0);

  // 点「握手检查」触发一次握手并更新状态与说明
  await authEntry.getByRole("button", { name: "握手检查" }).click();
  await expect(authEntry.locator(".ui-tag--warning", { hasText: "需要登录" })).toBeVisible();
  await expect(authEntry.locator(".acp-entry__detail--warning")).toContainText(
    "请在对应工具中完成登录后重新检查",
  );
  await expect(authEntry.locator(".acp-entry__checked")).not.toHaveText("");
  await expect(page.locator(".ui-error-banner")).toHaveCount(0);

  // error 为危险色，不是警告色
  const errorEntry = acpEntry(page, KEEP);
  await expect(errorEntry.locator(".ui-tag--danger", { hasText: "异常" })).toBeVisible();
  await expect(errorEntry.locator(".acp-entry__detail--danger")).toContainText("/usr/bin/false");
  await expect(errorEntry.locator(".acp-entry__detail--warning")).toHaveCount(0);

  // unknown 为中性色；说明行文案见下方 UI-ACP-010 缺陷用例
  const unknownEntry = acpEntry(page, MOCK_UNCHECKED);
  await expect(unknownEntry.locator(".ui-tag--neutral", { hasText: "未检查" })).toBeVisible();
  await expect(unknownEntry.locator(".acp-entry__detail--warning")).toHaveCount(0);
  await expect(unknownEntry.locator(".acp-entry__detail--danger")).toHaveCount(0);

  // 正常「已连接」不使用警告色，且 Mock 实例带「测试用」标签
  const boundEntry = acpEntry(page, MOCK_BOUND);
  await expect(boundEntry.locator(".ui-tag", { hasText: "已连接" })).toBeVisible();
  await expect(boundEntry.locator(".acp-entry__detail--muted")).toContainText("握手成功");
  await expect(boundEntry.locator(".acp-entry__detail--warning")).toHaveCount(0);
  await expect(boundEntry.locator(".acp-entry__detail--danger")).toHaveCount(0);
  await expect(boundEntry.locator(".acp-entry__name")).toContainText(`${MOCK_BOUND}（测试用）`);
  await expect(boundEntry.locator(".ui-tag", { hasText: "Mock 测试用" })).toBeVisible();

  // 4) 归档未被绑定的接入：成功后从列表移除，绑定选择中不再出现
  await acpEntry(page, FRONTEND_LOCAL).getByRole("button", { name: "编辑" }).click();
  await page.getByRole("button", { name: "归档接入" }).click();
  await expect(page).toHaveURL(new RegExp(`/teams/${teamId}/roles\\?tab=harness$`));
  await expect(acpEntry(page, FRONTEND_LOCAL)).toHaveCount(0);

  await page.goto(`/#/teams/${teamId}/roles`);
  await page.locator(".role-chip", { hasText: ROLE_ARCH }).click();
  await page.getByRole("button", { name: "绑定 Harness", exact: true }).click();
  await expect(
    page.getByLabel("选择本团队的 Harness 实例").locator("option", { hasText: FRONTEND_LOCAL }),
  ).toHaveCount(0);

  // 5) 归档已被岗位绑定的接入：被拒并提示仍被绑定，接入与绑定关系均不变
  await openHarnessTab(page);
  await acpEntry(page, MOCK_BOUND).getByRole("button", { name: "编辑" }).click();
  await page.getByRole("button", { name: "归档接入" }).click();
  await expect(page.locator(".ui-error-banner")).toContainText(`仍被岗位「${ROLE_DEV}」绑定`);
  await expect(page).toHaveURL(new RegExp(`/teams/${teamId}/harnesses/[^/]+$`));
  await page.getByRole("button", { name: "返回接入列表" }).click();
  await expect(acpEntry(page, MOCK_BOUND)).toContainText(ROLE_DEV);
});

test("UI-ACP-010 未握手接入的说明行：应显示「尚未握手。」", async ({ page }) => {
  test.fixme(
    true,
    "缺陷 UI-ACP-010：未做过握手检查的接入（status=unknown、detail 为空）整行不渲染说明，列表里看不到「尚未握手。」——HarnessInstancesSection.tsx:182 在该状态下直接返回 null，导致第 186 行的「尚未握手。」永不可达。",
  );

  await openHarnessTab(page);
  const unknownEntry = acpEntry(page, MOCK_UNCHECKED);
  await expect(unknownEntry.locator(".ui-tag--neutral", { hasText: "未检查" })).toBeVisible();
  await expect(unknownEntry.locator(".acp-entry__detail--muted")).toContainText("尚未握手。");
});

// --------------------------------------------------------------------------
// UI-HARNESS-005 异常与导航兜底
// --------------------------------------------------------------------------

test("UI-HARNESS-005 异常与导航兜底：已归档与不存在的 id 显示「找不到该接入」、加载中显示「正在加载接入」、返回落到 Harness 接入页签", async ({
  page,
}) => {
  await openHarnessTab(page);
  await createMockHarness(page, MOCK_ARCHIVED);
  await acpEntry(page, MOCK_ARCHIVED).getByRole("button", { name: "编辑" }).click();
  const archivedUrl = page.url();
  await page.getByRole("button", { name: "归档接入" }).click();
  await expect(page).toHaveURL(new RegExp(`/teams/${teamId}/roles\\?tab=harness$`));

  // 已归档的 id
  await page.goto(archivedUrl);
  await expect(page.getByText("找不到该接入")).toBeVisible();
  await expect(page.getByText("它可能已被归档。")).toBeVisible();

  // 不存在的 id
  await page.goto(`/#/teams/${teamId}/harnesses/no-such-harness`);
  await expect(page.getByText("找不到该接入")).toBeVisible();
  await expect(page.getByText("它可能已被归档。")).toBeVisible();

  // 路由不含 teamId 时由 IndexRedirect 兜底（详情页永远拿得到 teamId），
  // 应用不出现空白页或崩溃。
  await page.goto("/#/harnesses/no-such-harness");
  await expect(page.getByRole("heading", { name: "团队空间" })).toBeVisible();

  // 加载中
  await openHarnessTab(page);
  await acpEntry(page, MOCK_BOUND).getByRole("button", { name: "编辑" }).click();
  const detailUrl = page.url();
  const undoDelay = await delayInvoke(page, "list_harnesses", 3000);
  // 纯 hash 跳转不会重新挂载、也命中 react-query 缓存：用 reload 真正重新加载一次。
  await page.goto(detailUrl);
  await page.reload();
  await expect(page.getByText("正在加载接入")).toBeVisible();
  await undoDelay();
  await expect(
    page.getByRole("heading", { name: new RegExp(`${MOCK_BOUND} · ACP 接入设置`) }),
  ).toBeVisible();

  // 在详情页点返回：回到岗位与绑定页的 Harness 接入页签
  await page.getByRole("button", { name: "返回接入列表" }).click();
  await expect(page).toHaveURL(new RegExp(`/teams/${teamId}/roles\\?tab=harness$`));
  await expect(page.getByRole("tab", { name: "Harness 接入" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await expect(page.getByRole("heading", { name: "接入列表" })).toBeVisible();
});
