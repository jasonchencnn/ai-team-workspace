// M1.1 产品调整端到端回归与探索测试：
// 覆盖：通用非开发团队流程、普通文件夹工作区、岗位自定义与前缀建议规则、岗位编辑稳定性、
// 文件夹边界（空、大量文件、隐藏与 node_modules 过滤、中文与空格路径）、从示例添加与删除、全页面文案纯净性。
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { type Locator, type Page, expect, test } from "@playwright/test";

const artifactsDir = fileURLToPath(new URL("../e2e-artifacts/", import.meta.url));
mkdirSync(artifactsDir, { recursive: true });

let shotIndex = 0;
async function captureQaShot(page: Page, title: string, focus?: Locator): Promise<string> {
  if (focus !== undefined) {
    await focus.scrollIntoViewIfNeeded();
  }
  shotIndex += 1;
  const seq = String(shotIndex).padStart(2, "0");
  const filename = `qa2-${seq}-${title}.png`;
  const targetPath = join(artifactsDir, filename);
  await page.screenshot({ path: targetPath, fullPage: false });
  return targetPath;
}

function writeSampleFile(root: string, relPath: string, content: string): void {
  const full = join(root, relPath);
  mkdirSync(dirname(full), { recursive: true });
  writeFileSync(full, content, "utf8");
}

function extractTeamId(page: Page): string {
  const matched = /\/teams\/([^/?]+)/.exec(page.url())?.[1] ?? "";
  expect(matched).not.toBe("");
  return matched;
}

test.describe.configure({ mode: "serial" });

test.describe("M1.1 回归与探索测试套件", () => {
  const tempFolders: string[] = [];
  let videoTeamId = "";
  let videoPlainDir = "";

  test.afterAll(() => {
    for (const folder of tempFolders) {
      try {
        rmSync(folder, { recursive: true, force: true });
      } catch {
        // 忽略测试清理错误
      }
    }
  });

  test("01. 通用（非开发）团队完整流程：普通文件夹、自定义中文岗位建议与改写、Mock绑定、任务引用与流转完成", async ({
    page,
  }) => {
    // 准备非 Git 普通文件夹素材
    videoPlainDir = mkdtempSync(join(tmpdir(), "qa2-plain-media-"));
    tempFolders.push(videoPlainDir);
    writeSampleFile(videoPlainDir, "raw/镜头01.mov", "video clip 01\n");
    writeSampleFile(videoPlainDir, "raw/镜头02.mov", "video clip 02\n");
    writeSampleFile(videoPlainDir, "分镜脚本.md", "# 宣传片分镜脚本\n- 场景 1\n- 场景 2\n");
    writeSampleFile(videoPlainDir, "调色参考.png", "color lookup mock\n");

    // 1. 进入创建团队页
    await page.goto("/#/teams/new");
    await expect(page.getByRole("heading", { name: "创建团队" })).toBeVisible();
    await page.getByLabel("团队名称").fill("纪录片影视后期制作组");
    await page
      .getByLabel("说明")
      .first()
      .fill("用于影视素材粗剪、精剪、调色与审片验收的专业非开发团队");

    // 2. 检测普通工作空间
    await page.getByLabel("手动输入工作空间路径").fill(videoPlainDir);
    await page.getByRole("button", { name: "检测", exact: true }).click();
    await expect(page.getByText("普通文件夹", { exact: true })).toBeVisible();
    await expect(page.getByText(/Git 仓库：/)).toHaveCount(0);
    await captureQaShot(page, "video-team-plain-workspace-detected");

    // 3. 自定义岗位 1：剪辑师（中文名前缀自动建议 R1，手动改写为 ED）
    await page.getByRole("button", { name: "+ 添加岗位" }).click();
    const card1 = page.locator(".tc-role").nth(0);
    await card1.getByLabel("岗位名称").fill("剪辑师");
    await card1.getByLabel("岗位名称").blur();
    const prefixInput1 = card1.getByLabel("任务编号前缀");
    await expect(prefixInput1).toHaveValue("R1");
    // 手动改写前缀为 ED
    await prefixInput1.fill("ED");
    await card1.getByLabel("职责").fill("负责粗剪、精剪与时间线梳理");
    await captureQaShot(page, "role-editor-custom-prefix-ed", card1);

    // 4. 自定义岗位 2：调色师（因 ED 不占用 R1，自动建议应为 R1）
    await page.getByRole("button", { name: "+ 添加岗位" }).click();
    const card2 = page.locator(".tc-role").nth(1);
    await card2.getByLabel("岗位名称").fill("调色师");
    await card2.getByLabel("岗位名称").blur();
    const prefixInput2 = card2.getByLabel("任务编号前缀");
    await expect(prefixInput2).toHaveValue("R1");
    await card2.getByLabel("职责").fill("负责调色、画质还原与风格定调");
    await captureQaShot(page, "role-colorist-suggested-prefix-r1", card2);

    // 5. 自定义岗位 3：审片人（避开 ED 与 R1，自动建议应为 R2，设为 Leader）
    await page.getByRole("button", { name: "+ 添加岗位" }).click();
    const card3 = page.locator(".tc-role").nth(2);
    await card3.getByLabel("岗位名称").fill("审片人");
    await card3.getByLabel("岗位名称").blur();
    const prefixInput3 = card3.getByLabel("任务编号前缀");
    await expect(prefixInput3).toHaveValue("R2");
    await card3.getByLabel("职责").fill("把控成片节奏、画面与音画同步标准，负责终审验收");
    // 设为 Leader
    const leaderCheckbox = card3.locator(".tc-role__leader input[type='checkbox']");
    await leaderCheckbox.check();
    await expect(leaderCheckbox).toBeChecked();
    await captureQaShot(page, "role-reviewer-leader-r2", card3);

    // 6. 提交创建团队
    await page.getByRole("button", { name: "创建团队", exact: true }).click();
    await expect(page).toHaveURL(/\/teams\/(?!new$)[^/]+$/);
    await expect(page.getByRole("heading", { name: "团队空间" })).toBeVisible();
    videoTeamId = extractTeamId(page);

    // 验证团队空间：3 个岗位，Leader 在前
    const roleCards = page.locator(".th-roles .th-role");
    await expect(roleCards).toHaveCount(3);
    await expect(roleCards.nth(0)).toContainText("审片人");
    await expect(roleCards.nth(1)).toContainText("剪辑师");
    await expect(roleCards.nth(2)).toContainText("调色师");

    // 验证上下文卡片标明「普通文件夹」且无 Git 分支信息
    const contextPanel = page.locator(".th-panel", { hasText: "团队共用的上下文" });
    await expect(contextPanel.getByText("普通文件夹", { exact: true })).toBeVisible();
    await expect(page.getByText(/Git 仓库：/)).toHaveCount(0);
    // 顶栏：仅显示「工作区」，无分支与提交
    await expect(page.locator(".topbar__branch-label")).toHaveText("工作区");
    await expect(page.locator(".topbar__commit")).toHaveCount(0);
    await captureQaShot(page, "video-teamspace-home-overview");

    // 7. 新建 Mock Harness 并健康检查
    await page.goto(`/#/teams/${videoTeamId}/roles?tab=harness`);
    await page.getByRole("button", { name: "新建实例" }).click();
    await page.getByLabel("实例名称").fill("mock-post-pipeline");
    await page.getByLabel("类型").selectOption("mock");
    await page.getByRole("button", { name: "注册实例" }).click();

    const harnessCard = page.locator(".harness-card", { hasText: "mock-post-pipeline" });
    await expect(harnessCard).toBeVisible();
    await expect(harnessCard).toContainText("测试用");
    await harnessCard.getByRole("button", { name: "查看详情" }).click();
    await expect(page.getByRole("heading", { name: "健康检查结果" })).toBeVisible();
    await page.getByRole("button", { name: "重新健康检查" }).click();
    await expect(page.locator(".harness-detail__health .ui-tag")).toHaveText("在线");
    await expect(page.getByText("Mock Harness，无需检测")).toBeVisible();
    await page.getByRole("button", { name: "返回实例列表" }).click();
    await captureQaShot(page, "mock-harness-registered-healthy");

    // 8. 把「剪辑师」岗位绑定到该 Mock 实例
    await page.goto(`/#/teams/${videoTeamId}/roles`);
    await page.getByRole("button", { name: /^剪辑师/ }).click();
    await page.getByRole("button", { name: "绑定 Harness", exact: true }).click();
    const harnessSelect = page.getByLabel("选择本团队的 Harness 实例");
    const harnessOptVal = await harnessSelect
      .locator("option", { hasText: "mock-post-pipeline" })
      .getAttribute("value");
    await harnessSelect.selectOption(harnessOptVal ?? "");
    await page.getByRole("button", { name: "确认绑定" }).click();
    await expect(page.getByText("当前绑定 mock-post-pipeline")).toBeVisible();
    await captureQaShot(page, "editor-role-bound-to-mock-harness");

    // 9. 创建引用普通文件夹文件的任务
    await page.goto(`/#/teams/${videoTeamId}/tasks/new`);
    await page.getByLabel("任务名称").fill("宣传片首版粗剪与场景标记");
    const ownerSelect = page.getByLabel("负责人岗位");
    const editorVal = await ownerSelect
      .locator("option", { hasText: "剪辑师" })
      .getAttribute("value");
    await ownerSelect.selectOption(editorVal ?? "");

    // 搜索并添加普通文件夹中的「分镜脚本.md」和「raw/镜头01.mov」
    await page.locator("#task-file-search").fill("分镜");
    const sugg1 = page.locator(".task-form__suggestion", { hasText: "分镜脚本.md" });
    await expect(sugg1).toBeVisible();
    await sugg1.click();

    await page.locator("#task-file-search").fill("镜头01");
    const sugg2 = page.locator(".task-form__suggestion", { hasText: "raw/镜头01.mov" });
    await expect(sugg2).toBeVisible();
    await sugg2.click();

    await expect(page.locator(".task-form__chip", { hasText: "分镜脚本.md" })).toBeVisible();
    await expect(page.locator(".task-form__chip", { hasText: "raw/镜头01.mov" })).toBeVisible();

    await page.getByRole("radio", { name: "READY", exact: true }).check();
    await captureQaShot(page, "create-task-referencing-plain-files");
    await page.getByRole("button", { name: "创建并分配" }).click();

    // 10. 进入任务详情页，验证编号为 ED-001，文件引用正常
    await expect(page).toHaveURL(new RegExp(`/teams/${videoTeamId}/tasks/(?!new$)[^/]+$`));
    const statusLine = page.locator(".task-detail__status");
    await expect(statusLine).toContainText("READY / ED-001");
    await expect(page.locator(".file-chip", { hasText: "分镜脚本.md" })).toBeVisible();
    await expect(page.locator(".file-chip", { hasText: "raw/镜头01.mov" })).toBeVisible();
    // 引用的文件都真实存在于普通文件夹中，不应有「文件不存在」提示
    await expect(page.locator(".file-chip--missing")).toHaveCount(0);
    await captureQaShot(page, "task-detail-ready-ed001");

    // 11. 状态流转：READY → 执行中 → 待验收 → 已完成
    await page.getByRole("button", { name: "执行中", exact: true }).click();
    await expect(statusLine).toContainText("执行中 / ED-001");
    await captureQaShot(page, "task-transition-to-doing");

    await page.getByRole("button", { name: "待验收", exact: true }).click();
    await expect(statusLine).toContainText("待验收 / ED-001");
    await captureQaShot(page, "task-transition-to-review");

    await page.getByRole("button", { name: "已完成", exact: true }).click();
    await expect(statusLine).toContainText("已完成 / ED-001");
    await captureQaShot(page, "task-transition-to-done");

    // 12. 看板中勾选「显示已完成」后显示该任务卡片
    await page.goto(`/#/teams/${videoTeamId}/tasks`);
    await page.getByLabel(/显示已完成/).check();
    const doneCard = page.locator("a.task-card-link", { hasText: /ED-001.*宣传片首版粗剪/ });
    await expect(doneCard).toBeVisible();
    await captureQaShot(page, "task-board-shows-completed-ed001");
  });

  test("02. 前缀建议完备性矩阵：英文多词、单词、缩写、中文、冲突递增、手填防冲掉及快速连续修改", async ({
    page,
  }) => {
    await page.goto("/#/teams/new");
    await page.getByLabel("团队名称").fill("前缀测试专用团队");

    // 辅助闭包：添加并检验前缀
    const addRoleAndVerify = async (name: string, expected: string) => {
      await page.getByRole("button", { name: "+ 添加岗位" }).click();
      const lastCard = page.locator(".tc-role").last();
      await lastCard.getByLabel("岗位名称").fill(name);
      await lastCard.getByLabel("岗位名称").blur();
      await expect(lastCard.getByLabel("任务编号前缀")).toHaveValue(expected);
      return lastCard;
    };

    // A. 英文多词：首字母拼接（最多 4 位）
    await addRoleAndVerify("Sound Designer", "SD");
    await addRoleAndVerify("Chief Creative Officer", "CCO");

    // B. 单个英文单词：取前两个字母
    await addRoleAndVerify("Director", "DI");
    await addRoleAndVerify("Producer", "PR");

    // C. 全大写英文缩写：原样保留
    await addRoleAndVerify("QA", "QA");
    await addRoleAndVerify("VFX", "VFX");
    await addRoleAndVerify("PM", "PM");

    // D. 中文名：无英文字母，取 R1、R2...
    await addRoleAndVerify("调音师", "R1");
    await addRoleAndVerify("制片人", "R2");

    // E. 冲突递增：
    // 已有 "Animation Director" 占用 "AD"
    await addRoleAndVerify("Animation Director", "AD");
    // 新增 "Art Director" 本应建议 "AD"，已被占用，自动递增为 "AD2"
    await addRoleAndVerify("Art Director", "AD2");
    // 新增 "Audio Director" 本应建议 "AD"，前两者已被占用，自动递增为 "AD3"
    await addRoleAndVerify("Audio Director", "AD3");

    // F. 全大写缩写冲突递增：已有 "VFX"，再次填写 "VFX" 自动建议 "VFX2"
    await addRoleAndVerify("VFX", "VFX2");

    // G. 手填前缀保护验证 1（建议到达前已手填）：
    await page.getByRole("button", { name: "+ 添加岗位" }).click();
    const manualCard1 = page.locator(".tc-role").last();
    // 先手填自定义前缀
    await manualCard1.getByLabel("任务编号前缀").fill("MAN1");
    // 后填名称并失焦
    await manualCard1.getByLabel("岗位名称").fill("Script Supervisor");
    await manualCard1.getByLabel("岗位名称").blur();
    // 前缀必须保留用户手填的 MAN1，不能被 SS 覆盖
    await expect(manualCard1.getByLabel("任务编号前缀")).toHaveValue("MAN1");

    // H. 手填前缀保护验证 2（建议到达后手改，后续名称再变也不覆盖）：
    await page.getByRole("button", { name: "+ 添加岗位" }).click();
    const manualCard2 = page.locator(".tc-role").last();
    await manualCard2.getByLabel("岗位名称").fill("Colorist");
    await manualCard2.getByLabel("岗位名称").blur();
    await expect(manualCard2.getByLabel("任务编号前缀")).toHaveValue("CO");
    // 手动修改前缀为 CLR
    await manualCard2.getByLabel("任务编号前缀").fill("CLR");
    // 再次修改岗位名称为 Senior Colorist 并失焦
    await manualCard2.getByLabel("岗位名称").fill("Senior Colorist");
    await manualCard2.getByLabel("岗位名称").blur();
    // 前缀必须依然保持 CLR，不能被 SC 覆盖
    await expect(manualCard2.getByLabel("任务编号前缀")).toHaveValue("CLR");

    // I. 用户清空手填前缀后重新接受建议：
    await manualCard2.getByLabel("任务编号前缀").fill("");
    await manualCard2.getByLabel("岗位名称").fill("Senior Colorist");
    await manualCard2.getByLabel("岗位名称").blur();
    await expect(manualCard2.getByLabel("任务编号前缀")).toHaveValue("SC");

    await captureQaShot(page, "prefix-matrix-comprehensive-validation");
  });

  test("02b. 名称快速多次修改时前缀建议只采用最新名称的结果", async ({ page }) => {
    await page.goto("/#/teams/new");
    await page.getByLabel("团队名称").fill("快速输入竞态验证团队");
    await page.getByRole("button", { name: "+ 添加岗位" }).click();
    const rapidCard = page.locator(".tc-role").last();
    const nameField = rapidCard.getByLabel("岗位名称");
    await nameField.fill("Stage One");
    await nameField.blur();
    await nameField.fill("Stage Two");
    await nameField.blur();
    await nameField.fill("Final Delivery");
    await nameField.blur();
    await captureQaShot(page, "prefix-rapid-edits-latest-name-wins", rapidCard);
    // 三次建议请求并发在途且可能乱序返回，最终前缀必须停在最后一次名称 "Final Delivery" -> "FD"，
    // 不能被迟到的旧响应（SO / ST）覆盖。
    await expect(rapidCard.getByLabel("任务编号前缀")).toHaveValue("FD");
  });

  test("03. 自定义岗位编辑：改名后任务编号不变、岗位代码标识不变", async ({ page }) => {
    expect(videoTeamId).not.toBe("");

    // 1. 进入岗位与绑定页，选择「剪辑师」
    await page.goto(`/#/teams/${videoTeamId}/roles`);
    await page.getByRole("button", { name: /^剪辑师/ }).click();

    // 记录原岗位标识
    const metaLine = page.locator(".role-detail__meta");
    await expect(metaLine).toContainText(/岗位标识\s+role_\d+（自动生成）/);
    const initialMetaText = (await metaLine.textContent()) ?? "";
    const matchedCode = /role_\d+/.exec(initialMetaText)?.[0] ?? "";
    expect(matchedCode).not.toBe("");

    // 2. 修改岗位名称为「资深剪辑指导」，修改职责
    const nameInput = page.getByLabel("岗位名称");
    await nameInput.fill("资深剪辑指导");
    await page.getByLabel("职责").fill("指导全部粗剪与精剪，制定后期节奏标准");

    await captureQaShot(page, "editing-role-name-and-description");
    await page.getByRole("button", { name: "保存岗位说明" }).click();

    // 3. 验证岗位标识不变
    await expect(page.locator(".role-detail__title")).toHaveText("资深剪辑指导");
    await expect(metaLine).toContainText(matchedCode);

    // 4. 前往任务面板，勾选「显示已完成」，验证原有任务编号依然保持 ED-001，没有被篡改
    await page.goto(`/#/teams/${videoTeamId}/tasks`);
    await page.getByLabel(/显示已完成/).check();
    const taskCard = page.locator("a.task-card-link", { hasText: "ED-001" });
    await expect(taskCard).toBeVisible();

    // 5. 进入任务详情页，验证负责人名称已联动更新为「资深剪辑指导」，编号仍为 ED-001
    await taskCard.click();
    await expect(page.locator(".task-detail__status")).toContainText("已完成 / ED-001");
    await expect(page.locator(".task-detail__owner")).toContainText("资深剪辑指导");
    await captureQaShot(page, "task-remains-ed001-with-updated-owner-name");
  });

  test("04. 普通文件夹边界场景：空文件夹、大量文件上限截断、跳过隐藏与node_modules、中文空格路径", async ({
    page,
  }) => {
    // 边界 1：空文件夹
    const emptyDir = mkdtempSync(join(tmpdir(), "qa2-empty-dir-"));
    tempFolders.push(emptyDir);

    await page.goto("/#/teams/new");
    await page.getByLabel("团队名称").fill("空文件夹边界团队");
    await page.getByLabel("手动输入工作空间路径").fill(emptyDir);
    await page.getByRole("button", { name: "检测", exact: true }).click();
    await expect(page.getByText("普通文件夹", { exact: true })).toBeVisible();
    await page.getByRole("button", { name: "+ 添加岗位" }).click();
    const emptyRole = page.locator(".tc-role").first();
    await emptyRole.getByLabel("岗位名称").fill("资料员");
    await emptyRole.getByLabel("岗位名称").blur();
    await page.getByRole("button", { name: "创建团队", exact: true }).click();
    await expect(page).toHaveURL(/\/teams\/(?!new$)[^/]+$/);
    const emptyTeamId = extractTeamId(page);

    // 在空文件夹团队中新建任务并搜索文件，验证 0 条匹配且界面稳定不崩溃
    await page.goto(`/#/teams/${emptyTeamId}/tasks/new`);
    await page.locator("#task-file-search").fill("anyfile");
    await expect(
      page.getByText("没有匹配的文件；可以直接输入工作空间内的相对路径。"),
    ).toBeVisible();
    await expect(page.locator(".task-form__suggestion")).toHaveCount(0);
    await captureQaShot(page, "boundary-empty-folder-search");

    // 边界 2：大量文件（验证搜索最多返回 50 条上限）
    const largeDir = mkdtempSync(join(tmpdir(), "qa2-large-dir-"));
    tempFolders.push(largeDir);
    for (let i = 1; i <= 65; i++) {
      writeSampleFile(
        largeDir,
        `clips/shot_${String(i).padStart(3, "0")}.txt`,
        `shot ${String(i)}\n`,
      );
    }

    await page.goto("/#/teams/new");
    await page.getByLabel("团队名称").fill("大文件量边界团队");
    await page.getByLabel("手动输入工作空间路径").fill(largeDir);
    await page.getByRole("button", { name: "检测", exact: true }).click();
    await expect(page.getByText("普通文件夹", { exact: true })).toBeVisible();
    await page.getByRole("button", { name: "+ 添加岗位" }).click();
    const largeRole = page.locator(".tc-role").first();
    await largeRole.getByLabel("岗位名称").fill("编目员");
    await largeRole.getByLabel("岗位名称").blur();
    await page.getByRole("button", { name: "创建团队", exact: true }).click();
    await expect(page).toHaveURL(/\/teams\/(?!new$)[^/]+$/);
    const largeTeamId = extractTeamId(page);

    await page.goto(`/#/teams/${largeTeamId}/tasks/new`);
    await page.locator("#task-file-search").fill("shot");
    const suggestions = page.locator(".task-form__suggestion");
    await expect(suggestions.first()).toBeVisible();
    // 搜索最多返回 50 条
    await expect(suggestions).toHaveCount(50);
    await captureQaShot(page, "boundary-large-dir-capped-at-50");

    // 边界 3：过滤隐藏目录、隐藏文件与 node_modules / target / dist / build
    const filterDir = mkdtempSync(join(tmpdir(), "qa2-filter-dir-"));
    tempFolders.push(filterDir);
    writeSampleFile(filterDir, ".hidden_secret/passwords.txt", "secret\n");
    writeSampleFile(filterDir, ".env.production", "SECRET_KEY=123\n");
    writeSampleFile(filterDir, "node_modules/my-lib/index.js", "console.log(1);\n");
    writeSampleFile(filterDir, "target/debug/app.exe", "binary\n");
    writeSampleFile(filterDir, "dist/bundle.js", "bundle\n");
    writeSampleFile(filterDir, "build/output.log", "log\n");
    writeSampleFile(filterDir, "docs/公开方案.md", "# 公开规范文档\n");
    writeSampleFile(filterDir, "scripts/check_assets.py", "print('ok')\n");

    await page.goto("/#/teams/new");
    await page.getByLabel("团队名称").fill("过滤规则验证团队");
    await page.getByLabel("手动输入工作空间路径").fill(filterDir);
    await page.getByRole("button", { name: "检测", exact: true }).click();
    await expect(page.getByText("普通文件夹", { exact: true })).toBeVisible();
    await page.getByRole("button", { name: "+ 添加岗位" }).click();
    const filterRole = page.locator(".tc-role").first();
    await filterRole.getByLabel("岗位名称").fill("排版员");
    await filterRole.getByLabel("岗位名称").blur();
    await page.getByRole("button", { name: "创建团队", exact: true }).click();
    await expect(page).toHaveURL(/\/teams\/(?!new$)[^/]+$/);
    const filterTeamId = extractTeamId(page);

    await page.goto(`/#/teams/${filterTeamId}/tasks/new`);
    const searchField = page.locator("#task-file-search");

    // 搜索隐藏文件/目录内容：不可见
    await searchField.fill("password");
    await expect(page.locator(".task-form__suggestion")).toHaveCount(0);
    await searchField.fill(".env");
    await expect(page.locator(".task-form__suggestion")).toHaveCount(0);

    // 搜索 node_modules 与构建产物：不可见
    await searchField.fill("index.js");
    await expect(page.locator(".task-form__suggestion")).toHaveCount(0);
    await searchField.fill("bundle.js");
    await expect(page.locator(".task-form__suggestion")).toHaveCount(0);
    await searchField.fill("app.exe");
    await expect(page.locator(".task-form__suggestion")).toHaveCount(0);

    // 搜索正常文件：可准确搜出
    await searchField.fill("公开方案");
    await expect(
      page.locator(".task-form__suggestion", { hasText: "docs/公开方案.md" }),
    ).toBeVisible();
    await searchField.fill("check_assets");
    await expect(
      page.locator(".task-form__suggestion", { hasText: "scripts/check_assets.py" }),
    ).toBeVisible();
    await captureQaShot(page, "boundary-filter-hidden-and-ignored-dirs");

    // 边界 4：路径中含有中文和空格
    const specialDir = mkdtempSync(join(tmpdir(), "影视 素材 归档 2026-测试-"));
    tempFolders.push(specialDir);
    writeSampleFile(specialDir, "现场 素材/特写 镜头 01.mov", "special clip\n");
    writeSampleFile(specialDir, "工程 脚本.md", "# 剪辑 工程 脚本\n");

    await page.goto("/#/teams/new");
    await page.getByLabel("团队名称").fill("中文与空格路径团队");
    await page.getByLabel("手动输入工作空间路径").fill(specialDir);
    await page.getByRole("button", { name: "检测", exact: true }).click();
    await expect(page.getByText("普通文件夹", { exact: true })).toBeVisible();
    // 验证显示的路径包含中文和空格
    await expect(page.getByText(specialDir)).toBeVisible();

    await page.getByRole("button", { name: "+ 添加岗位" }).click();
    const spRole = page.locator(".tc-role").first();
    await spRole.getByLabel("岗位名称").fill("总控师");
    await spRole.getByLabel("岗位名称").blur();
    await page.getByRole("button", { name: "创建团队", exact: true }).click();
    await expect(page).toHaveURL(/\/teams\/(?!new$)[^/]+$/);
    const specialTeamId = extractTeamId(page);

    // 验证搜索含中文和空格的文件
    await page.goto(`/#/teams/${specialTeamId}/tasks/new`);
    await page.locator("#task-file-search").fill("特写 镜头");
    await expect(
      page.locator(".task-form__suggestion", { hasText: "现场 素材/特写 镜头 01.mov" }),
    ).toBeVisible();
    await page.locator("#task-file-search").fill("工程 脚本");
    await expect(page.locator(".task-form__suggestion", { hasText: "工程 脚本.md" })).toBeVisible();
    await captureQaShot(page, "boundary-chinese-space-path-search");
  });

  test("05. 「从示例添加」后再删除部分示例岗位再创建", async ({ page }) => {
    const plainPath = process.env.E2E_PLAIN_WORKSPACE ?? "";
    expect(plainPath).not.toBe("");

    await page.goto("/#/teams/new");
    await page.getByLabel("团队名称").fill("示例增删组合验证团队");
    await page.getByLabel("手动输入工作空间路径").fill(plainPath);
    await page.getByRole("button", { name: "检测", exact: true }).click();
    await expect(page.getByText("普通文件夹", { exact: true })).toBeVisible();

    // 1. 点开「从示例添加」
    await page.getByRole("button", { name: "从示例添加" }).click();
    await expect(page.locator(".tc-example")).toHaveCount(5);

    // 2. 添加全部 5 个示例岗位
    const templateNames = ["Leader", "产品经理", "前端开发", "后端开发", "QA 测试"];
    for (const name of templateNames) {
      await page
        .locator(".tc-example", { hasText: name })
        .getByRole("button", { name: "添加" })
        .click();
    }
    await expect(page.locator(".tc-role")).toHaveCount(5);
    await captureQaShot(page, "all-five-example-roles-added");

    // 3. 删除「前端开发」与「QA 测试」
    const deleteRoleByName = async (name: string) => {
      const cards = page.locator(".tc-role");
      const count = await cards.count();
      for (let i = 0; i < count; i++) {
        const card = cards.nth(i);
        const val = await card.getByLabel("岗位名称").inputValue();
        if (val === name) {
          await card.getByRole("button", { name: "删除岗位" }).click();
          return;
        }
      }
      throw new Error(`Role card with name "${name}" not found to delete`);
    };

    const hasRoleCardWithName = async (name: string): Promise<boolean> => {
      const cards = page.locator(".tc-role");
      const count = await cards.count();
      for (let i = 0; i < count; i++) {
        const card = cards.nth(i);
        const val = await card.getByLabel("岗位名称").inputValue();
        if (val === name) {
          return true;
        }
      }
      return false;
    };

    await deleteRoleByName("前端开发");
    await deleteRoleByName("QA 测试");

    // 验证剩余 3 个岗位
    await expect(page.locator(".tc-role")).toHaveCount(3);
    expect(await hasRoleCardWithName("前端开发")).toBe(false);
    expect(await hasRoleCardWithName("QA 测试")).toBe(false);
    expect(await hasRoleCardWithName("Leader")).toBe(true);
    expect(await hasRoleCardWithName("产品经理")).toBe(true);
    expect(await hasRoleCardWithName("后端开发")).toBe(true);
    await captureQaShot(page, "roles-after-deleting-two-examples");

    // 4. 点击创建团队
    await page.getByRole("button", { name: "创建团队", exact: true }).click();
    await expect(page).toHaveURL(/\/teams\/(?!new$)[^/]+$/);
    await expect(page.getByRole("heading", { name: "团队空间" })).toBeVisible();

    // 验证团队空间恰好只有剩余的 3 个岗位卡片
    const teamRoles = page.locator(".th-roles .th-role");
    await expect(teamRoles).toHaveCount(3);
    await expect(page.locator(".th-role", { hasText: "Leader" })).toBeVisible();
    await expect(page.locator(".th-role", { hasText: "产品经理" })).toBeVisible();
    await expect(page.locator(".th-role", { hasText: "后端开发" })).toBeVisible();
    await expect(page.locator(".th-role", { hasText: "前端开发" })).toHaveCount(0);
    await expect(page.locator(".th-role", { hasText: "QA 测试" })).toHaveCount(0);
    await captureQaShot(page, "team-created-with-surviving-example-roles");
  });

  test("06. 界面文案纯净性：普通文件夹团队下所有页面无开发专用措辞或「Git 必需」暗示", async ({
    page,
  }) => {
    expect(videoTeamId).not.toBe("");

    // 1. 团队空间页（/teams/:id）
    await page.goto(`/#/teams/${videoTeamId}`);
    await expect(page.getByRole("heading", { name: "团队空间" })).toBeVisible();

    // 顶栏：不出现分支名、不出现 commit、不出现 (游离 HEAD)
    const topbar = page.locator(".topbar");
    await expect(topbar.locator(".topbar__branch-label")).toHaveText("工作区");
    await expect(topbar.locator(".topbar__commit")).toHaveCount(0);
    await expect(topbar).not.toContainText("游离");
    await expect(topbar).not.toContainText("HEAD");

    // 团队共用上下文卡片：显示普通文件夹，不显示 Git 仓库或提交
    const contextPanel = page.locator(".th-panel", { hasText: "团队共用的上下文" });
    await expect(contextPanel.getByText("普通文件夹", { exact: true })).toBeVisible();
    await expect(contextPanel).not.toContainText("Git 仓库");
    await expect(contextPanel).not.toContainText("代码");
    await captureQaShot(page, "text-audit-team-home-pure");

    // 2. 岗位与绑定页（/teams/:id/roles）
    await page.goto(`/#/teams/${videoTeamId}/roles`);
    await expect(page.getByRole("heading", { name: "岗位与绑定" })).toBeVisible();
    await expect(page.locator(".roles-page")).not.toContainText("代码仓库");
    await expect(page.locator(".roles-page")).not.toContainText("Git 必需");
    await captureQaShot(page, "text-audit-roles-page-pure");

    // 3. 任务表单页（/teams/:id/tasks/new）
    await page.goto(`/#/teams/${videoTeamId}/tasks/new`);
    await expect(page.getByRole("heading", { name: "创建任务" })).toBeVisible();
    // 页面说明是「需求正文放在工作空间的文件里」，而非「代码」
    await expect(page.getByText("需求正文放在工作空间的文件里")).toBeVisible();
    await expect(page.locator(".task-form")).not.toContainText("代码仓库");
    await expect(page.locator(".task-form")).not.toContainText("Git");
    await captureQaShot(page, "text-audit-task-form-pure");
  });
});
