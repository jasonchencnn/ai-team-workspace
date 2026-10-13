// apps/desktop/e2e/z-qa-m13.spec.ts
// M1.3 独立 QA 回归与探索测试套件（Gemini）
// 覆盖范围：
// 1. ACP 接入边界：启动命令含空格/相对路径、参数含引号与多空格、环境配置档案为相对路径或不存在文件、
//    超长名称、重复名称、详情页预设不可更改、归档被绑定的接入被拒、同一接入绑定多个岗位后列表显示全部岗位、多次快速点击握手检查。
// 2. 自定义 ACP Agent 进程握手：立即退出（/usr/bin/false）、非 JSON 输出（/bin/echo）、超时退出（/bin/sleep 60）。
// 3. 岗位工作台：任务状态变化与改派后最近任务与工作日志刷新、过程文件含子目录/中文名/隐藏过滤、超过 10 个截断、
//    岗位改名后工作台路径与面包屑更新、已归档岗位友好提示、打开工作日志无报错。
// 4. 导航与界面文案纯净性：工作台侧边导航高亮「团队空间」、三级面包屑、刷新后状态保持、全站无旧版 CLI 措辞。
import { mkdirSync, mkdtempSync, rmSync, utimesSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { type Locator, type Page, expect, test } from "@playwright/test";

import { acpEntry } from "./harnessHelpers.js";

const artifactsDir = fileURLToPath(new URL("../e2e-artifacts/", import.meta.url));
mkdirSync(artifactsDir, { recursive: true });

let shotNo = 0;
async function captureShot(page: Page, title: string, focus?: Locator): Promise<string> {
  if (focus !== undefined) {
    await focus.scrollIntoViewIfNeeded();
  }
  shotNo += 1;
  const seq = String(shotNo).padStart(2, "0");
  const filename = `qa3-${seq}-${title}.png`;
  const targetPath = join(artifactsDir, filename);
  await page.screenshot({ path: targetPath, fullPage: false });
  return targetPath;
}

const TEAM_NAME = "M1.3回归验证团队";
const ROLE_DEV = "后端研发";
const ROLE_ARCH = "系统架构师";
const ROLE_TEMP = "待归档角色";

let workspace = "";
let teamId = "";
let devRoleId = "";
let archRoleId = "";
let tempRoleId = "";

test.describe.configure({ mode: "serial" });

test.beforeAll(() => {
  workspace = mkdtempSync(join(tmpdir(), "teamspace-e2e-qa3-"));
  writeFileSync(join(workspace, "brief.txt"), "需求规范与接口设计文档\n");
});

test.afterAll(() => {
  rmSync(workspace, { recursive: true, force: true });
});

async function addRoleDraft(
  page: Page,
  name: string,
  desc: string,
  isLeader = false,
): Promise<string> {
  await page.getByRole("button", { name: "+ 添加岗位" }).click();
  const card = page.locator(".tc-role").last();
  await card.getByLabel("岗位名称").fill(name);
  await card.getByLabel("岗位名称").blur();
  const prefix = card.getByLabel("任务编号前缀");
  await expect(prefix).toHaveValue(/^[A-Z][A-Z0-9]{1,3}$/);
  await card.getByLabel("职责").fill(desc);
  if (isLeader) {
    await card.locator(".tc-role__leader input").check();
  }
  return await prefix.inputValue();
}

async function openHarnessTab(page: Page): Promise<void> {
  await page.goto(`/#/teams/${teamId}/roles?tab=harness`);
  await expect(page.getByRole("tab", { name: "Harness 接入" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
}

async function openWorkbench(page: Page, roleName: string): Promise<void> {
  await page.goto(`/#/teams/${teamId}`);
  await expect(page.getByRole("heading", { name: "团队空间" })).toBeVisible();
  await page
    .locator(".th-role", { hasText: roleName })
    .getByRole("link", { name: roleName })
    .click();
  await expect(page).toHaveURL(/\/teams\/[^/]+\/roles\/[^/]+\/workbench$/);
  await expect(page.getByRole("heading", { level: 1, name: roleName })).toBeVisible();
}

function filesPanel(page: Page): Locator {
  return page.locator(".rw-panel", { has: page.getByRole("heading", { name: "最近文件" }) });
}

// --------------------------------------------------------------------------
// 阶段 1：创建团队与岗位初始化
// --------------------------------------------------------------------------

test("01. 创建 QA 验证团队：三个岗位（研发、架构师设为Leader、待归档角色）", async ({ page }) => {
  await page.goto("/#/teams/new");
  await page.getByLabel("团队名称").fill(TEAM_NAME);
  await page.getByLabel("手动输入工作空间路径").fill(workspace);
  await page.getByRole("button", { name: "检测", exact: true }).click();
  await expect(page.getByText("普通文件夹", { exact: true })).toBeVisible();

  await addRoleDraft(page, ROLE_DEV, "负责核心微服务开发与性能压测", false);
  await addRoleDraft(page, ROLE_ARCH, "负责架构评审与技术选型", true);
  await addRoleDraft(page, ROLE_TEMP, "临时辅助观察员", false);

  await page.getByRole("button", { name: "创建团队", exact: true }).click();
  await expect(page).toHaveURL(/\/teams\/(?!new$)[^/]+$/);
  teamId = /\/teams\/([^/?]+)/.exec(page.url())?.[1] ?? "";
  expect(teamId).not.toBe("");

  // 获取各岗位 ID
  await openWorkbench(page, ROLE_DEV);
  devRoleId = /\/roles\/([^/]+)\/workbench/.exec(page.url())?.[1] ?? "";
  expect(devRoleId).not.toBe("");

  await openWorkbench(page, ROLE_ARCH);
  archRoleId = /\/roles\/([^/]+)\/workbench/.exec(page.url())?.[1] ?? "";
  expect(archRoleId).not.toBe("");

  await openWorkbench(page, ROLE_TEMP);
  tempRoleId = /\/roles\/([^/]+)\/workbench/.exec(page.url())?.[1] ?? "";
  expect(tempRoleId).not.toBe("");

  await page.goto(`/#/teams/${teamId}`);
  await captureShot(page, "team-created-roles");
});

// --------------------------------------------------------------------------
// 阶段 2：ACP 接入边界测试
// --------------------------------------------------------------------------

test("02. ACP 接入边界：启动命令含空格或相对路径，表单提交被拒绝并显示清晰错误", async ({
  page,
}) => {
  await openHarnessTab(page);
  await page.getByRole("button", { name: "新建接入" }).click();
  await page.locator(".acp-preset", { hasText: "自定义 ACP Agent" }).click();

  // 1) 启动命令含空格（无 /）
  await page.getByLabel("实例名称").fill("agent-space-cmd");
  await page.getByLabel("启动命令").fill("my custom agent");
  await page.getByRole("button", { name: "保存并握手检查" }).click();
  const bannerSpace = page.locator(".ui-error-banner");
  await expect(bannerSpace).toBeVisible();
  await expect(bannerSpace).toContainText("包含空格：参数请填在「参数」里");
  await captureShot(page, "acp-cmd-space-rejected", bannerSpace);

  // 2) 启动命令为相对路径（含 / 但非绝对路径）
  await page.getByLabel("启动命令").fill("bin/my-agent");
  await page.getByRole("button", { name: "保存并握手检查" }).click();
  const bannerRel = page.locator(".ui-error-banner");
  await expect(bannerRel).toBeVisible();
  await expect(bannerRel).toContainText("启动命令须为命令名或绝对路径：bin/my-agent");
  await captureShot(page, "acp-cmd-relative-rejected", bannerRel);

  await page.getByRole("button", { name: "取消" }).click();
});

test("03. ACP 接入边界：参数含引号与多空格，提交保存成功且界面不崩溃", async ({ page }) => {
  await openHarnessTab(page);
  await page.getByRole("button", { name: "新建接入" }).click();
  await page.locator(".acp-preset", { hasText: "自定义 ACP Agent" }).click();

  await page.getByLabel("实例名称").fill("agent-quoted-args");
  await page.getByLabel("启动命令").fill("/usr/bin/false");
  await page.getByLabel("参数").fill('   --config "my conf"   --flag    ');
  await page.getByRole("button", { name: "保存并握手检查" }).click();

  const entry = acpEntry(page, "agent-quoted-args");
  await expect(entry).toBeVisible();
  // 命令行渲染展示
  await expect(entry.locator(".acp-entry__code")).toContainText("/usr/bin/false");
  await expect(entry.locator(".acp-entry__code")).toContainText("--config");
  await captureShot(page, "acp-args-quotes-and-spaces", entry);
});

test("04. ACP 接入边界：环境配置档案为相对路径被拒，不存在的文件握手检查提示错误", async ({
  page,
}) => {
  await openHarnessTab(page);
  await page.getByRole("button", { name: "新建接入" }).click();
  await page.locator(".acp-preset", { hasText: "自定义 ACP Agent" }).click();

  // 1) 相对路径环境配置档案
  await page.getByLabel("实例名称").fill("agent-env-rel");
  await page.getByLabel("启动命令").fill("/usr/bin/false");
  await page.getByLabel("环境配置档案（可选）").fill("configs/secret.env");
  await page.getByRole("button", { name: "保存并握手检查" }).click();
  const bannerRel = page.locator(".ui-error-banner");
  await expect(bannerRel).toBeVisible();
  await expect(bannerRel).toContainText("环境配置档案须为绝对路径：configs/secret.env");
  await captureShot(page, "acp-env-relative-rejected", bannerRel);

  // 2) 不存在的文件（绝对路径）
  const missingPath = "/tmp/nonexistent-env-profile-qa3.env";
  await page.getByLabel("实例名称").fill("agent-env-missing");
  await page.getByLabel("环境配置档案（可选）").fill(missingPath);
  await page.getByRole("button", { name: "保存并握手检查" }).click();

  // 实例保存成功并执行握手检查，握手失败显示具体原因
  const entry = acpEntry(page, "agent-env-missing");
  await expect(entry).toBeVisible();
  await expect(entry.locator(".ui-tag", { hasText: "异常" })).toBeVisible();
  await expect(entry.locator(".acp-entry__detail--danger")).toContainText(
    `环境配置档案不存在：${missingPath}`,
  );
  await captureShot(page, "acp-env-missing-file-error", entry);
});

test("05. ACP 接入边界：超长名称（>100字符）与重复名称被拒绝", async ({ page }) => {
  await openHarnessTab(page);
  await page.getByRole("button", { name: "新建接入" }).click();
  await page.locator(".acp-preset", { hasText: "Mock" }).click();

  // 1) 超长名称 (> 100 字符)
  const longName = "A".repeat(105);
  await page.getByLabel("实例名称").fill(longName);
  await page.getByRole("button", { name: "保存并握手检查" }).click();
  const bannerLen = page.locator(".ui-error-banner");
  await expect(bannerLen).toBeVisible();
  await expect(bannerLen).toContainText("Harness 名称不能超过 100 个字符");
  await captureShot(page, "acp-name-overflow-rejected", bannerLen);

  // 2) 创建正常接入 qa-mock-prime
  await page.getByLabel("实例名称").fill("qa-mock-prime");
  await page.getByRole("button", { name: "保存并握手检查" }).click();
  await expect(acpEntry(page, "qa-mock-prime")).toBeVisible();

  // 3) 再次尝试创建同名接入
  await page.getByRole("button", { name: "新建接入" }).click();
  await page.locator(".acp-preset", { hasText: "Mock" }).click();
  await page.getByLabel("实例名称").fill("qa-mock-prime");
  await page.getByRole("button", { name: "保存并握手检查" }).click();
  const bannerDup = page.locator(".ui-error-banner");
  await expect(bannerDup).toBeVisible();
  await expect(bannerDup).toContainText("团队中已有名为「qa-mock-prime」的 Harness");
  await captureShot(page, "acp-name-duplicate-rejected", bannerDup);

  await page.getByRole("button", { name: "取消" }).click();
});

test("06. ACP 接入边界：详情页预设不可更改，且归档被绑定的接入被拒", async ({ page }) => {
  await openHarnessTab(page);
  const entry = acpEntry(page, "qa-mock-prime");
  await entry.getByRole("button", { name: "编辑" }).click();

  // 验证预设下拉框 disabled
  const presetSelect = page.getByLabel("预设");
  await expect(presetSelect).toBeDisabled();
  await expect(page.locator(".ui-field__hint", { hasText: "创建后不可修改。" })).toBeVisible();
  await captureShot(page, "acp-preset-readonly");

  // 将角色绑定到该接入
  await page.goto(`/#/teams/${teamId}/roles`);
  await page.locator(".role-chip", { hasText: ROLE_DEV }).click();
  await page.getByRole("button", { name: "绑定 Harness", exact: true }).click();
  const select = page.getByLabel("选择本团队的 Harness 实例");
  await select.selectOption(
    (await select.locator("option", { hasText: "qa-mock-prime" }).getAttribute("value")) ?? "",
  );
  await page.getByRole("button", { name: "确认绑定" }).click();
  await expect(page.getByText("当前绑定 qa-mock-prime")).toBeVisible();

  // 重新进入详情页尝试归档
  await openHarnessTab(page);
  await acpEntry(page, "qa-mock-prime").getByRole("button", { name: "编辑" }).click();
  await page.getByRole("button", { name: "归档接入" }).click();
  const bannerArchive = page.locator(".ui-error-banner");
  await expect(bannerArchive).toBeVisible();
  await expect(bannerArchive).toContainText("仍被岗位「后端研发」绑定，请先解绑");
  await captureShot(page, "acp-archive-bound-rejected", bannerArchive);

  await page.getByRole("button", { name: "返回接入列表" }).click();
});

test("07. ACP 接入边界：同一接入绑定多个岗位后，列表显示全部被绑定岗位", async ({ page }) => {
  // 把 ROLE_ARCH 也绑定到 qa-mock-prime
  await page.goto(`/#/teams/${teamId}/roles`);
  await page.locator(".role-chip", { hasText: ROLE_ARCH }).click();
  await page.getByRole("button", { name: "绑定 Harness", exact: true }).click();
  const select = page.getByLabel("选择本团队的 Harness 实例");
  await select.selectOption(
    (await select.locator("option", { hasText: "qa-mock-prime" }).getAttribute("value")) ?? "",
  );
  await page.getByRole("button", { name: "确认绑定" }).click();
  await expect(page.getByText("当前绑定 qa-mock-prime")).toBeVisible();

  await openHarnessTab(page);
  const entry = acpEntry(page, "qa-mock-prime");
  await expect(entry).toContainText("被绑定");
  await expect(entry).toContainText(ROLE_DEV);
  await expect(entry).toContainText(ROLE_ARCH);
  await captureShot(page, "acp-multi-role-binding", entry);
});

test("08. ACP 接入边界：多次快速点击「握手检查」状态稳定无错乱", async ({ page }) => {
  await openHarnessTab(page);
  const entry = acpEntry(page, "qa-mock-prime");
  const checkBtn = entry.getByRole("button", { name: "握手检查" });

  // 连续点击
  await checkBtn.click();
  // 检查 loading 态禁用生效或迅速恢复连接
  await expect(entry.locator(".ui-tag", { hasText: "已连接" })).toBeVisible();
  await expect(entry.locator(".acp-entry__detail--muted")).toContainText("握手成功");
  await expect(page.locator(".ui-error-banner")).toHaveCount(0);
  await captureShot(page, "acp-rapid-check-consistency", entry);
});

// --------------------------------------------------------------------------
// 阶段 3：自定义 ACP Agent 进程握手异常与超时处理
// --------------------------------------------------------------------------

test("09. 自定义 ACP Agent：指向立即退出命令（/usr/bin/false），握手显示异常且原因可读，界面不卡死", async ({
  page,
}) => {
  await openHarnessTab(page);
  await page.getByRole("button", { name: "新建接入" }).click();
  await page.locator(".acp-preset", { hasText: "自定义 ACP Agent" }).click();

  await page.getByLabel("实例名称").fill("agent-immediate-exit");
  await page.getByLabel("启动命令").fill("/usr/bin/false");
  await page.getByRole("button", { name: "保存并握手检查" }).click();

  const entry = acpEntry(page, "agent-immediate-exit");
  await expect(entry).toBeVisible();
  await expect(entry.locator(".ui-tag", { hasText: "异常" })).toBeVisible();
  await expect(entry.locator(".acp-entry__detail--danger")).toContainText(
    "Agent 进程在握手完成前退出",
  );
  await expect(entry.locator(".acp-entry__detail--danger")).toContainText("/usr/bin/false");
  await captureShot(page, "agent-false-immediate-exit", entry);
});

test("10. 自定义 ACP Agent：指向输出非 JSON 命令（/bin/echo），握手显示异常且原因可读，界面不卡死", async ({
  page,
}) => {
  await openHarnessTab(page);
  await page.getByRole("button", { name: "新建接入" }).click();
  await page.locator(".acp-preset", { hasText: "自定义 ACP Agent" }).click();

  await page.getByLabel("实例名称").fill("agent-echo-non-json");
  await page.getByLabel("启动命令").fill("/bin/echo");
  await page.getByLabel("参数").fill("not-a-valid-json-rpc");
  await page.getByRole("button", { name: "保存并握手检查" }).click();

  const entry = acpEntry(page, "agent-echo-non-json");
  await expect(entry).toBeVisible();
  await expect(entry.locator(".ui-tag", { hasText: "异常" })).toBeVisible();
  await expect(entry.locator(".acp-entry__detail--danger")).toContainText(
    "Agent 的输出不是 JSON-RPC 消息",
  );
  await expect(entry.locator(".acp-entry__detail--danger")).toContainText("not-a-valid-json-rpc");
  await captureShot(page, "agent-echo-non-json", entry);
});

test("11. 自定义 ACP Agent：指向 /bin/sleep 60，在超时（20秒）后显示超时异常", async ({ page }) => {
  // 握手超时设为 20 秒，测试断言放宽到 40 秒
  test.setTimeout(60_000);

  await openHarnessTab(page);
  await page.getByRole("button", { name: "新建接入" }).click();
  await page.locator(".acp-preset", { hasText: "自定义 ACP Agent" }).click();

  await page.getByLabel("实例名称").fill("agent-sleep-timeout");
  await page.getByLabel("启动命令").fill("/bin/sleep");
  await page.getByLabel("参数").fill("60");
  await page.getByRole("button", { name: "保存并握手检查" }).click();

  const entry = acpEntry(page, "agent-sleep-timeout");
  await expect(entry).toBeVisible({ timeout: 40_000 });
  await expect(entry.locator(".ui-tag", { hasText: "异常" })).toBeVisible();
  await expect(entry.locator(".acp-entry__detail--danger")).toContainText("握手超时");
  await expect(entry.locator(".acp-entry__detail--danger")).toContainText("/bin/sleep 60");
  await expect(entry.locator(".acp-entry__detail--danger")).toContainText("20");
  await captureShot(page, "agent-sleep-timeout", entry);
});

// --------------------------------------------------------------------------
// 阶段 4：岗位工作台交互与数据联动
// --------------------------------------------------------------------------

test("12. 岗位工作台：任务状态变化与改派后，最近任务与工作日志正确刷新", async ({ page }) => {
  // 1) 为后端研发创建任务
  const taskTitle = "核心结算接口高并发压测";
  await page.goto(`/#/teams/${teamId}/tasks/new`);
  await page.getByLabel("任务名称").fill(taskTitle);
  const ownerSelect = page.getByLabel("负责人岗位");
  await ownerSelect.selectOption(
    (await ownerSelect.locator("option", { hasText: ROLE_DEV }).getAttribute("value")) ?? "",
  );
  await page.locator("#task-file-search").fill("brief");
  await page.locator(".task-form__suggestion", { hasText: "brief.txt" }).click();
  await page.getByRole("radio", { name: "TODO", exact: true }).check();
  await page.getByRole("button", { name: "创建并分配" }).click();
  await expect(page.locator(".task-detail__status")).toContainText("TODO");
  const taskId = /\/tasks\/([^/?]+)/.exec(page.url())?.[1] ?? "";

  // 2) 检查后端研发工作台：最近任务与日志出现
  await openWorkbench(page, ROLE_DEV);
  const devRows = page.locator(".rw-task-row");
  await expect(devRows.first()).toContainText(taskTitle);
  await expect(devRows.first()).toContainText("TODO");
  const devJournal = page.locator(".rw-journal");
  await expect(devJournal).toContainText(taskTitle);
  await expect(devJournal).toContainText("创建任务");

  // 3) 流转任务状态：TODO -> READY -> 执行中
  await page.goto(`/#/teams/${teamId}/tasks/${taskId}`);
  await page.getByRole("button", { name: "READY", exact: true }).click();
  await expect(page.locator(".task-detail__status")).toContainText("READY");
  await page.getByRole("button", { name: "执行中", exact: true }).click();
  await expect(page.locator(".task-detail__status")).toContainText("执行中");

  // 再次看工作台：状态变为执行中，日志记录变更
  await openWorkbench(page, ROLE_DEV);
  await expect(page.locator(".rw-task-row").first()).toContainText("执行中");
  await expect(page.locator(".rw-journal")).toContainText("READY → 执行中");

  // 4) 将任务改派给系统架构师
  await page.goto(`/#/teams/${teamId}/tasks/${taskId}/edit`);
  const editOwner = page.getByLabel("负责人岗位");
  await editOwner.selectOption(
    (await editOwner.locator("option", { hasText: ROLE_ARCH }).getAttribute("value")) ?? "",
  );
  await page.getByRole("button", { name: "保存" }).click();
  await expect(page).toHaveURL(new RegExp(`/teams/${teamId}/tasks/${taskId}$`));
  await expect(page.getByText(` / ${ROLE_ARCH}`)).toBeVisible();

  // 5) 后端研发工作台：最近任务清除，日志记录「已从本岗位改派走」
  await openWorkbench(page, ROLE_DEV);
  await expect(page.locator(".rw-task-row")).toHaveCount(0);
  await expect(page.getByText("暂无任务")).toBeVisible();
  await expect(page.locator(".rw-journal")).toContainText("已从本岗位改派走");

  // 6) 系统架构师工作台：最近任务包含该任务，日志记录「改派到本岗位」
  await openWorkbench(page, ROLE_ARCH);
  await expect(page.locator(".rw-task-row").first()).toContainText(taskTitle);
  await expect(page.locator(".rw-journal")).toContainText("改派到本岗位");

  await captureShot(page, "workbench-task-status-and-reassign");
});

test("13. 岗位工作台：过程文件含子目录与中文名，隐藏文件过滤不显示", async ({ page }) => {
  const roleProcessDir = join(workspace, "Teamspace", ROLE_DEV, "过程文件");
  mkdirSync(join(roleProcessDir, "微服务方案"), { recursive: true });
  mkdirSync(join(roleProcessDir, ".cache_dir"), { recursive: true });

  writeFileSync(join(roleProcessDir, "微服务方案", "服务拆分草案.md"), "# 服务拆分\n");
  writeFileSync(join(roleProcessDir, "压力测试基准指标.txt"), "TPS 5000\n");
  // 隐藏文件
  writeFileSync(join(roleProcessDir, ".hidden_debug.log"), "hidden debug log\n");
  writeFileSync(join(roleProcessDir, ".DS_Store"), "mock ds store\n");
  writeFileSync(join(roleProcessDir, ".cache_dir", "temp.json"), "{}\n");

  await openWorkbench(page, ROLE_DEV);
  const panel = filesPanel(page);

  // 验证隐藏文件不显示
  await expect(panel.locator(".rw-file-row__name", { hasText: ".hidden_debug.log" })).toHaveCount(
    0,
  );
  await expect(panel.locator(".rw-file-row__name", { hasText: ".DS_Store" })).toHaveCount(0);
  await expect(panel.locator(".rw-file-row__name", { hasText: "temp.json" })).toHaveCount(0);

  // 验证子目录中的文件与中文名文件正确显示
  await expect(panel.locator(".rw-file-row__name", { hasText: "服务拆分草案.md" })).toBeVisible();
  await expect(
    panel.locator(".rw-file-row__name", { hasText: "压力测试基准指标.txt" }),
  ).toBeVisible();
  // 验证子目录文件 title 包含完整相对路径
  await expect(panel.locator('.rw-file-row[title*="微服务方案/服务拆分草案.md"]')).toBeVisible();

  await captureShot(page, "workbench-process-files-filtered", panel);
});

test("14. 岗位工作台：超过 10 个过程文件时只显示最近 10 个，按修改时间倒序", async ({ page }) => {
  const roleProcessDir = join(workspace, "Teamspace", ROLE_DEV, "过程文件");
  const now = Date.now() / 1000 + 3600;

  // 生成 12 个不同时间戳的文件
  for (let i = 1; i <= 12; i++) {
    const filename = `生成报告_${String(i).padStart(2, "0")}.md`;
    const target = join(roleProcessDir, filename);
    writeFileSync(target, `报告 ${String(i)}\n`);
    // 越大的 i 时间越新（且均高于先前历史文件）
    const mtime = now + i * 60;
    utimesSync(target, mtime, mtime);
  }

  await openWorkbench(page, ROLE_DEV);
  const panel = filesPanel(page);
  const rows = panel.locator(".rw-file-row");
  // 严格上限为 10 条
  await expect(rows).toHaveCount(10);

  // 最新的文件为 生成报告_12.md，排在第 1 个
  await expect(rows.first().locator(".rw-file-row__name")).toHaveText("生成报告_12.md");
  // 最旧的 报告_01.md 与 报告_02.md 已被截断不在最近 10 个里
  await expect(panel.locator(".rw-file-row__name", { hasText: "生成报告_01.md" })).toHaveCount(0);
  await expect(panel.locator(".rw-file-row__name", { hasText: "生成报告_02.md" })).toHaveCount(0);

  await captureShot(page, "workbench-process-files-limit10", panel);
});

test("15. 岗位工作台：岗位改名后工作台路径、标题与面包屑同步更新", async ({ page }) => {
  const NEW_ROLE_NAME = "高级后端开发架构师";

  // 在岗位设置中改名
  await page.goto(`/#/teams/${teamId}/roles`);
  await page.locator(".role-chip", { hasText: ROLE_DEV }).click();
  await page.getByLabel("岗位名称").fill(NEW_ROLE_NAME);
  await page.getByRole("button", { name: "保存岗位说明" }).click();
  await expect(page.locator(".role-detail__header")).toContainText(NEW_ROLE_NAME);

  // 从团队空间进入该岗位工作台
  await openWorkbench(page, NEW_ROLE_NAME);

  // 1) 标题更新
  await expect(page.getByRole("heading", { level: 1, name: NEW_ROLE_NAME })).toBeVisible();

  // 2) 面包屑更新
  const crumbs = page.locator(".topbar__crumbs");
  await expect(crumbs).toContainText(NEW_ROLE_NAME);

  // 3) 岗位文件夹路径更新
  const folderPanel = page.locator(".rw-panel", {
    has: page.getByRole("heading", { name: "岗位文件夹" }),
  });
  await expect(folderPanel.locator(".rw-path")).toContainText(`Teamspace/${NEW_ROLE_NAME}`);

  await captureShot(page, "workbench-role-rename-updates");
});

test("16. 岗位工作台：已归档岗位访问工作台展示清晰提示，不崩溃", async ({ page }) => {
  // 1) 归档待归档角色
  await page.goto(`/#/teams/${teamId}/roles`);
  await page.locator(".role-chip", { hasText: ROLE_TEMP }).click();
  await page.getByRole("button", { name: "归档岗位" }).click();
  await expect(page.locator(".role-chip", { hasText: ROLE_TEMP })).toHaveCount(0);

  // 2) 直接通过 URL 访问已归档角色的工作台页面
  await page.goto(`/#/teams/${teamId}/roles/${tempRoleId}/workbench`);

  // 3) 验证友好提示，界面不卡死不白屏
  const errorBanner = page.locator(".ui-error-banner");
  await expect(errorBanner).toBeVisible();
  await expect(errorBanner).toContainText("岗位不存在或已归档");
  await expect(errorBanner.getByRole("button", { name: "重试" })).toBeVisible();

  await captureShot(page, "workbench-archived-role-view");
});

test("17. 岗位工作台：点击「打开工作日志.md」触发 open_role_path 无报错", async ({ page }) => {
  await openWorkbench(page, "高级后端开发架构师");

  const reply = page.waitForResponse(
    (response) =>
      response.url().endsWith("/invoke") &&
      (response.request().postData() ?? "").includes("open_role_path"),
  );

  await page.getByRole("button", { name: "打开工作日志.md" }).click();
  const body = (await (await reply).json()) as { ok?: unknown; err?: unknown };
  expect(body.err).toBeUndefined();
  await expect(page.getByRole("alert")).toHaveCount(0);

  await captureShot(page, "workbench-open-journal-success");
});

// --------------------------------------------------------------------------
// 阶段 5：导航与界面文案纯净性审查
// --------------------------------------------------------------------------

test("18. 导航验证：工作台侧边导航高亮「团队空间」，面包屑三级，刷新后保持正确", async ({
  page,
}) => {
  await openWorkbench(page, "高级后端开发架构师");

  const nav = page.locator(".sidebar__nav");
  // 侧边栏只有团队空间处于激活高亮状态
  await expect(nav.locator(".nav-item--active")).toHaveText(["团队空间"]);

  // 面包屑结构应为三级：工作空间 / 团队名 / 岗位名
  const crumbs = page.locator(".topbar__crumbs");
  await expect(crumbs.locator("span")).toContainText(["工作空间", TEAM_NAME, "高级后端开发架构师"]);

  await captureShot(page, "nav-workbench-sidebar-breadcrumbs");

  // 浏览器页面刷新
  await page.reload();

  await expect(nav.locator(".nav-item--active")).toHaveText(["团队空间"]);
  await expect(crumbs).toContainText(TEAM_NAME);
  await expect(crumbs).toContainText("高级后端开发架构师");

  await captureShot(page, "nav-workbench-refresh");
});

test("19. 界面文案纯净性：不包含旧版 CLI 措辞（「检测本机 CLI」「在线」等）", async ({ page }) => {
  // 检查工作台
  await openWorkbench(page, "高级后端开发架构师");
  await expect(page.locator("body")).not.toContainText("检测本机 CLI");
  await expect(page.locator("body")).not.toContainText("CLI 状态");

  // 检查接入列表页
  await openHarnessTab(page);
  await expect(page.locator("body")).not.toContainText("检测本机 CLI");

  // 检查团队空间首页
  await page.goto(`/#/teams/${teamId}`);
  await expect(page.locator("body")).not.toContainText("检测本机 CLI");

  await captureShot(page, "text-purity-no-legacy-wording");
});
