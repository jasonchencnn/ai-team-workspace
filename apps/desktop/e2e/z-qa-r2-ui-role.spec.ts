// apps/desktop/e2e/z-qa-r2-ui-role.spec.ts
// QA 第二轮 · 岗位/岗位工作台（UI-ROLE-002/003/004/006/007/008/010/011/012、UI-WORK-003/004/005/009）。
// 真实界面 + 测试桥接 + 真实内核 + 临时普通文件夹工作空间；只用内置 Mock ACP 预设，不连任何真实 Agent、不联网。
// 本文件自建团队（每次 `pnpm run e2e` 都是全新临时库），不依赖其它用例文件的数据。
import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { type Locator, type Page, expect, test } from "@playwright/test";

import { createMockHarness } from "./harnessHelpers.js";

const artifacts = fileURLToPath(new URL("../e2e-artifacts/", import.meta.url));
mkdirSync(artifacts, { recursive: true });

let shotNo = 0;
async function shot(page: Page, name: string, focus?: Locator): Promise<void> {
  if (focus !== undefined) {
    await focus.scrollIntoViewIfNeeded();
  }
  shotNo += 1;
  await page.screenshot({
    path: `${artifacts}qa-ui2-${String(shotNo).padStart(2, "0")}-${name}.png`,
    fullPage: false,
  });
}

const TEAM_A = "岗位UI回归团队";
const TEAM_B = "无接入团队";
const ROLE_ASSIST = "助手"; // 团队里的第一个岗位 → role_1
const ROLE_LEAD = "领队";
const ROLE_PREFIX = "前缀测试员";
const ROLE_TEMP = "临时观察员";
const ROLE_B = "无接入观察员";
const MOCK = "qa-ui2-mock";
const DUTY_ASSIST = "整理需求清单并同步进度";
const GUIDELINES_LEAD = "每天先同步当天计划";
const RUN_TASK = "整理发布说明";

// 岗位文件夹所在目录：内核只有在需要写入时才会失败，所以「不可写」用例先 chmod 再触发一次写入。
let workspace = "";
let workspaceB = "";
let teamId = "";
let teamBId = "";

test.describe.configure({ mode: "serial" });

test.beforeAll(() => {
  workspace = mkdtempSync(join(tmpdir(), "teamspace-e2e-qa-ui2-"));
  writeFileSync(join(workspace, "brief.txt"), "需求说明\n");
  workspaceB = mkdtempSync(join(tmpdir(), "teamspace-e2e-qa-ui2-b-"));
  writeFileSync(join(workspaceB, "brief.txt"), "需求说明\n");
});

test.afterAll(() => {
  rmSync(workspace, { recursive: true, force: true });
  rmSync(workspaceB, { recursive: true, force: true });
});

// ---------------------------------------------------------------- 辅助

/** 创建团队（普通文件夹工作空间），可选岗位；返回团队 id。 */
async function createTeam(
  page: Page,
  name: string,
  ws: string,
  roles: { name: string; description: string }[] = [],
): Promise<string> {
  await page.goto("/#/teams/new");
  await page.getByLabel("团队名称").fill(name);
  await page.getByLabel("手动输入工作空间路径").fill(ws);
  await page.getByRole("button", { name: "检测", exact: true }).click();
  await expect(page.getByText("普通文件夹", { exact: true })).toBeVisible();
  for (const role of roles) {
    await addRoleOnCreateTeam(page, role.name, role.description);
  }
  await page.getByRole("button", { name: "创建团队", exact: true }).click();
  await expect(page).toHaveURL(/\/teams\/(?!new$)[^/]+$/);
  const id = /\/teams\/([^/?]+)/.exec(page.url())?.[1] ?? "";
  expect(id).not.toBe("");
  return id;
}

/** 创建团队页里的自定义岗位行：名称失焦拿到前缀建议后返回前缀。 */
async function addRoleOnCreateTeam(page: Page, name: string, description = ""): Promise<string> {
  await page.getByRole("button", { name: "+ 添加岗位" }).click();
  const card = page.locator(".tc-role").last();
  await card.getByLabel("岗位名称").fill(name);
  await card.getByLabel("岗位名称").blur();
  const prefix = card.getByLabel("任务编号前缀");
  await expect(prefix).toHaveValue(/^[A-Z][A-Z0-9]{1,3}$/);
  if (description !== "") {
    await card.getByLabel("职责").fill(description);
  }
  return await prefix.inputValue();
}

/** 「新增岗位」表单；`prefix` 为空时保留内核建议的前缀。 */
async function openRoleCreateForm(page: Page): Promise<Locator> {
  await page.goto(`/#/teams/${teamId}/roles`);
  await page.getByRole("button", { name: "新增岗位" }).first().click();
  const form = page.locator(".role-create");
  await expect(form).toBeVisible();
  return form;
}

async function fillRoleCreateForm(
  form: Locator,
  name: string,
  options: { prefix?: string; description?: string; guidelines?: string; leader?: boolean } = {},
): Promise<void> {
  await form.getByLabel("岗位名称").fill(name);
  await form.getByLabel("岗位名称").blur();
  // 建议前缀先落地，再手填，避免迟到的建议覆盖手填值。
  await expect(form.getByLabel("任务编号前缀")).toHaveValue(/^[A-Z][A-Z0-9]{1,3}$/);
  if (options.prefix !== undefined) {
    await form.getByLabel("任务编号前缀").fill(options.prefix);
  }
  if (options.description !== undefined) {
    await form.getByLabel("职责").fill(options.description);
  }
  if (options.guidelines !== undefined) {
    await form.getByLabel("工作约定").fill(options.guidelines);
  }
  if (options.leader === true) {
    await form.getByLabel("设为 Leader").check();
  }
}

/** 选中左侧列表里的岗位，右侧详情随之切换。 */
async function openRole(page: Page, team: string, name: string): Promise<void> {
  await page.goto(`/#/teams/${team}/roles`);
  await page.locator(".role-chip", { hasText: name }).click();
  await expect(page.getByRole("heading", { level: 2, name })).toBeVisible();
}

/** 从团队空间的岗位卡进入岗位工作台。 */
async function openWorkbench(page: Page, team: string, name: string): Promise<void> {
  await page.goto(`/#/teams/${team}`);
  await expect(page.getByRole("heading", { name: "团队空间" })).toBeVisible();
  await page.locator(".th-role", { hasText: name }).getByRole("link", { name }).click();
  await expect(page).toHaveURL(/\/teams\/[^/]+\/roles\/[^/]+\/workbench$/);
  await expect(page.getByRole("heading", { level: 1, name })).toBeVisible();
}

function bindingPanel(page: Page): Locator {
  return page.locator(".rw-panel", { has: page.getByRole("heading", { name: "当前绑定" }) });
}

function runsPanel(page: Page): Locator {
  return page.locator(".rw-panel", { has: page.getByRole("heading", { name: "最近执行" }) });
}

/** 岗位详情里的「岗位文件夹」区块。 */
function roleFolderSection(page: Page): Locator {
  return page.locator('[aria-label="岗位文件夹"]');
}

/** 保存岗位说明；不可写的用例用它触发一次真实的文件夹写入。 */
async function saveRoleDetail(page: Page): Promise<void> {
  await page.getByRole("button", { name: "保存岗位说明" }).click();
  await expect(page.getByRole("alert")).toHaveCount(0);
}

/** 创建任务并返回任务编号；`ref` 为 true 时引用 brief.txt。 */
async function createTask(
  page: Page,
  team: string,
  input: { title: string; owner: string; ref?: boolean; status?: "TODO" | "READY" },
): Promise<string> {
  await page.goto(`/#/teams/${team}/tasks/new`);
  await page.getByLabel("任务名称").fill(input.title);
  const owner = page.getByLabel("负责人岗位");
  await owner.selectOption(
    (await owner.locator("option", { hasText: input.owner }).getAttribute("value")) ?? "",
  );
  if (input.ref === true) {
    await page.locator("#task-file-search").fill("brief");
    await page.locator(".task-form__suggestion", { hasText: "brief.txt" }).click();
  }
  await page.getByRole("radio", { name: input.status ?? "TODO", exact: true }).check();
  await page.getByRole("button", { name: "创建并分配" }).click();
  const status = page.locator(".task-detail__status");
  await expect(status).toContainText(input.status ?? "TODO");
  const key = /\/ (\S+)$/.exec((await status.innerText()).trim())?.[1] ?? "";
  expect(key).not.toBe("");
  return key;
}

/** 把当前任务详情页的任务推到指定状态。 */
async function transitionTo(page: Page, labels: string[]): Promise<void> {
  for (const label of labels) {
    await page.getByRole("button", { name: label, exact: true }).click();
    await expect(page.locator(".task-detail__status")).toContainText(label);
  }
}

/** 目录当前是否还能写入（chmod 是否真的生效）。 */
function canWrite(dir: string): boolean {
  const probe = join(dir, ".qa-ui2-write-probe");
  try {
    writeFileSync(probe, "probe\n");
    rmSync(probe, { force: true });
    return true;
  } catch {
    return false;
  }
}

/**
 * 把 Teamspace 目录改成「不可读写」。
 * 注意：仅 0o500（r-x）只能挡住新建/追加文件，POSIX 下仍可覆盖已存在的文件，
 * 岗位说明.md 的重写照样成功，不会触发产品侧的不可写告警；故这里用 0o000，
 * 让读取同样失败，工作台才会读到 folder.problem 并给出兜底告警卡。
 * `blocked` 为 false 表示 chmod 对当前用户（如 root）无效，调用方据此跳过相关断言。
 */
function makeFolderUnwritable(): { restore: () => void; blocked: boolean } {
  const dir = join(workspace, "Teamspace");
  chmodSync(dir, 0o000);
  return {
    blocked: !canWrite(dir),
    restore: () => {
      chmodSync(dir, 0o700);
    },
  };
}

// ---------------------------------------------------------------- 用例

test("UI-ROLE-002 空状态：团队无岗位时显示「还没有岗位 / 先新增岗位，再为它绑定一个执行工具。」", async ({
  page,
}) => {
  teamId = await createTeam(page, TEAM_A, workspace);
  await page.goto(`/#/teams/${teamId}/roles`);

  // 左侧列表区标题与空态
  await expect(page.locator(".role-list__title")).toHaveText("团队岗位");
  await expect(page.locator(".role-chip")).toHaveCount(0);
  const list = page.locator(".role-list");
  await expect(list.getByText("还没有岗位")).toBeVisible();
  await expect(list.getByText("新增岗位后即可为它绑定执行者。")).toBeVisible();

  // 右侧详情区空态（带「新增岗位」按钮的那一块）
  const detailEmpty = page.locator(".ui-empty", {
    has: page.getByRole("button", { name: "新增岗位" }),
  });
  await expect(detailEmpty.getByText("还没有岗位")).toBeVisible();
  await expect(detailEmpty.getByText("先新增岗位，再为它绑定一个执行工具。")).toBeVisible();
  await shot(page, "roles-empty-state", detailEmpty);

  // 空态里的入口能打开新增岗位表单
  await detailEmpty.getByRole("button", { name: "新增岗位" }).click();
  await expect(page.locator(".role-create")).toBeVisible();
  await page.getByRole("button", { name: "取消" }).click();
});

test("UI-ROLE-002 空状态：无 teamId 的岗位路由应显示「未选择团队 / 请先在左侧选择一个团队。」", async ({
  page,
}) => {
  test.fixme(
    true,
    "缺陷 UI-ROLE-002: 路由表只有 teams/:teamId/roles，teamId 恒有值，#/roles 落到 IndexRedirect，「未选择团队」分支不可达",
  );
  await page.goto("/#/roles");
  await expect(page.getByRole("heading", { name: "未选择团队" })).toBeVisible();
  await expect(page.getByText("请先在左侧选择一个团队。")).toBeVisible();
});

test("UI-ROLE-010 从示例添加：加载中显示「正在加载示例…」，选中示例预填表单，按钮变「收起示例」", async ({
  page,
}) => {
  await openRoleCreateForm(page);

  // 延迟示例列表的响应，稳定观察到加载态。
  let release: () => void = () => undefined;
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.route("**/invoke", async (route) => {
    if ((route.request().postData() ?? "").includes("list_role_templates")) {
      await gate;
    }
    await route.continue();
  });

  await page.getByRole("button", { name: "从示例添加" }).click();
  await expect(page.locator(".role-create").getByText("正在加载示例…")).toBeVisible();
  await shot(page, "role-examples-loading");
  release();

  // 选中一个示例：内容预填进表单，按钮切成「收起示例」
  const example = page.locator(".role-create__example-list button", { hasText: "产品经理：" });
  await expect(example).toBeVisible();
  await example.click();
  const form = page.locator(".role-create");
  await expect(form.getByLabel("岗位名称")).toHaveValue("产品经理");
  await expect(form.getByLabel("任务编号前缀")).toHaveValue("PM");
  await expect(form.getByLabel("职责")).toHaveValue("需求澄清、验收标准与优先级。");
  await expect(page.getByRole("button", { name: "收起示例" })).toBeVisible();
  await shot(page, "role-examples-prefilled", form);

  // 预填只是快捷方式：不保存，团队仍然没有岗位。
  await page.getByRole("button", { name: "取消" }).click();
  await expect(page.locator(".role-chip")).toHaveCount(0);
});

test("UI-ROLE-010 从示例添加：无可用示例时显示「没有可用的示例岗位，直接自定义即可。」且表单仍可用", async ({
  page,
}) => {
  await page.route("**/invoke", async (route) => {
    if ((route.request().postData() ?? "").includes("list_role_templates")) {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({ ok: [] }),
      });
      return;
    }
    await route.continue();
  });

  const form = await openRoleCreateForm(page);
  await page.getByRole("button", { name: "从示例添加" }).click();
  await expect(form.getByText("没有可用的示例岗位，直接自定义即可。")).toBeVisible();
  await expect(form.locator(".role-create__example-list")).toHaveCount(0);
  await shot(page, "role-examples-none");

  // 表单仍可用：自定义字段照常可输入，保存入口保持可用。
  // 这里不真的落库，避免占用本团队的第一个岗位标识（UI-ROLE-004 要断言 role_1）。
  await form.getByLabel("岗位名称").fill("示例为空时的岗位");
  await form.getByLabel("任务编号前缀").fill("R1");
  await expect(form.getByLabel("岗位名称")).toHaveValue("示例为空时的岗位");
  await expect(form.getByLabel("任务编号前缀")).toHaveValue("R1");
  await expect(form.getByRole("button", { name: "创建岗位" })).toBeEnabled();
  await expect(form.getByRole("alert")).toHaveCount(0);
  await page.getByRole("button", { name: "取消" }).click();
});

test("UI-ROLE-010 从示例添加：加载失败时显示可重试的错误条", async ({ page }) => {
  test.fixme(
    true,
    "缺陷 UI-ROLE-010: 示例加载失败的 ErrorBanner 没有「重试」按钮，ErrorBanner 也没有 action 传入",
  );
  await page.route("**/invoke", async (route) => {
    if ((route.request().postData() ?? "").includes("list_role_templates")) {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({ err: { kind: "internal", message: "读取示例岗位失败" } }),
      });
      return;
    }
    await route.continue();
  });

  await openRoleCreateForm(page);
  await page.getByRole("button", { name: "从示例添加" }).click();
  const alert = page.locator(".role-create").getByRole("alert");
  await expect(alert).toBeVisible();
  await expect(alert).toContainText("读取示例岗位失败");
  await expect(alert.getByRole("button", { name: "重试" })).toBeVisible();
  await shot(page, "role-examples-failed", alert);
});

test("UI-ROLE-006 Leader：勾选「设为 Leader」保存后按 Leader 展示，不勾选则是普通岗位", async ({
  page,
}) => {
  const form = await openRoleCreateForm(page);
  // 先建普通岗位，再建 Leader 岗位：团队空间按 Leader 在前排序
  await fillRoleCreateForm(form, ROLE_ASSIST, { description: DUTY_ASSIST, prefix: "AS" });
  await form.getByRole("button", { name: "创建岗位" }).click();
  await expect(page.locator(".role-chip", { hasText: ROLE_ASSIST })).toBeVisible();

  const leaderForm = await openRoleCreateForm(page);
  await fillRoleCreateForm(leaderForm, ROLE_LEAD, {
    prefix: "LD",
    description: "拆解需求并分派",
    guidelines: GUIDELINES_LEAD,
    leader: true,
  });
  await leaderForm.getByRole("button", { name: "创建岗位" }).click();
  await expect(page.locator(".role-chip", { hasText: ROLE_LEAD })).toBeVisible();
  await expect(page.locator(".role-chip", { hasText: ROLE_LEAD })).toHaveText(
    new RegExp(`^${ROLE_LEAD}\\s*\\/\\s*未绑定$`),
  );

  // 团队空间：Leader 在前
  await page.goto(`/#/teams/${teamId}`);
  const cards = page.locator(".th-roles .th-role");
  await expect(cards).toHaveCount(2);
  await expect(cards.nth(0)).toContainText(ROLE_LEAD);
  await expect(cards.nth(1)).toContainText(ROLE_ASSIST);

  // 工作台岗位信息：Leader 岗位 / 普通岗位
  await openWorkbench(page, teamId, ROLE_LEAD);
  await expect(page.locator(".rw-meta")).toContainText("Leader 岗位");
  await expect(page.locator(".rw-meta__prefix")).toHaveText("LD");
  await shot(page, "leader-workbench-meta", page.locator(".rw-meta"));

  await openWorkbench(page, teamId, ROLE_ASSIST);
  await expect(page.locator(".rw-meta")).toContainText("普通岗位");
  await expect(page.locator(".rw-meta")).not.toContainText("Leader 岗位");
  await shot(page, "plain-workbench-meta", page.locator(".rw-meta"));
});

test("UI-ROLE-003 列表：标题「团队岗位」，未绑定项显示「未绑定」与「绑定执行者」，已绑定项显示接入名", async ({
  page,
}) => {
  await page.goto(`/#/teams/${teamId}/roles?tab=harness`);
  await createMockHarness(page, MOCK);

  await openRole(page, teamId, ROLE_ASSIST);
  await expect(page.locator(".role-list__title")).toHaveText("团队岗位");
  const chips = page.locator(".role-chip");
  await expect(chips).toHaveCount(2);

  // 未绑定且被选中：显示「未绑定」与「未绑定 Harness · 暂不派工」
  await expect(chips.filter({ hasText: ROLE_ASSIST })).toHaveText(
    new RegExp(`^${ROLE_ASSIST}\\s*\\/\\s*未绑定$`),
  );
  await expect(page.locator(".role-list__unbound")).toHaveText("未绑定 Harness · 暂不派工");

  // 未绑定且未被选中：给「绑定执行者」按钮
  const leaderItem = page.locator(".role-list__item", { hasText: ROLE_LEAD });
  await expect(leaderItem.getByRole("button", { name: "绑定执行者" })).toBeVisible();
  await shot(page, "role-list-unbound", page.locator(".role-list"));

  // 绑定后：列表项同步显示接入名
  await leaderItem.getByRole("button", { name: "绑定执行者" }).click();
  await page
    .getByLabel("选择本团队的 Harness 实例")
    .selectOption(
      (await page
        .getByLabel("选择本团队的 Harness 实例")
        .locator("option", { hasText: MOCK })
        .getAttribute("value")) ?? "",
    );
  await page.getByRole("button", { name: "确认绑定" }).click();
  await expect(chips.filter({ hasText: ROLE_LEAD })).toHaveText(
    new RegExp(`^${ROLE_LEAD}\\s*\\/\\s*${MOCK}$`),
  );
  await shot(page, "role-list-bound", page.locator(".role-list"));

  // 测试实例的提示，避免误绑正式岗位
  await expect(page.getByText("这是测试用实例：仅用于验证流程，不要绑定正式岗位。")).toBeVisible();
});

test("UI-ROLE-004 详情：自动生成标识与字段齐备，三个操作按钮在位，新增表单无岗位代码输入框", async ({
  page,
}) => {
  await openRole(page, teamId, ROLE_ASSIST);
  const detail = page.locator(".role-detail");
  await expect(detail.locator(".role-detail__header")).toContainText(ROLE_ASSIST);
  // 岗位标识由后端自动生成，界面不提供输入
  await expect(detail.locator(".role-detail__meta")).toHaveText(
    /岗位标识 role_1（自动生成） · 创建于 \d{4}\.\d{2}\.\d{2}/,
  );

  await expect(detail.getByLabel("岗位名称")).toHaveValue(ROLE_ASSIST);
  await expect(detail.getByLabel("编号前缀")).toHaveValue("AS");
  await expect(detail.getByLabel("职责")).toHaveValue(DUTY_ASSIST);
  await expect(detail.getByLabel("工作约定")).toHaveValue("");

  await expect(detail.getByRole("button", { name: "保存岗位说明" })).toBeVisible();
  await expect(detail.getByRole("button", { name: "归档岗位" })).toBeVisible();
  await expect(detail.getByRole("button", { name: "绑定 Harness", exact: true })).toBeVisible();
  await shot(page, "role-detail-fields", detail);

  // 新增表单里没有任何岗位代码/标识输入项
  const form = await openRoleCreateForm(page);
  await expect(form.getByLabel(/岗位(代码|标识)/)).toHaveCount(0);
  await expect(form.locator("input[name='code']")).toHaveCount(0);
  await expect(form.getByLabel("任务编号前缀")).toBeVisible();
  await shot(page, "role-create-no-code-input", form);
  await page.getByRole("button", { name: "取消" }).click();
});

test("UI-ROLE-007 前缀校验：R / ABCDE / 1AB / R-1 四种非法前缀均被拒并提示「需为 2–4 位大写字母或数字，且以字母开头」", async ({
  page,
}) => {
  const form = await openRoleCreateForm(page);
  await form.getByLabel("岗位名称").fill("前缀校验岗位");
  await form.getByLabel("岗位名称").blur();
  await expect(form.getByLabel("任务编号前缀")).toHaveValue(/^[A-Z][A-Z0-9]{1,3}$/);

  const prefix = form.getByLabel("任务编号前缀");
  const alert = form.getByRole("alert");
  for (const bad of ["R", "ABCDE", "1AB", "R-1"]) {
    await prefix.fill(bad);
    await form.getByRole("button", { name: "创建岗位" }).click();
    await expect(alert).toContainText(`任务前缀「${bad}」格式不正确`);
    await expect(alert).toContainText("需为 2–4 位大写字母或数字，且以字母开头");
    // 保存被拒绝：岗位没有被创建，表单仍在
    await expect(page.locator(".role-chip", { hasText: "前缀校验岗位" })).toHaveCount(0);
    await expect(form).toBeVisible();
    await shot(page, `prefix-rejected-${bad}`, alert);
  }

  await page.getByRole("button", { name: "取消" }).click();
});

test("UI-ROLE-008 前缀：填小写 dev1 存为大写 DEV1，改成 R9 后已有任务编号不变、新任务用 R9", async ({
  page,
}) => {
  const form = await openRoleCreateForm(page);
  await fillRoleCreateForm(form, ROLE_PREFIX, {
    prefix: "dev1",
    description: "验证前缀与历史编号",
  });
  await form.getByRole("button", { name: "创建岗位" }).click();
  await expect(page.locator(".role-chip", { hasText: ROLE_PREFIX })).toBeVisible();

  // 存储为大写：详情里的编号前缀与工作台都用 DEV1
  await openRole(page, teamId, ROLE_PREFIX);
  await expect(page.getByLabel("编号前缀")).toHaveValue("DEV1");
  await openWorkbench(page, teamId, ROLE_PREFIX);
  await expect(page.locator(".rw-meta__prefix")).toHaveText("DEV1");

  // 新任务编号使用 DEV1
  const firstKey = await createTask(page, teamId, {
    title: "前缀历史验证一",
    owner: ROLE_PREFIX,
    status: "READY",
  });
  expect(firstKey).toBe("DEV1-001");
  const firstUrl = page.url();

  // 改成 R9 并保存
  await openRole(page, teamId, ROLE_PREFIX);
  await page.getByLabel("编号前缀").fill("R9");
  await page.getByRole("button", { name: "保存岗位说明" }).click();
  await expect(page.getByLabel("编号前缀")).toHaveValue("R9");
  await shot(page, "prefix-updated-r9");

  // 已有任务编号不受影响
  await page.goto(firstUrl);
  await expect(page.locator(".task-detail__status")).toContainText(firstKey);
  // 新任务使用 R9
  const secondKey = await createTask(page, teamId, {
    title: "前缀历史验证二",
    owner: ROLE_PREFIX,
    status: "READY",
  });
  // 用例只要求「新任务使用 R9」前缀；序号按前缀分段计数，故为 R9-001，不额外断言序号。
  expect(secondKey).toMatch(/^R9-\d+$/);

  // 刷新后仍然如此
  await openWorkbench(page, teamId, ROLE_PREFIX);
  await expect(page.locator(".rw-meta__prefix")).toHaveText("R9");
  await page.reload();
  await expect(page.locator(".rw-meta__prefix")).toHaveText("R9");
  await expect(page.locator(".rw-task-row__key")).toHaveText([secondKey, firstKey]);
  const brief = readFileSync(join(workspace, "Teamspace", ROLE_PREFIX, "岗位说明.md"), "utf8");
  expect(brief).toContain("- 任务前缀：R9");
  await shot(page, "prefix-r9-history");
});

test("UI-ROLE-011 绑定：确认后按钮变「更换 Harness」并显示「当前绑定」卡片，列表同步；解绑后团队概览回到「暂不派工」；无接入时给出提示且无法绑定", async ({
  page,
}) => {
  await openRole(page, teamId, ROLE_LEAD);
  // 本文件是串行套件，UI-ROLE-003 已把 ROLE_LEAD 绑到 Mock；先解绑回到未绑定态再验绑定流程。
  if (await page.getByRole("button", { name: "解绑" }).isVisible()) {
    await page.getByRole("button", { name: "解绑" }).click();
    await expect(page.getByRole("button", { name: "绑定 Harness", exact: true })).toBeVisible();
  }
  await page.getByRole("button", { name: "绑定 Harness", exact: true }).click();
  const select = page.getByLabel("选择本团队的 Harness 实例");
  await select.selectOption(
    (await select.locator("option", { hasText: MOCK }).getAttribute("value")) ?? "",
  );
  await page.getByRole("button", { name: "确认绑定" }).click();

  // 按钮文案变为「更换 Harness」，并出现「当前绑定 <接入名>」卡片
  await expect(page.getByRole("button", { name: "更换 Harness", exact: true })).toBeVisible();
  await expect(page.getByRole("heading", { name: `当前绑定 ${MOCK}` })).toBeVisible();
  await expect(page.locator(".role-detail__binding-meta")).toContainText("Mock");
  await expect(page.locator(".role-detail__binding-meta")).toContainText("已连接");
  await shot(page, "role-bound-card", page.locator(".role-detail__binding"));

  // 岗位列表同步显示接入名
  await expect(page.locator(".role-chip", { hasText: ROLE_LEAD })).toHaveText(
    new RegExp(`^${ROLE_LEAD}\\s*\\/\\s*${MOCK}$`),
  );

  // 解绑后团队概览回到「未绑定 Harness · 暂不派工」
  await page.getByRole("button", { name: "解绑" }).click();
  await expect(page.getByText("未绑定 Harness · 暂不派工").first()).toBeVisible();
  await page.goto(`/#/teams/${teamId}`);
  const card = page.locator(".th-role", { hasText: ROLE_LEAD });
  await expect(card).toContainText("未绑定 Harness · 暂不派工");
  await expect(card).not.toContainText(`当前绑定 ${MOCK}`);
  await shot(page, "team-home-unbound-again", card);

  // 另一个没有任何接入的团队：绑定区给出空态，且没有可用的确认入口
  teamBId = await createTeam(page, TEAM_B, workspaceB, [
    { name: ROLE_B, description: "只看不动手" },
  ]);
  await openRole(page, teamBId, ROLE_B);
  await page.getByRole("button", { name: "绑定 Harness", exact: true }).click();
  const empty = page.locator(".role-detail__picker .ui-empty");
  await expect(empty.getByText("本团队还没有 Harness 实例")).toBeVisible();
  await expect(empty.getByText("请先在「Harness 实例」页签注册一个实例。")).toBeVisible();
  await expect(page.getByLabel("选择本团队的 Harness 实例")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "确认绑定" })).toHaveCount(0);
  await expect(page.locator(".role-chip", { hasText: ROLE_B })).toHaveText(
    new RegExp(`^${ROLE_B}\\s*\\/\\s*未绑定$`),
  );
  await shot(page, "role-bind-no-harness", empty);
});

test("UI-ROLE-012 岗位文件夹：区块含「路径：Teamspace/<岗位名>」；归档成功移除岗位且保留历史；归档失败显示错误条", async ({
  page,
}) => {
  await openRole(page, teamId, ROLE_ASSIST);
  const folder = roleFolderSection(page);
  await expect(folder.locator(".role-detail__folder-path")).toContainText("路径：");
  await expect(folder.locator(".role-detail__folder-path")).toContainText(
    `Teamspace/${ROLE_ASSIST}`,
  );
  await expect(folder.getByRole("link", { name: "打开岗位工作台" })).toBeVisible();
  await shot(page, "role-folder-section", folder);

  // 注入归档失败：该岗位还有一个未结束的任务
  const assistTask = await createTask(page, teamId, {
    title: "助手留档任务",
    owner: ROLE_ASSIST,
  });
  expect(assistTask).toBe("AS-001");
  await openRole(page, teamId, ROLE_ASSIST);
  await page.getByRole("button", { name: "归档岗位" }).click();
  const alert = page.getByRole("alert");
  await expect(alert).toBeVisible();
  await expect(alert).toContainText("还负责 1 个未结束的任务");
  // 失败时岗位不消失
  await expect(page.locator(".role-chip", { hasText: ROLE_ASSIST })).toHaveCount(1);
  await shot(page, "role-archive-rejected", alert);

  // 归档成功：先有一个已结束任务，归档后岗位移出列表，既有任务与历史仍保留
  const form = await openRoleCreateForm(page);
  await fillRoleCreateForm(form, ROLE_TEMP, { prefix: "TMP", description: "临时观察" });
  await form.getByRole("button", { name: "创建岗位" }).click();
  await expect(page.locator(".role-chip", { hasText: ROLE_TEMP })).toBeVisible();

  await createTask(page, teamId, { title: "观察记录", owner: ROLE_TEMP, status: "READY" });
  const tempTaskKey = "TMP-001";
  const tempUrl = page.url();
  await transitionTo(page, ["执行中", "待验收", "已完成"]);
  await openRole(page, teamId, ROLE_TEMP);
  await page.getByRole("button", { name: "归档岗位" }).click();
  await expect(page.locator(".role-chip", { hasText: ROLE_TEMP })).toHaveCount(0);
  await expect(page.locator(".role-chip")).toHaveCount(3);

  // 既有任务与历史保留：任务详情还在，负责人仍是归档前的岗位名
  await page.goto(tempUrl);
  await expect(page.locator(".task-detail__status")).toContainText(tempTaskKey);
  await expect(page.getByText(` / ${ROLE_TEMP}`)).toBeVisible();
  // 岗位文件夹与工作日志都还在（只归档、不硬删除）
  expect(existsSync(join(workspace, "Teamspace", ROLE_TEMP))).toBe(true);
  expect(readFileSync(join(workspace, "Teamspace", ROLE_TEMP, "工作日志.md"), "utf8")).toContain(
    `归档岗位「${ROLE_TEMP}」`,
  );
  // 团队空间不再列出该岗位
  await page.goto(`/#/teams/${teamId}`);
  await expect(page.locator(".th-role", { hasText: ROLE_TEMP })).toHaveCount(0);
  await shot(page, "role-archived-list");
});

test("UI-WORK-005 当前绑定：显示接入名、预设标签、握手状态、Agent 版本与最近检查；未绑定显示「暂不派工」", async ({
  page,
}) => {
  // 前置条件：岗位绑定健康接入
  await openRole(page, teamId, ROLE_PREFIX);
  await page.getByRole("button", { name: "绑定 Harness", exact: true }).click();
  const select = page.getByLabel("选择本团队的 Harness 实例");
  await select.selectOption(
    (await select.locator("option", { hasText: MOCK }).getAttribute("value")) ?? "",
  );
  await page.getByRole("button", { name: "确认绑定" }).click();
  await expect(page.getByRole("heading", { name: `当前绑定 ${MOCK}` })).toBeVisible();

  await openWorkbench(page, teamId, ROLE_PREFIX);
  const binding = bindingPanel(page);
  await expect(binding.locator(".rw-binding__label")).toHaveText(MOCK);
  // 预设标签（presetTagLabel：真实预设形如「Claude Code · ACP」，Mock 标「Mock 测试用」）
  await expect(binding.locator(".rw-binding__name .ui-tag")).toHaveText("Mock 测试用");
  await expect(binding.locator(".rw-fact", { hasText: "握手状态" })).toContainText("已连接");
  await expect(binding.locator(".rw-fact", { hasText: "Agent 版本" })).toContainText(
    "teamspace-mock-agent",
  );
  // 最近检查：显示真实检查时间，不落到「从未检查」占位
  const checked = binding.locator(".rw-fact", { hasText: "最近检查" }).locator(".rw-fact__value");
  await expect(checked).toBeVisible();
  await expect(checked).not.toHaveText("从未检查");
  await expect(checked).toHaveText(/刚刚|\d+ 分钟前|\d+ 小时前|\d{4}-\d{2}-\d{2} \d{2}:\d{2}/);
  await shot(page, "workbench-binding-facts", binding);

  // 未绑定岗位：标签「未绑定」+ 文案「暂不派工」+ 「绑定 Harness」按钮
  await openWorkbench(page, teamId, ROLE_LEAD);
  const unbound = bindingPanel(page).locator(".rw-binding__unbound");
  await expect(unbound.getByText("未绑定")).toBeVisible();
  await expect(unbound.getByText("暂不派工")).toBeVisible();
  await expect(bindingPanel(page).getByRole("button", { name: "绑定 Harness" })).toBeVisible();
  await shot(page, "workbench-unbound-facts", bindingPanel(page));
});

test("UI-WORK-004 工作日志：展示岗位建立、派工与状态变化条目", async ({ page }) => {
  await openWorkbench(page, teamId, ROLE_PREFIX);
  const items = page.locator(".rw-journal-item .rw-journal-text");
  await expect(items.filter({ hasText: `创建岗位「${ROLE_PREFIX}」` })).toHaveCount(1);
  await expect(items.filter({ hasText: `DEV1-001 · 创建任务` })).toHaveCount(1);
  await expect(items.filter({ hasText: `R9-001 · 创建任务` })).toHaveCount(1);
  await expect(items.filter({ hasText: `绑定 Harness「${MOCK}` })).toHaveCount(1);
  await expect(page.locator(".rw-journal-time").first()).toBeVisible();
  await shot(page, "workbench-journal-items", page.locator(".rw-journal"));
});

test("UI-WORK-009 最近执行：无执行时显示「暂无执行」；执行条目可进入任务详情；返回与刷新后状态一致", async ({
  page,
}) => {
  test.setTimeout(90_000);

  await openWorkbench(page, teamId, ROLE_PREFIX);
  const runs = runsPanel(page);
  await expect(runs.getByText("暂无执行")).toBeVisible();
  await expect(runs.getByText("任务派给这个岗位后，每次执行都会出现在这里。")).toBeVisible();
  await expect(runs).toContainText("点击进入任务详情，可查看这次执行的完整进度与改动。");
  await shot(page, "workbench-no-runs", runs);

  // 派一次真实执行（内置 Mock ACP 预设），让「最近执行」有内容
  await page.goto(`/#/teams/${teamId}/tasks/new`);
  await page.getByLabel("任务名称").fill(RUN_TASK);
  const owner = page.getByLabel("负责人岗位");
  await owner.selectOption(
    (await owner.locator("option", { hasText: ROLE_PREFIX }).getAttribute("value")) ?? "",
  );
  await page.locator("#task-file-search").fill("brief");
  await page.locator(".task-form__suggestion", { hasText: "brief.txt" }).click();
  await page.getByRole("radio", { name: "READY", exact: true }).check();
  await page.getByRole("button", { name: "创建并分配" }).click();
  await expect(page.locator(".task-detail__status")).toContainText("READY");
  const runTaskKey =
    /\/ (\S+)$/.exec((await page.locator(".task-detail__status").innerText()).trim())?.[1] ?? "";
  expect(runTaskKey).toMatch(/^R9-\d+$/);
  await page.getByRole("button", { name: "开始执行" }).click();
  const success = page.locator(".run-outcome--success");
  await expect(success).toBeVisible({ timeout: 40_000 });
  await expect(success).toContainText("执行完成");

  // 执行区列出执行条目，可点击进入任务详情
  await openWorkbench(page, teamId, ROLE_PREFIX);
  const runRow = page.locator(".rw-run-row", { hasText: runTaskKey });
  await expect(runRow).toContainText("第 1 次");
  await expect(runRow).toContainText(MOCK);
  await expect(runRow).toContainText("已完成");
  await shot(page, "workbench-run-row", runsPanel(page));
  await runRow.click();
  await expect(page).toHaveURL(new RegExp(`/teams/${teamId}/tasks/[^/]+$`));
  await expect(page.getByRole("heading", { level: 1, name: RUN_TASK })).toBeVisible();

  // 返回团队空间：落在 /teams/:teamId 且选中「团队空间」
  await openWorkbench(page, teamId, ROLE_PREFIX);
  await page.getByRole("button", { name: "返回团队空间" }).click();
  await expect(page).toHaveURL(new RegExp(`/teams/${teamId}$`));
  await expect(page.locator(".sidebar__nav .nav-item--active")).toHaveText(["团队空间"]);

  // 刷新后日志、最近任务、最近文件与当前绑定都和刷新前一致，不回落为空状态
  await openWorkbench(page, teamId, ROLE_PREFIX);
  const before = await snapshotWorkbench(page);
  expect(before.journal).toContain("创建岗位");
  expect(before.tasks.length).toBeGreaterThan(0);
  expect(before.files.length).toBeGreaterThan(0);
  await page.reload();
  await expect(page.locator(".rw-meta__prefix")).toHaveText("R9");
  expect(await snapshotWorkbench(page)).toEqual(before);
  await shot(page, "workbench-after-reload");
});

/** 刷新一致性快照：绝对时间与名称，不含相对时间（避免刷新瞬间变化）。 */
async function snapshotWorkbench(page: Page): Promise<{
  journal: string;
  tasks: string[];
  files: string[];
  bindingName: string;
  factKeys: string[];
}> {
  return {
    journal: (await page.locator(".rw-journal").innerText()).trim(),
    tasks: await page.locator(".rw-task-row__key").allInnerTexts(),
    files: await page.locator(".rw-file-row__name").allInnerTexts(),
    bindingName: await page.locator(".rw-binding__label").innerText(),
    factKeys: await page.locator(".rw-fact__key").allInnerTexts(),
  };
}

test("UI-WORK-003 文件夹不可写：显示「岗位文件夹无法写入」与兜底说明，派工仍可进行", async ({
  page,
}) => {
  test.setTimeout(60_000);

  const { restore, blocked } = makeFolderUnwritable();
  test.skip(!blocked, "chmod 0o500 对当前用户无效（可能以 root 运行），无法验证不可写兜底");

  try {
    await openRole(page, teamId, ROLE_LEAD);
    // 触发一次需要写岗位文件夹的操作：改职责 → 岗位说明重写
    await page.getByLabel("职责").fill("改到不可写时的职责");
    await saveRoleDetail(page);

    // 工作台：警告卡 + 兜底说明，明确不阻断派工
    await openWorkbench(page, teamId, ROLE_LEAD);
    const card = page.locator(".ui-card--warning", { hasText: "岗位文件夹无法写入" });
    await expect(card).toBeVisible();
    await expect(card).toContainText("无法写入");
    await expect(card).toContainText("工作日志暂存在应用内，恢复后自动补写；岗位仍可正常派工。");
    await expect(card.getByRole("button", { name: "重新检查" })).toBeVisible();
    await shot(page, "workbench-folder-unwritable", card);

    // 派工不受影响：任务照常创建，岗位照常可派
    const key = await createTask(page, teamId, {
      title: "不可写时的派工",
      owner: ROLE_LEAD,
      status: "READY",
    });
    expect(key).toBe("LD-001");
  } finally {
    restore();
  }

  // 恢复可写后重新同步：警告卡消失，兜底说明不再出现
  await openWorkbench(page, teamId, ROLE_LEAD);
  await expect(page.getByText("岗位文件夹无法写入")).toHaveCount(0);
});

test("UI-ROLE-012 岗位文件夹不可写：详情页显示原因文本，不静默失败", async ({ page }) => {
  test.setTimeout(60_000);

  const { restore, blocked } = makeFolderUnwritable();
  test.skip(!blocked, "chmod 0o500 对当前用户无效（可能以 root 运行），无法验证不可写原因文本");

  try {
    await openRole(page, teamId, ROLE_LEAD);
    await page.getByLabel("职责").fill("再改一次触发写入失败");
    await saveRoleDetail(page);
    const problem = roleFolderSection(page).locator(".role-detail__folder-problem");
    await expect(problem).toBeVisible();
    await expect(problem).toContainText("无法写入");
    await shot(page, "role-folder-problem-text", roleFolderSection(page));
  } finally {
    restore();
  }

  // 恢复后原因消失：不可写原因记录在岗位上，需要一次成功同步才会清除。
  await openRole(page, teamId, ROLE_LEAD);
  await page.getByLabel("职责").fill("恢复可写后的职责");
  await saveRoleDetail(page);
  await expect(roleFolderSection(page).locator(".role-detail__folder-problem")).toHaveCount(0);
});

test("UI-WORK-004 工作日志无记录时显示「暂无记录 / 岗位建立、派工和状态变化会自动记入这里。」", async ({
  page,
}) => {
  // 制造空日志：清空该岗位的工作日志文件后重新打开工作台
  const journal = join(workspace, "Teamspace", ROLE_PREFIX, "工作日志.md");
  expect(existsSync(journal)).toBe(true);
  writeFileSync(journal, "");

  await openWorkbench(page, teamId, ROLE_PREFIX);
  const panel = page.locator(".rw-panel", { has: page.getByRole("heading", { name: "工作日志" }) });
  await expect(panel.locator(".ui-empty__title")).toHaveText("暂无记录");
  await expect(panel.locator(".ui-empty__description")).toHaveText(
    "岗位建立、派工和状态变化会自动记入这里。",
  );
  await expect(page.locator(".rw-journal")).toHaveCount(0);
  await shot(page, "workbench-journal-empty", panel);
});
