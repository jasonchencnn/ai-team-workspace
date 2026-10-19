// 任务面板工具栏的共用操作。
import { type Page, expect } from "@playwright/test";

/**
 * 勾选任务面板工具栏上的勾选框（「显示已完成」「显示已取消」）。
 *
 * 勾选状态是地址栏回写的：点一下先改 URL，React 随后才把新的参数渲染成勾选状态，
 * 所以这里点完等它落定，不用 Playwright 的 check()（它会立刻断言状态，撞上这个间隙）。
 * 与活动与审计页的 tickKind 同一处理。
 */
export async function tickTaskBoardBox(page: Page, label: string | RegExp): Promise<void> {
  const box = page.getByLabel(label);
  const before = await box.isChecked();
  await box.click();
  await expect(box).toBeChecked({ checked: !before });
}
