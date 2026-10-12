import { defineConfig } from "@playwright/test";

import { UI_PORT } from "./e2e/ports.js";

// 无头 Chromium 驱动真实界面；invoke 经测试桥接落到真实内核和临时 Git 仓库。
// 用例共享同一个临时数据库并按顺序执行，所以只开一个 worker。
export default defineConfig({
  testDir: "./e2e",
  testMatch: "**/*.spec.ts",
  globalSetup: "./e2e/globalSetup.ts",
  fullyParallel: false,
  workers: 1,
  retries: 0,
  reporter: [["list"]],
  timeout: 30_000,
  expect: { timeout: 8_000 },
  outputDir: "./test-results",
  use: {
    actionTimeout: 8_000,
    baseURL: `http://127.0.0.1:${String(UI_PORT)}`,
    viewport: { width: 1440, height: 960 },
    locale: "zh-CN",
    trace: "off",
  },
});
