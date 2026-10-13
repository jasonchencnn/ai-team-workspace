// 各用例文件共用的 ACP 接入辅助：新建 Mock 接入并等它握手完成。
import { type Locator, type Page, expect } from "@playwright/test";

/** 接入列表里某个接入的条目。 */
export function acpEntry(page: Page, name: string): Locator {
  return page.locator(".acp-entry", { hasText: name });
}

/**
 * 在「Harness 接入」页签新建一个 Mock 接入（选 Mock 预设、改名、保存并握手检查），
 * 返回后条目已出现在接入列表且握手为「已连接」。调用前页面需已在 `/roles?tab=harness`。
 */
export async function createMockHarness(page: Page, name: string): Promise<void> {
  await page.getByRole("button", { name: "新建接入" }).click();
  await page.locator(".acp-preset", { hasText: "Mock" }).click();
  await page.getByLabel("实例名称").fill(name);
  await page.getByRole("button", { name: "保存并握手检查" }).click();
  const entry = acpEntry(page, name);
  await expect(entry).toBeVisible();
  await expect(entry).toContainText("已连接");
}
