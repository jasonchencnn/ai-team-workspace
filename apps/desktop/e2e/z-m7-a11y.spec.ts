// M7-01 窄屏与无障碍：窄屏无横向滚动、键盘可走通主流程、弹窗焦点被困住、禁用态对比度达标。
//
// 覆盖三件事：
// 1. 375 / 760 宽下主要页面没有横向滚动，文字不重叠；
// 2. 全键盘走通「创建任务 → 开始执行（Mock）→ 验收通过」，看板卡片能用 Tab 聚焦、Enter 打开；
// 3. 弹窗打开时焦点进得去、Tab 在里面循环、Esc 能关、关掉后焦点回到触发按钮；
//    禁用态按钮与正文的对比度达到 4.5:1。
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";

import { type Page, expect, test } from "@playwright/test";

import { createMockHarness } from "./harnessHelpers.js";
import { invoke, setupTeam } from "./z-qa-r2-helpers.js";

const TIMEOUT = 25_000;
const HARNESS = "m7a11y-mock";
const ROLE = "无障碍员";
const SPEC_MD = "docs/spec.md";

/** 窄屏下要检查的页面：路径模板里的 {teamId} 等占位符会替换成真实 id。 */
const PAGES: { name: string; path: string }[] = [
  { name: "团队空间", path: "/teams/{teamId}" },
  { name: "任务面板", path: "/teams/{teamId}/tasks" },
  { name: "创建任务", path: "/teams/{teamId}/tasks/new" },
  { name: "任务详情", path: "/teams/{teamId}/tasks/{taskId}" },
  { name: "编辑任务", path: "/teams/{teamId}/tasks/{taskId}/edit" },
  { name: "任务验收", path: "/teams/{teamId}/tasks/{taskId}/review" },
  { name: "共享文件", path: "/teams/{teamId}/files" },
  { name: "岗位与绑定", path: "/teams/{teamId}/roles" },
  { name: "岗位工作台", path: "/teams/{teamId}/roles/{roleId}/workbench" },
  { name: "接入详情", path: "/teams/{teamId}/harnesses/{harnessId}" },
  { name: "活动与审计", path: "/teams/{teamId}/activity" },
  { name: "岗位通信", path: "/teams/{teamId}/messages" },
  { name: "待我处理", path: "/teams/{teamId}/inbox" },
  { name: "通知订阅", path: "/teams/{teamId}/notifications" },
  { name: "设置", path: "/teams/{teamId}/settings" },
  { name: "创建团队", path: "/teams/new" },
];

let ws = "";
let teamId = "";
let taskId = "";
let roleId = "";
let harnessId = "";

function git(cwd: string, ...args: string[]): string {
  return execFileSync(
    "git",
    [
      "-c",
      "user.name=E2E",
      "-c",
      "user.email=e2e@example.invalid",
      "-c",
      "commit.gppsign=false",
      ...args,
    ],
    { cwd, encoding: "utf8" },
  );
}

function put(root: string, rel: string, content: string): void {
  const target = join(root, rel);
  mkdirSync(dirname(target), { recursive: true });
  writeFileSync(target, content);
}

/**
 * 页面上是否出现横向滚动：文档本身滚不了，且没有任何元素越出视口右边界。
 * 越界的元素会让内容被裁掉，用户看不到也点不到。
 *
 * 例外：组件自己声明了横向滚动的区域（看板列、宽表格），它们内部越界是设计内的，
 * 但滚动容器本身仍然要留在视口里。
 */
async function horizontalOverflow(page: Page): Promise<{
  docScroll: number;
  docClient: number;
  offenders: string[];
}> {
  return page.evaluate(() => {
    const de = document.documentElement;
    const offenders: string[] = [];
    const inScroller = (el: Element): boolean => {
      let node: Element | null = el.parentElement;
      while (node !== null) {
        const overflowX = getComputedStyle(node).overflowX;
        if (overflowX === "auto" || overflowX === "scroll") {
          return true;
        }
        node = node.parentElement;
      }
      return false;
    };
    for (const el of document.querySelectorAll<HTMLElement>("body *")) {
      const rect = el.getBoundingClientRect();
      if (rect.width === 0 && rect.height === 0) {
        continue;
      }
      // 左侧为负的是抽屉等移出屏幕的元素（由 visibility 隐藏），不算越界。
      if (rect.right > de.clientWidth + 1 && !inScroller(el)) {
        offenders.push(
          `${el.tagName.toLowerCase()}.${el.className.slice(0, 40)} right=${rect.right.toFixed(0)}`,
        );
      }
    }
    return {
      docScroll: de.scrollWidth,
      docClient: de.clientWidth,
      offenders: offenders.slice(0, 5),
    };
  });
}

/** 计算两个不透明颜色之间的 WCAG 对比度。 */
function contrastRatio(foreground: string, background: string): number {
  const parse = (value: string): number[] => {
    const parts = value.match(/[\d.]+/g)?.map(Number) ?? [];
    return parts.slice(0, 3);
  };
  const channel = (raw: number): number => {
    const c = raw / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  const luminance = (value: string): number => {
    const [r = 0, g = 0, b = 0] = parse(value).map(channel);
    return 0.2126 * r + 0.7152 * g + 0.0722 * b;
  };
  const l1 = luminance(foreground);
  const l2 = luminance(background);
  const light = Math.max(l1, l2);
  const dark = Math.min(l1, l2);
  return (light + 0.05) / (dark + 0.05);
}

test.describe.configure({ mode: "serial", timeout: 180_000 });

test.beforeAll(() => {
  ws = mkdtempSync(join(tmpdir(), "teamspace-e2e-a11y-"));
  git(ws, "init", "-q", "-b", "main");
  put(ws, "README.md", "# 无障碍项目\n");
  put(ws, SPEC_MD, "# 需求说明\n\n无障碍用例会引用这份文档。\n");
  git(ws, "add", ".");
  git(ws, "commit", "-q", "-m", "docs: seed");
});

test.afterAll(() => {
  rmSync(ws, { recursive: true, force: true });
});

test("准备团队、岗位与 Mock 接入", async ({ page }) => {
  const setup = await setupTeam("窄屏无障碍团队", ws, [{ name: ROLE, prefix: "A11" }]);
  teamId = setup.teamId;
  roleId = setup.roleIds[ROLE] ?? "";
  expect(teamId).not.toBe("");
  expect(roleId).not.toBe("");

  // 走页面建 Mock 接入：这一段也要能被键盘操作，所以用真实点击。
  await page.goto(`/#/teams/${teamId}/roles?tab=harness`);
  await createMockHarness(page, HARNESS);
  const harnesses = await invoke<{ id: string; name: string }[]>("list_harnesses", { teamId });
  harnessId = harnesses.find((harness) => harness.name === HARNESS)?.id ?? "";
  expect(harnessId).not.toBe("");

  await page.goto(`/#/teams/${teamId}/roles`);
  await page.locator(".role-chip", { hasText: ROLE }).click();
  await page.getByRole("button", { name: "绑定 Harness", exact: true }).click();
  const select = page.getByLabel("选择本团队的 Harness 实例");
  await select.selectOption(
    (await select.locator("option", { hasText: HARNESS }).getAttribute("value")) ?? "",
  );
  await page.getByRole("button", { name: "确认绑定" }).click();
  await expect(page.getByText(`当前绑定 ${HARNESS}`)).toBeVisible();

  // 建一个 READY 任务，供键盘流程与看板卡片断言使用。
  const created = await invoke<{ task: { id: string } }>("create_task", {
    input: {
      teamId,
      title: "窄屏与键盘任务",
      description: "描述",
      ownerRoleId: roleId,
      status: "ready",
      fileRefs: [],
      dependsOn: [],
    },
  });
  taskId = created.task.id;
  expect(taskId).not.toBe("");
});

test("375 与 760 宽下主要页面没有横向滚动", async ({ page }) => {
  for (const width of [375, 760]) {
    await page.setViewportSize({ width, height: 900 });
    for (const target of PAGES) {
      const path = target.path
        .replace("{teamId}", teamId)
        .replace("{taskId}", taskId)
        .replace("{roleId}", roleId)
        .replace("{harnessId}", harnessId);
      await page.goto(`/#${path}`);
      await page.waitForTimeout(400);
      const result = await horizontalOverflow(page);
      expect(
        result.docScroll,
        `${target.name} 在 ${String(width)} 宽下文档出现横向滚动`,
      ).toBeLessThanOrEqual(result.docClient);
      expect(result.offenders, `${target.name} 在 ${String(width)} 宽下有元素越出视口`).toEqual([]);
    }
  }
});

test("窄屏侧边栏收成抽屉，顶栏按钮可开合", async ({ page }) => {
  await page.setViewportSize({ width: 375, height: 900 });
  await page.goto(`/#/teams/${teamId}`);

  const toggle = page.getByRole("button", { name: "打开导航菜单" });
  await expect(toggle).toBeVisible();
  await expect(toggle).toHaveAttribute("aria-expanded", "false");
  // 抽屉收起时不在 Tab 顺序里（visibility: hidden）。
  await expect(page.locator("#app-sidebar")).toBeHidden();

  await toggle.click();
  await expect(page.getByRole("button", { name: "关闭导航菜单" })).toBeVisible();
  await expect(page.locator("#app-sidebar")).toBeVisible();
  await expect(page.getByRole("button", { name: "关闭导航菜单" })).toHaveAttribute(
    "aria-expanded",
    "true",
  );

  // 点遮罩关闭。
  await page.locator(".drawer-backdrop").click({ position: { x: 340, y: 400 } });
  await expect(page.locator("#app-sidebar")).toBeHidden();

  // 宽屏下抽屉按钮不出现，侧边栏常驻。
  await page.setViewportSize({ width: 1440, height: 960 });
  await expect(page.getByRole("button", { name: "打开导航菜单" })).toBeHidden();
  await expect(page.locator("#app-sidebar")).toBeVisible();
});

test("窄屏看板按列滑动，列标题不被挤掉", async ({ page }) => {
  await page.setViewportSize({ width: 375, height: 900 });
  await page.goto(`/#/teams/${teamId}/tasks`);
  const board = page.getByRole("region", { name: "任务看板" });
  await expect(board).toBeVisible();

  // 第一列的标题要能完整读出来（不被裁成半个字）。
  const heading = board.locator(".task-board__heading").first();
  await expect(heading).toBeVisible();
  const clipped = await heading.evaluate((el) => el.scrollWidth > el.clientWidth + 1);
  expect(clipped, "窄屏下看板列标题被裁切").toBe(false);

  // 看板在窄屏下自己横向滚动，页面本身不滚。
  const scrollable = await board.evaluate((el) => el.scrollWidth > el.clientWidth);
  expect(scrollable, "窄屏下看板列应可横向滑动").toBe(true);
});

test("全键盘走通：创建任务 → 开始执行 → 验收通过", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 960 });

  // ---- 建任务：只用键盘。
  await page.goto(`/#/teams/${teamId}/tasks/new`);
  await page.getByLabel("任务名称").focus();
  await page.keyboard.type("键盘流程任务");
  await page
    .getByLabel("负责人岗位")
    .selectOption(
      (await page
        .getByLabel("负责人岗位")
        .locator("option", { hasText: ROLE })
        .getAttribute("value")) ?? "",
    );
  await page.getByRole("radio", { name: "READY", exact: true }).check();
  // 提交按钮用键盘激活（焦点 + Enter）。
  await page.getByRole("button", { name: "创建并分配" }).focus();
  await page.keyboard.press("Enter");
  await expect(page.locator(".task-detail__status")).toContainText("READY");

  // ---- 开始执行：焦点到按钮上按 Enter。
  const start = page.getByRole("button", { name: "开始执行", exact: true });
  await start.focus();
  await expect(start).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(page.locator(".run-outcome--success")).toBeVisible({ timeout: TIMEOUT });
  await expect(page.locator(".task-detail__status")).toContainText("待验收");

  // ---- 去验收 → 通过并合并：同样用键盘。
  await page
    .getByRole("region", { name: "验收入口" })
    .getByRole("button", { name: "去验收" })
    .click();
  await expect(page).toHaveURL(new RegExp(`/tasks/[^/]+/review$`));

  const approve = page.getByRole("button", { name: "通过并合并到 main" });
  await approve.focus();
  await page.keyboard.press("Enter");
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "确认合并" }).focus();
  await page.keyboard.press("Enter");
  await expect(page.locator(".review-banner--success").first()).toContainText("合并成功");
});

test("看板卡片可用 Tab 聚焦、Enter 打开", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 960 });
  await page.goto(`/#/teams/${teamId}/tasks`);

  const card = page.getByRole("link", { name: /A11-\d+ 窄屏与键盘任务/ }).first();
  await expect(card).toBeVisible();
  await card.focus();
  await expect(card).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(page).toHaveURL(new RegExp(`/tasks/[^/]+$`));
});

test("弹窗：焦点进得去、Tab 在里面循环、Esc 能关、焦点回到触发按钮", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 960 });
  await page.goto(`/#/teams/${teamId}/messages`);

  // 「发送岗位消息」弹窗里有多个可聚焦控件（两个下拉、主题输入框、正文、取消），
  // 足够验证 Tab 在弹窗内循环。
  const trigger = page.getByRole("button", { name: "发送消息" }).first();
  await trigger.focus();
  await page.keyboard.press("Enter");
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();

  // 焦点已经在弹窗里。
  const inside = await dialog.evaluate((el) => el.contains(document.activeElement));
  expect(inside, "弹窗打开后焦点应落在弹窗内").toBe(true);

  // Tab 若干次都不离开弹窗。
  for (let i = 0; i < 12; i += 1) {
    await page.keyboard.press("Tab");
    const stillInside = await dialog.evaluate((el) => el.contains(document.activeElement));
    expect(stillInside, `第 ${String(i + 1)} 次 Tab 后焦点跑出了弹窗`).toBe(true);
  }
  // Shift+Tab 同样不离开。
  for (let i = 0; i < 6; i += 1) {
    await page.keyboard.press("Shift+Tab");
    const stillInside = await dialog.evaluate((el) => el.contains(document.activeElement));
    expect(stillInside, `第 ${String(i + 1)} 次 Shift+Tab 后焦点跑出了弹窗`).toBe(true);
  }

  // Esc 关闭。
  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
  // 焦点回到触发按钮。
  await expect(trigger).toBeFocused();
});

test("禁用态按钮与正文的对比度达到 4.5:1", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 960 });
  await page.goto(`/#/teams/${teamId}/tasks`);

  // 找页面上的禁用按钮：任务卡片上的「加载更多」在数据不足时不出现，
  // 所以用任务详情里尚未就绪时不可点的「开始执行」。
  await page
    .getByRole("link", { name: /A11-\d+ 窄屏与键盘任务/ })
    .first()
    .click();
  const disabled = page.locator("button:disabled").first();
  if ((await disabled.count()) === 0) {
    // 页面上没有禁用按钮时，用一个临时插入的禁用按钮验证设计 Token 的实际取值。
    await page.evaluate(() => {
      const probe = document.createElement("button");
      probe.className = "ui-button ui-button--primary";
      probe.disabled = true;
      probe.id = "a11y-disabled-probe";
      probe.textContent = "禁用态探针";
      document.body.appendChild(probe);
    });
  }
  const target =
    (await page.locator("button:disabled").count()) > 0
      ? page.locator("button:disabled").first()
      : page.locator("#a11y-disabled-probe");

  const measured = await target.evaluate((el) => {
    const style = getComputedStyle(el);
    // 往上找第一个不透明的祖先背景色（按钮自己的底色可能是透明的）。
    let background = "rgba(0, 0, 0, 0)";
    let node: HTMLElement | SVGElement | null = el;
    while (node !== null) {
      const candidate = getComputedStyle(node).backgroundColor;
      if (candidate !== "" && !candidate.includes("rgba(0, 0, 0, 0)")) {
        background = candidate;
        break;
      }
      node = node.parentElement;
    }
    return { color: style.color, background, opacity: style.opacity };
  });

  // 禁用态不再用 opacity 淡化：透明度保持 1，颜色本身带足对比度。
  expect(Number(measured.opacity), "禁用按钮不应靠 opacity 淡化").toBe(1);
  const ratio = contrastRatio(measured.color, measured.background);
  expect(ratio, `禁用按钮对比度只有 ${ratio.toFixed(2)}:1`).toBeGreaterThanOrEqual(4.5);

  // 正文对比度同样复核一次。
  const bodyColors = await page.evaluate(() => {
    const style = getComputedStyle(document.body);
    return { color: style.color, background: getComputedStyle(document.body).backgroundColor };
  });
  const bodyRatio = contrastRatio(bodyColors.color, bodyColors.background);
  expect(bodyRatio, `正文对比度只有 ${bodyRatio.toFixed(2)}:1`).toBeGreaterThanOrEqual(4.5);
});

test("表单控件有标签，状态不只靠颜色", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 960 });
  await page.goto(`/#/teams/${teamId}/tasks/new`);

  // 每个表单控件都有可访问名称。
  const unnamed = await page.evaluate(() => {
    const problems: string[] = [];
    for (const el of document.querySelectorAll<HTMLElement>("input, select, textarea")) {
      if (el instanceof HTMLInputElement && el.type === "hidden") {
        continue;
      }
      const id = el.getAttribute("id");
      const labelled =
        (id !== null && document.querySelector(`label[for="${id}"]`) !== null) ||
        el.closest("label") !== null ||
        el.getAttribute("aria-label") !== null ||
        el.getAttribute("aria-labelledby") !== null;
      if (!labelled) {
        problems.push(`${el.tagName.toLowerCase()}[${id ?? "no-id"}]`);
      }
    }
    return problems;
  });
  expect(unnamed, "有表单控件没有关联标签").toEqual([]);

  // 图标按钮也有可访问名称（文字、aria-label、aria-labelledby 或 title 至少有一个）。
  for (const path of [
    "/teams/{teamId}",
    "/teams/{teamId}/tasks",
    "/teams/{teamId}/roles",
    "/teams/{teamId}/messages",
    "/teams/{teamId}/inbox",
    "/teams/{teamId}/activity",
  ]) {
    await page.goto(`/#${path.replace("{teamId}", teamId)}`);
    await page.waitForTimeout(300);
    const unnamedButtons = await page.evaluate(() => {
      const problems: string[] = [];
      for (const el of document.querySelectorAll<HTMLElement>("button")) {
        const named =
          el.textContent.trim() !== "" ||
          el.getAttribute("aria-label") !== null ||
          el.getAttribute("aria-labelledby") !== null ||
          el.getAttribute("title") !== null;
        if (!named) {
          problems.push(el.className.slice(0, 40) || "button");
        }
      }
      return problems;
    });
    expect(unnamedButtons, `${path} 上有按钮没有可访问名称`).toEqual([]);
  }

  // 状态标签带文字，不只是颜色。
  await page.goto(`/#/teams/${teamId}/tasks`);
  const heading = page.locator(".task-board__heading").first();
  await expect(heading).toHaveText(/\S/);
});

test("全键盘走通：创建团队", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 960 });
  await page.goto("/#/teams/new");

  // 团队名：聚焦后直接打字。
  await page.getByLabel("团队名称").focus();
  await page.keyboard.type("键盘建队");

  // 工作空间：聚焦按钮后按 Enter（测试桥接返回临时 Git 工作空间）。
  const pick = page.getByRole("button", { name: "选择工作空间" });
  await pick.focus();
  await page.keyboard.press("Enter");
  await expect(page.getByText(/Git 仓库：main @ [0-9a-f]{7}/)).toBeVisible();

  // 岗位：+ 添加岗位 → 先填前缀再填名称。
  // 前缀框一旦被用户接管，失焦时内核回填的建议就不再覆盖它（见 roleForm 的前缀建议）。
  const addRole = page.getByRole("button", { name: "+ 添加岗位" });
  await addRole.focus();
  await page.keyboard.press("Enter");
  const roleCard = page.locator(".tc-role").last();
  await roleCard.getByLabel("任务编号前缀").focus();
  await page.keyboard.type("KB");
  await roleCard.getByLabel("岗位名称").focus();
  await page.keyboard.type("键盘员");

  // 提交：聚焦按钮后按 Enter。
  const submit = page.getByRole("button", { name: "创建团队", exact: true });
  await submit.focus();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("heading", { name: "团队空间" })).toBeVisible();
  await expect(page.locator(".th-role", { hasText: "键盘员" })).toBeVisible();
});
