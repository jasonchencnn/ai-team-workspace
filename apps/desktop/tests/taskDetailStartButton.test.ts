// 任务详情页头的「开始执行 / 重新执行」入口：验收通过（done）后不再显示。
// 能不能开始执行由后端的 runProblems 决定（置灰与否），这里只锁住「已完成不再给入口」这条显示规则。
import { describe, expect, it } from "vitest";

import type { TaskStatus } from "../src/bindings/TaskStatus.js";
import { showStartRunButton } from "../src/pages/task-detail/run/runUi.js";

describe("页头开始执行入口", () => {
  it("已验收通过（done）不显示", () => {
    expect(showStartRunButton("done")).toBe(false);
  });

  it("其余状态照旧显示（是否可用由后端 runProblems 决定）", () => {
    const shown: TaskStatus[] = ["todo", "ready", "doing", "blocked", "review", "cancelled"];
    expect(shown.filter(showStartRunButton)).toEqual(shown);
  });
});
