// 看板分页的两处纯逻辑：拼接去重与「还有没有没显示的」。
// 这里直接对着缺陷场景写断言，不依赖 React 与内核。
import { describe, expect, it } from "vitest";

import type { RoleRef } from "../src/bindings/RoleRef.js";
import type { TaskSummary } from "../src/bindings/TaskSummary.js";
import { columnHasHiddenTasks, mergeBoardTasks } from "../src/pages/tasks/boardPaging.js";

const OWNER: RoleRef = {
  id: "role-1",
  name: "前端开发",
  code: "fe",
  taskPrefix: "FE",
  archived: false,
};

function task(id: string, updatedAt = `2026-10-21T00:00:0${id}Z`): TaskSummary {
  return {
    id,
    key: `FE-${id}`,
    title: `任务 ${id}`,
    status: "todo",
    owner: OWNER,
    fileRefCount: 0,
    dependencyCount: 0,
    openDependencyCount: 0,
    blockedReason: null,
    updatedAt,
    changeIncomplete: false,
  };
}

describe("mergeBoardTasks", () => {
  it("第一页与后续页不重叠时按顺序拼在一起", () => {
    const merged = mergeBoardTasks([task("1"), task("2")], [task("3"), task("4")]);
    expect(merged.map((item) => item.id)).toEqual(["1", "2", "3", "4"]);
  });

  it("看板刷新后第一页与已加载的后续页重叠时按 id 去重，不出现重复卡片", () => {
    // 第一页重新取回后与旧的追加页重叠（2、3 同时出现在两边）。
    const merged = mergeBoardTasks(
      [task("1"), task("2"), task("3")],
      [task("2"), task("3"), task("4")],
    );
    expect(merged.map((item) => item.id)).toEqual(["1", "2", "3", "4"]);
  });

  it("重复时以第一页的副本为准", () => {
    const stale = { ...task("1"), title: "旧标题" };
    const fresh = { ...task("1"), title: "新标题" };
    const merged = mergeBoardTasks([fresh], [stale]);
    expect(merged).toHaveLength(1);
    expect(merged[0]?.title).toBe("新标题");
  });

  it("后续页内部自己重复时也只留一份", () => {
    const merged = mergeBoardTasks([], [task("5"), task("5"), task("6")]);
    expect(merged.map((item) => item.id)).toEqual(["5", "6"]);
  });

  it("两边都空时结果为空", () => {
    expect(mergeBoardTasks([], [])).toEqual([]);
  });
});

describe("columnHasHiddenTasks", () => {
  it("后端还有没取回的时有", () => {
    expect(columnHasHiddenTasks(true, 200, 200)).toBe(true);
  });

  it("后端翻完了但本地已取回的还没显示时也要有", () => {
    // 合并列的典型场景：后端两状态都无更多，本地已取回 600 条，只显示 400 条。
    expect(columnHasHiddenTasks(false, 600, 400)).toBe(true);
  });

  it("取回的都显示出来了时没有", () => {
    expect(columnHasHiddenTasks(false, 400, 400)).toBe(false);
  });

  it("后端有更多时即使本地显示数一样也算有", () => {
    expect(columnHasHiddenTasks(true, 200, 200)).toBe(true);
  });
});
