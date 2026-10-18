// M3-09 端到端：M3「可用标志」一条龙。真实界面 + 测试桥接 + 真实内核；Harness 是内置 Mock ACP Agent，
// 工作空间是临时 Git 仓库。流程：建团队 → 岗位绑定 Mock → 建任务 A、B（B 依赖 A）→ A 开始执行 → 去验收 →
// 看 diff → 通过并合并（确认弹窗）→ 合并成功提示里出现 B 自动就绪 → 看板上 B 为 READY →
// 活动与审计按任务 A 筛选看到「验收通过」→ 共享文件页打开 A 引用的 md 并看到引用任务。
// 另有一条：合并冲突后「重新执行」走完整条路径（M3-06 只验证了按钮可用）。
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";

import { type Page, expect, test } from "@playwright/test";

import { createMockHarness } from "./harnessHelpers.js";
import { type RunInfo, runsOf } from "./z-qa-r2-helpers.js";

const TIMEOUT = 25_000;
const HARNESS = "m3f-mock";
const ROLE = "全流程员";
const OK_MARK = "written by shell";
const SPEC_MD = "docs/spec.md";

let ws = "";
let teamId = "";

interface TaskRef {
  id: string;
  key: string;
  title: string;
  url: string;
}

test.describe.configure({ mode: "serial", timeout: 120_000 });

function git(cwd: string, ...args: string[]): string {
  return execFileSync(
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
    { cwd, encoding: "utf8" },
  );
}

function put(root: string, rel: string, content: string): void {
  const target = join(root, rel);
  mkdirSync(dirname(target), { recursive: true });
  writeFileSync(target, content);
}

test.beforeAll(() => {
  ws = mkdtempSync(join(tmpdir(), "teamspace-e2e-m3f-"));
  git(ws, "init", "-q", "-b", "main");
  put(ws, "README.md", "# 全流程项目\n");
  put(ws, SPEC_MD, "# 需求说明\n\n任务 A 引用这份文档。\n");
  put(ws, "notes/x.txt", "base\n");
  put(ws, "notes/y.txt", "base\n");
  git(ws, "add", ".");
  git(ws, "commit", "-q", "-m", "docs: seed");
});

test.afterAll(() => {
  rmSync(ws, { recursive: true, force: true });
});

/** 在任务表单里建任务（页面操作），返回编号等信息。 */
async function createTaskUi(
  page: Page,
  title: string,
  options: { status: "TODO" | "READY"; ref?: string; dependsOn?: string } = { status: "READY" },
): Promise<TaskRef> {
  await page.goto(`/#/teams/${teamId}/tasks/new`);
  await page.getByLabel("任务名称").fill(title);
  const select = page.getByLabel("负责人岗位");
  await select.selectOption(
    (await select.locator("option", { hasText: ROLE }).getAttribute("value")) ?? "",
  );
  if (options.ref !== undefined) {
    await page.locator("#task-file-search").fill(options.ref);
    await page.locator(".task-form__suggestion", { hasText: SPEC_MD }).click();
  }
  if (options.dependsOn !== undefined) {
    await page.getByRole("checkbox", { name: new RegExp(options.dependsOn) }).check();
    await expect(page.getByText(/前置完成后会自动变为 READY/)).toBeVisible();
  }
  await page.getByRole("radio", { name: options.status, exact: true }).check();
  await page.getByRole("button", { name: "创建并分配" }).click();
  await expect(page.locator(".task-detail__status")).toContainText(options.status);
  const key = /\/ (\S+)$/.exec(
    (await page.locator(".task-detail__status").innerText()).trim(),
  )?.[1];
  const url = page.url();
  return { url, id: url.split("/").pop() ?? "", key: key ?? "", title };
}

/** 开始执行并等到「待验收」。 */
async function runToReview(page: Page): Promise<void> {
  await page
    .getByRole("button", { name: /^(开始执行|重新执行)$/ })
    .first()
    .click();
  await expect(page.locator(".run-outcome--success")).toBeVisible({ timeout: TIMEOUT });
  await expect(page.locator(".task-detail__status")).toContainText("待验收");
}

async function goReview(page: Page): Promise<void> {
  await page
    .getByRole("region", { name: "验收入口" })
    .getByRole("button", { name: "去验收" })
    .click();
}

async function approveAndMerge(page: Page): Promise<void> {
  await page.getByRole("button", { name: "通过并合并到 main" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toContainText("确认合并");
  await dialog.getByRole("button", { name: "确认合并" }).click();
}

let taskA: TaskRef;
let taskB: TaskRef;

test("建团队、岗位绑定 Mock、建任务 A 与依赖 A 的任务 B", async ({ page }) => {
  await page.goto("/#/teams/new");
  await page.getByLabel("团队名称").fill("全流程团队");
  await page.getByLabel("手动输入工作空间路径").fill(ws);
  await page.getByRole("button", { name: "检测", exact: true }).click();
  await expect(page.getByText("Git 仓库").first()).toBeVisible();
  await page.getByRole("button", { name: "+ 添加岗位" }).click();
  const card = page.locator(".tc-role").last();
  await card.getByLabel("岗位名称").fill(ROLE);
  await card.getByLabel("岗位名称").blur();
  await expect(card.getByLabel("任务编号前缀")).toHaveValue(/^[A-Z][A-Z0-9]{1,3}$/);
  await card.getByLabel("职责").fill("跑通 M3 全流程");
  await page.getByRole("button", { name: "创建团队", exact: true }).click();
  await expect(page).toHaveURL(/\/teams\/(?!new$)[^/]+$/);
  teamId = /\/teams\/([^/?]+)/.exec(page.url())?.[1] ?? "";

  await page.goto(`/#/teams/${teamId}/roles?tab=harness`);
  await createMockHarness(page, HARNESS);
  await page.goto(`/#/teams/${teamId}/roles`);
  await page.locator(".role-chip", { hasText: ROLE }).click();
  await page.getByRole("button", { name: "绑定 Harness", exact: true }).click();
  const select = page.getByLabel("选择本团队的 Harness 实例");
  await select.selectOption(
    (await select.locator("option", { hasText: HARNESS }).getAttribute("value")) ?? "",
  );
  await page.getByRole("button", { name: "确认绑定" }).click();
  await expect(page.getByText(`当前绑定 ${HARNESS}`)).toBeVisible();

  taskA = await createTaskUi(page, "撰写说明 [mock:shellwrite=docs/flow-a.txt]", {
    status: "READY",
    ref: "spec",
  });
  taskB = await createTaskUi(page, "下游联调", {
    status: "TODO",
    dependsOn: "撰写说明",
  });
  // B 依赖 A：A 未完成前 B 不能开始。
  await expect(page.getByRole("button", { name: "开始执行", exact: true })).toBeDisabled();
});

test("A 执行 → 去验收 → 看 diff → 通过并合并 → B 自动就绪", async ({ page }) => {
  await page.goto(taskA.url);
  await runToReview(page);
  await goReview(page);
  await expect(page).toHaveURL(new RegExp(`/tasks/${taskA.id}/review$`));

  const row = page.locator(".review-file", { hasText: "docs/flow-a.txt" });
  await expect(row).toContainText("新增");
  await row.getByRole("button", { name: "docs/flow-a.txt" }).click();
  await expect(
    page.locator(".diff-card", { hasText: "docs/flow-a.txt" }).locator(".diff-row--add"),
  ).toContainText(OK_MARK);

  await approveAndMerge(page);
  const banner = page.locator(".review-banner--success").first();
  await expect(banner).toContainText("合并成功");
  await expect(banner).toContainText("已合并到 main");
  // 合并成功提示里出现下游自动就绪
  await expect(banner).toContainText("下游已自动就绪");
  await expect(banner).toContainText(`${taskB.key} ${taskB.title}`);

  // 基线分支：文件在、合并提交有两个父、worktree 与执行分支已清理
  expect(readFileSync(join(ws, "docs/flow-a.txt"), "utf8")).toContain(OK_MARK);
  expect(git(ws, "log", "-1", "--format=%p").trim().split(" ")).toHaveLength(2);
  expect(git(ws, "branch", "--list", `teamspace/${taskA.key}-r1`).trim()).toBe("");
  const runA = (await runsOf(taskA.id))[0] as RunInfo;
  expect(existsSync(runA.execDir)).toBe(false);

  // 看板：A 已完成，B 为 READY
  await page.goto(`/#/teams/${teamId}/tasks`);
  const cardB = page.getByRole("link", { name: `${taskB.key} ${taskB.title}` });
  await expect(cardB).toContainText("READY");
  await expect(cardB.locator("xpath=ancestor::section")).toContainText("TODO");
  await expect(cardB).not.toContainText("依赖");
  await page.goto(taskB.url);
  await expect(page.locator(".task-detail__status")).toContainText("READY");
  await expect(page.getByRole("button", { name: "开始执行", exact: true })).toBeEnabled();
});

test("活动与审计按任务 A 筛选，看到「验收通过」", async ({ page }) => {
  await page.goto(`/#/teams/${teamId}/activity`);
  await page.reload();
  await page.getByLabel("任务", { exact: true }).selectOption({
    label: `${taskA.key} ${taskA.title}`,
  });
  await expect(page).toHaveURL(new RegExp(`task=${taskA.id}`));
  await expect(
    page.locator(".act-table__text", { hasText: `${taskA.key} 第 1 次执行验收通过` }),
  ).toHaveCount(1);
  await expect(page.locator(".act-table__text", { hasText: taskB.key })).toHaveCount(0);
});

test("共享文件页打开 A 引用的 md，右侧看到引用任务 A", async ({ page }) => {
  await page.goto(taskA.url);
  await page.locator(".task-detail__files .file-chip__link", { hasText: SPEC_MD }).click();
  await expect(page.getByRole("heading", { level: 1, name: "共享文件" })).toBeVisible();
  await expect(page.locator(".files-doc__path")).toHaveText(SPEC_MD);
  await expect(page.locator(".files-doc__body h1")).toHaveText("需求说明");
  const refs = page.locator(".files-refs .files-ref");
  await expect(refs).toHaveCount(1);
  await expect(refs).toContainText(taskA.key);
  await expect(refs).toContainText("撰写说明");
  await refs.getByRole("link", { name: "打开任务 →" }).click();
  await expect(page.getByRole("heading", { level: 1, name: taskA.title })).toBeVisible();
});

/** 制造一次合并冲突并点「重新执行」确认丢弃；返回回到任务详情（READY）时的任务与旧执行。 */
async function conflictThenRerun(page: Page, title: string, file: string) {
  const task = await createTaskUi(page, `${title} [mock:shellwrite=${file}]`, { status: "READY" });
  await runToReview(page);
  const first = (await runsOf(task.id))[0] as RunInfo;
  expect(first.branch).toBe(`teamspace/${task.key}-r1`);

  put(ws, file, "main side\n");
  git(ws, "add", file);
  git(ws, "commit", "-q", "-m", `docs: main side ${file}`);

  await goReview(page);
  await approveAndMerge(page);
  await expect(page.locator(".review-banner--warning")).toContainText("合并有冲突");

  // 重新执行：确认弹窗说明会丢弃旧执行，确认后回到任务详情，任务 READY
  await page.getByRole("button", { name: "重新执行" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toContainText("会删除旧的执行目录与执行分支");
  await dialog.getByRole("button", { name: "确认，丢弃旧执行" }).click();
  await expect(page).toHaveURL(new RegExp(`/tasks/${task.id}$`));
  await expect(page.locator(".task-detail__status")).toContainText("READY");

  // 旧执行分支与目录已清理；基线分支没有被冲突弄脏
  expect(git(ws, "branch", "--list", first.branch ?? "").trim()).toBe("");
  expect(existsSync(first.execDir)).toBe(false);
  expect(git(ws, "status", "--porcelain", "--untracked-files=no").trim()).toBe("");
  expect(readFileSync(join(ws, file), "utf8")).toBe("main side\n");
  return task;
}

test("合并冲突 → 重新执行（确认丢弃旧执行）→ 再执行 → 再验收 → 合并成功", async ({ page }) => {
  const task = await conflictThenRerun(page, "重做改写", "notes/x.txt");

  // 重新执行后任务详情提供「开始执行」，第 2 次执行走界面触发
  await page.goto(task.url);
  await page
    .getByRole("button", { name: /^(开始执行|重新执行)$/ })
    .first()
    .click();
  await expect.poll(async () => (await runsOf(task.id))[0]?.attempt).toBe(2);
  await expect
    .poll(async () => (await runsOf(task.id))[0]?.status, { timeout: TIMEOUT })
    .toBe("succeeded");
  const runs = await runsOf(task.id);
  expect(runs[0]?.branch).toBe(`teamspace/${task.key}-r2`);

  // 第 2 次执行基于最新基线；验收页显示第 2 次，通过后合并成功
  await page.goto(`${task.url}/review`);
  await expect(page.getByLabel("执行信息")).toContainText("第 2 次执行（当前）");
  await approveAndMerge(page);
  await expect(page.locator(".review-banner--success").first()).toContainText("合并成功");
  expect(readFileSync(join(ws, "notes/x.txt"), "utf8")).toContain(OK_MARK);
  await page.goto(task.url);
  await expect(page.locator(".task-detail__status")).toContainText("已完成");
});

test("合并冲突重新执行后，任务详情提供「开始执行」入口", async ({ page }) => {
  await conflictThenRerun(page, "重做入口", "notes/y.txt");
  await page.reload();
  await expect(page.getByRole("button", { name: /^(开始执行|重新执行)$/ }).first()).toBeVisible();
  await expect(page.locator(".run-outcome--success")).not.toContainText("待验收");
  await expect(page.getByRole("button", { name: "去验收" })).toHaveCount(0);
});
