// apps/desktop/e2e/z-qa-r2-ui-create-nav.spec.ts
// QA 第二轮「创建团队页 + 团队空间 + 导航 + 活动记录」补测（UI-CREATE-003/004/007、
// UI-HOME-003/004/005/006/007/008、UI-NAV-002/003/004/005/006、UI-ACT-001/004）。
//
// 约束：
// - 只用 Mock ACP 预设与临时目录，不启动任何真实 Agent，不联网。
// - 全套自建团队（每次 pnpm run e2e 都是全新临时 SQLite），不依赖其它用例文件建的数据。
// - 工作空间全部是本用例自己 mkdtemp/git init 出来的目录：普通文件夹、Git 仓库、
//   会被删掉的目录、以及「Teamspace 被文件占位」的目录（用于岗位文件夹未建立）。
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { type Locator, type Page, expect, test } from "@playwright/test";

import { acpEntry } from "./harnessHelpers.js";

const TEAM_A = "导航验收组";
const TEAM_B = "普通文件夹组";
const TEAM_C = "工作区失联组";
const TEAM_D = "岗位文件夹未建组";
const ROLE_A = "验收负责人";
const ROLE_B = "研发主力";
const ROLE_D = "待建档岗位";
const HARNESS_A = "mock-导航验收";
const TASK_DOING = "登录链路联调";
const TASK_TODO = "补充回归用例";
const TASK_ACTIVITY = "活动顺序验证任务";
/** 写进环境配置档案的假密钥：界面任何位置都不该出现它。 */
const SECRET = "sk-qa-secret-9f3a2b7c";

/** 界面上的本地时间：zh-CN 的「年/月/日 时:分」，不含 ISO 的 T/Z，也不含 UTC 偏移串。 */
const LOCAL_DATETIME = /^\d{4}[/-]\d{2}[/-]\d{2} \d{2}:\d{2}$/;
/** 工作日志行首时间：本地时间 + 显式偏移（如 `2026-10-14 10:48:01 UTC+08:00`），不是 UTC 原始串。 */
const JOURNAL_DATETIME = /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2} UTC[+-]\d{2}:\d{2}$/;

let root = "";
let gitWorkspace = "";
let plainDir = "";
let lostDir = "";
let blockedDir = "";
let envProfile = "";
let teamA = "";
let teamB = "";
let teamC = "";
let teamD = "";
let doingKey = "";
let doingTaskId = "";

test.describe.configure({ mode: "serial" });

test.beforeAll(() => {
  root = mkdtempSync(join(tmpdir(), "teamspace-e2e-qa-ui1-"));

  // Git 仓库工作空间（UI-HOME-008 的「Git 仓库」分支；分支与短 SHA 由断言核对）。
  gitWorkspace = join(root, "git-repo");
  mkdirSync(gitWorkspace);
  const git = (...args: string[]): void => {
    execFileSync(
      "git",
      [
        "-c",
        "user.name=E2E",
        "-c",
        "user.email=e2e@example.invalid",
        "-c",
        "commit.gpgsign=false",
        ...args,
      ],
      { cwd: gitWorkspace, stdio: "ignore" },
    );
  };
  git("init", "-q", "-b", "main");
  writeFileSync(join(gitWorkspace, "README.md"), "# 导航验收仓库\n");
  git("add", ".");
  git("commit", "-q", "-m", "chore: seed qa workspace");

  // 普通文件夹工作空间（UI-CREATE-007 与 UI-HOME-008 的「普通文件夹」分支）。
  plainDir = join(root, "plain-folder");
  mkdirSync(plainDir);
  writeFileSync(join(plainDir, "notes.md"), "# 素材大纲\n");

  // 建团队后会被删掉的工作空间（UI-HOME-008 的「工作区不可用」分支）。
  lostDir = join(root, "lost-folder");
  mkdirSync(lostDir);

  // Teamspace 被一个同名「文件」占位：岗位文件夹建不出来（UI-HOME-005 的未建立分支）。
  blockedDir = join(root, "blocked-teamspace");
  mkdirSync(blockedDir);
  writeFileSync(join(blockedDir, "Teamspace"), "占位，不是目录\n");

  // Mock 接入的环境配置档案（UI-ACT-004：只记录路径，不保存密钥）。
  envProfile = join(root, "agent.env");
  writeFileSync(envProfile, `ANTHROPIC_API_KEY=${SECRET}\n`);
});

test.afterAll(() => {
  rmSync(root, { recursive: true, force: true });
});

// --------------------------------------------------------------------------
// 局部选择器
// --------------------------------------------------------------------------

/** 创建团队页里装着「团队名称」输入框的那张卡的标题（即预览出来的团队名）。 */
function teamNameCardTitle(page: Page): Locator {
  return page
    .locator(".tc-stack-card", { has: page.getByLabel("团队名称") })
    .locator(".ui-card__title");
}

/** 「本地工作空间」卡片里的路径行。 */
function workspacePathLine(page: Page): Locator {
  return page
    .locator(".tc-stack-card", { has: page.getByRole("heading", { name: "本地工作空间" }) })
    .locator("p", { hasText: "路径：" });
}

/** 团队空间右侧「团队共用的上下文」面板。 */
function contextPanel(page: Page): Locator {
  return page.locator(".th-panel", {
    has: page.getByRole("heading", { name: "团队共用的上下文" }),
  });
}

/** 团队空间右侧「最近活动」所在的上下文面板。 */
function activityPanel(page: Page): Locator {
  return contextPanel(page);
}

function roleCard(page: Page, roleName: string): Locator {
  return page.locator(".th-role", { hasText: roleName });
}

function teamIdOf(page: Page): string {
  const id = /\/teams\/([^/?]+)/.exec(page.url())?.[1] ?? "";
  expect(id).not.toBe("");
  return id;
}

/** 打开团队空间并等概览加载完。 */
async function gotoTeamHome(page: Page, team: string): Promise<void> {
  await page.goto(`/#/teams/${team}`);
  // 只改 hash 的跳转是同文档导航，React Query 缓存会原样留着，概览可能是上一次的数据；
  // 强制整页重载，拿到的一定是内核当前的返回。
  await page.reload();
  await expect(page.getByRole("heading", { name: "团队空间" })).toBeVisible();
}

/** 在创建团队页「+ 添加岗位」自定义一个岗位，返回自动填入的任务编号前缀。 */
async function addCustomRole(
  page: Page,
  name: string,
  description: string,
  expectedPrefix: string,
): Promise<string> {
  await page.getByRole("button", { name: "+ 添加岗位" }).click();
  const card = page.locator(".tc-role").last();
  await card.getByLabel("岗位名称").fill(name);
  await card.getByLabel("岗位名称").blur();
  const prefix = card.getByLabel("任务编号前缀");
  await expect(prefix).toHaveValue(expectedPrefix);
  await card.getByLabel("职责").fill(description);
  return await prefix.inputValue();
}

/** 手动输入路径 + 检测，并等检测结果出来。 */
async function detectWorkspace(page: Page, dir: string): Promise<void> {
  await page.getByLabel("手动输入工作空间路径").fill(dir);
  await page.getByRole("button", { name: "检测", exact: true }).click();
  await expect(page.locator("p", { hasText: /^Git 仓库：|^普通文件夹$/ })).toBeVisible();
}

/** 新建团队（普通文件夹工作空间），返回团队 id。 */
async function createPlainTeam(page: Page, name: string, dir: string): Promise<string> {
  await page.goto("/#/teams/new");
  await page.getByLabel("团队名称").fill(name);
  await detectWorkspace(page, dir);
  if (name === TEAM_D) {
    await addCustomRole(page, ROLE_D, "等待文件夹建立", "R1");
  }
  await page.getByRole("button", { name: "创建团队", exact: true }).click();
  await expect(page).toHaveURL(/\/teams\/(?!new$)[^/]+$/);
  await expect(page.getByRole("heading", { name: "团队空间" })).toBeVisible();
  return teamIdOf(page);
}

/** 创建任务并返回任务编号（形如 R1-001）。 */
async function createTask(
  page: Page,
  input: { team: string; title: string; owner: string; status: "TODO" | "READY" },
): Promise<string> {
  await page.goto(`/#/teams/${input.team}/tasks/new`);
  await page.reload();
  await page.getByLabel("任务名称").fill(input.title);
  const owner = page.getByLabel("负责人岗位");
  await owner.selectOption(
    (await owner.locator("option", { hasText: input.owner }).getAttribute("value")) ?? "",
  );
  await page.getByRole("radio", { name: input.status, exact: true }).check();
  await page.getByRole("button", { name: "创建并分配" }).click();
  const status = page.locator(".task-detail__status");
  await expect(status).toContainText(input.status);
  return /\/ ([A-Z0-9]+-\d+)/.exec((await status.textContent()) ?? "")?.[1] ?? "";
}

/** 从团队空间的岗位卡进入岗位工作台，返回岗位 id。 */
async function openWorkbench(page: Page, team: string, roleName: string): Promise<string> {
  await gotoTeamHome(page, team);
  await roleCard(page, roleName).getByRole("link", { name: roleName }).click();
  await expect(page).toHaveURL(/\/roles\/[^/]+\/workbench$/);
  await expect(page.getByRole("heading", { level: 1, name: roleName })).toBeVisible();
  return /\/roles\/([^/]+)\/workbench/.exec(page.url())?.[1] ?? "";
}

// ==========================================================================
// 创建团队页
// ==========================================================================

test("UI-CREATE-003 团队名称为空提交：显示「请填写团队名称」且不发起创建请求", async ({ page }) => {
  test.fixme(
    true,
    "缺陷 UI-CREATE-003：TeamCreatePage.submit()（:128-140）无条件调用 createTeam，名称为空仍向内核发起创建请求，界面弹出「团队名称不能为空」错误横幅，与预期结果「不发起创建请求」不符（已实测：前 3 项断言通过，仅此项失败）。",
  );
  await page.goto("/#/teams/new");
  await detectWorkspace(page, plainDir);
  await expect(teamNameCardTitle(page)).toHaveText("未命名团队");

  await page.getByRole("button", { name: "创建团队", exact: true }).click();

  // 名称字段下方出现错误
  const nameField = page.locator(".ui-field--invalid", { has: page.getByLabel("团队名称") });
  await expect(nameField.getByRole("alert")).toHaveText("请填写团队名称");
  await expect(page.getByLabel("团队名称")).toHaveAttribute("aria-invalid", "true");
  // 卡片标题仍是占位
  await expect(teamNameCardTitle(page)).toHaveText("未命名团队");
  // 不发起创建请求：留在创建页且没有任何内核错误
  await expect(page).toHaveURL(/\/teams\/new$/);
  await expect(page.locator(".ui-error-banner")).toHaveCount(0);
});

test("UI-CREATE-004 团队名称实时回显卡片标题：由「未命名团队」变为「验收测试组」", async ({
  page,
}) => {
  await page.goto("/#/teams/new");
  await expect(teamNameCardTitle(page)).toHaveText("未命名团队");
  await page.getByLabel("团队名称").fill("验收测试组");
  await expect(teamNameCardTitle(page)).toHaveText("验收测试组");
});

test("UI-CREATE-007 手动输入路径后按新路径重检测：显示「普通文件夹」、完整路径与检测提示", async ({
  page,
}) => {
  await page.goto("/#/teams/new");

  // 先检测一次 Git 仓库，再改成普通文件夹路径重检测，验证「按新路径」而不是沿用旧结果。
  await detectWorkspace(page, gitWorkspace);
  await expect(page.getByText(/Git 仓库：main @ [0-9a-f]{7}/)).toBeVisible();

  await page.getByLabel("手动输入工作空间路径").fill(plainDir);
  // 路径一改，检测结果作废，回到未检测
  await expect(page.getByText("尚未检测")).toBeVisible();
  await page.getByRole("button", { name: "检测", exact: true }).click();

  await expect(page.getByText("普通文件夹", { exact: true })).toBeVisible();
  await expect(page.getByText(/Git 仓库：/)).toHaveCount(0);
  // 路径行显示填写的完整路径
  await expect(workspacePathLine(page)).toContainText(plainDir);
  // 提示文案
  await expect(
    page.locator(".ui-field__hint", { hasText: "填写后点「检测」按该路径重新检测。" }),
  ).toBeVisible();

  // ---- 承接后续用例的夹具：创建主团队（Git 工作空间、两个岗位）----
  await page.getByLabel("团队名称").fill(TEAM_A);
  await detectWorkspace(page, gitWorkspace);
  await expect(page.getByText(/Git 仓库：main @ [0-9a-f]{7}/)).toBeVisible();
  await addCustomRole(page, ROLE_A, "负责验收与缺陷跟踪", "R1");
  await addCustomRole(page, ROLE_B, "负责主流程开发", "R2");
  await page.getByRole("button", { name: "创建团队", exact: true }).click();
  await expect(page).toHaveURL(/\/teams\/(?!new$)[^/]+$/);
  await expect(page.getByRole("heading", { name: "团队空间" })).toBeVisible();
  teamA = teamIdOf(page);
  expect(teamA).not.toBe("");
});

// ==========================================================================
// 团队空间
// ==========================================================================

test("UI-HOME-006 推进中工作空状态：显示空状态文案与「创建任务」入口", async ({ page }) => {
  expect(teamA).not.toBe("");
  await gotoTeamHome(page, teamA);
  const panel = page.locator(".th-panel", {
    has: page.getByRole("heading", { name: "推进中的工作" }),
  });
  await expect(panel.getByText("团队已就绪，创建第一项任务")).toBeVisible();
  await expect(
    panel.getByText(
      "将任务分配给岗位，并关联已有需求文件。岗位当前绑定的 Harness 会在满足依赖后领取任务。",
    ),
  ).toBeVisible();
  // 有创建入口
  const cta = panel.getByRole("button", { name: "创建任务" });
  await expect(cta).toBeVisible();
  await cta.click();
  await expect(page).toHaveURL(new RegExp(`/teams/${teamA}/tasks/new$`));
  await expect(page.getByRole("heading", { name: "创建任务" })).toBeVisible();
});

test("UI-HOME-003 岗位卡未绑定状态：显示「未绑定 Harness · 暂不派工」且不显示 Harness 名", async ({
  page,
}) => {
  await gotoTeamHome(page, teamA);
  const card = roleCard(page, ROLE_A);
  await expect(card.getByText("未绑定 Harness · 暂不派工")).toBeVisible();
  // 绑定位不出现任何 Harness 名，也不出现握手健康区
  await expect(card.getByText(/当前绑定/)).toHaveCount(0);
  await expect(card.getByText(/Mock/)).toHaveCount(0);
  await expect(card.locator(".th-health")).toHaveCount(0);
});

test("UI-HOME-004 岗位卡任务数与握手健康：显示「未结束任务 2」与已连接握手", async ({ page }) => {
  // ---- 夹具：注册带环境档案的 Mock 接入，绑定给研发主力，并派 2 项未结束任务 ----
  await page.goto(`/#/teams/${teamA}/roles?tab=harness`);
  await expect(page.getByRole("tab", { name: "Harness 接入" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await page.getByRole("button", { name: "新建接入" }).click();
  await page.locator(".acp-preset", { hasText: "Mock" }).click();
  await page.getByLabel("实例名称").fill(HARNESS_A);
  await page.getByLabel("环境配置档案（可选）").fill(envProfile);
  await page.getByRole("button", { name: "保存并握手检查" }).click();
  await expect(acpEntry(page, HARNESS_A)).toContainText("已连接");

  await page.goto(`/#/teams/${teamA}/roles`);
  await page.locator(".role-chip", { hasText: ROLE_B }).click();
  await page.getByRole("button", { name: "绑定 Harness", exact: true }).click();
  await page.getByLabel("选择本团队的 Harness 实例").selectOption({ index: 1 });
  await page.getByRole("button", { name: "确认绑定" }).click();
  await expect(page.getByText(`当前绑定 ${HARNESS_A}`)).toBeVisible();

  // 一项推到执行中、一项留在 TODO：两项都未结束。
  doingKey = await createTask(page, {
    team: teamA,
    title: TASK_DOING,
    owner: ROLE_B,
    status: "READY",
  });
  expect(doingKey).not.toBe("");
  await page.getByRole("button", { name: "执行中", exact: true }).click();
  await expect(page.locator(".task-detail__status")).toContainText(`执行中 / ${doingKey}`);
  doingTaskId = /\/tasks\/([^/?]+)/.exec(page.url())?.[1] ?? "";
  expect(doingTaskId).not.toBe("");
  await createTask(page, { team: teamA, title: TASK_TODO, owner: ROLE_B, status: "TODO" });

  // ---- 断言 ----
  await gotoTeamHome(page, teamA);
  const card = roleCard(page, ROLE_B);
  await expect(card.getByText(`当前绑定 Mock（测试用） · ${HARNESS_A}`)).toBeVisible();
  // 握手健康：成功态标签 + 握手 detail 文案
  const health = card.locator(".th-health");
  await expect(health.locator(".ui-tag")).toHaveText("已连接");
  await expect(health.locator(".ui-tag--success")).toBeVisible();
  await expect(health.locator(".th-note")).toContainText("握手成功");
  // 未结束任务数
  await expect(card.getByText("未结束任务 2")).toBeVisible();
});

test("UI-HOME-007 进行中任务行展示与跳转：显示 KEY/标题/状态/负责人并跳到任务详情", async ({
  page,
}) => {
  expect(doingKey).not.toBe("");
  expect(doingTaskId).not.toBe("");
  await gotoTeamHome(page, teamA);
  const row = page.locator(".th-task-row", { hasText: doingKey });
  await expect(row.locator(".th-task-row__key")).toHaveText(doingKey);
  await expect(row.locator(".th-task-row__title")).toHaveText(TASK_DOING);
  await expect(row.locator(".th-task-row__owner")).toHaveText(ROLE_B);
  await expect(row.locator(".ui-tag")).toHaveText("执行中");

  await row.click();
  await expect(page).toHaveURL(new RegExp(`/teams/${teamA}/tasks/${doingTaskId}$`));
  await expect(page.getByRole("heading", { level: 1, name: TASK_DOING })).toBeVisible();
});

test("UI-HOME-008 共用上下文：普通文件夹 / Git 仓库 / 工作区不可用三种形态", async ({ page }) => {
  // 1) Git 仓库
  await gotoTeamHome(page, teamA);
  await expect(contextPanel(page).getByText(/Git 仓库：main @ [0-9a-f]{7}/)).toBeVisible();
  await expect(contextPanel(page).locator(".th-path")).toContainText("路径：");

  // 2) 普通文件夹
  teamB = await createPlainTeam(page, TEAM_B, plainDir);
  expect(teamB).not.toBe("");
  await expect(contextPanel(page).locator(".th-path")).toContainText(plainDir);
  await expect(contextPanel(page).getByText("普通文件夹", { exact: true })).toBeVisible();
  await expect(contextPanel(page).getByText(/Git 仓库：/)).toHaveCount(0);

  // 3) 工作区不可用：先建团队，再把工作空间目录删掉
  teamC = await createPlainTeam(page, TEAM_C, lostDir);
  expect(teamC).not.toBe("");
  rmSync(lostDir, { recursive: true, force: true });
  await page.goto(`/#/teams/${teamC}/roles`);
  await expect(page.getByRole("heading", { name: "岗位与绑定" })).toBeVisible();
  await gotoTeamHome(page, teamC);
  const warn = page.locator(".ui-card--warning", { hasText: "工作区不可用" });
  await expect(warn.getByRole("heading", { name: "工作区不可用" })).toBeVisible();
  await expect(warn).toContainText("工作空间目录不存在");
  // 顶栏同时给出异常提示
  await expect(page.locator(".topbar__branch--warn")).toContainText(
    `工作区异常：工作空间目录不存在：${lostDir}`,
  );
});

test("UI-HOME-005 岗位文件夹入口：已建立时 title 为 Teamspace/<岗位名>，未建立时为「岗位文件夹尚未建立」", async ({
  page,
}) => {
  // 1) 已建立
  await gotoTeamHome(page, teamA);
  const ready = roleCard(page, ROLE_A).getByRole("button", { name: "岗位文件夹" });
  await expect(ready).toHaveAttribute("title", `Teamspace/${ROLE_A}`);
  await ready.click();
  await expect(page.locator(".ui-error-banner")).toHaveCount(0);
  await expect(page.getByRole("alert")).toHaveCount(0);
  await expect(page.getByRole("heading", { name: "团队空间" })).toBeVisible();

  // 2) 未建立：工作空间里 Teamspace 是同名文件，岗位文件夹建不出来
  teamD = await createPlainTeam(page, TEAM_D, blockedDir);
  expect(teamD).not.toBe("");
  const missing = roleCard(page, ROLE_D).getByRole("button", { name: "岗位文件夹" });
  await expect(missing).toHaveAttribute("title", "岗位文件夹尚未建立");
  await missing.click();
  // 点击不报错不崩溃：页面与岗位卡都还在，按钮恢复可用；界面给出的是中文原因
  // （岗位文件夹建不出来时内核记录的具体原因优先于通用占位说明，见 api.rs:429-434），
  // 而不是空白页或未捕获异常。
  await expect(page.getByRole("heading", { name: "团队空间" })).toBeVisible();
  await expect(roleCard(page, ROLE_D)).toBeVisible();
  await expect(missing).toBeEnabled();
  await expect(page.getByRole("alert")).toContainText(/岗位文件夹|无法写入/);
});

// ==========================================================================
// 活动记录
// ==========================================================================

test("UI-ACT-001 团队空间最近活动：每条含本地时间与摘要、最新在最前、空态文案", async ({
  page,
}) => {
  // 先制造一条最新活动，用来判断「最新的在最前」
  const latestKey = await createTask(page, {
    team: teamA,
    title: TASK_ACTIVITY,
    owner: ROLE_A,
    status: "TODO",
  });
  expect(latestKey).not.toBe("");

  await gotoTeamHome(page, teamA);
  const panel = activityPanel(page);
  const events = panel.locator(".th-event");
  const count = await events.count();
  expect(count).toBeGreaterThan(0);

  // 标题下显示条数
  await expect(panel.getByText(`共 ${String(count)} 条，最新的在最前。`)).toBeVisible();

  // 每条都含本地时间与事件摘要
  for (const index of Array.from({ length: count }, (_, i) => i)) {
    const event = events.nth(index);
    await expect(event.locator(".th-event__time")).toHaveText(LOCAL_DATETIME);
    await expect(event.locator(".th-event__summary")).not.toBeEmpty();
  }

  // 最新的在最前：刚创建的任务排在第一条
  await expect(events.first()).toContainText(`创建任务 ${latestKey}「${TASK_ACTIVITY}」`);

  // 空态分支（UI-HOME-008 新建的团队活动同样非空）：
  // 「暂无活动记录。」要求团队没有任何事件，而内核在创建团队时必定写入
  // 「创建团队「<名称>」」（crates/teamspace-core/src/service/team.rs:79-93），
  // 因此界面不可能存在「无事件团队」，该分支在此轮无法通过界面构造。
});

test("UI-ACT-004 时间本地化且不泄露凭据：团队空间与工作台都是本地时间，活动不含密钥", async ({
  page,
}) => {
  // 给岗位的过程文件目录放一个文件，让工作台的「最近文件」有内容（README 会被过滤掉）。
  const processDir = join(gitWorkspace, "Teamspace", ROLE_A, "过程文件");
  mkdirSync(processDir, { recursive: true });
  writeFileSync(join(processDir, "验收笔记.md"), "# 验收记录\n");

  // 1) 团队空间：活动时间是本地时区可读格式，不是 UTC 原始串
  await gotoTeamHome(page, teamA);
  const times = activityPanel(page).locator(".th-event__time");
  const timeCount = await times.count();
  expect(timeCount).toBeGreaterThan(0);
  for (const index of Array.from({ length: timeCount }, (_, i) => i)) {
    const text = (await times.nth(index).textContent()) ?? "";
    expect(text).toMatch(LOCAL_DATETIME);
    expect(text).not.toContain("T");
    expect(text).not.toContain("Z");
    expect(text).not.toContain("UTC");
    expect(text).not.toMatch(/[+-]\d{2}:\d{2}$/);
  }

  // 2) 工作台：最近文件时间同样是本地格式；工作日志行首带显式本地偏移，不是 UTC 原始串
  await openWorkbench(page, teamA, ROLE_A);
  const fileTime = page.locator(".rw-file-row__time").first();
  await expect(fileTime).toHaveText(LOCAL_DATETIME);
  const journalTime = page.locator(".rw-journal-time").first();
  await expect(journalTime).toHaveText(JOURNAL_DATETIME);
  // 最近任务用的是相对时间（刚刚 / N 分钟前），不是 UTC 串
  await expect(page.locator(".rw-task-row__time").first()).toHaveText(
    /^(刚刚|\d+ (分钟|小时)前|\d{4}[/-]\d{2}[/-]\d{2} \d{2}:\d{2})$/,
  );

  // 3) 活动摘要只记录动作与对象：环境档案里的假密钥不出现在任何界面文本里
  await gotoTeamHome(page, teamA);
  const activityText = (await activityPanel(page).locator(".th-event-list").textContent()) ?? "";
  expect(activityText).not.toContain(SECRET);
  expect(activityText).not.toContain("ANTHROPIC_API_KEY");
  expect(activityText).not.toContain("sk-");

  // 接入详情只显示环境档案路径，不显示文件内容
  await page.goto(`/#/teams/${teamA}/roles?tab=harness`);
  await expect(acpEntry(page, HARNESS_A)).toContainText(HARNESS_A);
  await acpEntry(page, HARNESS_A).getByRole("button", { name: "编辑" }).click();
  await expect(page.getByRole("heading", { name: "握手结果" })).toBeVisible();
  await expect(page.getByLabel("环境配置档案")).toHaveValue(envProfile);
  expect((await page.locator("body").textContent()) ?? "").not.toContain(SECRET);
});

// ==========================================================================
// 导航
// ==========================================================================

test("UI-NAV-002 M3/M5 未交付项不可点击：保持禁用态、不跳转不报错", async ({ page }) => {
  await page.goto(`/#/teams/${teamA}/tasks`);
  await expect(page.getByRole("heading", { name: "任务面板" })).toBeVisible();

  for (const item of [
    { name: "共享文件", note: "M3 提供" },
    { name: "待我处理", note: "M5 提供" },
  ]) {
    const entry = page.getByRole("link", { name: item.name });
    await expect(entry).toHaveAttribute("aria-disabled", "true");
    await expect(entry).toHaveClass(/nav-item--disabled/);
    await expect(entry).toHaveAttribute("title", item.note);
    await expect(entry.locator(".nav-item__note")).toHaveText(item.note);
    // 点击无效：不跳转、不报错。
    // aria-disabled 的元素 Playwright 会拒绝常规点击，这里 force 绕过可操作性检查，
    // 证明「即使真被点到」也没有反应。
    await entry.click({ force: true });
    await expect(page).toHaveURL(new RegExp(`/teams/${teamA}/tasks$`));
    await expect(page.getByRole("alert")).toHaveCount(0);
    await expect(page.getByRole("heading", { name: "任务面板" })).toBeVisible();
  }
});

test("UI-NAV-003 岗位工作台归属团队空间高亮：工作台亮「团队空间」，/roles 亮「岗位与绑定」", async ({
  page,
}) => {
  await openWorkbench(page, teamA, ROLE_B);
  await expect(page.locator(".nav-item--active", { hasText: "团队空间" })).toHaveCount(1);
  await expect(page.locator(".nav-item--active", { hasText: "岗位与绑定" })).toHaveCount(0);

  await page.goto(`/#/teams/${teamA}/roles`);
  await expect(page.getByRole("heading", { name: "岗位与绑定" })).toBeVisible();
  // /roles 用精确匹配：只有「岗位与绑定」选中
  await expect(page.locator(".nav-item--active", { hasText: "岗位与绑定" })).toHaveCount(1);
  await expect(page.locator(".nav-item--active", { hasText: "团队空间" })).toHaveCount(0);
});

test("UI-NAV-004 团队切换器展开与当前项标识：role=listbox、列出全部团队、当前项 aria-selected", async ({
  page,
}) => {
  expect(teamB).not.toBe("");
  expect(teamC).not.toBe("");
  expect(teamD).not.toBe("");
  await gotoTeamHome(page, teamA);

  await expect(page.locator(".team-switcher__trigger")).toContainText(TEAM_A);
  await page.locator(".team-switcher__trigger").click();

  const listbox = page.getByRole("listbox", { name: "切换团队" });
  await expect(listbox).toBeVisible();
  // 列出全部团队（外加固定的「+ 创建团队」入口）。整套 e2e 共用一个临时数据库，
  // 其他 spec 也会建团队，故不写死总数，只断言本 spec 的团队与创建入口都在列表里。
  const options = listbox.getByRole("option");
  await expect(options.first()).toBeVisible();
  for (const name of [TEAM_A, TEAM_B, TEAM_C, TEAM_D]) {
    await expect(listbox.getByRole("option", { name, exact: true })).toBeVisible();
  }
  await expect(listbox.getByRole("option", { name: "+ 创建团队" })).toBeVisible();
  // 当前团队项
  const current = listbox.getByRole("option", { name: TEAM_A, exact: true });
  await expect(current).toHaveAttribute("aria-selected", "true");
  await expect(current.locator("button")).toHaveClass(/team-switcher__option--current/);
  for (const name of [TEAM_B, TEAM_C, TEAM_D]) {
    await expect(listbox.getByRole("option", { name, exact: true })).toHaveAttribute(
      "aria-selected",
      "false",
    );
  }
});

test("UI-NAV-005 切换团队与创建团队入口：切到 B 后内容变 B，「+ 创建团队」进 /teams/new", async ({
  page,
}) => {
  expect(teamB).not.toBe("");
  await gotoTeamHome(page, teamA);
  await page.locator(".team-switcher__trigger").click();
  await page
    .getByRole("listbox", { name: "切换团队" })
    .getByRole("option", { name: TEAM_B, exact: true })
    .getByRole("button")
    .click();

  await expect(page).toHaveURL(new RegExp(`/teams/${teamB}$`));
  // 触发按钮文案与内容都变成 B
  await expect(page.locator(".team-switcher__trigger")).toContainText(TEAM_B);
  await expect(page.locator(".topbar__crumbs")).toContainText(TEAM_B);
  await expect(page.getByRole("heading", { name: "团队空间" })).toBeVisible();
  await expect(page.locator(".th-path")).toContainText(plainDir);

  // 「+ 创建团队」入口
  await page.locator(".team-switcher__trigger").click();
  await page
    .getByRole("listbox", { name: "切换团队" })
    .getByRole("option")
    .filter({ hasText: "+ 创建团队" })
    .getByRole("button")
    .click();
  await expect(page).toHaveURL(/\/teams\/new$/);
  await expect(page.getByRole("heading", { name: "创建团队" })).toBeVisible();
});

test("UI-NAV-006 面包屑层级与未知路径回退：两/三级面包屑 + aria-current，未知路径按 IndexRedirect 回退", async ({
  page,
}) => {
  const crumbs = page.locator(".topbar__crumbs");

  // 1) 团队空间、任务面板、岗位与绑定：都是「工作空间 / <团队>」
  for (const path of ["", "/tasks", "/roles"]) {
    await page.goto(`/#/teams/${teamA}${path}`);
    await page.reload();
    await expect(crumbs).toHaveText(`工作空间/${TEAM_A}`);
    await expect(page.locator('.topbar__crumbs [aria-current="page"]')).toHaveText(TEAM_A);
  }

  // 2) 岗位工作台：「工作空间 / <团队> / <岗位>」
  await openWorkbench(page, teamA, ROLE_B);
  await expect(crumbs).toHaveText(`工作空间/${TEAM_A}/${ROLE_B}`);
  await expect(page.locator('.topbar__crumbs [aria-current="page"]')).toHaveText(ROLE_B);

  // 3) 未知路径：不空白、不崩溃，按 IndexRedirect 回退到某个团队空间
  await page.goto(`/#/teams/${teamA}/does-not-exist`);
  await expect(page).toHaveURL(/\/teams\/[^/]+$/);
  await expect(page.locator(".sidebar")).toBeVisible();
  await expect(page.getByRole("heading", { name: "团队空间" })).toBeVisible();
  await expect(page.locator(".ui-error-banner")).toHaveCount(0);
});
